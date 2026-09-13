//! Refresh only owned links. Directory descriptors pin both the lock and final
//! manifest publication, including concurrent pathname replacement.
use crate::Result;
use cap_std::fs::{Dir, DirBuilder, DirBuilderExt, MetadataExt, OpenOptions, OpenOptionsExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsStr,
    fs,
    io::{self, Read},
    os::fd::AsRawFd,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_ENTRIES: usize = 100_000;
const MAX_MANIFEST: u64 = 16 * 1024 * 1024;
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    generation: String,
    #[serde(default)]
    links: BTreeMap<String, String>,
}
fn collect(
    source: &Path,
    relative: &Path,
    active: &mut HashSet<PathBuf>,
    links: &mut BTreeMap<String, String>,
    legacy: bool,
    visited: &mut usize,
    bytes: &mut usize,
) -> Result {
    if relative.components().count() > 128 {
        return Err("configuration tree exceeds its depth limit".into());
    }
    let directory = source.join(relative);
    let canonical = fs::canonicalize(&directory)?;
    if !active.insert(canonical.clone()) {
        return Err("configuration directory symlink cycle".into());
    }
    for entry in fs::read_dir(&directory)? {
        *visited += 1;
        if *visited > MAX_ENTRIES {
            return Err("configuration tree exceeds its entry limit".into());
        }
        let entry = entry?;
        let path = entry.path();
        let name = relative.join(entry.file_name());
        if legacy && entry.file_type()?.is_symlink() {
            let target = fs::read_link(path)?;
            insert_link(links, &name, &target, bytes)?;
        } else if match fs::metadata(&path) {
            Ok(metadata) => metadata.is_dir(),
            Err(e) if e.kind() == io::ErrorKind::NotFound && entry.file_type()?.is_symlink() => {
                false
            }
            Err(e) => return Err(e.into()),
        } {
            collect(source, &name, active, links, legacy, visited, bytes)?;
        } else {
            insert_link(links, &name, &path, bytes)?;
        }
    }
    active.remove(&canonical);
    Ok(())
}
fn insert_link(
    links: &mut BTreeMap<String, String>,
    name: &Path,
    target: &Path,
    bytes: &mut usize,
) -> Result {
    let name = name.to_str().ok_or("non-UTF8 configuration path")?;
    let target = target.to_str().ok_or("non-UTF8 configuration target")?;
    *bytes += serde_json::to_vec(&(name, target))?.len();
    if *bytes > MAX_MANIFEST as usize - 65536 {
        return Err("configuration manifest exceeds its byte limit".into());
    }
    links.insert(name.into(), target.into());
    Ok(())
}
fn create_dir(root: &Dir, name: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.mode(0o700);
    match root.create_dir_with(name, &builder) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}
