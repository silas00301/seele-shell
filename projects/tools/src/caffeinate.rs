//! Caffeinate owns one logind idle inhibitor, the task whose end releases it,
//! and the private socket the menu bar and the launcher both read.
//!
//! The inhibitor is a `block` lock on `idle` only. Hypridle's default
//! `ignore_systemd_inhibit = 0` makes it watch logind's `BlockInhibited`
//! property and suppress every listener action while `idle` is inhibited, so
//! the configured automatic lock and display-off never run; logind's own
//! `IdleAction` is inhibited by the same lock. No `sleep` lock is taken,
//! because a block lock on `sleep` would also refuse an explicit
//! `systemctl suspend`, and nothing here touches another application's lock.
use crate::command::{epoch, runtime_home, shutdown_signal};
use crate::daemon;
use crate::Result;
use dbus::blocking::Connection;
use seele_runtime::wire::{self, RpcLimits};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

const MAX_REQUEST: usize = 64 * 1024;
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const MAX_PROCESSES: usize = 384;
const MAX_SCAN: usize = 4096;
const MAX_KEY: usize = 128;
/// A transfer service that cannot be reached is not evidence that the transfer
/// is still running. Tolerate a restart, then end the session rather than leave
/// the machine awake for a task nobody can observe.
const LOST_TASK_TOLERANCE: u32 = 3;
const TICK: Duration = Duration::from_millis(500);

const USAGE: &str = r#"Usage: seele-caffeinate <serve|request|watch>

  serve      Run the session service
  request    Send one bounded JSON request on stdin and print its reply
  watch      Stream session snapshots until the service or this process stops
"#;

// ---------------------------------------------------------------- inhibition

/// Holding the descriptor is the inhibition and closing it is the release, so
/// owner exit, logout and an unexpected crash all release it without a record.
struct Inhibitor {
    _descriptor: Option<OwnedFd>,
}

fn inhibit() -> Result<Inhibitor> {
    let connection = Connection::new_system()?;
    let (handle,): (dbus::arg::OwnedFd,) = connection
        .with_proxy(
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            Duration::from_secs(10),
        )
        .method_call(
            "org.freedesktop.login1.Manager",
            "Inhibit",
            // The reason is world-readable through `loginctl list-inhibitors`,
            // so it names the feature and never the tracked task.
            (
                "idle",
                "Seele Caffeinate",
                "Keeping this session awake",
                "block",
            ),
        )?;
    // libdbus hands out a duplicate of the inhibitor descriptor, which dbus-rs
    // wraps and would close on drop. SAFETY: `into_raw_fd` gives up that
    // ownership, and nothing else refers to the descriptor afterwards.
    let descriptor = unsafe { OwnedFd::from_raw_fd(handle.into_raw_fd()) };
    Ok(Inhibitor {
        _descriptor: Some(descriptor),
    })
}

// ---------------------------------------------------------------- task model

fn clip(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn program_name(value: &str) -> &str {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(value)
        .trim_end_matches("-wrapped")
}

/// Commands whose whole purpose is to build something, and multi-purpose tools
/// paired with the sub-commands that make them a build. Anything else is
/// tracked honestly as the selected process rather than guessed at.
const BUILD_PROGRAMS: &[&str] = &[
    "bazel",
    "cabal",
    "cmake",
    "dune",
    "esbuild",
    "gradle",
    "make",
    "mvn",
    "ninja",
    "nix-build",
    "nixos-rebuild",
    "pytest",
    "rollup",
    "seele-rebuild",
    "stack",
    "tox",
    "tsc",
    "vite",
    "webpack",
];
const BUILD_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("bun", &["build", "install", "run", "test"]),
    (
        "cargo",
        &["bench", "build", "check", "clippy", "doc", "run", "test"],
    ),
    ("go", &["build", "generate", "install", "test"]),
    ("meson", &["compile", "setup", "test"]),
    ("nh", &["darwin", "home", "os"]),
    ("nix", &["build", "run"]),
    ("npm", &["ci", "install", "run", "test"]),
    ("pnpm", &["build", "install", "run", "test"]),
    ("swift", &["build", "test"]),
    ("yarn", &["build", "install", "run", "test"]),
    ("zig", &["build", "test"]),
];

