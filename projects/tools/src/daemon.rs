//! Desktop daemon records identify a process lifetime, never just a reusable PID.
use crate::{command::atomic_write, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const MAX_RECORD: usize = 4096;
#[derive(Clone, Serialize, Deserialize)]
struct Identity {
    pid: u32,
    started: u64,
}
impl Identity {
    fn read(pid: u32) -> Option<Self> {
        if !(2..=i32::MAX as u32).contains(&pid) {
            return None;
        }
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        if proc.metadata().ok()?.uid() != unsafe { libc::geteuid() } {
            return None;
        }
        let text = std::fs::read_to_string(proc.join("stat")).ok()?;
        let fields: Vec<_> = text
            .get(text.rfind(") ")? + 2..)?
            .split_ascii_whitespace()
            .collect();
        if fields
            .first()
            .is_some_and(|state| matches!(*state, "Z" | "X"))
        {
            return None;
        }
        Some(Self {
            pid,
            started: fields.get(19)?.parse().ok()?,
        })
    }
}
#[derive(Serialize, Deserialize)]
struct Record {
    identity: Identity,
    program: String,
}
pub(crate) struct Pinned {
    fd: File,
    identity: Identity,
}
impl Pinned {
    pub(crate) fn open(pid: u32) -> io::Result<Self> {
        if !(2..=i32::MAX as u32).contains(&pid) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) as i32 };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { File::from_raw_fd(fd) };
        let identity = Identity::read(pid).ok_or(io::ErrorKind::NotFound)?;
        Ok(Self { fd, identity })
    }
    pub(crate) fn signal(&self, signal: i32) -> io::Result<()> {
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
fn normalized(value: &str) -> &str {
    value
        .trim_start_matches('.')
        .trim_end_matches("-wrapped")
        .trim_start_matches("seele-")
}
fn argv(pid: u32) -> Option<Vec<String>> {
    let mut file = File::open(format!("/proc/{pid}/cmdline")).ok()?;
    let mut bytes = Vec::new();
    (&mut file).take(65537).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 65536 {
        return None;
    }
    Some(
        bytes
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
    )
}
fn matches_command(pid: u32, program: &str, argument: Option<&str>) -> bool {
    let Some(args) = argv(pid) else {
        return false;
    };
    let name = |value: &str| {
        Path::new(value)
            .file_name()
            .and_then(|v| v.to_str())
            .map(|name| normalized(name).to_owned())
            .unwrap_or_default()
    };
    let expected = normalized(program);
    let first = args.first().map(|v| name(v)).unwrap_or_default();
    let command = first == expected
        || (first == "tools" && args.get(1).is_some_and(|v| normalized(v) == expected))
        || (matches!(first.as_str(), "bash" | "sh" | "dash")
            && args.get(1).is_some_and(|v| name(v) == expected));
    command && argument.is_none_or(|argument| args.iter().any(|value| value == argument))
}
fn load(path: &Path, program: &str, argument: Option<&str>) -> Option<Pinned> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return None;
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take((MAX_RECORD + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_RECORD {
        return None;
    }
    let record = serde_json::from_slice::<Record>(&bytes).ok();
    let pid = if let Some(record) = &record {
        if normalized(&record.program) != normalized(program) {
            return None;
        }
        record.identity.pid
    } else {
        std::str::from_utf8(&bytes).ok()?.trim().parse().ok()?
    };
    let pinned = Pinned::open(pid).ok()?;
    if record.is_some_and(|r| r.identity.started != pinned.identity.started)
        || !matches_command(pid, program, argument)
    {
        return None;
    }
    Some(pinned)
}
pub(crate) fn active(path: &Path, program: &str, argument: Option<&str>) -> bool {
    load(path, program, argument).is_some()
}
pub(crate) fn stop(path: &Path, program: &str) {
    if let Some(process) = load(path, program, None) {
        let _ = process.signal(libc::SIGTERM);
    }
    let _ = std::fs::remove_file(path);
}
struct Starting(Option<Child>);
impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
        }
    }
}
pub(crate) fn start(
    path: &Path,
    program: &str,
    arguments: &[&str],
    environment: &[(&str, String)],
    log: Option<&Path>,
) -> Result {
    stop(path, program);
    seele_runtime::fs::private_directory(path.parent().ok_or("daemon directory required")?)?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .envs(environment.iter().map(|(key, value)| (*key, value)))
        .stdin(Stdio::null());
    if let Some(log) = log {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(log)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.nlink() != 1
        {
            return Err("unsafe daemon log".into());
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.set_len(0)?;
        command.stdout(file.try_clone()?).stderr(file);
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let bounded_log = log.is_some();
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if bounded_log {
                let mut limit: libc::rlimit = std::mem::zeroed();
                if libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
                limit.rlim_cur = limit.rlim_cur.min(8 * 1024 * 1024);
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = Starting(Some(command.spawn()?));
    let pid = child.0.as_ref().unwrap().id();
    let deadline = Instant::now() + Duration::from_secs(2);
    let identity = loop {
        if let Some(identity) = Identity::read(pid) {
            if matches_command(pid, program, None) {
                break identity;
            }
        }
        // Observe without reaping so cleanup cannot signal a reused group.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } == 0
            && unsafe { info.si_pid() } != 0
        {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            let status = child.0.as_mut().unwrap().wait()?;
            child.0.take();
            return if status.success() {
                Ok(())
            } else {
                Err("desktop daemon failed to start".into())
            };
        }
        if Instant::now() >= deadline {
            return Err("desktop daemon did not start".into());
        }
        thread::sleep(Duration::from_millis(5));
    };
    atomic_write(
        path,
        &serde_json::to_vec(&Record {
            identity,
            program: program.to_owned(),
        })?,
    )?;
    let mut child = child.0.take().unwrap();
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pid_ranges_never_address_process_groups() {
        for pid in [0, 1, i32::MAX as u32 + 1, u32::MAX] {
            assert!(Pinned::open(pid).is_err());
        }
    }
    #[test]
    fn exact_executable_match_and_stale_start_time_fail_closed() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let identity = Identity::read(child.id()).unwrap();
        assert!(
            matches_command(child.id(), "sleep", Some("30")),
            "fixture argv: {:?}",
            argv(child.id())
        );
        assert!(!matches_command(child.id(), "bluetoothctl", None));
        assert!(!matches_command(child.id(), "sleep", Some("3")));
        let root = std::env::temp_dir().join(format!("seele-daemon-test-{}", uuid_free()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("record");
        let record = Record {
            identity: Identity {
                pid: identity.pid,
                started: identity.started + 1,
            },
            program: "sleep".into(),
        };
        atomic_write(&path, &serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(!active(&path, "sleep", None));
        stop(&path, "sleep");
        assert!(child.try_wait().unwrap().is_none());
        let _ = child.kill();
        let _ = child.wait();
        std::fs::remove_dir(root).unwrap();
    }
    fn uuid_free() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
}
