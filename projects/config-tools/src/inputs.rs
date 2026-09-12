//! Bounded, offline version-7 lock graph inspection. Follows resolution and
//! dependency-cycle detection are iterative, including deeply nested aliases.
use crate::{text::terminal, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_WORK: usize = 1_000_000;
type Edge = (String, String);
#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum Reference {
    Node(String),
    Follows(Vec<String>),
}
#[derive(Deserialize)]
struct Node {
    #[serde(default)]
    inputs: BTreeMap<String, Reference>,
    locked: Option<Map<String, Value>>,
}
#[derive(Deserialize)]
struct Lock {
    version: u64,
    root: String,
    nodes: BTreeMap<String, Node>,
}
#[derive(Serialize)]
struct Row {
    input: String,
    node: String,
    r#type: String,
    source: String,
    revision: Option<String>,
    last_modified: Option<String>,
    follows: Option<String>,
    shared_with: Option<String>,
    cycle: bool,
}
struct Graph {
    lock: Lock,
    resolved: HashMap<Edge, String>,
    work: usize,
}

fn modified(locked: &Map<String, Value>) -> Result<Option<String>> {
    let Some(value) = locked.get("lastModified") else {
        return Ok(None);
    };
    let seconds = value
        .as_u64()
        .filter(|seconds| *seconds <= 253_402_300_799)
        .ok_or("lastModified must be a supported nonnegative integer")?;
    Ok(Some(
        seele_runtime::time::format_timestamp(seconds as libc::time_t)
            .ok_or("lastModified is outside the supported date range")?,
    ))
}
fn string<'a>(locked: &'a Map<String, Value>, key: &str, default: &'a str) -> &'a str {
    locked.get(key).and_then(Value::as_str).unwrap_or(default)
}
fn safe_url(value: &str) -> String {
    if let Some((scheme, rest)) = value.split_once("://") {
        if let Ok(parsed) = url::Url::parse(value) {
            if parsed.host_str().is_some() {
                let authority = rest
                    .split(['/', '?', '#'])
                    .next()
                    .unwrap_or("")
                    .rsplit('@')
                    .next()
                    .unwrap_or("");
                // Parsing validates host/port; retain an explicitly written
                // default port, which URL normalization otherwise removes.
                return format!(
                    "{}://{}{}",
                    scheme.to_ascii_lowercase(),
                    authority.to_ascii_lowercase(),
                    parsed.path()
                );
            }
            if parsed.scheme() == "file" {
                return format!("file://{}", parsed.path());
            }
        }
        return "[URL omitted]".into();
    }
    if let Some(path) = value.strip_prefix("file:") {
        return format!("file://{}", path.split(['?', '#']).next().unwrap_or(""));
    }
    let value = value
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("");
    if let Some((host, path)) = value.split_once(':') {
        if !host.is_empty() && !path.is_empty() && !host.contains(['/', ':']) {
            return format!("{host}:{path}");
        }
    }
    "[URL omitted]".into()
}
fn source(locked: &Map<String, Value>) -> String {
    if let Some(value) = locked.get("url").and_then(Value::as_str) {
        return safe_url(value);
    }
    let kind = string(locked, "type", "root");
    match kind {
        "path" => format!("path:{}", string(locked, "path", "[unspecified]")),
        "github" | "gitlab" | "sourcehut" => {
            let repository = format!(
                "{}/{}",
                string(locked, "owner", "?"),
                string(locked, "repo", "?")
            );
            let host = locked
                .get("host")
                .and_then(Value::as_str)
                .map(|host| {
                    format!(
                        "{}/",
                        safe_url(&format!("https://{host}"))
                            .trim_start_matches("https://")
                            .trim_end_matches('/')
                    )
                })
                .unwrap_or_default();
            format!("{kind}:{host}{repository}")
        }
        _ => kind.into(),
    }
}
impl Graph {
    fn new(lock: Lock) -> Result<Self> {
        if lock.version != 7 {
            return Err("expected a version-7 flake.lock object".into());
        }
        if !lock.nodes.contains_key(&lock.root) || lock.nodes.len() > 100_000 {
            return Err("lock file needs a bounded nodes object and an existing root node".into());
        }
        let mut edges = 0;
        for (name, node) in &lock.nodes {
            if name.is_empty() {
                return Err("node names must be nonempty".into());
            }
            edges += node.inputs.len();
            if edges > 100_000 {
                return Err("lock graph exceeds its edge limit".into());
            }
            for (key, reference) in &node.inputs {
                if key.is_empty() || key.contains('/') {
                    return Err("input names must be nonempty strings without slashes".into());
                }
                match reference {
                    Reference::Node(target) if !lock.nodes.contains_key(target) => {
                        return Err("input references a missing node".into())
                    }
                    Reference::Follows(path)
                        if path.len() > 4096
                            || path
                                .iter()
                                .any(|part| part.is_empty() || part.contains('/')) =>
                    {
                        return Err("invalid follows path".into())
                    }
                    _ => (),
                }
            }
            if name == &lock.root && node.locked.is_none() {
                continue;
            }
            let locked = node
                .locked
                .as_ref()
                .ok_or("node needs locked source metadata")?;
            if string(locked, "type", "").is_empty() {
                return Err("locked source needs a type".into());
            }
            for key in ["url", "path", "owner", "repo", "host", "rev"] {
                if locked.get(key).is_some_and(|value| !value.is_string()) {
                    return Err(format!("locked {key} must be a string").into());
                }
            }
            modified(locked)?;
        }
        Ok(Self {
            lock,
            resolved: HashMap::new(),
            work: 0,
        })
    }
    fn resolve(&mut self, path: &[String]) -> Result<String> {
        struct Frame {
            node: String,
            path: VecDeque<String>,
            edge: Option<Edge>,
        }
        let mut frames = vec![Frame {
            node: self.lock.root.clone(),
            path: path.iter().cloned().collect(),
            edge: None,
        }];
        let mut active = HashSet::new();
        while let Some(frame) = frames.last_mut() {
            self.work += 1;
            if self.work > MAX_WORK {
                return Err("lock graph exceeds its traversal limit".into());
            }
            let Some(part) = frame.path.pop_front() else {
                let frame = frames.pop().unwrap();
                if let Some(edge) = frame.edge {
                    active.remove(&edge);
                    self.resolved.insert(edge, frame.node.clone());
                }
                if let Some(parent) = frames.last_mut() {
                    parent.node = frame.node;
                    continue;
                }
                return Ok(frame.node);
            };
            let edge = (frame.node.clone(), part.clone());
            let reference = self.lock.nodes[&frame.node]
                .inputs
                .get(&part)
                .ok_or_else(|| {
                    format!(
                        "missing input {} on node {}",
                        terminal(&part),
                        terminal(&frame.node)
                    )
                })?;
            match reference {
                Reference::Node(node) => frame.node = node.clone(),
                Reference::Follows(_) if self.resolved.contains_key(&edge) => {
                    frame.node = self.resolved[&edge].clone()
                }
                Reference::Follows(path) => {
                    if !active.insert(edge.clone()) {
                        return Err("cyclic follows".into());
                    }
                    frames.push(Frame {
                        node: self.lock.root.clone(),
                        path: path.clone().into(),
                        edge: Some(edge),
                    });
                }
            }
        }
        Err("could not resolve input".into())
    }
    fn report(&mut self, all: bool, selected: Option<&str>) -> Result<Vec<Row>> {
        let initial: Vec<Vec<String>> = if let Some(selected) = selected {
            let path: Vec<String> = selected.split('/').map(String::from).collect();
            if path.len() > 4096 || path.iter().any(String::is_empty) {
                return Err("INPUT must be a slash-separated input path".into());
            }
            vec![path]
        } else {
            self.lock.nodes[&self.lock.root]
                .inputs
                .keys()
                .map(|key| vec![key.clone()])
                .collect()
        };
        let mut queue: VecDeque<_> = initial
            .into_iter()
            .map(|path| (path, HashSet::from([self.lock.root.clone()])))
            .collect();
        let mut expanded = HashMap::from([(self.lock.root.clone(), "<root>".to_string())]);
        let mut rows = Vec::new();
        let mut row_parents = Vec::new();
        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut path_budget = 0;
        let mut output_budget = 0;
        let mut queued_budget: usize = queue
            .iter()
            .map(|(path, _)| path.iter().map(String::len).sum::<usize>())
            .sum();
        while let Some((path, mut ancestors)) = queue.pop_front() {
            path_budget += path.len();
            if path_budget > MAX_WORK {
                return Err("lock report exceeds its path limit".into());
            }
            let node = self.resolve(&path)?;
            let parent = self.resolve(&path[..path.len() - 1])?;
            let reference = &self.lock.nodes[&parent].inputs[path.last().unwrap()];
            let root_metadata = Map::from_iter([("type".into(), Value::String("root".into()))]);
            let locked = self.lock.nodes[&node]
                .locked
                .as_ref()
                .unwrap_or(&root_metadata);
            let input = path.join("/");
            rows.push(Row {
                input: input.clone(),
                node: node.clone(),
                r#type: string(locked, "type", "root").into(),
                source: source(locked),
                revision: locked.get("rev").and_then(Value::as_str).map(String::from),
                last_modified: modified(locked)?,
                follows: match reference {
                    Reference::Follows(path) => Some(path.join("/")),
                    _ => None,
                },
                shared_with: expanded.get(&node).cloned(),
                cycle: ancestors.contains(&node),
            });
            output_budget += serde_json::to_vec(rows.last().unwrap())?.len();
            if output_budget > MAX_BYTES as usize * 2 {
                return Err("lock report exceeds its output limit".into());
            }
            edges
                .entry(parent.clone())
                .or_default()
                .insert(node.clone());
            row_parents.push(parent);
            if !all || expanded.contains_key(&node) {
                continue;
            }
            expanded.insert(node.clone(), input);
            ancestors.insert(node.clone());
            for key in self.lock.nodes[&node].inputs.keys() {
                queued_budget += path.iter().map(String::len).sum::<usize>() + key.len();
                if queued_budget > MAX_BYTES as usize {
                    return Err("lock graph exceeds its queued path limit".into());
                }
                let mut child = path.clone();
                child.push(key.clone());
                queue.push_back((child, ancestors.clone()));
            }
        }
        let mut colors = HashMap::new();
        let mut cycle_edges = HashSet::new();
        for start in edges.keys() {
            if colors.contains_key(start) {
                continue;
            }
            colors.insert(start.clone(), 1);
            let children = |node: &String| {
                edges
                    .get(node)
                    .map(|set| set.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
                    .into_iter()
            };
            let mut stack = vec![(start.clone(), children(start))];
            while let Some((parent, children_iter)) = stack.last_mut() {
                let Some(child) = children_iter.next() else {
                    let (parent, _) = stack.pop().unwrap();
                    colors.insert(parent, 2);
                    continue;
                };
                if colors.get(&child) == Some(&1) {
                    cycle_edges.insert((parent.clone(), child));
                } else if !colors.contains_key(&child) {
                    colors.insert(child.clone(), 1);
                    stack.push((child.clone(), children(&child)));
                }
            }
        }
        for (row, parent) in rows.iter_mut().zip(row_parents) {
            row.cycle |= cycle_edges.contains(&(parent, row.node.clone()));
        }
        rows.sort_by(|a, b| a.input.cmp(&b.input));
        Ok(rows)
    }
}
fn render(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "No inputs.".into();
    }
    let mut lines =
        vec!["INPUT\tNODE\tTYPE\tSOURCE\tREVISION\tMODIFIED UTC\tFOLLOWS / SHARED".into()];
    for row in rows {
        let mut notes = Vec::new();
        if let Some(follows) = &row.follows {
            notes.push(format!(
                "follows {}",
                if follows.is_empty() {
                    "<root>"
                } else {
                    follows
                }
            ));
        }
        if let Some(shared) = &row.shared_with {
            notes.push(format!(
                "{} {shared}",
                if row.cycle { "cycle to" } else { "shared with" }
            ));
        }
        let rev: String = row
            .revision
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(12)
            .collect();
        lines.push(
            [
                row.input.as_str(),
                row.node.as_str(),
                row.r#type.as_str(),
                row.source.as_str(),
                if rev.is_empty() { "—" } else { &rev },
                row.last_modified.as_deref().unwrap_or("—"),
                &if notes.is_empty() {
                    "—".into()
                } else {
                    notes.join("; ")
                },
            ]
            .map(terminal)
            .join("\t"),
        );
    }
    lines.join("\n")
}
pub fn run(args: &[String]) -> Result {
    let mut lock = PathBuf::from("flake.lock");
    let mut all = false;
    let mut json = false;
    let mut selected = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("usage: seele-inputs [--lock-file PATH] [--all] [--json] [INPUT]\nInspect version-7 flake lock graphs without fetching or evaluating inputs.");
                return Ok(());
            }
            "--lock-file" => lock = iter.next().ok_or("--lock-file requires a path")?.into(),
            "--all" => all = true,
            "--json" => json = true,
            _ if arg.starts_with('-') || selected.is_some() => {
                return Err("invalid arguments; see --help".into())
            }
            _ => selected = Some(arg.as_str()),
        }
    }
    let bytes = seele_runtime::fs::read_bounded(&lock, MAX_BYTES as usize, false)?;
    let mut graph = Graph::new(serde_json::from_slice(&bytes)?)?;
    let rows = graph.report(all, selected)?;
    println!(
        "{}",
        if json {
            serde_json::to_string_pretty(&rows)?
        } else {
            render(&rows)
        }
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_urls_remove_credentials_without_losing_explicit_ports() {
        for (value, expected) in [
            (
                "https://user:secret@host.example:443/path?token=secret#secret",
                "https://host.example:443/path",
            ),
            (
                "git+ssh://user:secret@[::1]:22/repo?secret#secret",
                "git+ssh://[::1]:22/repo",
            ),
            (
                "git@host.example:org/repo?secret#secret",
                "host.example:org/repo",
            ),
            (
                "user:secret@host.example:org/repo?secret",
                "host.example:org/repo",
            ),
            ("https://user:secret@[invalid/path", "[URL omitted]"),
        ] {
            assert_eq!(safe_url(value), expected);
        }
    }
}
