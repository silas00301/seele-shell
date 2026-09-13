//! Files are opened once and publication is relative to a pinned directory.
use crate::common::Result;
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Path, PathBuf},
};
pub fn filename(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.len() > 255
        || matches!(value, "." | "..")
        || value.contains(['/', '\\', '\0'])
    {
        Err("invalid-filename")
    } else {
        Ok(value)
    }
}
pub fn regular(path: &Path) -> Result<PathBuf> {
    let path = if let Ok(s) = path.strip_prefix("~/") {
        crate::common::xdg("HOME", "").join(s)
    } else {
        path.to_owned()
    };
    let path = path.canonicalize().map_err(|_| "source-missing")?;
    let file = open_source(&path)?;
    drop(file);
    Ok(path)
}
pub fn open_source(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "source-missing")?;
    if !file.metadata().map_err(|_| "source-missing")?.is_file() {
        return Err("not-a-file");
    }
    Ok(file)
}
fn cstring(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| "invalid-filename")
}
fn directory(path: &Path) -> Result<(PathBuf, File)> {
    let path = path.canonicalize().map_err(|_| "destination-unavailable")?;
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| "destination-unavailable")?;
    let info = dir.metadata().map_err(|_| "destination-unavailable")?;
    if info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o022 != 0 {
        return Err("unsafe-destination");
    }
    Ok((path, dir))
}
fn numbered(name: &str, index: usize) -> String {
    if index == 0 {
        return name.to_owned();
    }
    let path = Path::new(name);
    let suffix = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    format!("{stem} ({index}){suffix}")
}
pub struct Staged {
    directory: File,
    root: PathBuf,
    name: CString,
    pub file: File,
    live: bool,
}
impl Staged {
    pub fn new(path: &Path) -> Result<Self> {
        let (root, directory) = directory(path)?;
        let name = cstring(&format!(".seele-{}.part", uuid::Uuid::new_v4()))?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err("destination-unavailable");
        }
        Ok(Self {
            directory,
            root,
            name,
            file: unsafe { File::from_raw_fd(fd) },
            live: true,
        })
    }
    pub fn publish(mut self, name: &str) -> Result<PathBuf> {
        filename(name)?;
        self.file
            .sync_all()
            .map_err(|_| "destination-unavailable")?;
        for index in 0..100000 {
            let name = numbered(name, index);
            let destination = cstring(&name)?;
            // Hard-link publication is atomic and never replaces a file or symlink.
            if unsafe {
                libc::linkat(
                    self.directory.as_raw_fd(),
                    self.name.as_ptr(),
                    self.directory.as_raw_fd(),
                    destination.as_ptr(),
                    0,
                )
            } == 0
            {
                // If publication cannot be made durable, remove only this new
                // link while the private staging link still owns the inode.
                // A failed barrier must not leave an unrecorded final file that
                // the next attempt would duplicate under a numbered name.
                if self.directory.sync_all().is_err() {
                    unsafe {
                        libc::unlinkat(self.directory.as_raw_fd(), destination.as_ptr(), 0);
                    }
                    return Err("destination-unavailable");
                }
                unsafe {
                    libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
                }
                self.live = false;
                // The final link is already durable; only temporary-name
                // cleanup remains if this second barrier fails.
                let _ = self.directory.sync_all();
                return Ok(self.root.join(name));
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
                return Err("destination-unavailable");
            }
        }
        Err("destination-full")
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        if self.live {
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    modified: i64,
    modified_ns: i64,
    changed: i64,
    changed_ns: i64,
}
impl Identity {
    pub fn of(info: &std::fs::Metadata) -> Self {
        Self {
            device: info.dev(),
            inode: info.ino(),
            size: info.len(),
            modified: info.mtime(),
            modified_ns: info.mtime_nsec(),
            changed: info.ctime(),
            changed_ns: info.ctime_nsec(),
        }
    }
    pub fn matches(&self, info: &std::fs::Metadata) -> bool {
        self.device == info.dev()
            && self.inode == info.ino()
            && self.size == info.len()
            && self.modified == info.mtime()
            && self.modified_ns == info.mtime_nsec()
            && self.changed == info.ctime()
            && self.changed_ns == info.ctime_nsec()
    }
}
pub fn unchanged(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    Identity::of(before).matches(after)
}

pub fn move_file(path: &Path, destination: &Path) -> Result<PathBuf> {
    let mut source = open_source(path)?;
    let original = source.metadata().map_err(|_| "source-missing")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid-filename")?;
    filename(name)?;
    let (root, dir) = directory(destination)?;
    let source_name = CString::new(path.as_os_str().as_bytes()).map_err(|_| "source-missing")?;
    for index in 0..100000 {
        let name = numbered(name, index);
        let new = cstring(&name)?;
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source_name.as_ptr(),
                dir.as_raw_fd(),
                new.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } == 0
        {
            dir.sync_all().map_err(|_| "destination-unavailable")?;
            return Ok(root.join(name));
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EXDEV) {
            break;
        }
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err("desktop-action-failed");
        }
        if index == 99999 {
            return Err("destination-full");
        }
    }
    let mut staged = Staged::new(destination)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = source.read(&mut buf).map_err(|_| "source-changed")?;
        if n == 0 {
            break;
        }
        staged
            .file
            .write_all(&buf[..n])
            .map_err(|_| "destination-unavailable")?;
    }
    if !unchanged(&original, &source.metadata().map_err(|_| "source-changed")?)
        || !unchanged(
            &original,
            &std::fs::symlink_metadata(path).map_err(|_| "source-changed")?,
        )
    {
        return Err("source-changed");
    }
    let target = staged.publish(name)?;
    std::fs::remove_file(path).map_err(|_| "desktop-action-failed")?;
    Ok(target)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn invalid_names_and_atomic_collision_publication() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "existing").unwrap();
        symlink("a.txt", root.path().join("a (1).txt")).unwrap();
        let mut staged = Staged::new(root.path()).unwrap();
        staged.file.write_all(b"received").unwrap();
        let path = staged.publish("a.txt").unwrap();
        assert_eq!(path.file_name().unwrap(), "a (2).txt");
        assert_eq!(std::fs::read(path.clone()).unwrap(), b"received");
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read(root.path().join("a.txt")).unwrap(),
            b"existing"
        );
        for name in ["../escape", "/absolute", "a/b", "..", "a\\b", ""] {
            assert!(filename(name).is_err());
        }
    }
    #[test]
    fn interrupted_stage_removes_only_owned_private_temporary() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("safe"), "stay").unwrap();
        {
            let mut staged = Staged::new(root.path()).unwrap();
            staged.file.write_all(b"partial").unwrap();
        }
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
