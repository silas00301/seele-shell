//! Theme-only edits to stopped Brave profiles, pinned to opened directories.
use serde_json::{Map, Value};
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
const MAX_PREFERENCES: u64 = 16 * 1024 * 1024;
fn pinned(directory: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}
fn running(root: &File) -> bool {
    fs::symlink_metadata(pinned(root).join("SingletonLock")).is_ok()
}
fn same(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
}
fn object(value: &mut Value) -> io::Result<&mut Map<String, Value>> {
    if value.is_null() {
        *value = Value::Object(Map::new());
    }
    value
        .as_object_mut()
        .ok_or_else(|| io::ErrorKind::InvalidData.into())
}
fn update_profile(root: &File, name: &std::ffi::OsStr) -> io::Result<()> {
    let name = CString::new(name.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: root and name stay live; no profile symlink is followed.
    let descriptor = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openat transfers one descriptor to this owner.
    let profile = unsafe { File::from_raw_fd(descriptor) };
    let metadata = profile.metadata()?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let path = pinned(&profile).join("Preferences");
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)?;
    let before = file.metadata()?;
    if !before.is_file()
        || before.uid() != unsafe { libc::geteuid() }
        || before.mode() & 0o022 != 0
        || before.nlink() != 1
        || before.len() > MAX_PREFERENCES
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_PREFERENCES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PREFERENCES {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|_| io::ErrorKind::InvalidData)?;
    let extensions = object(&mut value)?
        .entry("extensions")
        .or_insert(Value::Null);
    let theme = object(extensions)?.entry("theme").or_insert(Value::Null);
    let theme = object(theme)?;
    if theme.get("system_theme").and_then(Value::as_f64) == Some(2.0)
        && theme
            .get("id")
            .is_none_or(|value| value.is_null() || value.as_str() == Some(""))
        && theme
            .get("pack")
            .is_none_or(|value| value.is_null() || value.as_str() == Some(""))
        && before.mode() & 0o077 == 0
    {
        return Ok(());
    }
    theme.insert("system_theme".into(), Value::from(2));
    theme.insert("id".into(), Value::from(""));
    theme.remove("pack");
    let mut output = serde_json::to_vec(&value)?;
    output.push(b'\n');
    if running(root)
        || !same(&before, &file.metadata()?)
        || !same(&before, &fs::symlink_metadata(&path)?)
    {
        return Ok(());
    }
    seele_runtime::fs::atomic_write_at(&profile, std::ffi::OsStr::new("Preferences"), &output)
}
pub fn update(root: &Path) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let directory = seele_runtime::fs::private_directory(root)?;
    if running(&directory) {
        return Ok(());
    }
    for entry in fs::read_dir(pinned(&directory))?.take(4096).flatten() {
        if running(&directory) {
            break;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = update_profile(&directory, &entry.file_name());
        }
    }
    Ok(())
}
pub fn configured_root() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    config
        .is_absolute()
        .then(|| config.join("BraveSoftware/Brave-Browser"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn profile(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(name);
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let file = path.join("Preferences");
        fs::write(&file, bytes).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        file
    }
    #[test]
    fn changes_only_theme_fields_and_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let file=profile(root.path(),"Default",br#"{"extensions":{"theme":{"system_theme":1,"id":"old","pack":"old","other":true}},"keep":{"text":"\ud83e\udd80","enabled":false}}"#);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        update(root.path()).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(
            value["extensions"]["theme"],
            serde_json::json!({"system_theme":2,"id":"","other":true})
        );
        assert_eq!(
            value["keep"],
            serde_json::json!({"text":"🦀","enabled":false})
        );
        let before = file.metadata().unwrap();
        assert_eq!(before.mode() & 0o777, 0o600);
        assert_eq!(before.nlink(), 1);
        update(root.path()).unwrap();
        assert!(same(&before, &file.metadata().unwrap()));
        let missing = root.path().join("missing");
        update(&missing).unwrap();
        assert!(!missing.exists());
    }
    #[test]
    fn active_browser_and_unsafe_profiles_are_untouched() {
        let root = tempfile::tempdir().unwrap();
        let file = profile(root.path(), "Default", b"{}");
        symlink("missing-process", root.path().join("SingletonLock")).unwrap();
        update(root.path()).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{}");
        fs::remove_file(root.path().join("SingletonLock")).unwrap();
        let external = profile(root.path(), "External", b"{}");
        fs::remove_file(&file).unwrap();
        symlink(&external, &file).unwrap();
        // The external directory also has an invalid shape, avoiding a legitimate edit.
        fs::write(&external, b"{\"extensions\":[]}").unwrap();
        update(root.path()).unwrap();
        assert_eq!(fs::read(&external).unwrap(), b"{\"extensions\":[]}");
        assert!(file.symlink_metadata().unwrap().file_type().is_symlink());
        fs::remove_file(&file).unwrap();
        fs::hard_link(&external, &file).unwrap();
        update(root.path()).unwrap();
        assert_eq!(file.metadata().unwrap().nlink(), 2);
        fs::remove_file(&file).unwrap();
        fs::write(&file, b"{}").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o666)).unwrap();
        update(root.path()).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{}");
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(update(root.path()).is_err());
    }
}
