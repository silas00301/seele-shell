//! Real native agent on a private fake BlueZ bus; no host Bluetooth access.
use dbus::{
    arg::Variant,
    blocking::SyncConnection,
    channel::{MatchingReceiver, Sender},
    message::{MatchRule, MessageType},
    Message, Path as BusPath,
};
use dbus_crossroads::Crossroads;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn executable(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("fixture needs {name}"))
}
fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "fixture timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn private_bus_authorization_cancel_release_owner_and_sender() {
    let temporary = tempfile::tempdir().unwrap();
    let runtime = temporary.path().join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let bin = temporary.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for name in ["seele-shellctl", "bluetoothctl"] {
        let path = bin.join(name);
        fs::write(&path, format!("#!{}\nexit 0\n", executable("sh").display())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut bus = ChildGuard(
        Command::new(executable("dbus-daemon"))
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let address = address.trim().to_owned();
    // This integration-test process has one test; never points at the host bus.
    std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", &address);
    let connection = Arc::new(SyncConnection::new_system().unwrap());
    connection
        .request_name("org.bluez", false, true, false)
        .unwrap();
    let registered = Arc::new(Mutex::new(None::<String>));
    let trusted = Arc::new(AtomicUsize::new(0));
    let replies = Arc::new(Mutex::new(Vec::<Message>::new()));
    let mut crossroads = Crossroads::new();
    let registration = registered.clone();
    let manager = crossroads.register("org.bluez.AgentManager1", move |builder| {
        let registration = registration.clone();
        builder.method(
            "RegisterAgent",
            ("path", "capability"),
            (),
            move |ctx, _: &mut (), (_path, capability): (BusPath<'static>, String)| {
                assert_eq!(capability, "KeyboardDisplay");
                *registration.lock().unwrap() = Some(ctx.message().sender().unwrap().to_string());
                Ok(())
            },
        );
        builder.method(
            "RequestDefaultAgent",
            ("path",),
            (),
            |_, _: &mut (), (_path,): (BusPath<'static>,)| Ok(()),
        );
        builder.method(
            "UnregisterAgent",
            ("path",),
            (),
            |_, _: &mut (), (_path,): (BusPath<'static>,)| Ok(()),
        );
    });
    crossroads.insert("/org/bluez", &[manager], ());
    let trust = trusted.clone();
    let properties = crossroads.register("org.freedesktop.DBus.Properties", move |builder| {
        builder.method(
            "Get",
            ("interface", "name"),
            ("value",),
            |_, _: &mut (), (_interface, name): (String, String)| {
                Ok((Variant(
                    if name == "Icon" {
                        "phone"
                    } else {
                        "Fixture device"
                    }
                    .to_owned(),
                ),))
            },
        );
        let trust = trust.clone();
        builder.method(
            "Set",
            ("interface", "name", "value"),
            (),
            move |_, _: &mut (), (_interface, name, value): (String, String, Variant<bool>)| {
                assert_eq!(name, "Trusted");
                assert!(value.0);
                trust.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
    });
    crossroads.insert("/org/bluez/hci0/dev_fixture", &[properties], ());
    let crossroads = Mutex::new(crossroads);
    connection.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            let _ = crossroads
                .lock()
                .unwrap()
                .handle_message(message, connection);
            true
        }),
    );
    for kind in [MessageType::MethodReturn, MessageType::Error] {
        let replies = replies.clone();
        let mut rule = MatchRule::new();
        rule.msg_type = Some(kind);
        connection.start_receive(
            rule,
            Box::new(move |message, _| {
                replies.lock().unwrap().push(message);
                true
            }),
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker_bus = connection.clone();
    let worker = thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            if worker_bus.process(Duration::from_millis(10)).is_err() {
                break;
            }
        }
    });
    let launch = || {
        *registered.lock().unwrap() = None;
        let child = ChildGuard(
            Command::new(env!("CARGO_BIN_EXE_seele-bt-agent"))
                .env("DBUS_SYSTEM_BUS_ADDRESS", &address)
                .env("XDG_RUNTIME_DIR", &runtime)
                .env(
                    "PATH",
                    std::env::join_paths(
                        std::iter::once(bin.clone())
                            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                    )
                    .unwrap(),
                )
                .env("SEELE_BLUETOOTH_PAIRING_WINDOW", "30")
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        until(|| registered.lock().unwrap().is_some());
        child
    };
    let request_path = runtime.join("seele-shell/bluetooth-pairing.json");
    let answer_path = runtime.join("seele-shell/bluetooth-pairing.answer");
    let send = |method: &str, with_device: bool| {
        let destination = registered.lock().unwrap().clone().unwrap();
        let mut message = Message::new_method_call(
            destination,
            "/org/seele/bluetooth/agent",
            "org.bluez.Agent1",
            method,
        )
        .unwrap();
        if with_device {
            message = message.append1(BusPath::new("/org/bluez/hci0/dev_fixture").unwrap());
        }
        connection.send(message).unwrap()
    };
    let reply = |serial: u32| {
        until(|| {
            replies
                .lock()
                .unwrap()
                .iter()
                .any(|message| message.get_reply_serial() == Some(serial))
        });
        let mut replies = replies.lock().unwrap();
        let index = replies
            .iter()
            .position(|message| message.get_reply_serial() == Some(serial))
            .unwrap();
        replies.remove(index)
    };
    let token = || {
        until(|| request_path.is_file());
        serde_json::from_slice::<serde_json::Value>(&fs::read(&request_path).unwrap()).unwrap()
            ["token"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let mut child = launch();
    let intruder = SyncConnection::new_system().unwrap();
    let denied: Result<(), _> = intruder
        .with_proxy(
            registered.lock().unwrap().clone().unwrap(),
            "/org/seele/bluetooth/agent",
            Duration::from_secs(2),
        )
        .method_call(
            "org.bluez.Agent1",
            "RequestAuthorization",
            (BusPath::new("/org/bluez/hci0/dev_fixture").unwrap(),),
        );
    assert_eq!(
        denied.unwrap_err().name(),
        Some("org.freedesktop.DBus.Error.AccessDenied")
    );
    let first = send("RequestAuthorization", true);
    let old_token = token();
    let duplicate = send("RequestAuthorization", true);
    assert_eq!(reply(duplicate).msg_type(), MessageType::Error);
    let cancelled = send("Cancel", false);
    assert_eq!(reply(cancelled).msg_type(), MessageType::MethodReturn);
    assert_eq!(reply(first).msg_type(), MessageType::Error);
    assert!(!request_path.exists());
    assert_eq!(trusted.load(Ordering::SeqCst), 0);
    // An old accepted answer must never authorize the next generation.
    seele_runtime::fs::atomic_write(&answer_path, format!("{old_token} accept \n").as_bytes())
        .unwrap();
    let next = send("RequestPasskey", true);
    let current = token();
    assert_ne!(current, old_token);
    let queued_cancel = send("Cancel", false);
    seele_runtime::fs::atomic_write(
        &answer_path,
        format!("{current} accept 123456\n").as_bytes(),
    )
    .unwrap();
    assert_eq!(reply(queued_cancel).msg_type(), MessageType::MethodReturn);
    assert_eq!(reply(next).msg_type(), MessageType::Error);
    assert_eq!(trusted.load(Ordering::SeqCst), 0);
    let accepted = send("RequestPasskey", true);
    let current = token();
    let mut answer = Command::new(env!("CARGO_BIN_EXE_seele-control"))
        .arg("bluetooth-pairing-answer-stdin")
        .env("XDG_RUNTIME_DIR", &runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    answer
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::to_string(
                &serde_json::json!({"token":current,"verdict":"accept","value":"000123"}),
            )
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
    assert!(answer.wait().unwrap().success());
    let accepted = reply(accepted);
    assert_eq!(accepted.msg_type(), MessageType::MethodReturn);
    assert_eq!(accepted.read1::<u32>().unwrap(), 123);
    assert_eq!(trusted.load(Ordering::SeqCst), 1);
    send("RequestAuthorization", true);
    token();
    let released = send("Release", false);
    assert_eq!(reply(released).msg_type(), MessageType::MethodReturn);
    until(|| child.0.try_wait().unwrap().is_some());
    assert!(!request_path.exists());
    assert_eq!(trusted.load(Ordering::SeqCst), 1);
    let mut child = launch();
    send("RequestAuthorization", true);
    token();
    let release = Message::new_method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReleaseName",
    )
    .unwrap()
    .append1("org.bluez");
    let release = connection.send(release).unwrap();
    assert_eq!(reply(release).msg_type(), MessageType::MethodReturn);
    until(|| child.0.try_wait().unwrap().is_some());
    assert!(!request_path.exists());
    assert_eq!(trusted.load(Ordering::SeqCst), 1);
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    assert!(Path::new(&runtime).is_dir());
}
