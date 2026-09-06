use dbus::{
    blocking::Connection,
    channel::{Channel, Sender},
    Message,
};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Work(PathBuf);
impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn executable(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join(name))
        .find(|path| path.is_file())
        .unwrap()
}
fn stub(work: &Path, name: &str, body: &str) {
    let path = work.join("bin").join(name);
    fs::write(
        &path,
        format!(
            "#!{}\nprintf '%s\\n' '{name}' >> \"$MOCK_LOG\"\n{body}\n",
            executable("sh").display()
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn bus(address: &str) -> ChildGuard {
    // Do not load the host's session.conf or activation directories. They are
    // absent in a Nix sandbox and are unrelated to this private fixture bus.
    let config = Path::new(address.strip_prefix("unix:path=").unwrap()).with_extension("conf");
    fs::write(
        &config,
        r#"<busconfig>
      <type>session</type><auth>EXTERNAL</auth>
      <listen>unix:tmpdir=/tmp</listen>
      <policy context="default">
        <allow user="*"/><allow own="*"/>
        <allow send_destination="*"/><allow receive_sender="*"/>
      </policy>
    </busconfig>"#,
    )
    .unwrap();
    let mut child = Command::new("dbus-daemon")
        .args([
            "--config-file",
            config.to_str().unwrap(),
            "--nofork",
            "--print-address=1",
            "--address",
            address,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert!(!ready.is_empty(), "private bus did not start");
    ChildGuard(child)
}
fn owner(address: &str) -> Vec<Connection> {
    [
        "org.bluez",
        "org.freedesktop.NetworkManager",
        "org.freedesktop.Notifications",
    ]
    .iter()
    .map(|name| {
        let mut channel = Channel::open_private(address).unwrap();
        channel.register().unwrap();
        let connection = Connection::from(channel);
        connection.request_name(*name, false, true, false).unwrap();
        connection
    })
    .collect()
}
fn signal(connection: &Connection) {
    connection
        .send(
            Message::new_signal(
                "/fixture",
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
            )
            .unwrap()
            .append3("fixture", dbus::arg::PropMap::new(), vec!["changed"]),
        )
        .unwrap();
}
fn wait_for(
    receiver: &mpsc::Receiver<Value>,
    state: &mut Value,
    predicate: impl Fn(&Value) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate(state) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let update = receiver
            .recv_timeout(remaining)
            .expect("status update timed out");
        for (key, value) in update.as_object().unwrap() {
            state[key] = value.clone();
        }
    }
}
fn calls(work: &Path, name: &str) -> usize {
    fs::read_to_string(work.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter(|line| *line == name)
        .count()
}
fn bluez(work: &Path, powered: bool) {
    fs::write(
        work.join("bluez"),
        json!({"data":[{"/org/bluez/hci0":{"org.bluez.Adapter1":{
            "Powered":{"data":powered},"Discoverable":{"data":false}
        }}}]})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn subscriptions_bootstrap_reconnect_and_refresh_only_the_requested_source() {
    let work = Work(std::env::temp_dir().join(format!("seele-live-test-{}", std::process::id())));
    fs::create_dir_all(work.0.join("bin")).unwrap();
    let address = format!("unix:path={}/bus", work.0.display());
    let mut daemon = bus(&address);
    let mut connection = owner(&address);
    for name in ["tailscale", "ip"] {
        stub(&work.0, name, "printf '{}\\n'");
    }
    for name in ["openlogi", "v4l2-ctl"] {
        stub(&work.0, name, "exit 0");
    }
    stub(&work.0, "systemctl", "exit 1");
    stub(&work.0, "voxtype", "printf 'idle\\n'");
    stub(
        &work.0,
        "nmcli",
        r#"case "$*" in
      *TYPE,NAME*) printf '802-3-ethernet:Fixture\n' ;;
      *'TYPE device'*) printf 'wifi\n' ;;
      *WIFI*) printf 'enabled\n' ;;
      *) printf 'full\n' ;;
    esac"#,
    );
    stub(
        &work.0,
        "busctl",
        "test ! -e \"$MOCK_DIR/offline\" || exit 1; cat \"$MOCK_DIR/bluez\"",
    );
    stub(
        &work.0,
        "makoctl",
        r#"case "$1" in
      list) cat "$MOCK_DIR/notifications" ;;
      history) printf '[]\n' ;;
      mode) printf 'default\n' ;;
    esac"#,
    );
    stub(&work.0, "wpctl", "printf 'Volume: 0.50\\n'");
    stub(
        &work.0,
        "pw-dump",
        "printf '%s\\n' \"$$\" > \"$MOCK_DIR/pipewire-pid\"; exec cat \"$MOCK_DIR/graph\"",
    );
    assert!(Command::new("mkfifo")
        .arg(work.0.join("graph"))
        .status()
        .unwrap()
        .success());
    bluez(&work.0, true);
    fs::write(work.0.join("notifications"), "[]").unwrap();
    let mut controller = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_seele-tools"))
            .args(["control", "watch-status"])
            .env("DBUS_SYSTEM_BUS_ADDRESS", &address)
            .env("DBUS_SESSION_BUS_ADDRESS", &address)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    work.0.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("MOCK_LOG", work.0.join("calls"))
            .env("MOCK_DIR", &work.0)
            .env("XDG_RUNTIME_DIR", work.0.join("runtime"))
            .env("XDG_STATE_HOME", work.0.join("state"))
            .env("XDG_CONFIG_HOME", work.0.join("config"))
            .env("SEELE_NOTHING_HEADPHONES_DISABLE_DAEMON", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = controller.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let value = serde_json::from_str(&line).unwrap();
            if sender.send(value).is_err() {
                break;
            }
        }
    });
    // RDWR avoids a blocking open if the reader is still starting.
    let mut graph = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(work.0.join("graph"))
        .unwrap();
    writeln!(
        graph,
        "{}",
        json!([{"id":1,"type":"PipeWire:Interface:Node","info":{
            "props":{"media.class":"Audio/Sink","node.name":"speaker","node.description":"Speaker"},
            "params":{"Props":[{"mute":false}]},"state":"idle"
        }}])
    )
    .unwrap();
    let mut state = json!({});
    wait_for(&receiver, &mut state, |s| {
        s["volume"] == 50
            && s["bluetoothPowered"] == true
            && s["connection"] == "Fixture"
            && s.get("notifications").is_some()
            && s.get("tailscale").is_some()
    });
    // No timer launches network, BlueZ, volume, or graph probes while idle.
    let before = [
        calls(&work.0, "nmcli"),
        calls(&work.0, "busctl"),
        calls(&work.0, "wpctl"),
        calls(&work.0, "pw-dump"),
        calls(&work.0, "makoctl"),
    ];
    std::thread::sleep(Duration::from_millis(5300));
    assert_eq!(
        before,
        [
            calls(&work.0, "nmcli"),
            calls(&work.0, "busctl"),
            calls(&work.0, "wpctl"),
            calls(&work.0, "pw-dump"),
            calls(&work.0, "makoctl")
        ]
    );
    fs::write(
        work.0.join("notifications"),
        "[{\"id\":1,\"summary\":\"Fixture\"}]",
    )
    .unwrap();
    signal(&connection[2]);
    wait_for(&receiver, &mut state, |s| s["notifications"]["count"] == 1);
    assert_eq!(before[0], calls(&work.0, "nmcli"));
    assert_eq!(before[1], calls(&work.0, "busctl"));
    writeln!(controller.0.stdin.as_mut().unwrap(), "bluetooth").unwrap();
    loop {
        let update = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        if update.get("bluetoothPowered") == Some(&json!(true)) {
            break;
        }
    }
    // A real property signal refreshes the device, and owner loss/reappearance
    // refreshes even without a PropertiesChanged event.
    bluez(&work.0, false);
    signal(&connection[0]);
    wait_for(&receiver, &mut state, |s| s["bluetoothPowered"] == false);
    bluez(&work.0, true);
    connection[0].release_name("org.bluez").unwrap();
    wait_for(&receiver, &mut state, |s| s["bluetoothPowered"] == true);
    connection[0]
        .request_name("org.bluez", false, true, false)
        .unwrap();
    // Activity-only PipeWire deltas do not query volumes again.
    let volumes = calls(&work.0, "wpctl");
    writeln!(
        graph,
        "{}",
        json!([{"id":2,"info":{"props":{"media.class":"Stream/Input/Audio"},"state":"running"}}])
    )
    .unwrap();
    wait_for(&receiver, &mut state, |s| s["microphoneActive"] == true);
    assert_eq!(volumes, calls(&work.0, "wpctl"));
    writeln!(graph, "{}", json!([{"id":2,"info":null}])).unwrap();
    wait_for(&receiver, &mut state, |s| s["microphoneActive"] == false);
    // Explicit audio acknowledgements are sent even when the value is equal.
    writeln!(controller.0.stdin.as_mut().unwrap(), "audio").unwrap();
    loop {
        let update = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        if update.get("volume") == Some(&json!(50)) {
            break;
        }
    }
    let old_reader: u32 = fs::read_to_string(work.0.join("pipewire-pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    unsafe {
        libc::kill(old_reader as i32, libc::SIGTERM);
    }
    wait_for(&receiver, &mut state, |s| s["audioDevices"] == json!([]));
    let deadline = Instant::now() + Duration::from_secs(5);
    while calls(&work.0, "pw-dump") < 2 {
        assert!(
            Instant::now() < deadline,
            "PipeWire monitor did not restart"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    writeln!(
        graph,
        "{}",
        json!([{"id":1,"info":{"props":{
            "media.class":"Audio/Sink","node.name":"new","node.description":"Reconnected"
        }}}])
    )
    .unwrap();
    wait_for(&receiver, &mut state, |s| {
        s["audioDevices"][0]["name"] == "Reconnected"
    });
    fs::write(work.0.join("offline"), "").unwrap();
    daemon.0.kill().unwrap();
    daemon.0.wait().unwrap();
    wait_for(&receiver, &mut state, |s| s["bluetoothAvailable"] == false);
    daemon = bus(&address);
    connection = owner(&address);
    fs::remove_file(work.0.join("offline")).unwrap();
    signal(&connection[0]);
    wait_for(&receiver, &mut state, |s| s["bluetoothAvailable"] == true);
    writeln!(controller.0.stdin.as_mut().unwrap(), "all").unwrap();
    let mut acknowledged = json!({});
    wait_for(&receiver, &mut acknowledged, |s| {
        [
            "volume",
            "bluetoothPowered",
            "connection",
            "notifications",
            "tailscale",
        ]
        .iter()
        .all(|key| s.get(key).is_some())
    });
    let pipewire_pid: u32 = fs::read_to_string(work.0.join("pipewire-pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    drop(controller.0.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "controller did not exit on stdin EOF"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    loop {
        let status = fs::read_to_string(format!("/proc/{pipewire_pid}/stat")).unwrap_or_default();
        if status.is_empty()
            || status
                .rsplit_once(") ")
                .is_some_and(|(_, fields)| fields.starts_with('Z'))
        {
            break;
        }
        assert!(Instant::now() < deadline, "pw-dump survived its controller");
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(connection);
    drop(daemon);
}
