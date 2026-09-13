//! Drives the real daemon and request binary over a private systemd-style
//! inherited socket. No host service, desktop command or model is invoked.
use seele_maintenance::{LIMIT, SNAPSHOT_LIMIT};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    process::{Child, Command, Stdio},
    sync::atomic::AtomicUsize,
    thread,
    time::{Duration, Instant},
};
struct Daemon {
    child: Child,
    root: tempfile::TempDir,
}
impl Drop for Daemon {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Daemon {
    fn launch() -> Self {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = root.path().join("maintenance.sock");
        let listener = UnixListener::bind(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut config = json!({});
        for source in seele_maintenance::model::SOURCES {
            config[source] = json!({"enabled":false});
        }
        config["backups"] = json!({"enabled":true,"items":[]});
        fs::write(root.path().join("config.json"), config.to_string()).unwrap();
        let fd = listener.as_raw_fd();
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "export LISTEN_PID=$$; exec \"$@\"",
                "maintenance-test",
                env!("CARGO_BIN_EXE_seele-maintenance"),
                "serve",
                "--config",
            ])
            .arg(root.path().join("config.json"))
            .arg("--state")
            .arg(root.path().join("inbox.json"))
            .env("LISTEN_FDS", "1")
            .env("XDG_RUNTIME_DIR", root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(fd, 3) < 0 || libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        drop(listener);
        let daemon = Self { child, root };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if daemon.rpc(&json!({"op":"list"}))["ok"] == true {
                break;
            }
            assert!(Instant::now() < deadline, "daemon did not become ready");
            thread::sleep(Duration::from_millis(10));
        }
        daemon
    }
    fn rpc(&self, value: &Value) -> Value {
        seele_runtime::wire::rpc(
            &self.root.path().join("maintenance.sock"),
            value,
            seele_runtime::wire::RpcLimits {
                timeout: Duration::from_secs(2),
                request_bytes: LIMIT,
                response_bytes: SNAPSHOT_LIMIT,
            },
            &AtomicUsize::new(0),
        )
        .unwrap_or(json!({"ok":false}))
    }
}
#[test]
fn real_daemon_socket_client_persistence_and_restart_contract() {
    let mut daemon = Daemon::launch();
    let finding = json!({"key":"fixture","title":"Maintenance fixture","urgency":"informational","lifecycle":"notice","actions":["recheck"],"diagnostic":"private diagnostic"});
    assert_eq!(
        daemon.rpc(&json!({"op":"publish","source":"backups","finding":finding}))["ok"],
        true
    );
    // Initial source startup owns a complete empty snapshot. Repeat the
    // fixture publication until that initial transaction has finished.
    let deadline = Instant::now() + Duration::from_secs(2);
    let snapshot = loop {
        let snapshot = daemon.rpc(&json!({"op":"list"}));
        if snapshot["active"][0]["canAnalyze"] == true {
            break snapshot;
        }
        assert!(Instant::now() < deadline);
        daemon.rpc(&json!({"op":"publish","source":"backups","finding":finding}));
    };
    let row = &snapshot["active"][0];
    assert_eq!(row["canAnalyze"], true);
    assert!(!snapshot.to_string().contains("private diagnostic"));
    assert_eq!(
        daemon.rpc(&json!({"op":"snooze","id":row["id"],"revision":9999,"seconds":60}))["ok"],
        false
    );
    let mut client = Command::new(env!("CARGO_BIN_EXE_seele-maintenance"))
        .arg("request")
        .arg("--socket")
        .arg(daemon.root.path().join("maintenance.sock"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    client
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"op\":\"list\"}\n")
        .unwrap();
    let result = client.wait_with_output().unwrap();
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["active"][0]["id"],
        row["id"]
    );
    assert_eq!(
        daemon.rpc(&json!({"op":"done","id":row["id"],"revision":row["revision"]}))["ok"],
        true
    );
    assert_eq!(
        daemon.rpc(&json!({"op":"list"}))["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let text = fs::read_to_string(daemon.root.path().join("inbox.json")).unwrap();
    assert!(!text.contains("private diagnostic"));
    assert_eq!(
        fs::metadata(daemon.root.path().join("inbox.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    unsafe {
        libc::kill(daemon.child.id() as i32, libc::SIGTERM);
    }
    assert!(daemon.child.wait().unwrap().success());
}
#[test]
fn oversized_malformed_and_slow_clients_do_not_block_other_requests() {
    let daemon = Daemon::launch();
    let path = daemon.root.path().join("maintenance.sock");
    let mut malformed = UnixStream::connect(&path).unwrap();
    malformed
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    malformed.write_all(b"[]\n").unwrap();
    let mut reply = String::new();
    malformed.read_to_string(&mut reply).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["ok"], false);
    let mut oversized = UnixStream::connect(&path).unwrap();
    oversized
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let _ = oversized.write_all(&vec![b'x'; LIMIT + 1]);
    oversized
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reply = String::new();
    let _ = oversized.read_to_string(&mut reply);
    assert!(!reply.is_empty());
    let mut slow = UnixStream::connect(&path).unwrap();
    slow.write_all(b"{").unwrap();
    let start = Instant::now();
    assert_eq!(daemon.rpc(&json!({"op":"list"}))["ok"], true);
    assert!(start.elapsed() < Duration::from_secs(1));
}
#[test]
fn daemon_refuses_unmanaged_socket_and_client_rejects_oversized_stdin() {
    let output = Command::new(env!("CARGO_BIN_EXE_seele-maintenance"))
        .arg("serve")
        .env_remove("LISTEN_PID")
        .env_remove("LISTEN_FDS")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("config.json"));
    let mut client = Command::new(env!("CARGO_BIN_EXE_seele-maintenance"))
        .arg("request")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    client
        .stdin
        .take()
        .unwrap()
        .write_all(&vec![b'x'; LIMIT + 1])
        .unwrap();
    let output = client.wait_with_output().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"],
        "Invalid request"
    );
}