fn open_dir(root: &Dir, name: &Path) -> Result<Dir> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = root.open_with(name, &options)?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
    {
        return Err("unsafe configuration directory".into());
    }
    Ok(Dir::from_std_file(directory.into_std()))
}
fn parent(root: &Dir, name: &Path, create: bool) -> Result<Option<(Dir, PathBuf)>> {
    let parts: Vec<_> = name.components().collect();
    if parts.is_empty()
        || parts
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Ok(None);
    }
    let mut dir = root.try_clone()?;
    for part in &parts[..parts.len() - 1] {
        let part = Path::new(part.as_os_str());
        match dir.symlink_metadata(part) {
            Ok(metadata) if metadata.is_dir() => (),
            Ok(_) => return Ok(None),
            Err(e) if e.kind() == io::ErrorKind::NotFound && create => create_dir(&dir, part)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        dir = open_dir(&dir, part)?;
    }
    Ok(Some((
        dir,
        PathBuf::from(parts.last().unwrap().as_os_str()),
    )))
}
fn metadata_file(root: &Dir, name: &str, limit: u64, private: bool) -> Result<Option<Vec<u8>>> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    let file = match root.open_with(name, &options) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & if private { 0o077 } else { 0o022 } != 0
        || metadata.len() > limit
    {
        return Err("unsafe configuration metadata".into());
    }
    let mut bytes = vec![];
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("configuration metadata exceeds its limit".into());
    }
    Ok(Some(bytes))
}
fn lock(root: &Dir, name: &str) -> Result<cap_std::fs::File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    let lock = root.open_with(name, &options)?;
    let metadata = lock.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        return Err("unsafe configuration lock".into());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(lock);
        }
        let error = io::Error::last_os_error();
        if !matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ) {
            return Err(error.into());
        }
        if Instant::now() >= deadline {
            return Err("configuration refresh is busy; retry".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub fn materialize(source: &Path, destination: &Path) -> Result {
    materialize_with(source, destination, || {})
}
fn materialize_with(source: &Path, destination: &Path, before_publish: impl FnOnce()) -> Result {
    // Resolve only the source root. Recursive aliases keep their lexical paths so
    // each generated link continues to address the correct configuration subtree.
    let source = fs::canonicalize(source)?;
    if !source.is_dir() {
        return Err("configuration source is not a directory".into());
    }
    let source_text = source.to_str().ok_or("non-UTF8 configuration source")?;
    let outer = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let outer = Dir::from_std_file(seele_runtime::fs::private_directory(outer)?);
    let name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("invalid destination")?;
    let _lock = lock(&outer, &format!(".{name}.lock"))?;
    create_dir(&outer, Path::new(name))?;
    let root = open_dir(&outer, Path::new(name))?;
    let encoded = metadata_file(&root, ".seele-manifest.json", MAX_MANIFEST, true)?;
    let mut previous: Manifest = match &encoded {
        Some(bytes) => serde_json::from_slice(bytes)?,
        None => Manifest::default(),
    };
    if previous.links.len() > MAX_ENTRIES {
        return Err("configuration manifest exceeds its entry limit".into());
    }
    if previous.generation == source_text {
        return Ok(());
    }
    if encoded.is_none() {
        if let Some(bytes) = metadata_file(&root, ".seele-generation", 4096, false)? {
            let generation = std::str::from_utf8(&bytes)?.trim();
            let generation = Path::new(generation);
            if generation.starts_with("/nix/store")
                && !generation
                    .components()
                    .any(|part| matches!(part, Component::ParentDir))
            {
                let legacy = generation.join(".config");
                if legacy.is_dir() {
                    collect(
                        &legacy,
                        Path::new(""),
                        &mut HashSet::new(),
                        &mut previous.links,
                        true,
                        &mut 0,
                        &mut 0,
                    )?;
                }
            }
        }
    }
    let mut wanted = BTreeMap::new();
    collect(
        &source,
        Path::new(""),
        &mut HashSet::new(),
        &mut wanted,
        false,
        &mut 0,
        &mut 0,
    )?;
    for (name, target) in previous.links {
        if let Some((dir, file)) = parent(&root, Path::new(&name), false)? {
            if dir.read_link_contents(&file).ok().as_deref() == Some(Path::new(&target)) {
                dir.remove_file(file)?;
            }
        }
    }
    let mut installed = BTreeMap::new();
    for (name, target) in wanted {
        let Some((dir, file)) = parent(&root, Path::new(&name), true)? else {
            continue;
        };
        match dir.symlink_metadata(&file) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                dir.symlink_contents(&target, &file)?;
                installed.insert(name, target);
            }
            Ok(_) if dir.read_link_contents(&file).ok().as_deref() == Some(Path::new(&target)) => {
                installed.insert(name, target);
            }
            Ok(_) => (),
            Err(e) => return Err(e.into()),
        }
    }
    let encoded = serde_json::to_vec(&Manifest {
        generation: source_text.into(),
        links: installed,
    })?;
    if encoded.len() as u64 > MAX_MANIFEST {
        return Err("configuration manifest exceeds its byte limit".into());
    }
    before_publish();
    seele_runtime::fs::atomic_write_at(
        &root.into_std_file(),
        OsStr::new(".seele-manifest.json"),
        &encoded,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn private() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    fn source(root: &Path, name: &str) -> PathBuf {
        let source = root.join(name);
        fs::create_dir(&source).unwrap();
        fs::write(source.join("setting"), name).unwrap();
        source
    }
    #[test]
    fn refresh_preserves_user_files_and_removes_only_owned_links() {
        let root = private();
        let first = source(root.path(), "first");
        let second = source(root.path(), "second");
        let destination = root.path().join("config");
        materialize(&first, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("setting")).unwrap(),
            "first"
        );
        fs::write(destination.join("personal"), "keep").unwrap();
        materialize(&second, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("setting")).unwrap(),
            "second"
        );
        assert_eq!(
            fs::read_to_string(destination.join("personal")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn missing_source_retains_existing_generation() {
        let root = private();
        let input = source(root.path(), "source");
        let destination = root.path().join("config");
        materialize(&input, &destination).unwrap();
        let manifest = fs::read(destination.join(".seele-manifest.json")).unwrap();
        assert!(materialize(&root.path().join("missing"), &destination).is_err());
        assert_eq!(
            fs::read(destination.join(".seele-manifest.json")).unwrap(),
            manifest
        );
        assert!(destination.join("setting").is_symlink());
    }
    #[test]
    fn relative_source_is_resolved_before_generating_links() {
        let root = private();
        let input = source(root.path(), "source");
        let destination = root.path().join("config");
        let cwd = std::env::current_dir().unwrap();
        let inside = tempfile::Builder::new()
            .prefix(".materialize-test-")
            .tempdir_in(&cwd)
            .unwrap();
        let relative = inside.path().strip_prefix(&cwd).unwrap();
        fs::write(inside.path().join("setting"), "relative").unwrap();
        materialize(relative, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("setting")).unwrap(),
            "relative"
        );
        let _ = input;
    }
    #[test]
    fn lock_and_manifest_reject_symlinks_fifos_modes_and_hardlinks() {
        for kind in ["symlink", "fifo", "mode", "hardlink"] {
            let root = private();
            let input = source(root.path(), "source");
            let destination = root.path().join("config");
            let lock = root.path().join(".config.lock");
            let target = root.path().join("target");
            fs::write(&target, b"untouched").unwrap();
            match kind {
                "symlink" => symlink(&target, &lock).unwrap(),
                "fifo" => {
                    let name = std::ffi::CString::new(lock.to_str().unwrap()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                }
                "mode" => {
                    fs::write(&lock, "").unwrap();
                    fs::set_permissions(&lock, fs::Permissions::from_mode(0o666)).unwrap();
                }
                _ => fs::hard_link(&target, &lock).unwrap(),
            };
            let start = Instant::now();
            assert!(materialize(&input, &destination).is_err());
            assert!(start.elapsed() < Duration::from_secs(1));
            assert_eq!(fs::read(&target).unwrap(), b"untouched");
        }
        for kind in ["symlink", "fifo"] {
            let root = private();
            let input = source(root.path(), "source");
            let destination = root.path().join("config");
            fs::create_dir(&destination).unwrap();
            let manifest = destination.join(".seele-manifest.json");
            if kind == "symlink" {
                symlink(input.join("setting"), &manifest).unwrap();
            } else {
                let name = std::ffi::CString::new(manifest.to_str().unwrap()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            assert!(materialize(&input, &destination).is_err());
        }
    }
    #[test]
    fn manifest_commit_stays_with_open_directory_during_path_replacement() {
        let root = private();
        let input = source(root.path(), "source");
        let destination = root.path().join("config");
        let retired = root.path().join("retired");
        materialize_with(&input, &destination, || {
            fs::rename(&destination, &retired).unwrap();
            fs::create_dir(&destination).unwrap();
            fs::write(destination.join("personal"), "keep").unwrap();
        })
        .unwrap();
        assert!(retired.join(".seele-manifest.json").is_file());
        assert!(!destination.join(".seele-manifest.json").exists());
        assert_eq!(
            fs::read_to_string(destination.join("personal")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn directory_cycles_and_parent_symlinks_do_not_escape_destination() {
        let root = private();
        let input = source(root.path(), "source");
        symlink(&input, input.join("cycle")).unwrap();
        assert!(materialize(&input, &root.path().join("config")).is_err());
        fs::remove_file(input.join("cycle")).unwrap();
        let other = private();
        let destination = root.path().join("alias");
        symlink(other.path(), &destination).unwrap();
        assert!(materialize(&input, &destination).is_err());
        assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
    }
    #[test]
    fn directory_aliases_and_broken_leaf_links_keep_their_source_paths() {
        let root = private();
        let input = source(root.path(), "source");
        fs::create_dir(input.join("nested")).unwrap();
        fs::write(input.join("nested/value"), "nested").unwrap();
        symlink("nested", input.join("alias")).unwrap();
        symlink("missing", input.join("broken")).unwrap();
        let destination = root.path().join("config");
        materialize(&input, &destination).unwrap();
        assert_eq!(
            fs::read_link(destination.join("alias/value")).unwrap(),
            input.join("alias/value")
        );
        assert_eq!(
            fs::read_to_string(destination.join("alias/value")).unwrap(),
            "nested"
        );
        assert_eq!(
            fs::read_link(destination.join("broken")).unwrap(),
            input.join("broken")
        );
    }
    #[test]
    fn traversal_counts_empty_directories_and_serialized_manifest_bytes() {
        let root = private();
        fs::create_dir(root.path().join("empty")).unwrap();
        let mut visited = MAX_ENTRIES;
        assert!(collect(
            root.path(),
            Path::new(""),
            &mut HashSet::new(),
            &mut BTreeMap::new(),
            false,
            &mut visited,
            &mut 0
        )
        .is_err());
        let mut links = BTreeMap::new();
        let mut bytes = MAX_MANIFEST as usize - 65536 - 10;
        assert!(insert_link(
            &mut links,
            Path::new("a\n\n\n\n\n"),
            Path::new("b"),
            &mut bytes
        )
        .is_err());
        assert!(links.is_empty());
    }
    #[test]
    fn malformed_metadata_preserves_existing_links() {
        let root = private();
        let first = source(root.path(), "first");
        let second = source(root.path(), "second");
        let destination = root.path().join("config");
        materialize(&first, &destination).unwrap();
        fs::write(destination.join(".seele-manifest.json"), b"{invalid").unwrap();
        assert!(materialize(&second, &destination).is_err());
        assert_eq!(
            fs::read_link(destination.join("setting")).unwrap(),
            first.join("setting")
        );
        assert_eq!(
            fs::read(destination.join(".seele-manifest.json")).unwrap(),
            b"{invalid"
        );
    }
}
