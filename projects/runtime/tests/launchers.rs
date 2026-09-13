use seele_runtime::process::{discard, discard_detaching, Limits};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[test]
fn only_explicit_successful_launchers_keep_their_background_owner() {
    let root = tempfile::tempdir().unwrap();
    for (detach, status) in [(true, 0), (true, 7), (false, 0)] {
        let receipt = root.path().join(format!("{detach}-{status}"));
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "(sleep 0.08; printf survived > \"$1\") & exit \"$2\"",
                "sh",
            ])
            .arg(&receipt)
            .arg(status.to_string());
        let limits = Limits {
            timeout: Duration::from_secs(2),
            output: 128,
        };
        let result = if detach {
            discard_detaching(&mut command, b"", limits, &AtomicUsize::new(0))
        } else {
            discard(&mut command, b"", limits, &AtomicUsize::new(0))
        }
        .unwrap();
        assert_eq!(result.code(), Some(status));
        std::thread::sleep(Duration::from_millis(180));
        assert_eq!(receipt.exists(), detach && status == 0);
    }
}

#[test]
fn bounded_file_reader_rejects_fifos_and_private_aliases() {
    use std::ffi::CString;
    use std::fs;
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{symlink, PermissionsExt},
    };
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("fifo");
    let encoded = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(encoded.as_ptr(), 0o600) }, 0);
    let started = std::time::Instant::now();
    assert!(seele_runtime::fs::read_bounded(&fifo, 128, false).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    let path = root.path().join("state");
    fs::write(&path, b"exact").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(seele_runtime::fs::read_private(&path, 5).unwrap(), b"exact");
    assert!(seele_runtime::fs::read_bounded(&path, 4, false).is_err());
    let alias = root.path().join("alias");
    symlink(&path, &alias).unwrap();
    assert!(seele_runtime::fs::read_private(&alias, 128).is_err());
    assert_eq!(
        seele_runtime::fs::read_bounded(&alias, 128, false).unwrap(),
        b"exact"
    );
    fs::hard_link(&path, root.path().join("hardlink")).unwrap();
    assert!(seele_runtime::fs::read_private(&path, 128).is_err());
}

#[test]
fn detached_receiver_gets_complete_stdin_and_abandoned_input_fails() {
    let root = tempfile::tempdir().unwrap();
    let receipt = root.path().join("received");
    let input = vec![b'x'; 2 * 1024 * 1024];
    // The parent exits before the descendant starts draining more than the
    // kernel pipe capacity. Its successful exit alone is not a data handoff.
    let mut command = Command::new("sh");
    command
        .args([
            "-c",
            "exec 3<&0; (sleep 0.08; cat > \"$1\") <&3 & exit 0",
            "sh",
        ])
        .arg(&receipt);
    let result = discard_detaching(
        &mut command,
        &input,
        Limits {
            timeout: Duration::from_secs(5),
            output: 0,
        },
        &AtomicUsize::new(0),
    )
    .unwrap();
    assert!(result.success());
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::fs::metadata(&receipt).map_or(0, |m| m.len()) != input.len() as u64 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::fs::read(&receipt).unwrap(), input);
    let result = discard_detaching(
        Command::new("sh").args(["-c", "exit 0"]),
        &input,
        Limits {
            timeout: Duration::from_secs(1),
            output: 0,
        },
        &AtomicUsize::new(0),
    );
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
}
