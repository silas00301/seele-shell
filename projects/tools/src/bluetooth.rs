use crate::command::{atomic_write, runtime_home, status};
use crate::Result;
use dbus::arg::{RefArg, Variant};
use dbus::blocking::Connection;
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use dbus::Path as DbusPath;
use dbus_crossroads::{Crossroads, MethodErr};
use serde_json::json;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const AGENT_PATH: &str = "/org/seele/bluetooth/agent";
const CAPABILITY: &str = "KeyboardDisplay";

fn files() -> (PathBuf, PathBuf) {
    let directory = runtime_home().join("seele-shell");
    (
        directory.join("bluetooth-pairing.json"),
        directory.join("bluetooth-pairing.answer"),
    )
}

fn clear_files() {
    let (request, answer) = files();
    let _ = fs::remove_file(request);
    let _ = fs::remove_file(answer);
}
fn shell(arguments: &[&str]) {
    let mut values = vec!["-q"];
    values.extend_from_slice(arguments);
    status("seele-shellctl", values);
}

struct Agent;
impl Agent {
    fn property(path: &DbusPath<'static>, name: &str) -> String {
        let Ok(connection) = Connection::new_system() else {
            return String::new();
        };
        let proxy = connection.with_proxy("org.bluez", path.clone(), Duration::from_secs(5));
        let result: std::result::Result<(Variant<Box<dyn RefArg>>,), _> = proxy.method_call(
            "org.freedesktop.DBus.Properties",
            "Get",
            ("org.bluez.Device1", name),
        );
        result
            .ok()
            .and_then(|(value,)| value.0.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
    fn name(path: &DbusPath<'static>) -> String {
        ["Alias", "Name", "Address"]
            .iter()
            .map(|name| Self::property(path, name))
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown device".into())
    }
    fn trust(path: &DbusPath<'static>) {
        if let Ok(connection) = Connection::new_system() {
            let proxy = connection.with_proxy("org.bluez", path.clone(), Duration::from_secs(5));
            let _: std::result::Result<(), _> = proxy.method_call(
                "org.freedesktop.DBus.Properties",
                "Set",
                ("org.bluez.Device1", "Trusted", Variant(true)),
            );
        }
    }
    fn token() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{:016x}", nanos ^ u128::from(std::process::id()))
    }
    fn publish(
        kind: &str,
        path: &DbusPath<'static>,
        passkey: String,
    ) -> std::result::Result<String, MethodErr> {
        let token = Self::token();
        let request = json!({
            "token":token,"kind":kind,"address":Self::property(path,"Address"),"name":Self::name(path),
            "icon":Self::property(path,"Icon"),"passkey":passkey
        });
        let (request_path, answer_path) = files();
        let _ = fs::remove_file(answer_path);
        let encoded =
            serde_json::to_vec(&request).map_err(|error| MethodErr::failed(&error.to_string()))?;
        atomic_write(&request_path, &encoded)
            .map_err(|error| MethodErr::failed(&error.to_string()))?;
        shell(&["bluetooth-pairing", &request.to_string()]);
        Ok(token)
    }
    fn ask(
        kind: &str,
        path: &DbusPath<'static>,
        passkey: Option<u32>,
    ) -> std::result::Result<String, MethodErr> {
        println!("request kind={kind} device={path}");
        let token = Self::publish(kind, path, passkey.map(|value| format!("{value:06}")).unwrap_or_default())?;
        let (_, answer_path) = files();
        for _ in 0..900 {
            if let Ok(answer) = fs::read_to_string(&answer_path) {
                let mut fields = answer.trim().splitn(3, ' ');
                if fields.next() == Some(token.as_str()) {
                    match fields.next() {
                        Some("accept") => {
                            let value = fields.next().unwrap_or("").to_owned();
                            clear_files();
                            shell(&["bluetooth-pairing-dismiss"]);
                            Self::trust(path);
                            println!("settle kind={kind} accepted=true");
                            return Ok(value);
                        }
                        Some("reject") => break,
                        _ => {}
                    }
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        clear_files();
        shell(&["bluetooth-pairing-dismiss"]);
        println!("settle kind={kind} accepted=false");
        Err(("org.bluez.Error.Rejected", "Rejected").into())
    }
}

fn close_window() {
    let restore = env::var("SEELE_BLUETOOTH_DISCOVERABLE_TIMEOUT").unwrap_or_else(|_| "180".into());
    for args in [
        vec!["discoverable", "off"],
        vec!["pairable", "off"],
        vec!["discoverable-timeout", restore.as_str()],
    ] {
        status("bluetoothctl", args);
    }
}

pub fn agent(arguments: &[String]) -> Result {
    if arguments.first().map(String::as_str) == Some("--describe") {
        println!("{{\"capability\":\"{CAPABILITY}\",\"methods\":[\"RequestConfirmation\",\"RequestAuthorization\",\"RequestPasskey\",\"RequestPinCode\",\"DisplayPasskey\"]}}");
        return Ok(());
    }
    let window = env::var("SEELE_BLUETOOTH_PAIRING_WINDOW")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(120);
    let connection = Connection::new_system()?;
    let mut crossroads = Crossroads::new();
    let interface = crossroads.register("org.bluez.Agent1", |builder| {
        builder.method("Release", (), (), |_, _: &mut Agent, ()| {
            clear_files();
            shell(&["bluetooth-pairing-dismiss"]);
            Ok(())
        });
        builder.method("Cancel", (), (), |_, _: &mut Agent, ()| {
            clear_files();
            shell(&["bluetooth-pairing-dismiss"]);
            Ok(())
        });
        builder.method(
            "RequestConfirmation",
            ("device", "passkey"),
            (),
            |_, _: &mut Agent, (device, passkey): (DbusPath<'static>, u32)| {
                Agent::ask("confirm", &device, Some(passkey)).map(|_| ())
            },
        );
        builder.method(
            "RequestAuthorization",
            ("device",),
            (),
            |_, _: &mut Agent, (device,): (DbusPath<'static>,)| {
                Agent::ask("authorize", &device, None).map(|_| ())
            },
        );
        builder.method(
            "AuthorizeService",
            ("device", "uuid"),
            (),
            |_, _: &mut Agent, (_device, _uuid): (DbusPath<'static>, String)| Ok(()),
        );
        builder.method(
            "DisplayPasskey",
            ("device", "passkey", "entered"),
            (),
            |_, _: &mut Agent, (device, passkey, _entered): (DbusPath<'static>, u32, u16)| {
                Agent::publish("display", &device, format!("{passkey:06}")).map(|_| ())
            },
        );
        builder.method(
            "DisplayPinCode",
            ("device", "pincode"),
            (),
            |_, _: &mut Agent, (device, pincode): (DbusPath<'static>, String)| {
                Agent::publish("display", &device, pincode).map(|_| ())
            },
        );
        builder.method(
            "RequestPasskey",
            ("device",),
            ("passkey",),
            |_, _: &mut Agent, (device,): (DbusPath<'static>,)| {
                let digits: String = Agent::ask("passkey", &device, None)?
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect();
                let value = digits
                    .parse::<u32>()
                    .map_err(|_| MethodErr::from(("org.bluez.Error.Rejected", "Rejected")))?;
                Ok((value,))
            },
        );
        builder.method(
            "RequestPinCode",
            ("device",),
            ("pincode",),
            |_, _: &mut Agent, (device,): (DbusPath<'static>,)| {
                let value = Agent::ask("pincode", &device, None)?;
                if value.is_empty() {
                    Err(("org.bluez.Error.Rejected", "Rejected").into())
                } else {
                    Ok((value,))
                }
            },
        );
    });
    crossroads.insert(AGENT_PATH, &[interface], Agent);
    connection.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            let _ = crossroads.handle_message(message, connection);
            true
        }),
    );
    let manager = connection.with_proxy("org.bluez", "/org/bluez", Duration::from_secs(5));
    let _: () = manager.method_call(
        "org.bluez.AgentManager1",
        "RegisterAgent",
        (DbusPath::new(AGENT_PATH)?, CAPABILITY),
    )?;
    let _: () = manager.method_call(
        "org.bluez.AgentManager1",
        "RequestDefaultAgent",
        (DbusPath::new(AGENT_PATH)?,),
    )?;
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::SeqCst))?;
    let deadline = Instant::now() + Duration::from_secs(window);
    while running.load(Ordering::SeqCst) && Instant::now() < deadline {
        connection.process(Duration::from_millis(250))?;
    }
    clear_files();
    shell(&["bluetooth-pairing-dismiss"]);
    let _: std::result::Result<(), _> = manager.method_call(
        "org.bluez.AgentManager1",
        "UnregisterAgent",
        (DbusPath::new(AGENT_PATH)?,),
    );
    close_window();
    Ok(())
}

pub fn watch_yubikey() -> Result {
    let runtime = env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let socket = PathBuf::from(runtime).join("yubikey-touch-detector.socket");
    loop {
        if let Ok(mut stream) = UnixStream::connect(&socket) {
            let mut event = [0_u8; 5];
            while stream.read_exact(&mut event).is_ok() {
                std::io::stdout().write_all(&event)?;
                std::io::stdout().write_all(b"\n")?;
                std::io::stdout().flush()?;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}
