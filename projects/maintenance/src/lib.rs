pub mod model;
pub mod publishers;
pub mod service;
use model::Result;
use std::{
    process::Command,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};
pub const LIMIT: usize = 256 * 1024;
pub const SNAPSHOT_LIMIT: usize = 16 * 1024 * 1024;

pub use seele_runtime::fs::{read_bounded, read_private};

pub trait Executor: Send + Sync {
    fn run(&self, args: &[String], input: &[u8], timeout: Duration) -> Result<(i32, String)>;
}
pub struct ProcessExecutor {
    pub cancelled: Arc<AtomicUsize>,
}
impl Executor for ProcessExecutor {
    fn run(&self, args: &[String], input: &[u8], timeout: Duration) -> Result<(i32, String)> {
        let (program, args) = args.split_first().ok_or("invalid_command")?;
        let output = seele_runtime::process::capture(
            Command::new(program)
                .args(args)
                .env("LC_ALL", "C")
                .env("TZ", "UTC"),
            input,
            seele_runtime::process::Limits {
                timeout,
                output: LIMIT,
            },
            &self.cancelled,
        )
        .map_err(|_| "operation_failed")?;
        Ok((
            output.status.code().unwrap_or(128),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ))
    }
}
pub fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}
