use crate::command::{output, status};
use crate::Result;
use std::env;
use std::process::Command;
use std::thread;
use std::time::Duration;

const LOCK_USAGE: &str = "Usage: seele-lock [--status]\n\nLock the current Wayland session and wait until the compositor confirms that every output is secure.\n";

fn lock_state(quickshell: &str, config: &str) -> Option<String> {
    output(
        quickshell,
        ["ipc", "-p", config, "call", "seele-lock", "status"],
    )
    .map(|value| value.trim().to_owned())
}

pub fn lock(arguments: &[String]) -> Result {
    let quickshell = env::var("SEELE_QUICKSHELL").unwrap_or_else(|_| "quickshell".into());
    let config = env::var("SEELE_CONFIG").map_err(|_| "SEELE_CONFIG is not set")?;
    match arguments.first().map(String::as_str) {
        Some("--status") => {
            println!(
                "{}",
                lock_state(&quickshell, &config).unwrap_or_else(|| "unlocked".into())
            );
            return Ok(());
        }
        Some("-h" | "--help") => {
            print!("{LOCK_USAGE}");
            return Ok(());
        }
        None | Some("--immediate") => {}
        _ => return Err(LOCK_USAGE.into()),
    }
    if let Some(user) = env::var_os("USER") {
        if let Some(entry) = output("getent", ["passwd", &user.to_string_lossy()]) {
            if let Some(name) = entry
                .split(':')
                .nth(4)
                .and_then(|field| field.split(',').next())
                .filter(|name| !name.is_empty())
            {
                env::set_var("SEELE_LOCK_NAME", name);
            }
        }
    }
    if lock_state(&quickshell, &config).as_deref() == Some("unlocked") {
        status(&quickshell, ["kill", "-p", config.as_str()]);
    }
    if !status(&quickshell, ["-n", "-d", "-p", config.as_str()]) {
        return Err("could not start Seele lock".into());
    }
    for _ in 0..100 {
        if lock_state(&quickshell, &config).as_deref() == Some("secure") {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("seele-lock: compositor did not confirm a secure lock".into())
}

pub fn greeter(arguments: &[String]) -> Result {
    if arguments.first().map(String::as_str) == Some("--help") {
        println!("Usage: seele-greeter");
        return Ok(());
    }
    let quickshell = env::var("SEELE_QUICKSHELL").unwrap_or_else(|_| "quickshell".into());
    let config = env::var("SEELE_CONFIG").map_err(|_| "SEELE_CONFIG is not set")?;
    let result = Command::new(&quickshell)
        .args(["-n", "-p", &config])
        .status()?;
    let hyprctl = env::var("SEELE_HYPRCTL").unwrap_or_else(|_| "hyprctl".into());
    status(&hyprctl, ["dispatch", "hl.dsp.exit()"]);
    std::process::exit(result.code().unwrap_or(1));
}