fn is_build(arguments: &[String]) -> bool {
    let Some(program) = arguments.first().map(|value| program_name(value)) else {
        return false;
    };
    BUILD_PROGRAMS.contains(&program)
        || BUILD_SUBCOMMANDS.iter().any(|(name, subcommands)| {
            *name == program
                && arguments
                    .get(1)
                    .is_some_and(|value| subcommands.contains(&value.as_str()))
        })
}

/// The program and the sub-commands that say what it is doing. Remaining
/// arguments carry paths, prompts and occasionally credentials, and none of
/// them makes the row easier to recognize.
fn command_label(arguments: &[String]) -> String {
    let program = arguments
        .first()
        .map(|value| program_name(value))
        .unwrap_or_default();
    let mut label = clip(program, 64);
    if let Some(value) = arguments.get(1).filter(|value| {
        BUILD_SUBCOMMANDS
            .iter()
            .any(|(name, commands)| *name == program && commands.contains(&value.as_str()))
    }) {
        label.push(' ');
        label.push_str(value);
        if program == "nh" {
            if let Some(action) = arguments
                .get(2)
                .filter(|action| matches!(action.as_str(), "switch" | "build" | "boot" | "test"))
            {
                label.push(' ');
                label.push_str(action);
            }
        }
    }
    label
}

/// A build's project is the checkout it runs in. Anything shallower than a
/// version-control root is a guess, so an unreadable or unmarked working
/// directory simply has no project.
fn project(pid: u32) -> Option<String> {
    let mut directory = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    for _ in 0..32 {
        if directory.join(".git").exists() || directory.join(".jj").exists() {
            return directory
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| clip(name, 64));
        }
        if !directory.pop() {
            break;
        }
    }
    None
}

fn transfers_socket() -> PathBuf {
    std::env::var_os("SEELE_CAFFEINATE_TRANSFERS")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_home().join("seele-transfers.sock"))
}

fn transfers_snapshot() -> Option<Value> {
    wire::rpc(
        &transfers_socket(),
        &json!({"op": "snapshot"}),
        RpcLimits {
            timeout: Duration::from_secs(2),
            request_bytes: 4096,
            response_bytes: 8 * 1024 * 1024,
        },
        &shutdown_signal(),
    )
    .ok()
}

fn transfer_running(state: &str) -> bool {
    matches!(state, "sending" | "receiving" | "retrying")
}

/// A transfer belongs to the long-running service, so its row names the files
/// and the device rather than the process that happens to be moving them.
fn transfer_label(group: &Value) -> String {
    let files = group["files"].as_array().map_or(0, Vec::len);
    let device = clip(group["device"].as_str().unwrap_or("a personal device"), 64);
    if group["direction"] == "incoming" {
        format!("Receiving {files} file(s) from {device}")
    } else {
        format!("Sending {files} file(s) to {device}")
    }
}

enum Selector {
    Process { pid: u32, started: u64 },
    Transfer(String),
}

fn parse_key(key: &str) -> Option<Selector> {
    if key.len() > MAX_KEY {
        return None;
    }
    let (kind, rest) = key.split_once(':')?;
    match kind {
        "process" => {
            let (pid, started) = rest.split_once(':')?;
            Some(Selector::Process {
                pid: pid.parse().ok()?,
                started: started.parse().ok()?,
            })
        }
        "transfer" if !rest.is_empty() => Some(Selector::Transfer(rest.to_owned())),
        _ => None,
    }
}

fn process_row(pid: u32, started: u64, arguments: &[String], build: bool) -> Value {
    json!({
        "key": format!("process:{pid}:{started}"),
        "kind": if build { "build" } else { "process" },
        "label": command_label(arguments),
        "project": if build { project(pid) } else { None },
        "pid": pid,
    })
}

