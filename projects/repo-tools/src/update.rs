use crate::{capture, interactive, Result};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;

fn git() -> Command {
    let mut command = Command::new("git");
    command
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env_remove("GIT_GLOB_PATHSPECS")
        .env_remove("GIT_NOGLOB_PATHSPECS")
        .env_remove("GIT_ICASE_PATHSPECS");
    command
}
fn revision_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn repository_root(cancel: &AtomicUsize) -> Result<PathBuf> {
    Ok(PathBuf::from(
        capture(git().args(["rev-parse", "--show-toplevel"]), cancel)?.trim_end(),
    ))
}
pub fn submodule(args: &[String], cancel: &AtomicUsize) -> Result<()> {
    let mut pr = false;
    let mut keep_lock = false;
    let mut name = None;
    for arg in args {
        match arg.as_str() {
            "--pr" if !pr => pr = true,
            "--keep-lock" if !keep_lock => keep_lock = true,
            value if !value.starts_with('-') && name.is_none() => name = Some(value),
            _ => {
                eprintln!("usage: update-submodule [--pr [--keep-lock]] [submodule]");
                return Err(2);
            }
        }
    }
    if keep_lock && !pr {
        eprintln!(
            "error: --keep-lock requires --pr and independently reviewed unchanged flake inputs"
        );
        return Err(2);
    }
    let name = name.unwrap_or("seele-shell");
    let relative = Path::new(name);
    if relative.as_os_str().is_empty()
        || name.starts_with('-')
        || name
            .chars()
            .any(|c| c.is_control() || !seele_runtime::redact::visible(c))
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        eprintln!("Choose a submodule path inside the repository.");
        return Err(2);
    }
    let superproject = capture(
        git().args(["rev-parse", "--show-superproject-working-tree"]),
        cancel,
    )
    .unwrap_or_default();
    let root = if superproject.trim().is_empty() {
        repository_root(cancel)?
    } else {
        PathBuf::from(superproject.trim_end())
    };
    let root = root.canonicalize().map_err(|_| 1)?;
    let path = root.join(relative);
    if !path.canonicalize().map_err(|_| 1)?.starts_with(&root) {
        eprintln!("Choose a submodule path inside the repository.");
        return Err(2);
    }
    if capture(
        git()
            .arg("-C")
            .arg(&path)
            .args(["rev-parse", "--is-inside-work-tree"]),
        cancel,
    )?
    .trim()
        != "true"
    {
        return Err(1);
    }
    if !capture(
        git()
            .arg("-C")
            .arg(&path)
            .args(["status", "--porcelain=v1"]),
        cancel,
    )?
    .is_empty()
    {
        eprintln!("error: commit the clean {name} submodule before updating its parent pointer");
        return Err(1);
    }
    let tree = capture(
        git()
            .arg("-C")
            .arg(&root)
            .args(["ls-tree", "HEAD", "--", name]),
        cancel,
    )?;
    let fields = tree.split_whitespace().collect::<Vec<_>>();
    if fields.first() != Some(&"160000") || fields.len() < 4 || !revision_id(fields[2]) {
        eprintln!("error: {name} is not tracked as a submodule in the parent");
        return Err(1);
    }
    let old = fields[2];
    let new = capture(
        git().arg("-C").arg(&path).args(["rev-parse", "HEAD"]),
        cancel,
    )?;
    let new = new.trim();
    if !revision_id(new) {
        return Err(1);
    }
    if pr
        && capture(
            git()
                .arg("-C")
                .arg(&root)
                .args(["rev-parse", "--abbrev-ref", "HEAD"]),
            cancel,
        )?
        .trim()
            != "HEAD"
    {
        eprintln!("error: PR mode requires Jujutsu's detached Git HEAD so no Git branch advances");
        return Err(1);
    }
    if keep_lock {
        let lock_blob = |repository: &Path, revision: &str| -> Result<String> {
            let tree = capture(
                git()
                    .arg("-C")
                    .arg(repository)
                    .args(["ls-tree", revision, "--", "flake.lock"]),
                cancel,
            )?;
            let fields = tree.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 4
                || !matches!(fields[0], "100644" | "100755")
                || fields[1] != "blob"
                || !revision_id(fields[2])
            {
                eprintln!("error: --keep-lock requires regular tracked flake.lock files");
                return Err(1);
            }
            Ok(fields[2].to_owned())
        };
        lock_blob(&root, "HEAD")?;
        if lock_blob(&path, old)? != lock_blob(&path, new)? {
            eprintln!("error: submodule flake.lock changed; refresh transitive locks with Nix");
            return Err(1);
        }
        // Check index and working tree separately: one can differ from HEAD
        // while the other was restored to the original bytes.
        for args in [
            vec!["diff", "--quiet", "--", "flake.lock"],
            vec!["diff", "--cached", "--quiet", "--", "flake.lock"],
        ] {
            if capture(git().arg("-C").arg(&root).args(args), cancel).is_err() {
                eprintln!("error: --keep-lock requires an unchanged parent flake.lock");
                return Err(1);
            }
        }
    }
    interactive(
        Command::new("jj")
            .arg("-R")
            .arg(&path)
            .args(["git", "fetch", "--remote", "origin"]),
        cancel,
    )?;
    let published = capture(
        Command::new("jj").arg("-R").arg(&path).args([
            "log",
            "-r",
            &format!("remote_bookmarks(remote=origin) & descendants({new})"),
            "--no-graph",
            "-T",
            "commit_id",
        ]),
        cancel,
    )?;
    if published.is_empty() {
        eprintln!("error: push {name} revision {new} before updating the parent");
        return Err(1);
    }
    if old != new {
        // Jujutsu does not yet snapshot submodule gitlinks. This narrowly
        // scoped compatibility transaction preserves unrelated staged paths.
        interactive(git().arg("-C").arg(&root).args(["add", "--", name]), cancel)?;
        interactive(
            git().arg("-C").arg(&root).args([
                "commit",
                "--only",
                "-m",
                &format!("Update {name} submodule"),
                "--",
                name,
            ]),
            cancel,
        )?;
        interactive(
            Command::new("jj")
                .arg("-R")
                .arg(&root)
                .args(["git", "import"]),
            cancel,
        )?;
        if pr {
            println!(
                "PR mode left all bookmarks unchanged; select the parent PR bookmark with Jujutsu"
            );
        } else if capture(
            Command::new("jj").arg("-R").arg(&root).args([
                "log",
                "-r",
                "main",
                "--no-graph",
                "-T",
                "commit_id",
            ]),
            cancel,
        )
        .is_ok_and(|id| !id.is_empty())
        {
            interactive(
                Command::new("jj")
                    .arg("-R")
                    .arg(&root)
                    .args(["bookmark", "set", "main", "-r", "@-"]),
                cancel,
            )?;
            println!("advanced bookmark main to the new parent commit");
        } else {
            eprintln!("no main bookmark exists; advance the active bookmark manually");
        }
    }
    if keep_lock {
        println!("Kept identical submodule and unchanged parent lock files. No Nix evaluation or lock refresh was performed; unchanged flake inputs require independent review.");
    } else {
        interactive(
            Command::new("nix")
                .current_dir(&root)
                .args(["flake", "update", name]),
            cancel,
        )?;
    }
    println!("{name} now points to {new}");
    if pr {
        println!("Review the gitlink commit and any flake.lock changes, then set and push only the parent PR bookmark.");
    } else {
        println!("commit the refreshed flake.lock if it changed, then push the parent with: jj git push --bookmark main");
    }
    Ok(())
}
fn version(tag: &str) -> Option<&str> {
    let value = tag.strip_prefix('v').unwrap_or(tag);
    (!value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
        && value.as_bytes()[0].is_ascii_alphanumeric())
    .then_some(value)
}
fn releases(repository: &str, cancel: &AtomicUsize) -> Result<Value> {
    let data = capture(
        Command::new("curl").args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            &format!("https://api.github.com/repos/{repository}/releases?per_page=20"),
        ]),
        cancel,
    )?;
    serde_json::from_str(&data).map_err(|_| 1)
}
fn release(values: &Value, nightly: bool) -> Result<&Value> {
    values
        .as_array()
        .ok_or(1)?
        .iter()
        .find(|entry| {
            if nightly {
                entry["prerelease"] == true
                    && entry["tag_name"]
                        .as_str()
                        .is_some_and(|tag| tag.contains("-nightly."))
            } else {
                entry["draft"] == false && entry["prerelease"] == false
            }
        })
        .ok_or(1)
}
fn asset<'a>(release: &'a Value, name: &str) -> Result<&'a Value> {
    release["assets"]
        .as_array()
        .ok_or(1)?
        .iter()
        .find(|asset| asset["name"] == name)
        .ok_or(1)
}
fn sri(value: &str) -> bool {
    value.strip_prefix("sha256-").is_some_and(|base64| {
        base64.len() == 44
            && base64.ends_with('=')
            && base64[..43]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/".contains(&b))
    })
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn replace_pin(text: &str, key: &str, value: &str) -> Result<String> {
    let marker = format!("{key} = \"");
    let start = text.find(&marker).ok_or(1)? + marker.len();
    let end = text[start..].find('"').ok_or(1)? + start;
    if !text[end..].starts_with("\";") {
        return Err(1);
    }
    let mut updated = String::with_capacity(text.len() + value.len());
    updated.push_str(&text[..start]);
    updated.push_str(value);
    updated.push_str(&text[end..]);
    Ok(updated)
}
fn read_source(path: &Path) -> Result<(String, fs::Permissions)> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| 1)?;
    let metadata = file.metadata().map_err(|_| 1)?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() > 1024 * 1024
    {
        return Err(1);
    }
    let mut text = String::new();
    file.take(1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| 1)?;
    Ok((text, metadata.permissions()))
}
fn prepare_source(
    path: &Path,
    text: &str,
    permissions: fs::Permissions,
) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or(1)?).map_err(|_| 1)?;
    file.as_file().set_permissions(permissions).map_err(|_| 1)?;
    file.write_all(text.as_bytes()).map_err(|_| 1)?;
    file.as_file().sync_all().map_err(|_| 1)?;
    Ok(file)
}
pub fn packaged(args: &[String], cancel: &AtomicUsize) -> Result<()> {
    if !args.is_empty() {
        eprintln!("usage: update-packaged");
        return Err(2);
    }
    let root = repository_root(cancel)?;
    let codex_path = root.join("modules/packages/codexbar.nix");
    let t3_path = root.join("modules/packages/t3code.nix");
    let (codex_source, codex_permissions) = read_source(&codex_path)?;
    let (t3_source, t3_permissions) = read_source(&t3_path)?;
    let codex_releases = releases("steipete/CodexBar", cancel)?;
    let codex = release(&codex_releases, false)?;
    let codex_version = version(codex["tag_name"].as_str().ok_or(1)?).ok_or(1)?;
    let url = asset(
        codex,
        &format!("CodexBarCLI-v{codex_version}-linux-x86_64.tar.gz"),
    )?["browser_download_url"]
        .as_str()
        .filter(|url| {
            url.starts_with("https://github.com/steipete/CodexBar/releases/download/")
                && !url.chars().any(char::is_control)
        })
        .ok_or(1)?;
    let hash: Value = serde_json::from_str(&capture(
        Command::new("nix").args(["store", "prefetch-file", "--json", url]),
        cancel,
    )?)
    .map_err(|_| 1)?;
    let codex_hash = hash["hash"].as_str().filter(|value| sri(value)).ok_or(1)?;
    let t3_releases = releases("pingdotgg/t3code", cancel)?;
    let t3 = release(&t3_releases, true)?;
    let t3_version = version(t3["tag_name"].as_str().ok_or(1)?).ok_or(1)?;
    let digest = asset(t3, &format!("T3-Code-{t3_version}-x86_64.AppImage"))?["digest"]
        .as_str()
        .and_then(|value| value.strip_prefix("sha256:"))
        .filter(|value| hex_digest(value))
        .ok_or(1)?;
    let t3_hash = capture(
        Command::new("nix").args([
            "hash",
            "convert",
            "--hash-algo",
            "sha256",
            "--from",
            "base16",
            "--to",
            "sri",
            digest,
        ]),
        cancel,
    )?;
    let t3_hash = t3_hash.trim();
    if !sri(t3_hash) {
        return Err(1);
    }
    let codex_updated = replace_pin(
        &replace_pin(&codex_source, "version", codex_version)?,
        "hash",
        codex_hash,
    )?;
    let t3_updated = replace_pin(
        &replace_pin(&t3_source, "version", t3_version)?,
        "hash",
        t3_hash,
    )?;
    let codex_pending = prepare_source(&codex_path, &codex_updated, codex_permissions)?;
    let t3_pending = prepare_source(&t3_path, &t3_updated, t3_permissions)?;
    if read_source(&codex_path)?.0 != codex_source || read_source(&t3_path)?.0 != t3_source {
        eprintln!("Package files changed while releases were being checked; no pins were written.");
        return Err(1);
    }
    codex_pending.persist(&codex_path).map_err(|_| 1)?;
    t3_pending.persist(&t3_path).map_err(|_| 1)?;
    println!("Updated CodexBar to {codex_version}\nUpdated T3 Code nightly to {t3_version}");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_literal_pins_accept_release_metadata() {
        assert!(version("v1.2.3-nightly.4").is_some());
        assert!(version("v1\";malicious=\"yes").is_none());
        assert!(!hex_digest("not-a-hash"));
        assert_eq!(
            replace_pin("version = \"old\";\nhash = \"old\";", "version", "new").unwrap(),
            "version = \"new\";\nhash = \"old\";"
        );
    }
}
