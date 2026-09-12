//! Native launcher lifecycle tests. All executables and state are synthetic;
//! no compositor, accounts, desktop session or application is accessed.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct Fixture {
    root: tempfile::TempDir,
    path: String,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = std::env::var("PATH").unwrap();
        let shell = std::env::split_paths(&path)
            .map(|directory| directory.join("sh"))
            .find(|file| file.is_file())
            .unwrap();
        let scripts = [
            (
                "getent",
                "printf 'fixture:x:1:1:Fixture Person:/private:/fixture\\n'\n",
            ),
            (
                "hyprctl",
                "printf '%s\\n' \"$*\" >\"$FIXTURE_ROOT/hyprctl\"\n",
            ),
            (
                "systemctl",
                r#"
printf '%s\n' "$*" >"$FIXTURE_ROOT/systemctl"
printf '%s\n' "$$" >"$FIXTURE_ROOT/foreground"
if [ "$FIXTURE_MODE" = restart-wait ]; then exec sleep 30; fi
"#,
            ),
            (
                "quickshell",
                r#"
printf '%s\n' "$*" >>"$FIXTURE_ROOT/argv"
if [ "$1" = ipc ]; then
  case "$FIXTURE_MODE" in
    notes-live) exit 0 ;;
    notes-new) exit 1 ;;
  esac
  if [ -f "$FIXTURE_ROOT/owner" ] && [ "$FIXTURE_MODE" = silent ]; then sleep 30; fi
  if [ -f "$FIXTURE_ROOT/state" ]; then cat "$FIXTURE_ROOT/state"; else printf 'unlocked\n'; fi
  exit 0
fi
if [ "$1" = kill ]; then exit 0; fi
if [ "$2" = -d ]; then
  if [ "$FIXTURE_MODE" = secure ]; then
    (sleep .15; printf 'secure\n' >"$FIXTURE_ROOT/state"; exec sleep 30) &
  else
    sleep 30 &
  fi
  printf '%s\n' "$!" >"$FIXTURE_ROOT/owner"
  if [ "$FIXTURE_MODE" = failure ]; then exit 7; fi
  exit 0