/// Builds and transfers first, because they are the tasks worth waiting for;
/// everything else follows, newest first, because a process started recently is
/// the one most likely to be the reason for staying awake.
fn tasks() -> Vec<Value> {
    let mut rows = Vec::new();
    let transfers = transfers_snapshot();
    if let Some(groups) = transfers
        .as_ref()
        .and_then(|snapshot| snapshot["groups"].as_array())
    {
        for group in groups {
            if !transfer_running(group["state"].as_str().unwrap_or("")) {
                continue;
            }
            let Some(id) = group["id"].as_str().filter(|id| id.len() < MAX_KEY) else {
                continue;
            };
            rows.push(json!({
                "key": format!("transfer:{id}"),
                "kind": "transfer",
                "label": transfer_label(group),
                "project": Value::Null,
                "pid": Value::Null,
            }));
        }
    }
    let me = std::process::id();
    let mut scanned = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if scanned.len() >= MAX_SCAN {
                break;
            }
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            if pid == me {
                continue;
            }
            let Some(started) = daemon::start_time(pid) else {
                continue;
            };
            let Some(arguments) = daemon::command_line(pid).filter(|args| !args.is_empty()) else {
                continue;
            };
            scanned.push((pid, started, arguments));
        }
    }
    scanned.sort_by(|left, right| right.1.cmp(&left.1).then(right.0.cmp(&left.0)));
    let (builds, others): (Vec<_>, Vec<_>) = scanned
        .into_iter()
        .partition(|(_, _, arguments)| is_build(arguments));
    for (pid, started, arguments) in &builds {
        rows.push(process_row(*pid, *started, arguments, true));
    }
    for (pid, started, arguments) in others.iter().take(MAX_PROCESSES) {
        rows.push(process_row(*pid, *started, arguments, false));
    }
    rows
}

// -------------------------------------------------------------------- session

enum Tracker {
    Process(daemon::Pinned),
    Transfer(String),
}

struct Session {
    generation: u64,
    mode: &'static str,
    started: i64,
    deadline: Option<i64>,
    task: Value,
    tracker: Option<Tracker>,
    misses: u32,
    _inhibitor: Inhibitor,
}

#[derive(Default)]
struct State {
    session: Option<Session>,
    generation: u64,
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn snapshot(state: &State, now: i64) -> Value {
    let Some(session) = &state.session else {
        return json!({
            "version": 1,
            "active": false,
            "mode": "",
            "elapsed": 0,
            "remaining": 0,
            "task": Value::Null,
        });
    };
    json!({
        "version": 1,
        "active": true,
        "mode": session.mode,
        "since": session.started,
        "elapsed": (now - session.started).max(0),
        "until": session.deadline,
        "remaining": session.deadline.map_or(0, |deadline| (deadline - now).max(0)),
        "task": session.task.clone(),
    })
}

fn duration_seconds(request: &Value) -> std::result::Result<u64, &'static str> {
    let text = request["duration"]
        .as_str()
        .filter(|text| text.len() <= 32)
        .ok_or("invalid-duration")?;
    // Parsing and bounds live with the rest of the UI policy, so the panel, the
    // launcher and this service agree on what a typed duration means.
    let parsed = seele_qml_core::call("caffeinate.parseDuration", &[json!(text)])
        .map_err(|_| "invalid-duration")?;
    match parsed["error"].as_str() {
        Some("duration-out-of-range") => Err("duration-out-of-range"),
        Some(_) => Err("invalid-duration"),
        None => parsed["seconds"].as_u64().ok_or("invalid-duration"),
    }
}

/// Resolve the picker's selection against the live system. A PID recycled
/// between listing and starting resolves to a different start time and is
/// refused; a transfer that has reached a terminal state is refused too.
fn resolve(key: &str) -> std::result::Result<(Value, Tracker), &'static str> {
    match parse_key(key).ok_or("invalid-request")? {
        Selector::Process { pid, started } => {
            let pinned = daemon::Pinned::open(pid).map_err(|_| "task-unavailable")?;
            if pinned.started() != started || pinned.exited() {
                return Err("task-unavailable");
            }
            let arguments = daemon::command_line(pid)
                .filter(|arguments| !arguments.is_empty())
                .ok_or("task-unavailable")?;
            let row = process_row(pid, started, &arguments, is_build(&arguments));
            Ok((row, Tracker::Process(pinned)))
        }
        Selector::Transfer(id) => {
            let snapshot = transfers_snapshot().ok_or("task-unavailable")?;
            let group = snapshot["groups"]
                .as_array()
                .ok_or("task-unavailable")?
                .iter()
                .find(|group| group["id"].as_str() == Some(id.as_str()))
                .filter(|group| transfer_running(group["state"].as_str().unwrap_or("")))
                .ok_or("task-unavailable")?;
            let row = json!({
                "key": key,
                "kind": "transfer",
                "label": transfer_label(group),
                "project": Value::Null,
                "pid": Value::Null,
            });
            Ok((row, Tracker::Transfer(id)))
        }
    }
}

