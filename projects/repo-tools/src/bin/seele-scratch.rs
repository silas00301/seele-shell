//! Explicit disposable Fish session; never copies or changes the caller's tree.
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    process::Command,
    time::Duration,
};
fn run() -> Result<i32, &'static str> {
    if std::env::args_os().len() != 1 {
        return Err("Usage: seele-scratch (no arguments)");
    }
    let work = tempfile::Builder::new()
        .prefix("seele-scratch-")
        .tempdir()
        .map_err(|_| "Cannot create private scratch workspace.")?;
    fs::set_permissions(work.path(), fs::Permissions::from_mode(0o700))
        .map_err(|_| "Cannot make scratch workspace private.")?;
    let state = work.path().join(".seele-state");
    for kind in ["config", "data", "cache", "state"] {
        fs::create_dir_all(state.join(kind)).map_err(|_| "Cannot create private scratch state.")?;
    }
    let cancel = seele_runtime::process::termination_signal()
        .map_err(|_| "Cannot establish cancellation.")?;
    // Fish job control moves jobs into their own process groups, and a
    // disowned job outlives Fish. As a subreaper this process stays the
    // ancestor of everything the session starts, so it can end all of it
    // before the workspace those jobs may still be using is removed.
    #[cfg(target_os = "linux")]
    if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0 {
        return Err("Cannot take ownership of scratch jobs.");
    }
    let mut fish = Command::new("fish");
    fish.args([
        "--private",
        "--no-config",
        "--interactive",
        "--init-command",
        "function fish_prompt; printf 'scratch> '; end; function fish_right_prompt; end",
    ])
    .current_dir(work.path())
    .env("XDG_CONFIG_HOME", state.join("config"))
    .env("XDG_DATA_HOME", state.join("data"))
    .env("XDG_CACHE_HOME", state.join("cache"))
    .env("XDG_STATE_HOME", state.join("state"));
    unsafe {
        fish.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }
    eprintln!("Disposable scratch · private Fish · exit removes this workspace.");
    let result =
        seele_runtime::process::interactive(&mut fish, &cancel, Duration::from_secs(24 * 60 * 60));
    #[cfg(target_os = "linux")]
    end_session_jobs();
    // Explicit close reports cleanup errors instead of relying on Drop alone.
    work.close()
        .map_err(|_| "Scratch ended, but its temporary directory could not be removed.")?;
    let result =
        result.map_err(|_| "Scratch session cancelled or expired; its workspace was removed.")?;
    Ok(result.code().unwrap_or(1))
}
/// Live descendants of this process, from /proc parent links.
#[cfg(target_os = "linux")]
fn descendants() -> Vec<u32> {
    use std::collections::HashMap;
    let me = std::process::id();
    let mut parents = HashMap::new();
    for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if let Some((state, parent)) = stat(pid) {
            if state != 'Z' {
                parents.insert(pid, parent);
            }
        }
    }
    parents
        .keys()
        .copied()
        .filter(|&pid| descends(pid, me, &|p| parents.get(&p).copied()))
        .collect()
}
/// State and parent from /proc/<pid>/stat; the command name may hold spaces.
#[cfg(target_os = "linux")]
fn stat(pid: u32) -> Option<(char, u32)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let mut fields = text.get(text.rfind(')')? + 2..)?.split_whitespace();
    let state = fields.next()?.chars().next()?;
    Some((state, fields.next()?.parse().ok()?))
}
#[cfg(target_os = "linux")]
fn descends(mut pid: u32, ancestor: u32, parent: &dyn Fn(u32) -> Option<u32>) -> bool {
    for _ in 0..4096 {
        match parent(pid) {
            Some(next) if next == ancestor => return true,
            Some(next) if next > 1 => pid = next,
            _ => return false,
        }
    }
    false
}
/// Hang up every remaining session job, then kill what ignores it, reaping all.
#[cfg(target_os = "linux")]
fn end_session_jobs() {
    let me = std::process::id();
    let signal = |number| {
        for pid in descendants() {
            // Re-check the ancestry through a pinned handle, so a recycled PID
            // can never receive the signal.
            seele_runtime::process::signal_checked(pid, number, &mut || {
                descends(pid, me, &|p| stat(p).map(|(_, parent)| parent))
            });
        }
    };
    let reap = || while unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) } > 0 {};
    signal(libc::SIGHUP);
    signal(libc::SIGCONT);
    for _ in 0..40 {
        reap();
        if descendants().is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    signal(libc::SIGKILL);
    for _ in 0..40 {
        reap();
        if descendants().is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
