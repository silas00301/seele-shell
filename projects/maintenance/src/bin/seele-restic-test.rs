//! Restore selected real snapshot files into a private, disposable tree. Only
//! a private receipt and success timestamp persist; bytes never reach logs.
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const FILE_LIMIT: usize = 16 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    restic: PathBuf,
    host: String,
    samples: Vec<PathBuf>,
    runtime: PathBuf,
    receipt: PathBuf,
}
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn main() {
    std::panic::set_hook(Box::new(|_| eprintln!("backup restore check failed")));
    if run().is_err() {
        eprintln!("backup restore check failed");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let stop = seele_runtime::process::termination_signal()?;
    if arguments
        .first()
        .is_some_and(|arg| arg == "--validate-credentials")
    {
        if arguments.len() != 1 {
            return Err("invalid operation".into());
        }
        validate_credentials()?;
        return Ok(());
    }
    let [configuration] = arguments.as_slice() else {
        return Err("configuration required".into());
    };
    let cfg: Config = serde_json::from_slice(&seele_runtime::fs::read_bounded(
        Path::new(configuration),
        64 * 1024,
        false,
    )?)?;
    if !cfg.restic.is_absolute()
        || !cfg.runtime.is_absolute()
        || !cfg.receipt.is_absolute()
        || cfg.host.is_empty()
        || cfg.host.len() > 128
        || !cfg
            .host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || cfg.samples.is_empty()
        || cfg.samples.len() > 16
        || cfg.samples.iter().any(|path| !valid_sample(path))
    {
        return Err("invalid restore configuration".into());
    }
    validate_credentials()?;
    let runtime = seele_runtime::fs::private_directory(&cfg.runtime)?;
    if runtime.metadata()?.mode() & 0o077 != 0 {
        return Err("private runtime required".into());
    }
    let temporary = tempfile::Builder::new()
        .prefix("restore-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(&cfg.runtime)?;
    let invoke = |args: &[String], limit: usize| -> Result<Vec<u8>> {
        let output = seele_runtime::process::capture(
            Command::new(&cfg.restic).args(args),
            b"",
            seele_runtime::process::Limits {
                timeout: Duration::from_secs(900),
                output: limit,
            },
            &stop,
        )?;
        if !output.status.success() {
            return Err("restic operation failed".into());
        }
        Ok(output.stdout)
    };
    let snapshots: serde_json::Value = serde_json::from_slice(&invoke(
        &[
            "snapshots".into(),
            "--json".into(),
            "--tag".into(),
            "seele".into(),
            "--host".into(),
            cfg.host.clone(),
            "--latest".into(),
            "1".into(),
        ],
        1024 * 1024,
    )?)?;
    let snapshot = snapshots
        .as_array()
        .ok_or("invalid snapshot list")?
        .iter()
        .filter(|row| {
            row["id"]
                .as_str()
                .is_some_and(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        .max_by_key(|row| row["time"].as_str().unwrap_or(""))
        .ok_or("no backup snapshot")?;
    let id = snapshot["id"].as_str().unwrap();
    let mut restored = Vec::new();
    for sample in &cfg.samples {
        let sample_text = sample.to_str().ok_or("invalid sample")?;
        let expected = invoke(&["dump".into(), id.into(), sample_text.into()], FILE_LIMIT)?;
        invoke(
            &[
                "restore".into(),
                id.into(),
                "--target".into(),
                temporary.path().to_string_lossy().into(),
                "--include".into(),
                sample_text.into(),
                "--verify".into(),
            ],
            1024 * 1024,
        )?;
        let path = restored_path(temporary.path(), sample)?;
        let actual = seele_runtime::fs::read_bounded(&path, FILE_LIMIT, false)?;
        if actual != expected {
            return Err("restored content mismatch".into());
        }
        restored.push(json!({"path":sample,"bytes":actual.len(),"sha256":format!("{:x}",Sha256::digest(&actual))}));
    }
    let receipt = json!({"version":1,"snapshot":id,"completed":seele_maintenance::model::now(),"restored":restored});
    seele_runtime::fs::atomic_write(&cfg.receipt, &serde_json::to_vec(&receipt)?)?;
    Ok(())
}
fn validate_credentials() -> Result<()> {
    // Inspect only the deliberately configured file references. Restic itself
    // reads their contents; nothing here copies or prints credential bytes.
    let mut keys = vec!["RESTIC_REPOSITORY_FILE", "RESTIC_PASSWORD_FILE"];
    if std::env::var_os("SEELE_BACKUP_ENVIRONMENT_FILE").is_some() {
        keys.push("SEELE_BACKUP_ENVIRONMENT_FILE");
    }
    for key in keys {
        let path = PathBuf::from(std::env::var_os(key).ok_or("credential reference required")?);
        if !path.is_absolute() || path.starts_with("/nix/store") {
            return Err("runtime credential reference required".into());
        }
        let parent = path
            .parent()
            .ok_or("invalid credential reference")?
            .canonicalize()?;
        let directory = fs::metadata(parent)?;
        if !directory.is_dir()
            || directory.uid() != unsafe { libc::geteuid() }
            || directory.mode() & 0o022 != 0
        {
            return Err("unsafe credential directory".into());
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
            || metadata.len() > 64 * 1024
        {
            return Err("unsafe credential reference".into());
        }
    }
    Ok(())
}
fn valid_sample(path: &Path) -> bool {
    path.is_absolute()
        && path.to_str().is_some_and(|s| {
            s.len() <= 4096 && !s.chars().any(char::is_control) && !s.contains(['*', '?', '[', ']'])
        })
        && path
            .components()
            .skip(1)
            .all(|part| matches!(part, Component::Normal(_)))
        && path.components().count() > 1
}
fn restored_path(root: &Path, sample: &Path) -> Result<PathBuf> {
    if !valid_sample(sample) {
        return Err("invalid sample path".into());
    }
    let mut path = root.to_owned();
    for part in sample.components().skip(1) {
        let Component::Normal(part) = part else {
            return Err("invalid sample path".into());
        };
        path.push(part);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err("restored symlink refused".into());
        }
    }
    if !fs::metadata(&path)?.is_file() {
        return Err("restored regular file required".into());
    }
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_and_restored_paths_refuse_patterns_and_links() {
        for sample in ["/", "relative", "/a/../b", "/a/*", "/a/file\nname"] {
            assert!(!valid_sample(Path::new(sample)));
        }
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("a")).unwrap();
        fs::write(root.path().join("a/file"), "example").unwrap();
        assert_eq!(
            restored_path(root.path(), Path::new("/a/file")).unwrap(),
            root.path().join("a/file")
        );
        std::os::unix::fs::symlink("/tmp", root.path().join("link")).unwrap();
        assert!(restored_path(root.path(), Path::new("/link/file")).is_err());
    }
}