fn start(state: &Shared, request: &Value, now: i64) -> std::result::Result<Value, &'static str> {
    let (mode, deadline, task, tracker) = match request["mode"].as_str().unwrap_or("") {
        "manual" => ("manual", None, Value::Null, None),
        "duration" => {
            let seconds = duration_seconds(request)?;
            // An absolute deadline, so an explicit suspend in the middle of a
            // timed session neither extends nor shortens what the user chose.
            ("duration", Some(now + seconds as i64), Value::Null, None)
        }
        "task" => {
            let key = request["task"].as_str().ok_or("invalid-request")?;
            let (task, tracker) = resolve(key)?;
            ("task", None, task, Some(tracker))
        }
        _ => return Err("invalid-request"),
    };
    // Acquire before reporting the session active and before releasing a
    // replaced one, so a replacement opens no gap and leaves no orphan. A
    // failed acquisition leaves any existing session exactly as it was.
    let inhibitor = inhibit().map_err(|_| "inhibit-unavailable")?;
    let mut guard = lock(state);
    guard.generation += 1;
    let session = Session {
        generation: guard.generation,
        mode,
        started: now,
        deadline,
        task,
        tracker,
        misses: 0,
        _inhibitor: inhibitor,
    };
    drop(guard.session.replace(session));
    Ok(snapshot(&guard, now))
}

/// One lifecycle pass. The transfer probe runs without the lock so a stalled
/// transfer service cannot delay the bar's snapshot, and its result is applied
/// only to the session that asked for it.
fn sweep(state: &Shared, now: i64) {
    let probe = {
        let mut guard = lock(state);
        let mut ended = false;
        let mut probe = None;
        if let Some(session) = guard.session.as_ref() {
            if session.deadline.is_some_and(|deadline| now >= deadline) {
                ended = true;
            } else {
                match &session.tracker {
                    Some(Tracker::Process(pinned)) => ended = pinned.exited(),
                    Some(Tracker::Transfer(id)) => probe = Some((session.generation, id.clone())),
                    None => {}
                }
            }
        }
        if ended {
            guard.session = None;
        }
        probe
    };
    let Some((generation, id)) = probe else {
        return;
    };
    let running = transfers_snapshot().map(|snapshot| {
        snapshot["groups"].as_array().is_some_and(|groups| {
            groups.iter().any(|group| {
                group["id"].as_str() == Some(id.as_str())
                    && transfer_running(group["state"].as_str().unwrap_or(""))
            })
        })
    });
    let mut guard = lock(state);
    let ended = {
        let Some(session) = guard.session.as_mut() else {
            return;
        };
        if session.generation != generation {
            return;
        }
        match running {
            Some(true) => {
                session.misses = 0;
                false
            }
            Some(false) => true,
            None => {
                session.misses += 1;
                session.misses >= LOST_TASK_TOLERANCE
            }
        }
    };
    if ended {
        guard.session = None;
    }
}

fn dispatch(state: &Shared, request: &Value) -> std::result::Result<Value, &'static str> {
    if !request.is_object() {
        return Err("invalid-request");
    }
    let now = epoch();
    match request["op"].as_str().unwrap_or("") {
        "snapshot" => Ok(snapshot(&lock(state), now)),
        "tasks" => Ok(json!({"version": 1, "tasks": tasks()})),
        "start" => start(state, request, now),
        "stop" => {
            let mut guard = lock(state);
            // Stopping releases only the inhibitor; the tracked task is never
            // signalled, and its Tracker is dropped with the session.
            if guard.session.take().is_none() {
                return Err("no-session");
            }
            Ok(snapshot(&guard, now))
        }
        _ => Err("invalid-request"),
    }
}

fn reply(state: &Shared, request: &Value) -> Value {
    match dispatch(state, request) {
        Ok(mut value) => {
            value["ok"] = json!(true);
            value
        }
        Err(error) => json!({"ok": false, "error": error}),
    }
}

