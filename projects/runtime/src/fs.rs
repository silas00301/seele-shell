//! Descriptor-relative publication: no predictable temporary names, symlink
//! traversal, broad creation permissions, or pathname races after opening a
//! directory. A successful write includes the directory durability barrier.
use std::ffi::{CString, OsStr};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::Path;

fn name(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput.into())
}

/// Open an existing private destination directory or create it with mode 0700.
/// Ancestors are caller-owned configuration roots; the final component must
/// never be a symlink or writable by another user. No existing mode is changed.
pub fn private_directory(path: &Path) -> io::Result<File> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = directory.metadata()?;
    // SAFETY: geteuid has no preconditions.
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe state directory",
        ));
    }
    Ok(directory)
}

/// Read a bounded regular file without hanging on a FIFO or device. Private
/// state also rejects final symlinks, foreign owners, hardlinks and broad modes.
pub fn read_bounded(path: &Path, limit: usize, private: bool) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(
            libc::O_CLOEXEC | libc::O_NONBLOCK | if private { libc::O_NOFOLLOW } else { 0 },
        )
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.len() > limit as u64
        || (private
            && (metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.nlink() != 1))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invalid input file",
        ));
    }
    let mut bytes = Vec::new();
    file.take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(bytes)
}

pub fn read_private(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    read_bounded(path, limit, true)
}

struct Temporary<'a> {
    directory: &'a File,
    name: CString,
    active: bool,
}

impl<'a> Temporary<'a> {
    // Own cleanup only after O_EXCL creates this exact file. In particular an
    // unlikely random-name collision must never unlink another writer's file.
    fn create(directory: &'a File, filename: &OsStr) -> io::Result<(Self, File)> {
        let name = name(filename)?;
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            Self {
                directory,
                name,
                active: true,
            },
            unsafe { File::from_raw_fd(descriptor) },
        ))
    }
    fn remove(&mut self) -> io::Result<()> {
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        // SAFETY: both descriptor and NUL-terminated name remain live here.
        if self.active {
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    publish_path(path, bytes, false, 0o600)
}

/// Publish a new file atomically, returning AlreadyExists for every collision.
pub fn atomic_write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    publish_path(path, bytes, true, 0o600)
}

/// Preserve intentionally readable vault documents while keeping private state
/// private. Only 0600 and 0644 are supported; executable/public-write bits fail.
pub fn atomic_write_with_mode(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    exclusive: bool,
) -> io::Result<()> {
    if !matches!(mode, 0o600 | 0o644) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    publish_path(path, bytes, exclusive, mode)
}

fn publish_path(path: &Path, bytes: &[u8], exclusive: bool, mode: u32) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = private_directory(parent)?;
    publish_at(
        &directory,
        path.file_name().ok_or(io::ErrorKind::InvalidInput)?,
        bytes,
        exclusive,
        mode,
    )
}

/// Publish within a caller-owned directory descriptor, even if its pathname is
/// concurrently renamed. The destination is one basename, never a path.
pub fn atomic_write_at(directory: &File, destination: &OsStr, bytes: &[u8]) -> io::Result<()> {
    publish_at(directory, destination, bytes, false, 0o600)
}

