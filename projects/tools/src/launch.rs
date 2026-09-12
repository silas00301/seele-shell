use crate::command::{output_with_input, shutdown_signal};
use crate::Result;
use seele_runtime::process::{capture, discard, discard_detaching, interactive, Limits};
use std::env;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const LOCK_USAGE: &str = "Usage: seele-lock [--status]\n\nLock the current Wayland session and wait until the compositor confirms that every output is secure.\n";

fn configuration() -> Result<(String, String)> {
    Ok((
        env::var("SEELE_QUICKSHELL").unwrap_or_else(|_| "quickshell".into()),
        env::var("SEELE_CONFIG").map_err(|_| "SEELE_CONFIG is not set")?,
    ))
}

fn lock_state(
    quickshell: &str,
    config: &str,
    timeout: Duration,
    cancel: &AtomicUsize,
) -> Option<String> {
    let output = capture(
        Command::new(quickshell).args(["ipc", "-p", config, "call", "seele-lock", "status"]),
        b"",
        Limits {
            timeout,
            output: 4096,
        },
        cancel,
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
        .map(|value| value.trim().to_owned())
}

pub fn lock(arguments: &[String]) -> Result {
    if arguments.len() > 1 {
        return Err(LOCK_USAGE.into());
    }
    if matches!(arguments.first().map(String::as_str), Some("-h" | "--help")) {
        print!("{LOCK_USAGE}");
        return Ok(());
    }
    let (quickshell, config) = configuration()?;
    let cancel = shutdown_signal();
    if arguments.first().map(String::as_str) == Some("--status") {
        println!(
            "{}",
            lock_state(&quickshell, &config, Duration::from_secs(1), &cancel)
                .unwrap_or_else(|| "unlocked".into())
        );
        return Ok(());
    }
    if !matches!(
        arguments.first().map(String::as_str),
        None | Some("--immediate")
    ) {
        return Err(LOCK_USAGE.into());
    }
    match lock_state(&quickshell, &config, Duration::from_secs(1), &cancel).as_deref() {
        Some("secure") => return Ok(()),
        Some("unlocked") => {
            let _ = discard(
                Command::new(&quickshell).args(["kill", "-p", &config]),
                b"",
                Limits {
                    timeout: Duration::from_secs(2),
                    output: 0,
                },
                &*cancel,
            );
        }
        _ => (),
    }
    let name = output_with_input(
        "getent",
        ["passwd", &unsafe { libc::geteuid() }.to_string()],
        b"",
        Duration::from_secs(1),
        4096,
    )
    .and_then(|entry| {
        entry
            .split(':')
            .nth(4)
            .and_then(|field| field.split(',').next())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    });
    let mut command = Command::new(&quickshell);
    command.args(["-n", "-d", "-p", &config]);
    if let Some(name) = name {
        command.env("SEELE_LOCK_NAME", name);
    }
    // Quickshell's successful daemonizing parent hands ownership to the lock.
    // Ordinary process capture would kill that new owner when reaping it.
    if !discard_detaching(
        &mut command,
        b"",
        Limits {
            timeout: Duration::from_secs(10),
            output: 0,
        },
        &*cancel,
    )?
    .success()
    {
        return Err("could not start Seele lock".into());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if cancel.load(Ordering::Relaxed) != 0 {
            return Err("lock confirmation interrupted".into());
        }
        if lock_state(
            &quickshell,
            &config,
            remaining.min(Duration::from_millis(500)),
            &cancel,
        )
        .as_deref()
            == Some("secure")
        {
            return Ok(());
        }
        thread::sleep(remaining.min(Duration::from_millis(50)));
    }
    // Never terminate the daemon here: it may have secured the session while
    // IPC failed, or may still complete the compositor's lock handshake.
    Err("seele-lock: compositor did not confirm a secure lock".into())
}

pub fn greeter(arguments: &[String]) -> Result {
    if arguments == ["--help"] {
        println!("Usage: seele-greeter");
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("Usage: seele-greeter".into());
    }
    let (quickshell, config) = configuration()?;
    let cancel = shutdown_signal();
    let result = interactive(
        Command::new(&quickshell).args(["-n", "-p", &config]),
        &*cancel,
        Duration::from_secs(365 * 24 * 60 * 60),
    );
    let hyprctl = env::var("SEELE_HYPRCTL").unwrap_or_else(|_| "hyprctl".into());
    // Terminate the private greeter compositor even during greetd cancellation.
    let _ = discard(
        Command::new(hyprctl).args(["dispatch", "hl.dsp.exit()"]),
        b"",
        Limits {
            timeout: Duration::from_secs(2),
            output: 0,
        },
        &AtomicUsize::new(0),
    );
    let code = match result {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
        }
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
            128 + cancel.load(Ordering::Relaxed) as i32
        }
        Err(error) => return Err(error.into()),
    };
    std::process::exit(code);
}

pub fn notes(arguments: &[String]) -> Result {
    if arguments == ["--help"] {
        println!("Usage: seele-notes");
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("Usage: seele-notes".into());
    }
    let (quickshell, config) = configuration()?;
    let cancel = shutdown_signal();
    if discard(
        Command::new(&quickshell).args([
            "ipc",
            "-n",
            "-p",
            &config,
            "call",
            "--",
            "seele-notes",
            "open",
        ]),
        b"",
        Limits {
            timeout: Duration::from_secs(2),
            output: 0,
        },
        &*cancel,
    )
    .is_ok_and(|status| status.success())
    {
        return Ok(());
    }
    if cancel.load(Ordering::Relaxed) != 0 {
        return Err("Notes launch interrupted".into());
    }
    use std::os::unix::process::CommandExt;
    Err(Command::new(quickshell)
        .args(["-n", "-p", &config])
        .exec()
        .into())
}
