use seele_runtime::fs::{atomic_write_new, atomic_write_with_mode, rename_noreplace};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::{fs, io};

#[test]
fn concurrent_creators_cannot_overwrite_the_winner() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.md");
    let winners: Vec<_> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0u8..16)
            .map(|byte| {
                let path = &path;
                scope.spawn(move || match atomic_write_new(path, &[byte; 1024]) {
                    Ok(()) => Some(byte),
                    Err(error) => {
                        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
                        None
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .filter_map(|worker| worker.join().unwrap())
            .collect()
    });
    assert_eq!(winners.len(), 1);
    assert_eq!(fs::read(&path).unwrap(), vec![winners[0]; 1024]);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn exclusive_moves_preserve_both_files_and_symlink_targets_on_collision() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::write(&source, b"source").unwrap();
    fs::write(&target, b"target").unwrap();
    assert_eq!(
        rename_noreplace(&source, &target).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(&target).unwrap(), b"target");
    let alias = root.path().join("alias");
    symlink(&target, &alias).unwrap();
    assert_eq!(
        atomic_write_new(&alias, b"no").unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&target).unwrap(), b"target");
    let moved = root.path().join("moved");
    rename_noreplace(&source, &moved).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&moved).unwrap(), b"source");
}

#[test]
fn vault_modes_are_explicit_and_never_executable() {
    let root = tempfile::tempdir().unwrap();
    let document = root.path().join("document.md");
    atomic_write_with_mode(&document, b"complete", 0o644, true).unwrap();
    assert_eq!(
        fs::metadata(&document).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert!(atomic_write_with_mode(&document, b"no", 0o777, false).is_err());
    assert_eq!(fs::read(&document).unwrap(), b"complete");
    atomic_write_with_mode(&document, b"private", 0o600, false).unwrap();
    assert_eq!(
        fs::metadata(&document).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
