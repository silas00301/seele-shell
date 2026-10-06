//! Claude's apiKeyHelper: return only the explicitly provisioned broker wallet
//! entry. Never inherit an integration token or read a provider credential file.
use std::io::{self, Write};
use std::process::Command;
use std::time::Duration;
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    if run().is_err() {
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let check = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => return Err("invalid operation".into()),
    };
    let stop = seele_runtime::process::termination_signal()?;
    let mut command = Command::new(
        std::env::var_os("SEELE_BROKER_SECRET_TOOL").unwrap_or_else(|| "secret-tool".into()),
    );
    command.env_clear();
    for key in ["PATH", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.args(["lookup", "application", "seele-codex", "account", "claude"]);
    // Stay in the caller's process group. `capture` would put the lookup in its
    // own group, which job cancellation does not kill, and SIGKILL skips Drop.
    let output = seele_runtime::process::capture_inherited_group(
        &mut command,
        b"",
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(8),
            output: 8192,
        },
        &stop,
    )?;
    let key = std::str::from_utf8(&output.stdout)?.trim_end_matches(['\r', '\n']);
    if !output.status.success() || key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("wallet entry unavailable".into());
    }
    if !check {
        io::stdout().write_all(key.as_bytes())?;
    }
    Ok(())
}
