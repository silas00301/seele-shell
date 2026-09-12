pub mod collect;
pub mod generator;
pub mod privacy;
pub mod report;
pub mod text;
pub mod ui;
use seele_runtime::process::{capture, Limits};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
pub const MAX_COMMAND_OUTPUT: usize = 64 * 1024;
pub const MAX_REPORT: usize = 256 * 1024;
pub const MAX_AI_OUTPUT: usize = 24 * 1024;
pub fn executable(key: &str, fallback: &str) -> std::ffi::OsString {
    std::env::var_os(key).unwrap_or_else(|| fallback.into())
}
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
pub fn run(command: &mut Command, input: &[u8], timeout: u64, cancel: &AtomicUsize) -> Output {
    match capture(
        command,
        input,
        Limits {
            timeout: Duration::from_secs(timeout),
            output: MAX_REPORT,
        },
        cancel,
    ) {
        Ok(output) => Output {
            code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(_) => Output {
            code: 124,
            stdout: String::new(),
            stderr: "Command failed or exceeded its resource limit".into(),
        },
    }
}