fi
printf '%s\n' "$$" >"$FIXTURE_ROOT/foreground"
if [ "$FIXTURE_MODE" = greeter-wait ]; then exec sleep 30; fi
exit 17
"#,
            ),
        ];
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, script) in scripts {
            let file = bin.join(name);
            fs::write(&file, format!("#!{}\n{script}", shell.display())).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            path: format!("{}:{path}", bin.display()),
            root,
        }
    }
    fn command(&self, binary: &str, mode: &str) -> Command {
        let mut command = Command::new(binary);
        command
            .env_clear()
            .env("PATH", &self.path)
            .env("SEELE_QUICKSHELL", self.root.path().join("bin/quickshell"))
            .env("SEELE_HYPRCTL", self.root.path().join("bin/hyprctl"))
            .env("SEELE_CONFIG", self.root.path().join("config with spaces"))
            .env("FIXTURE_ROOT", self.root.path())
            .env("FIXTURE_MODE", mode)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn file(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    fn pid(&self, name: &str) -> i32 {
        fs::read_to_string(self.file(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(pid) = fs::read_to_string(self.file("owner")) {
            if let Ok(pid) = pid.trim().parse::<i32>() {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
}
fn eventually(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !condition() {
        assert!(Instant::now() < deadline, "fixture deadline exceeded");
        thread::sleep(Duration::from_millis(10));
    }
}
fn wait(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(9);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("native launcher exceeded deadline: {output:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn alive(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| !stat.contains(") Z "))
}
#[test]
fn lock_handoff_retains_daemon_and_requires_secure_confirmation() {
    let fixture = Fixture::new();
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-lock-run"), "secure")
            .spawn()
            .unwrap(),
    );
    assert!(output.status.success(), "{output:?}");
    assert!(alive(fixture.pid("owner")), "successful daemon was killed");
    assert_eq!(
        fs::read_to_string(fixture.file("state")).unwrap().trim(),
        "secure"
    );
    let before = fs::read_to_string(fixture.file("argv"))
        .unwrap()
        .matches("-n -d")
        .count();
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-lock-run"), "secure")
            .spawn()
            .unwrap(),
    );
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(fixture.file("argv"))
            .unwrap()
            .matches("-n -d")
            .count(),
        before
    );
}
#[test]
fn lock_failure_cleans_failed_daemon_but_missing_ack_never_unlocks() {
    let fixture = Fixture::new();
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-lock-run"), "failure")
            .spawn()
            .unwrap(),
    );
    assert!(!output.status.success());
    eventually(|| !alive(fixture.pid("owner")));
    fs::remove_file(fixture.file("owner")).unwrap();
    let before = Instant::now();
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-lock-run"), "silent")
            .spawn()
            .unwrap(),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("did not confirm"));
    assert!(before.elapsed() < Duration::from_secs(7));
    assert!(
        alive(fixture.pid("owner")),
        "failed acknowledgement must not unlock"
    );
}
#[test]
fn lock_confirmation_cancellation_keeps_detached_owner() {
    let fixture = Fixture::new();
    let child = fixture
        .command(env!("CARGO_BIN_EXE_seele-lock-run"), "silent")
        .spawn()
        .unwrap();
    eventually(|| fixture.file("owner").exists());
    // Parent daemonization has completed before interrupting confirmation.
    thread::sleep(Duration::from_millis(100));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    assert!(!wait(child).status.success());
    assert!(alive(fixture.pid("owner")));
}
#[test]
fn notes_opens_existing_instance_or_execs_exact_native_launcher() {
    let fixture = Fixture::new();
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-notes-run"), "notes-live")
            .spawn()
            .unwrap(),
    );
    assert!(output.status.success());
    assert!(!fixture.file("foreground").exists());
    let child = fixture
        .command(env!("CARGO_BIN_EXE_seele-notes-run"), "notes-new")
        .spawn()
        .unwrap();
    let pid = child.id();
    assert_eq!(wait(child).status.code(), Some(17));
    assert_eq!(
        fixture.pid("foreground") as u32,
        pid,
        "Notes must exec instead of adding a resident wrapper"
    );
}
#[test]
fn greeter_preserves_exit_and_cancels_owned_foreground_before_compositor_cleanup() {
    let fixture = Fixture::new();
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-greeter-run"), "greeter-exit")
            .spawn()
            .unwrap(),
    );
    assert_eq!(output.status.code(), Some(17));
    assert_eq!(
        fs::read_to_string(fixture.file("hyprctl")).unwrap().trim(),
        "dispatch hl.dsp.exit()"
    );
    fs::remove_file(fixture.file("foreground")).unwrap();
    fs::remove_file(fixture.file("hyprctl")).unwrap();
    let child = fixture
        .command(env!("CARGO_BIN_EXE_seele-greeter-run"), "greeter-wait")
        .spawn()
        .unwrap();
    eventually(|| fixture.file("foreground").exists());
    let foreground = fixture.pid("foreground");
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    assert_eq!(wait(child).status.code(), Some(143));
    eventually(|| !alive(foreground));
    assert!(fixture.file("hyprctl").exists());
}

#[test]
fn health_restart_validates_unit_and_cancels_owned_client() {
    let fixture = Fixture::new();
    for unit in [
        "--all.service",
        "bad;true.service",
        "../other.service",
        "fixture.socket",
        ".service",
    ] {
        let output = wait(
            fixture
                .command(env!("CARGO_BIN_EXE_seele-control"), "restart")
                .args(["restart-user-service", unit])
                .spawn()
                .unwrap(),
        );
        assert!(!output.status.success());
        assert!(!fixture.file("systemctl").exists());
    }
    let output = wait(
        fixture
            .command(env!("CARGO_BIN_EXE_seele-control"), "restart")
            .args(["restart-user-service", "fixture@123.service"])
            .spawn()
            .unwrap(),
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.file("systemctl"))
            .unwrap()
            .trim(),
        "--user restart -- fixture@123.service"
    );
    fs::remove_file(fixture.file("foreground")).unwrap();
    let child = fixture
        .command(env!("CARGO_BIN_EXE_seele-control"), "restart-wait")
        .args(["restart-user-service", "fixture.service"])
        .spawn()
        .unwrap();
    eventually(|| fixture.file("foreground").exists());
    let pid = fixture.pid("foreground");
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    assert!(!wait(child).status.success());
    eventually(|| !alive(pid));
}
