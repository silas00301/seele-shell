//! The resident Fish wrapper owns a bounded stderr ring and one failure record
//! in memory. Begin/finish/debug exchange metadata over its private socket; no
//! terminal byte causes a filesystem write or a fsync.
use crate::{Result, MAX_CAPTURE_BYTES, MAX_STDERR_BYTES, SESSION_ENV};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
            process::ExitStatusExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, LazyLock, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
const IPC_LIMIT: usize = 128 * 1024;
static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~])").unwrap()
});
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub command: String,
    pub status: i32,
    pub stderr: String,
}
#[derive(Default)]
pub struct CaptureState {
    bytes: VecDeque<u8>,
    failure: Option<Failure>,
    generation: u64,
    last_write: Option<Instant>,
}
impl CaptureState {
    pub fn begin(&mut self) {
        self.bytes.clear();
        self.generation = self.generation.wrapping_add(1);
        self.last_write = None;
    }
    pub fn append(&mut self, bytes: &[u8]) {
        let bytes = &bytes[bytes.len().saturating_sub(MAX_CAPTURE_BYTES)..];
        let remove = (self.bytes.len() + bytes.len()).saturating_sub(MAX_CAPTURE_BYTES);
        self.bytes.drain(..remove);
        self.bytes.extend(bytes);
        self.last_write = Some(Instant::now());
    }
    pub fn finish(&mut self, command: &str, status: i32) {
        if status == 0 {
            return;
        }
        let bytes = self.bytes.make_contiguous();
        self.failure = Some(Failure {
            command: crate::clean_display(command, 4096).trim().to_owned(),
            status,
            stderr: clean_stderr(bytes),
        });
    }
    pub fn failure(&self) -> Option<Failure> {
        self.failure.clone()
    }
}
pub fn clean_stderr(bytes: &[u8]) -> String {
    let bytes = &bytes[bytes.len().saturating_sub(MAX_STDERR_BYTES)..];
    let bytes = ANSI.replace_all(bytes, &b""[..]);
    let text = String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    crate::clean_display(&text, MAX_STDERR_BYTES)
        .trim()
        .to_owned()
}
fn private(path: &Path, create: bool) -> Result<File> {
    let file = if create {
        seele_runtime::fs::private_directory(path)
    } else {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }
    .map_err(|_| "shell-assistance runtime directory is unavailable")?;
    let info = file.metadata().map_err(|_| "invalid runtime directory")?;
    if info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        return Err("shell-assistance runtime directory is not private");
    }
    Ok(file)
}
pub fn runtime_root() -> Result<PathBuf> {
    let parent = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("seele-shell-ai-{}", unsafe { libc::geteuid() }))
        });
    private(&parent, true)?;
    let root = parent.join("seele-shell-ai");
    private(&root, true)?;
    Ok(root)
}
pub fn current_session(required: bool) -> Result<Option<PathBuf>> {
    let Some(raw) = std::env::var_os(SESSION_ENV) else {
        return if required {
            Err("this Fish session is not capturing command failures")
        } else {
            Ok(None)
        };
    };
    let root = runtime_root()?
        .canonicalize()
        .map_err(|_| "invalid runtime directory")?;
    let path = PathBuf::from(raw);
    private(&path, false)?;
    let path = path
        .canonicalize()
        .map_err(|_| "invalid shell-assistance session")?;
    if path.parent() != Some(root.as_path()) {
        return Err("shell-assistance session is outside the private runtime directory");
    }
    Ok(Some(path))
}
pub fn request(message: &Value, required: bool) -> Result<Value> {
    let Some(session) = current_session(required)? else {
        return Ok(json!({"ok":true}));
    };
    let response = seele_runtime::wire::rpc(
        &session.join("control.sock"),
        message,
        seele_runtime::wire::RpcLimits {
            timeout: Duration::from_secs(3),
            request_bytes: 32 * 1024,
            response_bytes: IPC_LIMIT,
        },
        &AtomicUsize::new(0),
    )
    .map_err(|_| "shell-assistance capture is unavailable")?;
    if response["ok"] != true {
        return Err("no failed command has been captured in this Fish session");
    }
    Ok(response)
}
pub fn load_failure() -> Result<Failure> {
    let response = request(&json!({"op":"failure"}), true)?;
    let failure: Failure = serde_json::from_value(response["failure"].clone())
        .map_err(|_| "the last failure record is invalid")?;
    if failure.command.len() > 16 * 1024
        || failure.stderr.len() > MAX_STDERR_BYTES * 4
        || failure.status == 0
    {
        return Err("the last failure record is invalid");
    }
    Ok(failure)
}
pub fn capture_eligible(arguments: &[String], stdin_tty: bool, stdout_tty: bool) -> bool {
    if arguments.is_empty() || !stdin_tty || !stdout_tty {
        return false;
    }
    if Path::new(&arguments[0])
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .trim_start_matches('-')
        != "fish"
    {
        return false;
    }
    arguments[1..].iter().all(|a| {
        ["-i", "--interactive", "-l", "--login"].contains(&a.as_str())
            || (a.starts_with('-') && a.len() > 2 && a[1..].bytes().all(|b| b"il".contains(&b)))
    })
}
pub fn should_capture(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(file) = File::open(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let mut bytes = vec![];
    if file.take(64 * 1024).read_to_end(&mut bytes).is_err() || bytes.len() >= 64 * 1024 {
        return false;
    }
    let arguments: Vec<_> = bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    capture_eligible(
        &arguments,
        unsafe { libc::isatty(0) } == 1,
        unsafe { libc::isatty(1) } == 1,
    )
}
struct Shared {
    state: Mutex<CaptureState>,
    changed: Condvar,
    stop: AtomicUsize,
}
fn control(shared: &Shared, request: &Value) -> Result<Value> {
    let object = request.as_object().ok_or("invalid capture request")?;
    let op = request["op"].as_str().ok_or("invalid capture operation")?;
    let mut state = shared.state.lock().unwrap();
    match op {
        "begin" if object.len() == 1 => state.begin(),
        "finish" if object.len() == 3 => {
            let command = request["command"]
                .as_str()
                .filter(|s| s.len() <= 16 * 1024)
                .ok_or("invalid captured command")?;
            let status = request["status"]
                .as_i64()
                .and_then(|s| i32::try_from(s).ok())
                .ok_or("invalid status")?;
            if status != 0 {
                let generation = state.generation;
                let started = Instant::now();
                loop {
                    let elapsed = started.elapsed();
                    let since = state.last_write.map(|t| t.elapsed()).unwrap_or(elapsed);
                    if (elapsed >= Duration::from_millis(40) && since >= Duration::from_millis(40))
                        || elapsed >= Duration::from_millis(200)
                        || shared.stop.load(Ordering::Relaxed) != 0
                    {
                        break;
                    }
                    state = shared
                        .changed
                        .wait_timeout(state, Duration::from_millis(20))
                        .unwrap()
                        .0;
                }
                if state.generation != generation {
                    return Err("capture changed before completion");
                }
                state.finish(command, status);
            }
        }
        "failure" if object.len() == 1 => {
            return state
                .failure()
                .map(|failure| json!({"ok":true,"failure":failure}))
                .ok_or("no captured failure")
        }
        _ => return Err("invalid capture operation"),
    }
    Ok(json!({"ok":true}))
}
fn connection(shared: &Shared, stream: UnixStream) {
    let response = (|| -> Result<Value> {
        if !seele_runtime::wire::same_uid(&stream).map_err(|_| "invalid peer")? {
            return Err("invalid peer");
        }
        let request =
            seele_runtime::wire::receive(&stream, 16 * 1024, Duration::from_secs(2), &shared.stop)
                .map_err(|_| "invalid request")?;
        control(shared, &request)
    })()
    .unwrap_or_else(|_| json!({"ok":false}));
    let _ = seele_runtime::wire::send(
        &stream,
        &response,
        IPC_LIMIT,
        Duration::from_secs(2),
        &shared.stop,
    );
}
fn window_size(fd: i32) {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(2, libc::TIOCGWINSZ, &mut size) } == 0 {
        unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &size) };
    }
}
struct ShellChild(Child);
impl std::ops::Deref for ShellChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for ShellChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for ShellChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
pub fn capture_fish(fish: &Path) -> Result<i32> {
    let root = runtime_root()?;
    let session = tempfile::Builder::new()
        .prefix("session-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(root)
        .map_err(|_| "could not create private capture session")?;
    let listener = UnixListener::bind(session.path().join("control.sock"))
        .map_err(|_| "could not create capture socket")?;
    fs::set_permissions(
        session.path().join("control.sock"),
        fs::Permissions::from_mode(0o600),
    )
    .map_err(|_| "could not protect capture socket")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "capture socket unavailable")?;
    let mut master = 0;
    let mut slave = 0;
    if unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    } != 0
    {
        return Err("could not create stderr terminal");
    }
    let mut master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    window_size(slave.as_raw_fd());
    for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err("could not protect capture descriptors");
        }
    }
    let mut command = Command::new(fish);
    command
        .arg("--interactive")
        .stderr(Stdio::from(slave))
        .env(SESSION_ENV, session.path())
        .env_remove("SEELE_SHELL_AI_LOGIN");
    if std::env::var("SEELE_SHELL_AI_LOGIN").ok().as_deref() == Some("1") {
        command.arg("--login");
    }
    if let Ok(level) = std::env::var("SHLVL")
        .unwrap_or_else(|_| "1".into())
        .parse::<u32>()
    {
        command.env("SHLVL", level.saturating_sub(1).to_string());
    }
    let mut child = ShellChild(command.spawn().map_err(|_| "could not start Fish")?);
    let signal = Arc::new(AtomicUsize::new(0));
    let resize = Arc::new(AtomicUsize::new(0));
    let mut handlers = vec![];
    for number in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        handlers.push(
            signal_hook::flag::register_usize(number, signal.clone(), number as usize)
                .map_err(|_| "could not monitor shell signals")?,
        );
    }
    handlers.push(
        signal_hook::flag::register_usize(signal_hook::consts::SIGWINCH, resize.clone(), 1)
            .map_err(|_| "could not monitor terminal resize")?,
    );
    for number in [signal_hook::consts::SIGINT, signal_hook::consts::SIGQUIT] {
        handlers.push(
            unsafe { signal_hook::low_level::register(number, || {}) }
                .map_err(|_| "could not monitor shell signals")?,
        );
    }
    let shared = Arc::new(Shared {
        state: Mutex::new(CaptureState::default()),
        changed: Condvar::new(),
        stop: AtomicUsize::new(0),
    });
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let shared = shared.clone();
            let listener = listener.try_clone().unwrap();
            thread::spawn(move || {
                while shared.stop.load(Ordering::Relaxed) == 0 {
                    match listener.accept() {
                        Ok((stream, _)) => connection(&shared, stream),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            let mut poll = libc::pollfd {
                                fd: listener.as_raw_fd(),
                                events: libc::POLLIN,
                                revents: 0,
                            };
                            unsafe { libc::poll(&mut poll, 1, 50) };
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                        Err(_) => break,
                    }
                }
            })
        })
        .collect();
    let mut exit = None;
    let mut exited_at = None;
    let mut forwarded_at = None;
    let mut failed = false;
    let mut terminal_open = true;
    loop {
        if resize.swap(0, Ordering::Relaxed) != 0 {
            window_size(master.as_raw_fd());
        }
        let number = signal.load(Ordering::Relaxed);
        if number != 0 && exit.is_none() {
            if forwarded_at.is_none() {
                unsafe { libc::kill(child.id() as i32, number as i32) };
                forwarded_at = Some(Instant::now());
            } else if forwarded_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(1)) {
                let _ = child.kill();
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                exit = Some(status);
                exited_at.get_or_insert_with(Instant::now);
            }
            Ok(None) => (),
            Err(_) => {
                failed = true;
                break;
            }
        }
        if exit.is_some()
            && (!terminal_open
                || exited_at.is_some_and(|t: Instant| t.elapsed() >= Duration::from_millis(100)))
        {
            break;
        }
        let mut poll = libc::pollfd {
            fd: if terminal_open {
                master.as_raw_fd()
            } else {
                -1
            },
            events: libc::POLLIN,
            revents: 0,
        };
        let status = unsafe { libc::poll(&mut poll, 1, 20) };
        if status < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            failed = true;
            break;
        }
        if status == 0 {
            continue;
        }
        let mut bytes = [0u8; 8192];
        match master.read(&mut bytes) {
            Ok(0) => terminal_open = false,
            Ok(count) => {
                let mut state = shared.state.lock().unwrap();
                state.append(&bytes[..count]);
                shared.changed.notify_all();
                drop(state);
                if io::stderr().lock().write_all(&bytes[..count]).is_err() {
                    failed = true;
                    break;
                }
            }
            Err(e) if e.raw_os_error() == Some(libc::EIO) => terminal_open = false,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    shared.stop.store(1, Ordering::Relaxed);
    shared.changed.notify_all();
    drop(listener);
    drop(master);
    for worker in workers {
        let _ = worker.join();
    }
    for handler in handlers {
        signal_hook::low_level::unregister(handler);
    }
    if exit.is_none() {
        if failed || signal.load(Ordering::Relaxed) != 0 {
            let _ = child.kill();
        }
        exit = child.wait().ok();
    }
    if failed {
        return Err("Fish stderr capture failed");
    }
    let status = exit.ok_or("Fish exited without status")?;
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)))
}
