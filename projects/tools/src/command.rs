use crate::Result;
use seele_runtime::process::{capture, discard, Limits};
use serde_json::Value;
use std::env;
use std::ffi::OsStr;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{atomic::AtomicUsize, Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One signal owner for all ordinary tool commands and resident daemons.
/// Native capture uses this same flag to terminate and reap subprocess groups.
pub fn shutdown_signal() -> Arc<AtomicUsize> {
    static SIGNAL: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
    SIGNAL
        .get_or_init(|| {
            seele_runtime::process::termination_signal().expect("tool termination signal")
        })
        .clone()
}
pub fn output_with_input<I, S>(
    program: &str,
    arguments: I,
    input: &[u8],
    timeout: Duration,
    limit: usize,
) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let result = capture(
        Command::new(program).args(arguments),
        input,
        Limits {
            timeout,
            output: limit,
        },
        &shutdown_signal(),
    )
    .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).into_owned())
}
pub fn output<I, S>(program: &str, arguments: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    output_with_input(
        program,
        arguments,
        b"",
        Duration::from_secs(20),
        16 * 1024 * 1024,
    )
}
pub fn status<I, S>(program: &str, arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    discard(
        Command::new(program).args(arguments),
        b"",
        Limits {
            timeout: Duration::from_secs(120),
            output: 0,
        },
        &shutdown_signal(),
    )
    .is_ok_and(|status| status.success())
}
/// Clipboard owners and desktop launchers intentionally outlive their caller.
/// Preserve their descendants only after a successful launcher exit.
pub fn launch_with_input<I, S>(
    program: &str,
    arguments: I,
    input: &[u8],
    timeout: Duration,
) -> Result
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if seele_runtime::process::discard_detaching(
        Command::new(program).args(arguments),
        input,
        Limits { timeout, output: 0 },
        &shutdown_signal(),
    )?
    .success()
    {
        Ok(())
    } else {
        Err(format!("{program} failed").into())
    }
}

pub fn clipboard(text: &str, mime: &str) -> Result {
    launch_with_input(
        "wl-copy",
        ["--type", mime],
        text.as_bytes(),
        Duration::from_secs(5),
    )
}

/// The explicitly opened OS session runs long foreground rebuilds and keeps
/// their terminal I/O; ordinary desktop actions use the bounded status helper.
pub fn interactive_status<I, S>(program: &str, arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    seele_runtime::process::interactive(
        Command::new(program).args(arguments),
        &shutdown_signal(),
        Duration::from_secs(24 * 60 * 60),
    )
    .is_ok_and(|status| status.success())
}

pub fn require_status<I, S>(program: &str, arguments: I) -> Result
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if status(program, arguments) {
        Ok(())
    } else {
        Err(format!("{program} failed").into())
    }
}

pub fn detached(program: &str, arguments: &[String]) -> Result {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn exec(program: &str, arguments: &[String]) -> Result {
    use std::os::unix::process::CommandExt;
    let error = Command::new(program).args(arguments).exec();
    Err(error.into())
}

pub fn json_output<I, S>(program: &str, arguments: I, fallback: Value) -> Value
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    output(program, arguments)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(fallback)
}

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn state_home() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
}

pub fn config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

pub fn runtime_home() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

#[cfg(test)]
use seele_runtime::time::format_timestamp;
pub use seele_runtime::time::timestamp;

pub fn epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result {
    seele_runtime::fs::atomic_write(path, data)?;
    Ok(())
}

pub fn process_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_preserve_utc_calendar_format() {
        for (seconds, expected) in [
            (-1, "1969-12-31T23:59:59Z"),
            (0, "1970-01-01T00:00:00Z"),
            (951_827_696, "2000-02-29T12:34:56Z"),
            (2_147_483_648, "2038-01-19T03:14:08Z"),
        ] {
            assert_eq!(format_timestamp(seconds).as_deref(), Some(expected));
        }
    }

    #[test]
    fn detached_child_is_reaped_without_blocking_the_caller() {
        let path = std::env::temp_dir().join(format!("seele-detached-test-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let started = std::time::Instant::now();
        detached(
            "sh",
            &[
                "-c".into(),
                "echo $$ > \"$1\"; sleep 0.2".into(),
                "sh".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_millis(150));
        let deadline = started + std::time::Duration::from_secs(3);
        let pid = loop {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(pid) = text.trim().parse::<u32>() {
                    break pid;
                }
            }
            assert!(std::time::Instant::now() < deadline, "child never started");
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        while process_alive(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = fs::remove_file(path);
        assert!(!process_alive(pid), "detached child remained a zombie");
    }
}
