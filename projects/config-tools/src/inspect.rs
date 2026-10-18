//! Read-only inspection of a generated, explicitly whitelisted Nix projection.
use crate::{text::terminal, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

const MAX_BYTES: usize = 16 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: u32,
    #[serde(rename = "sourceRoot")]
    source_root: PathBuf,
    rows: Vec<Row>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Row {
    kind: String,
    key: String,
    value: String,
    sources: Vec<String>,
}
fn read(path: &Path) -> Result<(PathBuf, Vec<Row>)> {
    let bytes = seele_runtime::fs::read_bounded(path, MAX_BYTES, false)?;
    let catalog: Catalog = serde_json::from_slice(&bytes)?;
    if catalog.version != 1 || !catalog.source_root.is_absolute() || catalog.rows.len() > 10000 {
        return Err("unsupported or oversized catalog".into());
    }
    let mut merged: BTreeMap<String, Row> = BTreeMap::new();
    for mut row in catalog.rows {
        if !["setting", "package", "shortcut"].contains(&row.kind.as_str())
            || row.key.is_empty()
            || row.key.len() > 512
            || row.value.len() > 4096
            || row.sources.len() > 256
            || row
                .sources
                .iter()
                .any(|source| !Path::new(source).is_absolute() || source.len() > 4096)
            || row.key.chars().any(char::is_control)
        {
            return Err("invalid catalog row".into());
        }
        row.sources.sort();
        row.sources.dedup();
        if let Some(existing) = merged.get_mut(&row.key) {
            if existing.kind != row.kind
                || (existing.kind != "package" && existing.value != row.value)
            {
                return Err("conflicting catalog identities".into());
            }
            existing.sources.extend(row.sources);
            existing.sources.sort();
            existing.sources.dedup();
        } else {
            merged.insert(row.key.clone(), row);
        }
    }
    Ok((catalog.source_root, merged.into_values().collect()))
}
fn matches(row: &Row, query: &str) -> bool {
    let text = format!("{} {} {}", row.key, row.kind, row.value).to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}
fn source(row: &Row, index: usize, repo: Option<&Path>, source_root: &Path) -> Result<PathBuf> {
    let source = PathBuf::from(
        row.sources
            .get(index.checked_sub(1).ok_or("source index starts at 1")?)
            .ok_or("source index is out of range")?,
    );
    if let Some(repo) = repo {
        // A source path maps only to this flake's modules in an explicitly chosen
        // checkout, never to an arbitrary upstream path or a traversal suffix.
        if let Ok(relative) = source.strip_prefix(source_root) {
            if relative.starts_with("modules")
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
            {
                let root = repo.canonicalize()?;
                let candidate = root.join(relative).canonicalize()?;
                if candidate.starts_with(&root) && candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
        return Err("source does not map to the chosen checkout".into());
    }
    if !source.is_file() {
        return Err("source is unavailable".into());
    }
    Ok(source)
}
pub fn run(arguments: &[String]) -> Result {
    let mut query = Vec::new();
    let mut json = false;
    let mut open = None;
    let mut index = 1;
    let mut source_given = false;
    let mut repo = None;
    let mut args = arguments.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--open" => open = Some(args.next().ok_or("--open needs an exact key")?.clone()),
            "--source" => {
                source_given = true;
                index = args
                    .next()
                    .ok_or("--source needs a positive index")?
                    .parse::<usize>()?
            }
            "--repo" => repo = Some(PathBuf::from(args.next().ok_or("--repo needs a checkout")?)),
            "--help" => {
                println!("seele-inspect [QUERY...] [--json] | --open KEY [--source N] [--repo CHECKOUT]\nDeclared settings, direct packages and supported shortcuts; never live state.");
                return Ok(());
            }
            value if value.starts_with('-') => return Err("unknown inspector option".into()),
            _ => query.push(arg.clone()),
        }
    }
    let path = env::var_os("SEELE_INSPECT_CATALOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config")
                })
                .join("seele-inspect/catalog.json")
        });
    let (source_root, rows) = read(&path)?;
    if let Some(key) = open {
        if json || !query.is_empty() {
            return Err("--open cannot be combined with search or --json".into());
        }
        let row = rows
            .iter()
            .find(|row| row.key == key)
            .ok_or("no exact catalog key matches")?;
        let path = source(row, index, repo.as_deref(), &source_root)?;
        let editor = env::var_os("SEELE_INSPECT_EDITOR").unwrap_or_else(|| "nvim".into());
        return Err(Command::new(editor).arg("--").arg(path).exec().into());
    }
    if repo.is_some() || source_given {
        return Err("--repo and --source require --open".into());
    }
    let selected: Vec<_> = rows
        .iter()
        .filter(|row| matches(row, &query.join(" ")))
        .collect();
    if json {
        println!("{}", serde_json::to_string(&selected)?);
    } else {
        println!(
            "Declared configuration · {} matches · rebuild to publish changes",
            selected.len()
        );
        for row in selected {
            println!(
                "\n{} [{}]\n  {}",
                terminal(&row.key),
                terminal(&row.kind),
                terminal(&row.value)
            );
            for (index, file) in row.sources.iter().enumerate() {
                println!("  {}. {}", index + 1, terminal(file));
            }
        }
        println!("\nOpen: seele-inspect --open KEY --source N [--repo CHECKOUT]");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn row() -> Row {
        Row {
            kind: "setting".into(),
            key: "home.programs.fish.enable".into(),
            value: "enabled".into(),
            sources: vec!["/nix/store/source/modules/features/fish.nix".into()],
        }
    }
    #[test]
    fn search_is_case_insensitive_and_requires_every_word() {
        assert!(matches(&row(), "FISH enabled"));
        assert!(!matches(&row(), "fish disabled"));
    }
    #[test]
    fn catalog_merges_sources_but_rejects_conflicting_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.json");
        let first = row();
        let mut second = first.clone();
        second.sources = vec!["/source/other.nix".into()];
        fs::write(&path,serde_json::to_vec(&serde_json::json!({"version":1,"sourceRoot":"/nix/store/source","rows":[first,second]})).unwrap()).unwrap();
        assert_eq!(read(&path).unwrap().1[0].sources.len(), 2);
        fs::write(&path,br#"{"version":1,"rows":[{"kind":"secret","key":"token","value":"secret","sources":[]}] }"#).unwrap();
        assert!(read(&path).is_err());
    }
    #[test]
    fn checkout_opening_cannot_escape_through_suffix_or_symlink() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("modules/features")).unwrap();
        fs::write(
            dir.path()
                .canonicalize()
                .unwrap()
                .join("modules/features/fish.nix"),
            "{}",
        )
        .unwrap();
        assert!(
            source(&row(), 1, Some(dir.path()), Path::new("/nix/store/source"))
                .unwrap()
                .starts_with(dir.path().canonicalize().unwrap())
        );
        let mut bad = row();
        bad.sources = vec!["/source/modules/../../outside.nix".into()];
        assert!(source(&bad, 1, Some(dir.path()), Path::new("/source")).is_err());
        assert!(source(&row(), 0, Some(dir.path()), Path::new("/nix/store/source")).is_err());
        std::os::unix::fs::symlink(
            "/etc/passwd",
            dir.path().join("modules/features/fish-link.nix"),
        )
        .unwrap();
        bad.sources = vec!["/source/modules/features/fish-link.nix".into()];
        assert!(source(&bad, 1, Some(dir.path()), Path::new("/source")).is_err());
    }
}
