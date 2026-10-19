//! A narrow read-only backup boundary: exact home paths, tagged snapshots, copies only.
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
pub const FILE_LIMIT: usize = 16 * 1024 * 1024;
pub type Result<T> = io::Result<T>;
pub fn valid_path(path: &Path, root: &Path) -> bool {
    let text = path.to_str().unwrap_or("");
    path.is_absolute()
        && path.starts_with(root)
        && path != root
        && text.len() <= 4096
        && !text.chars().any(|c| c.is_control())
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        && !text.contains("//")
        && !text.ends_with('/')
        && !text.split('/').any(|s| matches!(s, "." | ".."))
}
fn id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn pattern(path: &str) -> String {
    path.chars()
        .flat_map(|c| {
            if "*?[]\\".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}
fn credential(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let uid = unsafe { libc::geteuid() };
    if !path.is_absolute()
        || path.starts_with("/nix/store")
        || !meta.is_file()
        || meta.uid() != uid
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
        || meta.len() == 0
        || meta.len() > 64 * 1024
        || fs::canonicalize(path)? != path
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    for parent in path.ancestors().skip(1) {
        let m = fs::symlink_metadata(parent)?;
        let sticky_tmp = (parent == Path::new("/tmp") || parent == Path::new("/var/tmp"))
            && m.uid() == 0
            && m.mode() & 0o1000 != 0;
        if !m.is_dir() || ![0, uid].contains(&m.uid()) || (m.mode() & 0o022 != 0 && !sticky_tmp) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    Ok(())
}
pub struct Backend {
    pub restic: PathBuf,
    pub repository: PathBuf,
    pub password: PathBuf,
    pub environment: Option<PathBuf>,
    pub home: PathBuf,
    pub host: String,
}
impl Backend {
    pub fn from_env() -> Result<Self> {
        let get = |name| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .ok_or(io::ErrorKind::NotFound)
        };
        let cfg = Self {
            restic: get("SEELE_RESTIC_BIN")?,
            repository: get("SEELE_BACKUP_REPOSITORY_FILE")?,
            password: get("SEELE_BACKUP_PASSWORD_FILE")?,
            environment: std::env::var_os("SEELE_BACKUP_BACKEND_FILE")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            home: get("SEELE_BACKUP_ALLOWED_HOME")?,
            host: std::env::var("SEELE_BACKUP_HOST").map_err(|_| io::ErrorKind::NotFound)?,
        };
        if !cfg.restic.is_absolute()
            || !cfg.home.is_absolute()
            || cfg.home == Path::new("/")
            || cfg.host.is_empty()
            || cfg.host.len() > 128
            || !cfg
                .host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        credential(&cfg.repository)?;
        credential(&cfg.password)?;
        if let Some(path) = &cfg.environment {
            credential(path)?
        }
        Ok(cfg)
    }
    fn run(&self, args: &[&str], limit: usize, cancel: &AtomicUsize) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.restic);
        cmd.env_clear()
            .current_dir("/")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env(
                "PATH",
                std::env::var_os("SEELE_RESTIC_PATH").unwrap_or_default(),
            )
            .args(["--no-cache", "--no-lock", "--repository-file"])
            .arg(&self.repository)
            .arg("--password-file")
            .arg(&self.password);
        if let Some(path) = &self.environment {
            let bytes = seele_runtime::fs::read_private(path, 64 * 1024)?;
            let text = std::str::from_utf8(&bytes).map_err(|_| io::ErrorKind::InvalidData)?;
            for line in text
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
            {
                let (key, value) = line.split_once('=').ok_or(io::ErrorKind::InvalidData)?;
                let allowed = matches!(
                    key,
                    "AWS_ACCESS_KEY_ID"
                        | "AWS_SECRET_ACCESS_KEY"
                        | "AWS_SESSION_TOKEN"
                        | "AWS_DEFAULT_REGION"
                        | "B2_ACCOUNT_ID"
                        | "B2_ACCOUNT_KEY"
                        | "AZURE_ACCOUNT_NAME"
                        | "AZURE_ACCOUNT_KEY"
                        | "GOOGLE_APPLICATION_CREDENTIALS"
                        | "RESTIC_REST_USERNAME"
                        | "RESTIC_REST_PASSWORD"
                );
                if !allowed
                    || value.chars().any(|c| c.is_control())
                    || value.starts_with(['\'', '"'])
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                cmd.env(key, value);
            }
        }
        let out = seele_runtime::process::capture(
            cmd.args(args),
            b"",
            seele_runtime::process::Limits {
                timeout: Duration::from_secs(120),
                output: limit,
            },
            cancel,
        )?;
        if !out.status.success() {
            return Err(io::Error::other("backup unavailable"));
        }
        Ok(out.stdout)
    }
    pub fn versions(&self, path: &Path, cancel: &AtomicUsize) -> Result<Vec<Value>> {
        if !valid_path(path, &self.home) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let raw = self.run(
            &[
                "find",
                "--json",
                "--tag",
                "seele",
                "--host",
                &self.host,
                "--",
                &pattern(path.to_str().unwrap()),
            ],
            FILE_LIMIT,
            cancel,
        )?;
        let value: Value = serde_json::from_slice(&raw)?;
        let entries = value
            .as_array()
            .filter(|r| r.len() <= 4096)
            .ok_or(io::ErrorKind::InvalidData)?;
        let mut rows = vec![];
        for entry in entries {
            let snapshot = entry["snapshot"]
                .as_str()
                .filter(|s| id(s))
                .ok_or(io::ErrorKind::InvalidData)?;
            let matches = entry["matches"]
                .as_array()
                .filter(|r| r.len() <= 4096)
                .ok_or(io::ErrorKind::InvalidData)?;
            for item in matches
                .iter()
                .filter(|r| r["path"].as_str() == path.to_str() && r["type"] == "file")
            {
                let size = item["size"].as_u64().unwrap_or(0);
                if size > FILE_LIMIT as u64 {
                    continue;
                }
                let modified = item["mtime"]
                    .as_str()
                    .filter(|s| {
                        s.len() <= 64
                            && s.bytes()
                                .all(|b| b.is_ascii_digit() || b"TZtz.+:-".contains(&b))
                    })
                    .unwrap_or("Unknown modification time");
                rows.push(json!({"snapshot":snapshot,"modified":modified,"size":size}));
                if rows.len() > 256 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
        }
        Ok(rows)
    }
    pub fn dump(&self, snapshot: &str, path: &Path, cancel: &AtomicUsize) -> Result<Vec<u8>> {
        if !id(snapshot) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let rows = self.versions(path, cancel)?;
        let selected = rows
            .iter()
            .find(|row| row["snapshot"] == snapshot)
            .ok_or(io::ErrorKind::NotFound)?;
        let bytes = self.run(
            &[
                "dump",
                snapshot,
                path.to_str().ok_or(io::ErrorKind::InvalidInput)?,
            ],
            FILE_LIMIT,
            cancel,
        )?;
        if bytes.len() as u64
            != selected["size"]
                .as_u64()
                .ok_or(io::ErrorKind::InvalidData)?
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(bytes)
    }
}
pub fn current(path: &Path) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.len() > FILE_LIMIT as u64
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut bytes = vec![];
    file.by_ref()
        .take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > FILE_LIMIT {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_exact() {
        let root = Path::new("/home/test");
        assert!(valid_path(Path::new("/home/test/notes/[draft]*.md"), root));
        for p in [
            "/home/test/../other/x",
            "/home/test2/x",
            "/etc/passwd",
            "/home/test",
            "/home/test//x",
            "/home/test/x/",
        ] {
            assert!(!valid_path(Path::new(p), root), "{p}");
        }
        assert_eq!(
            pattern("/home/test/a*[b]?\\"),
            "/home/test/a\\*\\[b\\]\\?\\\\"
        );
    }
}
