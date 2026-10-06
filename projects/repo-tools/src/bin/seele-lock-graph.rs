//! Read-only lock topology. Never evaluates Nix or projects source URLs/keys.
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || b"_-+.".contains(&ch))
}
fn resolve(
    nodes: &Map<String, Value>,
    root: &str,
    value: &Value,
    visiting: &mut HashSet<(String, String)>,
    depth: usize,
) -> Result<String, &'static str> {
    if depth > 128 {
        return Err("Follows resolution exceeds 128 levels.");
    }
    if let Some(target) = value.as_str() {
        if !name(target) || !nodes.contains_key(target) {
            return Err("Input references a missing or invalid node.");
        }
        return Ok(target.to_owned());
    }
    let path = value
        .as_array()
        .ok_or("Input reference must be a node name or follows path.")?;
    if path.len() > 128 {
        return Err("Follows path exceeds 128 steps.");
    }
    let mut node = root.to_owned();
    for step in path {
        let key = step
            .as_str()
            .filter(|key| name(key))
            .ok_or("Invalid follows path name.")?;
        let marker = (node.clone(), key.to_owned());
        if !visiting.insert(marker.clone()) {
            return Err("Cyclic follows path.");
        }
        let reference = nodes[&node]
            .get("inputs")
            .and_then(|v| v.get(key))
            .ok_or("Follows path references a missing input.")?;
        let next = resolve(nodes, root, reference, visiting, depth + 1)?;
        visiting.remove(&marker);
        node = next;
    }
    Ok(node)
}
fn cycle(
    node: &str,
    edges: &HashMap<String, Vec<String>>,
    active: &mut HashSet<String>,
    done: &mut HashSet<String>,
    depth: usize,
) -> Result<(), &'static str> {
    if depth > 128 {
        return Err("Graph exceeds 128 levels.");
    }
    if done.contains(node) {
        return Ok(());
    }
    if !active.insert(node.to_owned()) {
        return Err("Lock input graph contains a cycle.");
    }
    for target in edges.get(node).into_iter().flatten() {
        cycle(target, edges, active, done, depth + 1)?;
    }
    active.remove(node);
    done.insert(node.to_owned());
    Ok(())
}
fn graph(lock: &Value) -> Result<Value, &'static str> {
    if lock.get("version").and_then(Value::as_u64) != Some(7) {
        return Err("Only flake.lock format version 7 is supported.");
    }
    let nodes = lock
        .get("nodes")
        .and_then(Value::as_object)
        .ok_or("Lock nodes are missing.")?;
    if nodes.len() > 1024 || nodes.is_empty() {
        return Err("Lock must contain 1–1,024 nodes.");
    }
    let root = lock
        .get("root")
        .and_then(Value::as_str)
        .filter(|name| nodes.contains_key(*name))
        .ok_or("Lock root is missing.")?;
    for (key, node) in nodes {
        if !name(key) || !node.is_object() {
            return Err("Invalid lock node.");
        }
        if let Some(inputs) = node.get("inputs") {
            let inputs = inputs.as_object().ok_or("Node inputs must be an object.")?;
            if inputs.keys().any(|key| !name(key)) {
                return Err("Invalid input name.");
            }
        }
    }
    let mut projected = Vec::new();
    let mut adjacency = HashMap::new();
    for (source, node) in nodes {
        if let Some(inputs) = node.get("inputs").and_then(Value::as_object) {
            for (input, reference) in inputs {
                if projected.len() >= 8192 {
                    return Err("Lock exceeds 8,192 input edges.");
                }
                let target = resolve(nodes, root, reference, &mut HashSet::new(), 0)?;
                adjacency
                    .entry(source.clone())
                    .or_insert_with(Vec::new)
                    .push(target.clone());
                projected.push(json!({"source":source,"input":input,"target":target,"follows":reference.is_array()}));
            }
        }
    }
    let mut done = HashSet::new();
    for node in nodes.keys() {
        cycle(node, &adjacency, &mut HashSet::new(), &mut done, 0)?;
    }
    Ok(json!({"root":root,"nodes":nodes.keys().collect::<Vec<_>>(),"edges":projected}))
}
fn run() -> Result<(), &'static str> {
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let as_json = args.first().is_some_and(|arg| arg == "--json");
    if as_json {
        args.remove(0);
    }
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
    }
    if args.len() > 1 {
        return Err("Usage: seele-lock-graph [--json] [--] [flake.lock]");
    }
    let path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("flake.lock"));
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Cannot open lock file (symlinks refused).")?;
    let metadata = file.metadata().map_err(|_| "Cannot inspect lock file.")?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err("Choose a regular lock file up to 8 MiB.");
    }
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read lock file.")?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Lock grew beyond 8 MiB.");
    }
    let lock: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid lock JSON.")?;
    let projection = graph(&lock)?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&projection).map_err(|_| "Cannot format graph.")?
        );
    } else {
        println!(
            "Root: {} · {} nodes",
            projection["root"].as_str().unwrap(),
            projection["nodes"].as_array().unwrap().len()
        );
        for edge in projection["edges"].as_array().unwrap() {
            println!(
                "{}.{} → {}{}",
                edge["source"].as_str().unwrap(),
                edge["input"].as_str().unwrap(),
                edge["target"].as_str().unwrap(),
                if edge["follows"] == true {
                    " (follows)"
                } else {
                    ""
                }
            );
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_and_projection_do_not_expose_sources() {
        let lock = json!({"version":7,"root":"root","nodes":{"root":{"inputs":{"pkg":"pkg","alias":["pkg","dep"]}},"pkg":{"inputs":{"dep":"lib"}},"lib":{"locked":{"url":"https://secret@example.invalid"}}}});
        let result = graph(&lock).unwrap();
        assert!(!result.to_string().contains("secret"));
        assert_eq!(result["edges"][0]["source"], "pkg");
        assert!(result["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["input"] == "alias"
                && edge["target"] == "lib"
                && edge["follows"] == true));
    }
    #[test]
    fn malformed_missing_and_cycles_fail_without_partial_graph() {
        for lock in [
            json!({"version":8,"nodes":{}}),
            json!({"version":7,"root":"root","nodes":{"root":{"inputs":{"bad":"absent"}}}}),
            json!({"version":7,"root":"root","nodes":{"root":{"inputs":{"a":["b"],"b":["a"]}}}}),
            json!({"version":7,"root":"root","nodes":{"root":{"inputs":{"self":"root"}}}}),
        ] {
            assert!(graph(&lock).is_err());
        }
    }
}