fn publish_at(
    directory: &File,
    destination: &OsStr,
    bytes: &[u8],
    exclusive: bool,
    mode: u32,
) -> io::Result<()> {
    let metadata = directory.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut components = Path::new(destination).components();
    if destination.as_bytes().contains(&b'/')
        || !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let destination = name(destination)?;
    let mut random = [0u8; 16];
    // SAFETY: getentropy writes at most the supplied buffer length (<= 256).
    if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let filename = format!(".seele-{:032x}.tmp", u128::from_ne_bytes(random));
    let (mut temporary, mut file) = Temporary::create(directory, OsStr::new(&filename))?;
    file.write_all(bytes)?;
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.sync_all()?;
    // Rename replaces a destination symlink itself, never its target. Both
    // paths are relative to the same pinned directory, even if it was moved.
    // SAFETY: descriptors and both C strings are live for the call.
    if unsafe {
        if exclusive {
            libc::linkat(
                directory.as_raw_fd(),
                temporary.name.as_ptr(),
                directory.as_raw_fd(),
                destination.as_ptr(),
                0,
            )
        } else {
            libc::renameat(
                directory.as_raw_fd(),
                temporary.name.as_ptr(),
                directory.as_raw_fd(),
                destination.as_ptr(),
            )
        }
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if exclusive {
        // Success includes a single-link private destination. Report unlink
        // failure rather than claiming success with an extra temporary alias.
        temporary.remove()?;
    } else {
        // renameat already removed our source name. Do not let Drop unlink a
        // subsequently created file at that now-free name.
        temporary.active = false;
    }
    directory.sync_all()
}

/// Move within a filesystem without replacing any concurrent destination. Both
/// parent directories are pinned; cross-filesystem moves fail without deletion.
pub fn rename_noreplace(source: &Path, target: &Path) -> io::Result<()> {
    let source_dir = private_directory(source.parent().unwrap_or(Path::new(".")))?;
    let target_dir = private_directory(target.parent().unwrap_or(Path::new(".")))?;
    let source = name(source.file_name().ok_or(io::ErrorKind::InvalidInput)?)?;
    let target = name(target.file_name().ok_or(io::ErrorKind::InvalidInput)?)?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            source_dir.as_raw_fd(),
            source.as_ptr(),
            target_dir.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            source_dir.as_raw_fd(),
            source.as_ptr(),
            target_dir.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result = unsafe {
        let linked = libc::linkat(
            source_dir.as_raw_fd(),
            source.as_ptr(),
            target_dir.as_raw_fd(),
            target.as_ptr(),
            0,
        );
        if linked == 0 {
            libc::unlinkat(source_dir.as_raw_fd(), source.as_ptr(), 0)
        } else {
            linked
        }
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    target_dir.sync_all()?;
    source_dir.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn failed_exclusive_temporary_creation_preserves_existing_file_and_symlink() {
        let root = tempfile::tempdir().unwrap();
        let directory = private_directory(root.path()).unwrap();
        let victim = root.path().join("victim");
        fs::write(&victim, b"untouched").unwrap();
        let alias = root.path().join("alias");
        symlink(&victim, &alias).unwrap();
        for name in ["victim", "alias"] {
            let result = Temporary::create(&directory, OsStr::new(name));
            assert!(
                matches!(result, Err(ref error) if error.kind() == io::ErrorKind::AlreadyExists)
            );
            assert!(fs::symlink_metadata(root.path().join(name)).is_ok());
        }
        assert_eq!(fs::read(&victim).unwrap(), b"untouched");
        let (mut temporary, _file) = Temporary::create(&directory, OsStr::new("owned")).unwrap();
        temporary.remove().unwrap();
        fs::write(root.path().join("owned"), b"successor").unwrap();
        drop(temporary);
        assert_eq!(fs::read(root.path().join("owned")).unwrap(), b"successor");
        let (mut temporary, _file) = Temporary::create(&directory, OsStr::new("gone")).unwrap();
        fs::remove_file(root.path().join("gone")).unwrap();
        assert_eq!(
            temporary.remove().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn replacement_does_not_follow_symlinks_and_is_private() {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let victim = root.path().join("victim");
        fs::write(&victim, "untouched").unwrap();
        let destination = root.path().join("state.json");
        symlink(&victim, &destination).unwrap();
        let old_temporary = destination.with_extension(format!("{}.tmp", std::process::id()));
        symlink(&victim, &old_temporary).unwrap();
        atomic_write(&destination, b"new").unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn concurrent_writers_publish_complete_documents_without_collisions() {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = root.path().join("state");
        std::thread::scope(|scope| {
            for byte in 0u8..16 {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..8 {
                        atomic_write(path, &vec![byte; 4096]).unwrap();
                    }
                });
            }
        });
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 4096);
        assert!(bytes.iter().all(|byte| *byte == bytes[0]));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn unsafe_directory_and_failed_publish_do_not_leave_temporary_files() {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let alias = root.path().join("alias");
        symlink(root.path(), &alias).unwrap();
        assert!(atomic_write(&alias.join("state"), b"no").is_err());
        let occupied = root.path().join("occupied");
        fs::create_dir(&occupied).unwrap();
        assert!(atomic_write(&occupied, b"no").is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        fs::set_permissions(&occupied, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(atomic_write(&occupied.join("state"), b"no").is_err());
    }
}

#[cfg(test)]
mod descriptor_tests {
    use super::*;
    #[test]
    fn renamed_directory_stays_pinned_and_names_cannot_escape() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let moved = root.path().join("moved");
        let directory = private_directory(&original).unwrap();
        fs::rename(&original, &moved).unwrap();
        fs::create_dir(&original).unwrap();
        atomic_write_at(&directory, OsStr::new("state"), b"pinned").unwrap();
        assert_eq!(fs::read(moved.join("state")).unwrap(), b"pinned");
        assert!(!original.join("state").exists());
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "/tmp/escape",
            "a/b",
            "a/",
            "a/.",
        ] {
            assert!(atomic_write_at(&directory, OsStr::new(name), b"no").is_err());
        }
    }
}
