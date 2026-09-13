use crate::{capture, interactive, Result};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

pub fn flip(args: &[String], cancel: &AtomicUsize) -> Result<()> {
    if !args.is_empty() {
        eprintln!("Usage: jj flip");
        return Err(2);
    }
    let current = capture(
        Command::new("jj").args(["log", "--no-graph", "-r", "@", "-T", "change_id"]),
        cancel,
    )?;
    let previous = capture(
        Command::new("jj").args(["log", "--no-graph", "-r", "@-", "-T", "change_id"]),
        cancel,
    )?;
    for id in [&current, &previous] {
        if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(1);
        }
    }
    interactive(
        Command::new("jj").args(["parallelize", &current, &previous]),
        cancel,
    )?;
    interactive(
        Command::new("jj").args(["rebase", "--branch", &previous, "--destination", &current]),
        cancel,
    )
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        && !value.starts_with('.')
        && !value.starts_with('-')
}
pub fn branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.ends_with(".lock")
        && !value.contains("..")
        && !value.contains("@{")
        && !value.contains("//")
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
}
fn selector(value: &str) -> bool {
    if value.parse::<u64>().is_ok_and(|id| id > 0) {
        return true;
    }
    value
        .strip_prefix("https://github.com/")
        .is_some_and(|path| {
            let parts = path.split('/').collect::<Vec<_>>();
            parts.len() == 4
                && identifier(parts[0])
                && identifier(parts[1])
                && parts[2] == "pull"
                && parts[3].parse::<u64>().is_ok_and(|id| id > 0)
        })
}
fn metadata(value: &Value) -> Result<(&str, &str, &str)> {
    let branch_name = value["headRefName"]
        .as_str()
        .filter(|v| branch(v))
        .ok_or(1)?;
    let owner = value["headRepositoryOwner"]["login"]
        .as_str()
        .filter(|v| identifier(v))
        .ok_or(1)?;
    let repository = value["headRepository"]["name"]
        .as_str()
        .filter(|v| identifier(v))
        .ok_or(1)?;
    Ok((branch_name, owner, repository))
}
struct Remote {
    name: String,
    active: bool,
}
impl Drop for Remote {
    fn drop(&mut self) {
        if self.active {
            let _ = seele_runtime::process::capture(
                Command::new("jj").args(["git", "remote", "remove", &self.name]),
                b"",
                seele_runtime::process::Limits {
                    timeout: Duration::from_secs(10),
                    output: 16 * 1024,
                },
                &AtomicUsize::new(0),
            );
        }
    }
}
fn choose(cancel: &AtomicUsize) -> Result<Option<String>> {
    let values: Value = serde_json::from_str(&capture(
        Command::new("gh").args(["pr", "list", "--json", "number,title"]),
        cancel,
    )?)
    .map_err(|_| 1)?;
    let rows = values.as_array().ok_or(1)?;
    if rows.is_empty() {
        println!("No PRs found");
        return Ok(None);
    }
    let mut choices = vec![];
    for row in rows.iter().take(512) {
        let number = row["number"].as_u64().filter(|n| *n > 0).ok_or(1)?;
        let title = row["title"]
            .as_str()
            .ok_or(1)?
            .chars()
            .filter(|c| !c.is_control() && seele_runtime::redact::visible(*c))
            .take(500)
            .collect::<String>();
        choices.push(format!("#{number} | {title}"));
    }
    let input = choices.join("\n") + "\n";
    let selected = seele_runtime::process::capture_interactive(
        Command::new("gum").args(["choose", "--header", "Pick a PR:"]),
        input.as_bytes(),
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(900),
            output: 1024 * 1024,
        },
        cancel,
    )
    .map_err(|_| 1)?;
    if !selected.status.success() {
        return Ok(None);
    }
    let selected = std::str::from_utf8(&selected.stdout).map_err(|_| 1)?.trim();
    if selected.is_empty() {
        return Ok(None);
    }
    if !choices.iter().any(|choice| choice == selected) {
        return Err(1);
    }
    Ok(Some(
        selected
            .split_once(' ')
            .ok_or(1)?
            .0
            .trim_start_matches('#')
            .into(),
    ))
}
pub fn pr(args: &[String], cancel: &AtomicUsize) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("submit") if args.len() <= 2 => {
            let rev = args.get(1).map_or("@", String::as_str);
            let bookmarks = capture(
                Command::new("jj").args([
                    "log",
                    "--revisions",
                    rev,
                    "--no-graph",
                    "--no-pager",
                    "--template",
                    r#"self.local_bookmarks().map(|b| b.name()).join("\n")"#,
                ]),
                cancel,
            )?;
            let bookmark = bookmarks.trim_end_matches('\n');
            if !branch(bookmark) {
                eprintln!("Choose a revision with exactly one local bookmark.");
                return Err(1);
            }
            interactive(
                Command::new("gh").args(["pr", "create", "--head", bookmark]),
                cancel,
            )
        }
        Some("checkout" | "co") if args.len() <= 2 => {
            let id = if let Some(value) = args.get(1) {
                value.clone()
            } else {
                let Some(value) = choose(cancel)? else {
                    return Ok(());
                };
                value
            };
            if !selector(&id) {
                eprintln!("Choose a PR number or a GitHub pull request URL.");
                return Err(2);
            }
            let details: Value = serde_json::from_str(&capture(
                Command::new("gh").args([
                    "pr",
                    "view",
                    &id,
                    "--json",
                    "headRefName,headRepository,headRepositoryOwner",
                ]),
                cancel,
            )?)
            .map_err(|_| 1)?;
            let (branch, owner, repository) = metadata(&details)?;
            let temporary = tempfile::tempdir().map_err(|_| 1)?;
            let name = format!(
                "seele-pr-{}",
                temporary.path().file_name().unwrap().to_string_lossy()
            );
            let mut remote = Remote {
                name,
                active: false,
            };
            interactive(
                Command::new("jj").args([
                    "git",
                    "remote",
                    "add",
                    &remote.name,
                    &format!("https://github.com/{owner}/{repository}.git"),
                ]),
                cancel,
            )?;
            remote.active = true;
            interactive(
                Command::new("jj").args([
                    "git",
                    "fetch",
                    "--remote",
                    &remote.name,
                    "--branch",
                    &format!("exact:{}", json!(branch)),
                ]),
                cancel,
            )?;
            interactive(
                Command::new("jj")
                    .args(["new", &format!("{}@{}", json!(branch), json!(remote.name))]),
                cancel,
            )
        }
        _ => {
            eprintln!("Usage: jj pr submit [revision] | checkout [number-or-url]");
            Err(2)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_git_metadata_cannot_add_remote_schemes_or_revset_operators() {
        assert!(metadata(&json!({"headRefName":"feature/topic","headRepositoryOwner":{"login":"owner"},"headRepository":{"name":"repo"}})).is_ok());
        assert!(!branch("x\n--config"));
        assert!(!branch("x..y"));
        assert!(!selector("--web"));
        assert!(!selector("https://evil.test/owner/repo/pull/2"));
        assert!(selector("https://github.com/owner/repo/pull/2"));
        assert!(metadata(&json!({"headRefName":"topic","headRepositoryOwner":{"login":"owner@evil"},"headRepository":{"name":"repo"}})).is_err());
    }
}
