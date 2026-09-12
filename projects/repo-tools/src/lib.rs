pub mod check;
pub mod generation;
pub mod jj;
pub mod pi;
pub mod update;
use std::{io, process::Command, sync::atomic::AtomicUsize, time::Duration};
pub type Result<T> = std::result::Result<T, i32>;
pub fn capture(command: &mut Command, cancel: &AtomicUsize) -> Result<String> {
    let output = seele_runtime::process::capture(
        command,
        b"",
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(120),
            output: 4 * 1024 * 1024,
        },
        cancel,
    )
    .map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            127
        } else {
            1
        }
    })?;
    if !output.status.success() {
        return Err(output.status.code().unwrap_or(1));
    }
    String::from_utf8(output.stdout).map_err(|_| 1)
}
pub fn interactive(command: &mut Command, cancel: &AtomicUsize) -> Result<()> {
    let result = seele_runtime::process::interactive(command, cancel, Duration::from_secs(86400))
        .map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            127
        } else {
            1
        }
    })?;
    if result.success() {
        Ok(())
    } else {
        Err(result.code().unwrap_or(1))
    }
}
pub fn main(run: impl FnOnce(&[String], &AtomicUsize) -> Result<()>) {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let cancel = match seele_runtime::process::termination_signal() {
        Ok(cancel) => cancel,
        Err(_) => std::process::exit(1),
    };
    if let Err(code) = run(&args, &cancel) {
        std::process::exit(code);
    }
}
