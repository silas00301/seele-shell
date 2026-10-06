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
    // Explicit close reports cleanup errors instead of relying on Drop alone.
    work.close()
        .map_err(|_| "Scratch ended, but its temporary directory could not be removed.")?;
    let result =
        result.map_err(|_| "Scratch session cancelled or expired; its workspace was removed.")?;
    Ok(result.code().unwrap_or(1))
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
