use crate::Result;
use serde_json::Value;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn output<I, S>(program: &str, arguments: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let result = Command::new(program).args(arguments).output().ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).into_owned())
}

pub fn status<I, S>(program: &str, arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
            libc::setsid();
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

pub fn timestamp() -> String {
    output("date", ["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub fn epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn process_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_child_is_reaped_without_blocking_the_caller() {
        let path = std::env::temp_dir().join(format!("seele-detached-test-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let started = std::time::Instant::now();
        detached("sh", &["-c".into(), "echo $$ > \"$1\"; sleep 0.2".into(), "sh".into(), path.to_string_lossy().into_owned()]).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_millis(150));
        let deadline = started + std::time::Duration::from_secs(3);
        let pid = loop {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(pid) = text.trim().parse::<u32>() { break pid; }
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
