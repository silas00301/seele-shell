//! Streaming SHA-256 of one stable regular file; no file writes or traversal.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
const MAX: u64 = 8 * 1024 * 1024 * 1024;
fn same(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
pub fn fingerprint(path: &Path, cancel: &AtomicUsize) -> Result<String, &'static str> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Cannot open a regular file (symbolic links are refused).")?;
    let before = file.metadata().map_err(|_| "Cannot inspect file.")?;
    if !before.is_file() || before.len() > MAX {
        return Err("Choose a regular file no larger than 8 GiB.");
    }
    let start = Instant::now();
    let mut total = 0;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) != 0 || start.elapsed() > Duration::from_secs(120) {
            return Err("Fingerprint cancelled or timed out.");
        }
        let read = file.read(&mut buffer).map_err(|_| "Cannot read file.")?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX || total > before.len() {
            return Err("File changed while hashing; no digest copied.");
        }
        digest.update(&buffer[..read]);
    }
    let after = file.metadata().map_err(|_| "Cannot recheck file.")?;
    let named = fs::symlink_metadata(path).map_err(|_| "File moved while hashing.")?;
    if total != before.len() || !same(&before, &after) || !same(&before, &named) {
        return Err("File changed while hashing; no digest copied.");
    }
    Ok(format!("{:x}", digest.finalize()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vectors_bounds_and_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("-a b");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            fingerprint(&path, &AtomicUsize::new(0)).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let before = fs::metadata(&path).unwrap();
        fs::write(&path, b"xyz changed").unwrap();
        assert!(!same(&before, &fs::metadata(&path).unwrap()));
        assert!(fingerprint(dir.path(), &AtomicUsize::new(0)).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(fingerprint(&link, &AtomicUsize::new(0)).is_err());
        assert!(fingerprint(&path, &AtomicUsize::new(1)).is_err());
        let huge = fs::File::create(dir.path().join("huge")).unwrap();
        huge.set_len(MAX + 1).unwrap();
        assert!(fingerprint(&dir.path().join("huge"), &AtomicUsize::new(0)).is_err());
    }
}
