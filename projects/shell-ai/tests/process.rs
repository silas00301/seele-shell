//! Real process, PTY and private socket fixtures. Models and terminal programs
//! are fixture executables; no account or desktop session is accessed.
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener, process::CommandExt},
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::{atomic::AtomicUsize, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
const BINARY: &str = env!("CARGO_BIN_EXE_seele-shell-ai");
fn executable(root: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = root.join(name);
    let shell = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|path| path.join("sh"))
        .find(|path| path.is_file())
        .unwrap();
    fs::write(
        &path,
        text.replacen("#!/bin/sh", &format!("#!{}", shell.display()), 1),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}
fn private() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn wait(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(8);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            panic!("fixture process timed out");
        }
        thread::sleep(Duration::from_millis(5));
    }
    child.wait_with_output().unwrap()
}
fn await_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "fixture readiness timeout");
        thread::sleep(Duration::from_millis(5));
    }
}
fn fake_broker(root: &Path, result: Value) -> (Arc<Mutex<Vec<Value>>>, thread::JoinHandle<()>) {
    let listener = UnixListener::bind(root.join("seele-codex.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    let recorded = calls.clone();
    let worker = thread::spawn(move || {
        for _ in 0..3 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "broker fixture timed out");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            let request = seele_runtime::wire::receive(
                &stream,
                256 * 1024,
                Duration::from_secs(2),
                &AtomicUsize::new(0),
            )
            .unwrap();
            let op = request["op"].clone();
            recorded.lock().unwrap().push(request);
            let response = json!({"ok":true,"epoch":"00000000-0000-4000-8000-000000000001","job":{"id":"00000000-0000-4000-8000-000000000002","state":"succeeded"},"result":result});
            seele_runtime::wire::send(
                &stream,
                &response,
                256 * 1024,
                Duration::from_secs(2),
                &AtomicUsize::new(0),
            )
            .unwrap();
            if op == "release" {
                break;
            }
        }
    });
    (calls, worker)
}
fn command() -> Command {
    let mut c = Command::new(BINARY);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    c
}
#[test]
fn real_capture_tees_stderr_debug_uses_memory_and_session_is_cleaned() {
    let root = private();
    let script = executable(
        root.path(),
        "fish",
        r#"#!/bin/sh
"$SEELE_TEST_BINARY" begin
printf '\033[31mcaptured failure\033[0m\n' >&2
"$SEELE_TEST_BINARY" finish --status 19 --command 'broken command'
"$SEELE_TEST_BINARY" suggest --mode debug -- 'fix the failure'
printf '%s' "$SEELE_SHELL_AI_SESSION" > "$SEELE_TEST_SESSION"
: > "$SEELE_TEST_READY"
while [ ! -e "$SEELE_TEST_CONTINUE" ]; do sleep 0.01; done
"#,
    );
    let (calls, broker) = fake_broker(
        root.path(),
        json!({"suggestions":[{"command":"printf corrected","description":"Correct the failure","destructive":false}]}),
    );
    let mut command = command();
    command
        .args(["capture", "--fish"])
        .arg(script)
        .env("XDG_RUNTIME_DIR", root.path())
        .env("SEELE_TEST_BINARY", BINARY)
        .env("SEELE_TEST_SESSION", root.path().join("session"))
        .env("SEELE_TEST_READY", root.path().join("ready"))
        .env("SEELE_TEST_CONTINUE", root.path().join("continue"));
    let child = command.spawn().unwrap();
    await_file(&root.path().join("ready"));
    let session = fs::read_to_string(root.path().join("session")).unwrap();
    let entries: Vec<_> = fs::read_dir(&session)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(entries, [std::ffi::OsString::from("control.sock")]);
    assert_eq!(
        fs::metadata(&session).unwrap().permissions().mode() & 0o777,
        0o700
    );
    fs::write(root.path().join("continue"), "").unwrap();
    let output = wait(child);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("captured failure"));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "printf corrected"
    );
    assert!(!Path::new(&session).exists());
    broker.join().unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls[0]["request"]["context"]["last_failure"],
        json!({"command":"broken command","exit_code":19,"stderr":"captured failure"})
    );
    assert_eq!(
        calls
            .iter()
            .map(|v| v["op"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["submit", "wait", "release"]
    );
}
#[test]
fn generation_uses_broker_and_never_executes_inserted_command() {
    let root = private();
    let marker = root.path().join("must-not-exist");
    let model_command = format!("touch {}", marker.display());
    let (calls, broker) = fake_broker(
        root.path(),
        json!({"suggestions":[{"command":model_command,"description":"Create marker","destructive":false}]}),
    );
    let output = wait(
        command()
            .args(["suggest", "--mode", "how", "--", "create marker"])
            .env("XDG_RUNTIME_DIR", root.path())
            .spawn()
            .unwrap(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        model_command
    );
    assert!(!marker.exists());
    broker.join().unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0]["request"]["consumer"], "shell-ai");
    assert_eq!(calls[0]["request"]["class"], "interactive");
    assert!(calls[0]["request"].get("tools").is_none());
    assert!(!calls[0]["request"]["context"]
        .to_string()
        .contains("API_SECRET"));
}
#[test]
fn multiple_choices_use_safe_explicit_fzf_selection() {
    let root = private();
    let fzf = executable(
        root.path(),
        "fzf",
        r#"#!/bin/sh
test -z "$FZF_DEFAULT_OPTS_FILE" || exit 9
test -z "$FZF_DEFAULT_COMMAND" || exit 9
case "$FZF_DEFAULT_OPTS" in *execute*) exit 9;; esac
sed -n '2p'
"#,
    );
    let (_, broker) = fake_broker(
        root.path(),
        json!({"suggestions":[{"command":"jj status","description":"Inspect status","destructive":false},{"command":"rm old.log","description":"Remove old log","destructive":false}]}),
    );
    let output = wait(
        command()
            .args(["suggest", "--mode", "how", "--", "choose"])
            .env("XDG_RUNTIME_DIR", root.path())
            .env("SEELE_SHELL_AI_FZF", fzf)
            .env(
                "FZF_DEFAULT_OPTS",
                "--bind=start:execute(touch /tmp/unsafe-picker)",
            )
            .env("FZF_DEFAULT_OPTS_FILE", "/untrusted")
            .env("FZF_DEFAULT_COMMAND", "touch /tmp/unsafe-picker")
            .spawn()
            .unwrap(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("# DESTRUCTIVE"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("rm old.log"));
    broker.join().unwrap();
}
#[test]
fn wrapper_forwards_termination_even_after_child_closes_stderr() {
    let root = private();
    let script = executable(
        root.path(),
        "fish",
        r#"#!/bin/sh
printf '%s' "$SEELE_SHELL_AI_SESSION" > "$SEELE_TEST_SESSION"
exec 2>/dev/null
exec sleep 30
"#,
    );
    let child = command()
        .args(["capture", "--fish"])
        .arg(script)
        .env("XDG_RUNTIME_DIR", root.path())
        .env("SEELE_TEST_SESSION", root.path().join("session"))
        .spawn()
        .unwrap();
    await_file(&root.path().join("session"));
    let session = fs::read_to_string(root.path().join("session")).unwrap();
    thread::sleep(Duration::from_millis(50));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let start = Instant::now();
    let output = wait(child);
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(output.status.code(), Some(143));
    assert!(!Path::new(&session).exists());
}
#[test]
fn failed_spawn_and_invalid_or_symlinked_sessions_do_not_leave_capture_artifacts() {
    let root = private();
    let output = wait(
        command()
            .args(["capture", "--fish", "/nonexistent/fish"])
            .env("XDG_RUNTIME_DIR", root.path())
            .spawn()
            .unwrap(),
    );
    assert!(!output.status.success());
    assert_eq!(
        fs::read_dir(root.path().join("seele-shell-ai"))
            .unwrap()
            .count(),
        0
    );
    let external = private();
    let sessionroot = root.path().join("seele-shell-ai");
    std::os::unix::fs::symlink(external.path(), sessionroot.join("session-link")).unwrap();
    let output = wait(
        command()
            .arg("begin")
            .env("XDG_RUNTIME_DIR", root.path())
            .env("SEELE_SHELL_AI_SESSION", sessionroot.join("session-link"))
            .spawn()
            .unwrap(),
    );
    assert!(!output.status.success());
    let output = wait(
        command()
            .args(["suggest", "--mode", "debug"])
            .env_remove("SEELE_SHELL_AI_SESSION")
            .env("XDG_RUNTIME_DIR", root.path())
            .spawn()
            .unwrap(),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not capturing"));
}

#[test]
fn real_fzf_paints_and_reads_the_foreground_terminal() {
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    let Some(fzf) = std::env::var_os("SEELE_TEST_FZF") else {
        return;
    };
    let root = private();
    let (_, broker) = fake_broker(
        root.path(),
        json!({"suggestions":[
        {"command":"jj status","description":"Inspect status","destructive":false},
        {"command":"jj diff","description":"Inspect changes","destructive":false}]}),
    );
    let (mut master, mut slave) = (0, 0);
    let size = libc::winsize {
        ws_row: 24,
        ws_col: 100,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        },
        0
    );
    let mut master = unsafe { fs::File::from_raw_fd(master) };
    let slave = unsafe { fs::File::from_raw_fd(slave) };
    assert_eq!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
        0
    );
    let mut command = Command::new(BINARY);
    command
        .args(["suggest", "--mode", "how", "--", "inspect checkout"])
        .env("XDG_RUNTIME_DIR", root.path())
        .env("SEELE_SHELL_AI_FZF", fzf)
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave))
        .stdout(Stdio::piped());
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut display = Vec::new();
    let mut selected = false;
    loop {
        let mut bytes = [0u8; 8192];
        match master.read(&mut bytes) {
            Ok(count) if count > 0 => {
                if bytes[..count].windows(4).any(|part| part == b"\x1b[6n") {
                    master.write_all(b"\x1b[1;1R").unwrap();
                }
                display.extend_from_slice(&bytes[..count]);
                if !selected && String::from_utf8_lossy(&display).contains("command >") {
                    master.write_all(b"\x0e\r").unwrap();
                    selected = true;
                }
            }
            Ok(_) => (),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == Some(libc::EIO) => {}
            Err(error) => panic!("{error}"),
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            panic!(
                "real picker did not render/respond: {}",
                String::from_utf8_lossy(&display)
            );
        }
        thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    broker.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&display)
    );
    assert!(selected);
    assert!(String::from_utf8_lossy(&display).contains("Choose a command to insert"));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "jj diff");
}
