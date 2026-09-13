use crate::command::{atomic_write, runtime_home};
use crate::Result;
use dbus::arg::{RefArg, Variant};
use dbus::blocking::Connection;
use dbus::channel::{MatchingReceiver, Sender};
use dbus::message::MatchRule;
use dbus::Path as DbusPath;
use dbus_crossroads::{Crossroads, MethodErr};
use serde_json::json;
use std::env;
use std::fs;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

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
    let _ = seele_runtime::process::discard(
        std::process::Command::new("seele-shellctl").args(values),
        b"",
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(2),
            output: 4096,
        },
        &std::sync::atomic::AtomicUsize::new(0),
    );
}

pub(crate) fn pairing_request(token: &str) -> Result<serde_json::Value> {
    if token.is_empty() || token.len() > 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid pairing token".into());
    }
    let bytes = seele_runtime::fs::read_private(&files().0, 4096)?;
    let request: serde_json::Value = serde_json::from_slice(&bytes)?;
    if request["token"].as_str() != Some(token) {
        return Err("pairing request expired".into());
    }
    Ok(request)
}

fn validate_pairing_value(kind: &str, value: &str) -> Result {
    let valid = match kind {
        "passkey" => {
            !value.is_empty() && value.len() <= 6 && value.bytes().all(|byte| byte.is_ascii_digit())
        }
        "pincode" => !value.is_empty() && value.len() <= 16 && !value.chars().any(char::is_control),
        "confirm" | "authorize" => value.is_empty(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("invalid pairing code".into())
    }
}

pub(crate) fn pairing_answer(token: &str, verdict: &str, value: &str) -> Result {
    let request = pairing_request(token)?;
    match verdict {
        "accept" => validate_pairing_value(request["kind"].as_str().unwrap_or(""), value)?,
        "reject" if value.is_empty() => {}
        _ => return Err("invalid pairing verdict".into()),
    }
    atomic_write(
        &files().1,
        format!("{token} {verdict} {value}\n").as_bytes(),
    )?;
    Ok(())
}

#[derive(Clone, Copy)]
enum ReplyKind {
    Empty,
    Passkey,
    Pin,
    Display,
}
struct Completion {
    generation: u64,
    token: String,
    path: DbusPath<'static>,
    kind: ReplyKind,
    context: dbus_crossroads::Context,
    result: std::result::Result<String, MethodErr>,
}
enum WorkerMessage {
    Publish {
        generation: u64,
        token: String,
        request: serde_json::Value,
    },
    Complete(Completion),
}
struct Job {
    cancel: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}
struct Agent {
    deadline: Instant,
    owner: String,
    generation: u64,
    token: String,
    job: Option<Job>,
    messages: std::sync::mpsc::SyncSender<WorkerMessage>,
    released: bool,
}
impl Agent {
    fn property(
        connection: &Connection,
        owner: &str,
        path: &DbusPath<'static>,
        name: &str,
    ) -> String {
        let result: std::result::Result<(Variant<Box<dyn RefArg>>,), _> = connection
            .with_proxy(owner, path.clone(), Duration::from_secs(1))
            .method_call(
                "org.freedesktop.DBus.Properties",
                "Get",
                ("org.bluez.Device1", name),
            );
        result
            .ok()
            .and_then(|(value,)| value.0.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
    fn trust(&self, path: &DbusPath<'static>) {
        if let Ok(connection) = Connection::new_system() {
            let _: std::result::Result<(), _> = connection
                .with_proxy(self.owner.as_str(), path.clone(), Duration::from_secs(1))
                .method_call(
                    "org.freedesktop.DBus.Properties",
                    "Set",
                    ("org.bluez.Device1", "Trusted", Variant(true)),
                );
        }
    }
    fn token() -> std::result::Result<String, MethodErr> {
        let mut random = [0u8; 16];
        // SAFETY: getentropy writes into this valid buffer of at most 256 bytes.
        if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
            return Err(MethodErr::failed("Pairing nonce unavailable"));
        }
        Ok(format!("{:032x}", u128::from_ne_bytes(random)))
    }
    fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.token.clear();
        if let Some(job) = &self.job {
            job.cancel.store(1, Ordering::Release);
        }
        clear_files();
        shell(&["bluetooth-pairing-dismiss"]);
    }
    fn start(
        &mut self,
        mut context: dbus_crossroads::Context,
        path: DbusPath<'static>,
        kind: &'static str,
        code: String,
        reply: ReplyKind,
    ) -> Option<dbus_crossroads::Context> {
        if self.job.is_some() || self.released || Instant::now() >= self.deadline {
            context.reply::<()>(Err((
                "org.bluez.Error.InProgress",
                "Authorization already active or closed",
            )
                .into()));
            return Some(context);
        }
        if (!code.is_empty()
            && validate_pairing_value(
                if matches!(reply, ReplyKind::Display) {
                    "pincode"
                } else {
                    "passkey"
                },
                &code,
            )
            .is_err())
            || code.len() > 16
        {
            context.reply::<()>(Err(rejected()));
            return Some(context);
        }
        let token = match Self::token() {
            Ok(token) => token,
            Err(error) => {
                context.reply::<()>(Err(error));
                return Some(context);
            }
        };
        self.generation = self.generation.wrapping_add(1);
        self.token.clone_from(&token);
        let generation = self.generation;
        let owner = self.owner.clone();
        let deadline = (Instant::now() + Duration::from_secs(90)).min(self.deadline);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_cancel = cancel.clone();
        let messages = self.messages.clone();
        let stop = crate::command::shutdown_signal();
        let thread = std::thread::spawn(move || {
            let cancelled = || {
                worker_cancel.load(Ordering::Acquire) != 0
                    || stop.load(Ordering::Relaxed) != 0
                    || Instant::now() >= deadline
            };
            let result = (|| {
                let connection = Connection::new_system().map_err(|_| rejected())?;
                let mut properties = Vec::with_capacity(4);
                for name in ["Address", "Alias", "Name", "Icon"] {
                    if cancelled() {
                        return Err(rejected());
                    }
                    properties.push(Self::property(&connection, &owner, &path, name));
                }
                if cancelled() {
                    return Err(rejected());
                }
                let name = [&properties[1], &properties[2], &properties[0]]
                    .into_iter()
                    .find(|value| !value.is_empty())
                    .map(String::as_str)
                    .unwrap_or("Unknown device");
                let request = json!({"token":token,"kind":kind,"address":properties[0],"name":name,"icon":properties[3],"passkey":code});
                // Exactly one publication and completion per worker; capacity two
                // never grows and remains independent of a slow desktop reader.
                messages
                    .send(WorkerMessage::Publish {
                        generation,
                        token: token.clone(),
                        request,
                    })
                    .map_err(|_| rejected())?;
                if matches!(reply, ReplyKind::Display) {
                    return Ok(String::new());
                }
                while !cancelled() {
                    if let Ok(bytes) = seele_runtime::fs::read_private(&files().1, 4096) {
                        let answer = std::str::from_utf8(&bytes).map_err(|_| rejected())?;
                        let mut fields = answer.trim_end_matches(['\r', '\n']).splitn(3, ' ');
                        if fields.next() == Some(token.as_str()) {
                            match fields.next() {
                                Some("accept") => {
                                    let value = fields.next().unwrap_or("");
                                    validate_pairing_value(kind, value).map_err(|_| rejected())?;
                                    if cancelled() {
                                        return Err(rejected());
                                    }
                                    return Ok(value.to_owned());
                                }
                                Some("reject") => return Err(rejected()),
                                _ => {}
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(rejected())
            })();
            let _ = messages.send(WorkerMessage::Complete(Completion {
                generation,
                token,
                path,
                kind: reply,
                context,
                result,
            }));
        });
        self.job = Some(Job {
            cancel,
            thread: Some(thread),
        });
        None
    }
    fn handle(&mut self, message: WorkerMessage, connection: &Connection) {
        match message {
            WorkerMessage::Publish {
                generation,
                token,
                request,
            } => {
                if generation != self.generation
                    || token != self.token
                    || self.released
                    || Instant::now() >= self.deadline
                {
                    return;
                }
                let published = seele_runtime::wire::json_frame(&request, 4096)
                    .and_then(|bytes| seele_runtime::fs::atomic_write(&files().0, &bytes));
                if published.is_ok() {
                    shell(&["bluetooth-pairing", &token]);
                } else {
                    self.cancel();
                }
            }
            WorkerMessage::Complete(mut completion) => {
                if let Some(mut job) = self.job.take() {
                    if let Some(thread) = job.thread.take() {
                        let _ = thread.join();
                    }
                }
                if completion.generation != self.generation
                    || completion.token != self.token
                    || self.released
                    || crate::command::shutdown_signal().load(Ordering::Relaxed) != 0
                    || Instant::now() >= self.deadline
                {
                    completion.result = Err(rejected());
                }
                if !matches!(completion.kind, ReplyKind::Display) {
                    if completion.result.is_ok() {
                        self.trust(&completion.path);
                    }
                    clear_files();
                    shell(&["bluetooth-pairing-dismiss"]);
                    self.token.clear();
                }
                match completion.kind {
                    ReplyKind::Empty | ReplyKind::Display => {
                        completion.context.reply(completion.result.map(|_| ()));
                    }
                    ReplyKind::Pin => {
                        completion
                            .context
                            .reply(completion.result.map(|value| (value,)));
                    }
                    ReplyKind::Passkey => {
                        completion
                            .context
                            .reply(completion.result.and_then(|value| {
                                value
                                    .parse::<u32>()
                                    .map(|value| (value,))
                                    .map_err(|_| rejected())
                            }));
                    }
                }
                let _ = completion.context.flush_messages(connection);
            }
        }
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        if let Some(mut job) = self.job.take() {
            job.cancel.store(1, Ordering::Release);
            if let Some(thread) = job.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
fn rejected() -> MethodErr {
    (
        "org.bluez.Error.Rejected",
        "Authorization cancelled or rejected",
    )
        .into()
}

fn close_window() {
    let restore = env::var("SEELE_BLUETOOTH_DISCOVERABLE_TIMEOUT").unwrap_or_else(|_| "180".into());
    for args in [
        vec!["discoverable", "off"],
        vec!["pairable", "off"],
        vec!["discoverable-timeout", restore.as_str()],
    ] {
        let _ = seele_runtime::process::discard(
            std::process::Command::new("bluetoothctl").args(args),
            b"",
            seele_runtime::process::Limits {
                timeout: Duration::from_secs(3),
                output: 0,
            },
            &std::sync::atomic::AtomicBool::new(false),
        );
    }
}

pub fn agent(arguments: &[String]) -> Result {
    if arguments.first().map(String::as_str) == Some("--describe") {
        println!("{{\"capability\":\"{CAPABILITY}\",\"methods\":[\"RequestConfirmation\",\"RequestAuthorization\",\"RequestPasskey\",\"RequestPinCode\",\"DisplayPasskey\"]}}");
        return Ok(());
    }
    let running = crate::command::shutdown_signal();
    struct Window;
    impl Drop for Window {
        fn drop(&mut self) {
            clear_files();
            close_window();
        }
    }
    let _window = Window;
    let window = env::var("SEELE_BLUETOOTH_PAIRING_WINDOW")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(120)
        .clamp(1, 300);
    let connection = Connection::new_system()?;
    let (bluez_owner,): (String,) = connection
        .with_proxy(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            Duration::from_secs(5),
        )
        .method_call("org.freedesktop.DBus", "GetNameOwner", ("org.bluez",))?;
    let deadline = Instant::now() + Duration::from_secs(window);
    let mut crossroads = Crossroads::new();
    let (messages, incoming) = std::sync::mpsc::sync_channel(2);
    let interface = crossroads.register("org.bluez.Agent1", |builder| {
        builder.method("Release", (), (), |_, agent: &mut Agent, ()| {
            agent.cancel();
            agent.released = true;
            Ok(())
        });
        builder.method("Cancel", (), (), |_, agent: &mut Agent, ()| {
            agent.cancel();
            Ok(())
        });
        builder.method_with_cr_custom::<_, (), _, _>(
            "RequestConfirmation",
            ("device", "passkey"),
            (),
            |ctx, cr, (device, passkey): (DbusPath<'static>, u32)| {
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(
                    ctx,
                    device,
                    "confirm",
                    format!("{passkey:06}"),
                    ReplyKind::Empty,
                )
            },
        );
        builder.method_with_cr_custom::<_, (), _, _>(
            "RequestAuthorization",
            ("device",),
            (),
            |ctx, cr, (device,): (DbusPath<'static>,)| {
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(ctx, device, "authorize", String::new(), ReplyKind::Empty)
            },
        );
        builder.method_with_cr_custom::<_, (), _, _>(
            "AuthorizeService",
            ("device", "uuid"),
            (),
            |ctx, cr, (device, _uuid): (DbusPath<'static>, String)| {
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(ctx, device, "authorize", String::new(), ReplyKind::Empty)
            },
        );
        builder.method_with_cr_custom::<_, (), _, _>(
            "DisplayPasskey",
            ("device", "passkey", "entered"),
            (),
            |mut ctx, cr, (device, passkey, _entered): (DbusPath<'static>, u32, u16)| {
                if passkey > 999_999 {
                    ctx.reply::<()>(Err(rejected()));
                    return Some(ctx);
                }
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(
                    ctx,
                    device,
                    "display",
                    format!("{passkey:06}"),
                    ReplyKind::Display,
                )
            },
        );
        builder.method_with_cr_custom::<_, (), _, _>(
            "DisplayPinCode",
            ("device", "pincode"),
            (),
            |ctx, cr, (device, pincode): (DbusPath<'static>, String)| {
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(ctx, device, "display", pincode, ReplyKind::Display)
            },
        );
        builder.method_with_cr_custom::<_, (u32,), _, _>(
            "RequestPasskey",
            ("device",),
            ("passkey",),
            |ctx, cr, (device,): (DbusPath<'static>,)| {
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(ctx, device, "passkey", String::new(), ReplyKind::Passkey)
            },
        );
        builder.method_with_cr_custom::<_, (String,), _, _>(
            "RequestPinCode",
            ("device",),
            ("pincode",),
            |ctx, cr, (device,): (DbusPath<'static>,)| {
                let agent: &mut Agent = cr.data_mut(ctx.path()).expect("registered agent");
                agent.start(ctx, device, "pincode", String::new(), ReplyKind::Pin)
            },
        );
    });
    crossroads.insert(
        AGENT_PATH,
        &[interface],
        Agent {
            deadline,
            owner: bluez_owner.clone(),
            generation: 0,
            token: String::new(),
            job: None,
            messages,
            released: false,
        },
    );
    let crossroads = std::sync::Arc::new(std::sync::Mutex::new(crossroads));
    let dispatch = crossroads.clone();
    let authenticated_owner = bluez_owner.clone();
    connection.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            if message.sender().as_deref() != Some(authenticated_owner.as_str()) {
                let _ = connection.send(message.error(
                    &"org.freedesktop.DBus.Error.AccessDenied".into(),
                    c"Only BlueZ may invoke the Bluetooth agent",
                ));
            } else {
                let _ = dispatch.lock().unwrap().handle_message(message, connection);
            }
            true
        }),
    );
    let owner_state = crossroads.clone();
    let owner_rule = MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged");
    connection.add_match_no_cb(&owner_rule.match_str())?;
    connection.start_receive(
        owner_rule,
        Box::new(move |message, _| {
            if message.sender().as_deref() == Some("org.freedesktop.DBus") {
                if let Ok((name, _old, new)) = message.read3::<String, String, String>() {
                    if name == "org.bluez" && new != bluez_owner {
                        let mut state = owner_state.lock().unwrap();
                        let agent: &mut Agent =
                            state.data_mut(&DbusPath::new(AGENT_PATH).unwrap()).unwrap();
                        agent.cancel();
                        agent.released = true;
                    }
                }
            }
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
    while running.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
        connection.process(Duration::from_millis(50))?;
        // Drain already queued D-Bus cancellation/owner changes before committing
        // any successful worker answer. Only this thread can grant trust.
        let mut drained = true;
        for index in 0..128 {
            if !connection.process(Duration::ZERO)? {
                break;
            }
            if index == 127 {
                drained = false;
            }
        }
        if !drained {
            continue;
        }
        let mut state = crossroads.lock().unwrap();
        let agent: &mut Agent = state.data_mut(&DbusPath::new(AGENT_PATH)?).unwrap();
        if let Ok(message) = incoming.try_recv() {
            agent.handle(message, &connection);
        }
        if agent.released {
            break;
        }
    }
    {
        let mut state = crossroads.lock().unwrap();
        let agent: &mut Agent = state.data_mut(&DbusPath::new(AGENT_PATH)?).unwrap();
        agent.cancel();
    }
    let _: std::result::Result<(), _> = manager.method_call(
        "org.bluez.AgentManager1",
        "UnregisterAgent",
        (DbusPath::new(AGENT_PATH)?,),
    );

    Ok(())
}

pub fn watch_yubikey() -> Result {
    let runtime = env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let socket = PathBuf::from(runtime).join("yubikey-touch-detector.socket");
    let stop = crate::command::shutdown_signal();
    let mut stdout = seele_runtime::wire::nonblocking_stdout()?;
    while stop.load(Ordering::Relaxed) == 0 {
        if let Ok(mut stream) = UnixStream::connect(&socket) {
            if !seele_runtime::wire::same_uid(&stream)? {
                return Err("Invalid YubiKey event publisher".into());
            }
            stream.set_read_timeout(Some(Duration::from_millis(250)))?;
            let mut event = [0_u8; 5];
            let mut used = 0;
            while stop.load(Ordering::Relaxed) == 0 {
                match stream.read(&mut event[used..]) {
                    Ok(0) => break,
                    Ok(count) => used += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        continue
                    }
                    Err(_) => break,
                }
                if used == event.len() {
                    let frame = [event[0], event[1], event[2], event[3], event[4], b'\n'];
                    seele_runtime::wire::write_bytes(
                        &mut stdout,
                        &frame,
                        Duration::from_secs(5),
                        &stop,
                    )?;
                    used = 0;
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_codes_reject_truncation_controls_and_invalid_digits() {
        assert!(validate_pairing_value("passkey", "000123").is_ok());
        for invalid in ["", "1000000", "12x3", "１２３", "12\n3"] {
            assert!(validate_pairing_value("passkey", invalid).is_err());
        }
        assert!(validate_pairing_value("pincode", "1234567890123456").is_ok());
        for invalid in ["", "12345678901234567", "123\n", "123\0"] {
            assert!(validate_pairing_value("pincode", invalid).is_err());
        }
        assert!(validate_pairing_value("confirm", "").is_ok());
        assert!(validate_pairing_value("confirm", "123456").is_err());
    }

    #[test]
    fn pairing_nonces_are_random_hex() {
        let first = Agent::token().unwrap();
        assert_eq!(first.len(), 32);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, Agent::token().unwrap());
    }
}