// --------------------------------------------------------------------- socket

fn socket_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SEELE_CAFFEINATE_SOCKET") {
        return Ok(PathBuf::from(path));
    }
    let directory = runtime_home();
    let info = directory.symlink_metadata()?;
    if !info.is_dir() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        return Err("unsafe runtime directory".into());
    }
    Ok(directory.join("seele-caffeinate.sock"))
}

struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Ok(info) = self.path.symlink_metadata() {
            if info.dev() == self.device && info.ino() == self.inode {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

fn bind(path: &Path) -> Result<(UnixListener, SocketGuard, std::fs::File)> {
    // A process-owned advisory lock prevents a second service removing a live
    // socket, and with it another session's inhibitor.
    let lockfile = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path.with_extension("lock"))?;
    let info = lockfile.metadata()?;
    if !info.is_file()
        || info.uid() != unsafe { libc::geteuid() }
        || info.mode() & 0o077 != 0
        || unsafe { libc::flock(lockfile.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
    {
        return Err("Caffeinate is already running".into());
    }
    if let Ok(info) = path.symlink_metadata() {
        if !info.file_type().is_socket() || info.uid() != unsafe { libc::geteuid() } {
            return Err("unsafe Caffeinate socket".into());
        }
        if UnixStream::connect(path).is_ok() {
            return Err("Caffeinate is already running".into());
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let info = path.symlink_metadata()?;
    listener.set_nonblocking(true)?;
    Ok((
        listener,
        SocketGuard {
            path: path.to_owned(),
            device: info.dev(),
            inode: info.ino(),
        },
        lockfile,
    ))
}

fn connection(state: &Shared, stop: &Arc<AtomicUsize>, stream: UnixStream) {
    let value = (|| -> io::Result<Value> {
        if !wire::same_uid(&stream)? {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let request = wire::receive(&stream, MAX_REQUEST, Duration::from_secs(5), stop)?;
        Ok(reply(state, &request))
    })()
    .unwrap_or_else(|_| json!({"ok": false, "error": "invalid-request"}));
    let _ = wire::send(&stream, &value, MAX_RESPONSE, Duration::from_secs(5), stop);
}

fn serve() -> Result {
    let path = socket_path()?;
    let (listener, _guard, _lock) = bind(&path)?;
    let stop = shutdown_signal();
    let state: Shared = Arc::new(Mutex::new(State::default()));
    let lifecycle = {
        let (state, stop) = (state.clone(), stop.clone());
        thread::spawn(move || {
            while stop.load(Ordering::Relaxed) == 0 {
                sweep(&state, epoch());
                thread::sleep(TICK);
            }
        })
    };
    // Fixed worker and queue counts keep a same-user slow client from
    // allocating unbounded threads or descriptors.
    let (sender, receiver) = mpsc::sync_channel::<UnixStream>(8);
    let receiver = Arc::new(Mutex::new(receiver));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (state, stop, receiver) = (state.clone(), stop.clone(), receiver.clone());
            thread::spawn(move || loop {
                // Release the queue before serving, so four workers are four.
                let next = receiver
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .recv();
                let Ok(stream) = next else { break };
                if stop.load(Ordering::Relaxed) == 0 {
                    connection(&state, &stop, stream);
                }
            })
        })
        .collect();
    while stop.load(Ordering::Relaxed) == 0 {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = sender.try_send(stream);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let mut descriptor = libc::pollfd {
                    fd: listener.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one initialized pollfd stays live for this call.
                unsafe {
                    libc::poll(&mut descriptor, 1, 100);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(_) => stop.store(1, Ordering::Relaxed),
        }
    }
    drop(listener);
    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }
    let _ = lifecycle.join();
    // Shutdown releases the inhibitor explicitly rather than relying on exit.
    lock(&state).session = None;
    Ok(())
}

// --------------------------------------------------------------------- client

fn emit(value: &Value) -> io::Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, value)?;
    output.write_all(b"\n")?;
    output.flush()
}

pub(crate) fn request(value: &Value) -> Value {
    let unavailable = json!({"ok": false, "error": "service-unavailable"});
    let Ok(path) = socket_path() else {
        return unavailable;
    };
    wire::rpc(
        &path,
        value,
        RpcLimits {
            timeout: Duration::from_secs(20),
            request_bytes: MAX_REQUEST,
            response_bytes: MAX_RESPONSE,
        },
        &shutdown_signal(),
    )
    .unwrap_or(unavailable)
}

fn watch() -> Result {
    let stop = shutdown_signal();
    let mut previous: Option<Value> = None;
    let mut heartbeat = std::time::Instant::now();
    while stop.load(Ordering::Relaxed) == 0 {
        let mut value = request(&json!({"op": "snapshot"}));
        if value["ok"] != json!(true) {
            value = json!({
                "version": 1,
                "active": false,
                "mode": "",
                "elapsed": 0,
                "remaining": 0,
                "task": Value::Null,
                "error": "service-unavailable",
            });
        }
        if let Some(object) = value.as_object_mut() {
            object.remove("ok");
        }
        // A low-rate heartbeat keeps broken-pipe detection while an unchanged
        // session costs the shell no parsing or rebinding.
        if previous.as_ref() != Some(&value) || heartbeat.elapsed() >= Duration::from_secs(20) {
            emit(&value)?;
            previous = Some(value);
            heartbeat = std::time::Instant::now();
        }
        thread::sleep(TICK);
    }
    Ok(())
}

pub fn run(arguments: &[String]) -> Result {
    match arguments.first().map(String::as_str) {
        Some("serve") if arguments.len() == 1 => serve(),
        Some("watch") if arguments.len() == 1 => watch(),
        Some("request") if arguments.len() == 1 => {
            let mut bytes = Vec::new();
            io::stdin()
                .take(MAX_REQUEST as u64 + 1)
                .read_to_end(&mut bytes)?;
            let value = if bytes.len() > MAX_REQUEST {
                json!({"ok": false, "error": "invalid-request"})
            } else {
                match serde_json::from_slice::<Value>(&bytes) {
                    Ok(parsed) => request(&parsed),
                    Err(_) => json!({"ok": false, "error": "invalid-request"}),
                }
            };
            emit(&value)?;
            Ok(())
        }
        Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        _ => {
            eprint!("{USAGE}");
            Err("unknown Caffeinate command".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(mode: &'static str, deadline: Option<i64>, tracker: Option<Tracker>) -> Session {
        Session {
            generation: 1,
            mode,
            started: 1_000,
            deadline,
            task: Value::Null,
            tracker,
            misses: 0,
            // A fixture never takes a real lock; the descriptor is the whole
            // inhibitor, so its absence is a released one.
            _inhibitor: Inhibitor { _descriptor: None },
        }
    }
    fn shared(session: Session) -> Shared {
        Arc::new(Mutex::new(State {
            generation: 1,
            session: Some(session),
        }))
    }

    #[test]
    fn a_timed_session_ends_at_its_deadline_and_not_before() {
        let state = shared(session("duration", Some(1_060), None));
        sweep(&state, 1_059);
        assert!(lock(&state).session.is_some());
        assert_eq!(snapshot(&lock(&state), 1_030)["remaining"], 30);
        sweep(&state, 1_060);
        assert!(lock(&state).session.is_none());
        assert_eq!(snapshot(&lock(&state), 1_060)["active"], false);
    }

    #[test]
    fn a_manual_session_only_ends_when_it_is_stopped() {
        let state = shared(session("manual", None, None));
        for now in [1_001, 9_999, 99_999] {
            sweep(&state, now);
        }
        assert_eq!(snapshot(&lock(&state), 1_780)["elapsed"], 780);
        assert!(dispatch(&state, &json!({"op": "stop"})).is_ok());
        assert!(lock(&state).session.is_none());
        assert_eq!(
            dispatch(&state, &json!({"op": "stop"})).unwrap_err(),
            "no-session"
        );
    }

    #[test]
    fn a_selected_process_ends_its_session_when_it_exits() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pinned = daemon::Pinned::open(child.id()).unwrap();
        let started = pinned.started();
        let state = shared(session("task", None, Some(Tracker::Process(pinned))));
        sweep(&state, 1_001);
        assert!(
            lock(&state).session.is_some(),
            "a live task keeps its session"
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while lock(&state).session.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "the task's exit was never noticed"
            );
            sweep(&state, 1_002);
            thread::sleep(Duration::from_millis(5));
        }
        // A live PID whose start time is not the one that was offered is a
        // different process, which is exactly what PID reuse looks like.
        let reused = format!("process:{}:{}", std::process::id(), started + 1);
        assert_eq!(resolve(&reused).err(), Some("task-unavailable"));
        assert_eq!(
            resolve(&format!("process:{}:{started}", child.id())).err(),
            Some("task-unavailable")
        );
    }

    #[test]
    fn an_unreadable_task_ends_its_session_rather_than_waiting_forever() {
        let state = shared(session(
            "task",
            None,
            Some(Tracker::Transfer("gone".into())),
        ));
        // No transfer service is reachable from this fixture, so every probe is
        // unknown. Tolerate a restart, then release.
        std::env::set_var(
            "SEELE_CAFFEINATE_TRANSFERS",
            "/nonexistent/seele-transfers.sock",
        );
        for _ in 0..LOST_TASK_TOLERANCE - 1 {
            sweep(&state, 1_001);
            assert!(lock(&state).session.is_some());
        }
        sweep(&state, 1_001);
        assert!(lock(&state).session.is_none());
        std::env::remove_var("SEELE_CAFFEINATE_TRANSFERS");
    }

    #[test]
    fn selection_keys_reject_anything_they_did_not_issue() {
        assert!(parse_key("").is_none());
        assert!(parse_key("process:1").is_none());
        assert!(parse_key("process:one:two").is_none());
        assert!(parse_key("transfer:").is_none());
        assert!(parse_key(&format!("transfer:{}", "x".repeat(MAX_KEY))).is_none());
        assert!(parse_key("service:1:2").is_none());
        assert!(matches!(
            parse_key("process:42:99"),
            Some(Selector::Process {
                pid: 42,
                started: 99
            })
        ));
        assert!(matches!(parse_key("transfer:a:b"), Some(Selector::Transfer(id)) if id == "a:b"));
    }

    #[test]
    fn a_launcher_row_names_the_command_without_its_arguments() {
        let arguments =
            |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            command_label(&arguments(&["/nix/store/x/bin/nix", "build", ".#shell"])),
            "nix build"
        );
        assert_eq!(
            command_label(&arguments(&["nh", "os", "switch"])),
            "nh os switch"
        );
        assert_eq!(
            command_label(&arguments(&["curl", "-H", "Authorization: Bearer secret"])),
            "curl"
        );
        assert_eq!(
            command_label(&arguments(&["cargo", "-v", "build"])),
            "cargo"
        );
        for values in [
            vec!["client", "secret123"],
            vec!["client", "login", "secret123"],
            vec!["cargo", "run", "secret123"],
            vec!["nh", "os", "secret123"],
        ] {
            assert!(!command_label(&arguments(&values)).contains("secret123"));
        }
        assert!(is_build(&arguments(&["cargo", "test"])));
        assert!(is_build(&arguments(&["/usr/bin/make"])));
        assert!(!is_build(&arguments(&["cargo", "fmt"])));
        assert!(!is_build(&arguments(&["ssh", "build"])));
    }

    #[test]
    fn only_bounded_durations_reach_a_deadline() {
        assert_eq!(duration_seconds(&json!({"duration": "1h30"})), Ok(5400));
        assert_eq!(duration_seconds(&json!({"duration": "15m"})), Ok(900));
        assert_eq!(
            duration_seconds(&json!({"duration": "25h"})),
            Err("duration-out-of-range")
        );
        assert_eq!(
            duration_seconds(&json!({"duration": "later"})),
            Err("invalid-duration")
        );
        assert_eq!(duration_seconds(&json!({})), Err("invalid-duration"));
    }

    #[test]
    fn unknown_operations_and_modes_never_reach_the_session() {
        let state: Shared = Arc::new(Mutex::new(State::default()));
        for request in [
            json!({"op": "resume"}),
            json!({"op": "start"}),
            json!({"op": "start", "mode": "forever"}),
            json!({"op": "start", "mode": "task"}),
            json!([]),
        ] {
            assert!(dispatch(&state, &request).is_err(), "{request}");
        }
        assert_eq!(reply(&state, &json!({"op": "snapshot"}))["ok"], true);
        assert_eq!(lock(&state).generation, 0);
    }
}
