//! Private report storage. Opened descriptors, not a prior pathname check,
//! establish ownership and file type before reads and append operations.
use crate::{text::bounded, MAX_AI_OUTPUT, MAX_REPORT};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn runtime_directory() -> io::Result<PathBuf> {
    let path = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() })));
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(path)
}
pub fn directory() -> io::Result<PathBuf> {
    let shell = runtime_directory()?.join("seele-shell");
    seele_runtime::fs::private_directory(&shell)?;
    let path = shell.join("failures");
    seele_runtime::fs::private_directory(&path)?;
    for entry in fs::read_dir(&path)?.filter_map(Result::ok).take(4096) {
        if let Ok(metadata) = entry.path().symlink_metadata() {
            if metadata.is_file()
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata
                    .modified()
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age > Duration::from_secs(86400))
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(path)
}
pub fn random_hex(bytes: usize) -> io::Result<String> {
    if bytes > 256 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut random = vec![0u8; bytes];
    if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(random.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn create(report: &str) -> io::Result<(String, PathBuf)> {
    let directory = directory()?;
    if fs::read_dir(&directory)?.take(1024).count() >= 1024 {
        return Err(io::Error::other("report capacity reached"));
    }
    for _ in 0..8 {
        let id = random_hex(8)?;
        let path = directory.join(format!("{id}.txt"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path);
        match file {
            Ok(mut file) => {
                file.write_all(bounded(report, MAX_REPORT, false).as_bytes())?;
                return Ok((id, path));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}
fn checked(path: &Path, write: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o600
        || metadata.nlink() != 1
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(file)
}
pub fn path(id: &str) -> io::Result<PathBuf> {
    if id.len() != 16
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let path = directory()?.join(format!("{id}.txt"));
    checked(&path, false)?;
    Ok(path)
}
pub fn append(path: &Path, heading: &str, text: &str) -> io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut file = checked(path, true)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let addition = format!(
        "\n\n[{heading}]\n{}\n",
        bounded(text.trim(), MAX_AI_OUTPUT, false)
    );
    if file.metadata()?.len() as usize + addition.len() > MAX_REPORT + 2 * MAX_AI_OUTPUT + 256 {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    file.seek(SeekFrom::End(0))?;
    file.write_all(addition.as_bytes())
}
pub fn read(path: &Path) -> io::Result<String> {
    let file = checked(path, false)?;
    let mut value = String::new();
    file.take((MAX_REPORT + 2 * MAX_AI_OUTPUT + 257) as u64)
        .read_to_string(&mut value)?;
    if value.len() > MAX_REPORT + 2 * MAX_AI_OUTPUT + 256 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn unsafe_files_and_symlinks_are_rejected() {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let safe = root.path().join("safe");
        fs::write(&safe, b"safe").unwrap();
        fs::set_permissions(&safe, fs::Permissions::from_mode(0o600)).unwrap();
        let alias = root.path().join("alias");
        symlink(&safe, &alias).unwrap();
        assert!(read(&alias).is_err());
        let hard = root.path().join("hard");
        fs::hard_link(&safe, &hard).unwrap();
        assert!(read(&hard).is_err());
        fs::remove_file(&hard).unwrap();
        append(&safe, "Analysis", "test").unwrap();
        assert!(read(&safe).unwrap().contains("Analysis"));
        fs::set_permissions(&safe, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read(&safe).is_err());
    }
}
