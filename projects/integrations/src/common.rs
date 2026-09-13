//! Shared integration boundaries. Untrusted errors never cross a public protocol.
use serde_json::Value;
use std::{
    io,
    path::PathBuf,
    process::Command,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
pub type Result<T> = std::result::Result<T, &'static str>;
pub const MAX_RESPONSE: usize = 2 * 1024 * 1024;

pub fn xdg(name: &str, fallback: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback)
        })
}
pub fn clean(value: &Value, token: &str, limit: usize) -> String {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let text = if token.is_empty() {
        text
    } else {
        text.replace(token, "[redacted]")
    };
    text.chars().filter(|c| !c.is_control() && !matches!(*c, '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')).take(limit).collect()
}
pub use seele_runtime::reactor::{read_frame, FdIo};
pub async fn write_json<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
) -> io::Result<()> {
    seele_runtime::reactor::write_json(writer, value, MAX_RESPONSE, Duration::from_secs(5)).await
}
pub async fn response_json(mut response: reqwest::Response) -> Result<Value> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "invalid-response")? {
        if chunk.len() > MAX_RESPONSE.saturating_sub(bytes.len()) {
            return Err("response-too-large");
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid-response")
}
/// Run an owned process group off the reactor; cancellation kills and reaps it.
pub async fn command(
    command: Command,
    input: Vec<u8>,
    timeout: Duration,
    output: usize,
    cancel: CancellationToken,
) -> Result<Vec<u8>> {
    execute(command, input, timeout, output, cancel, false).await
}
/// Desktop applications may deliberately fork after accepting an open request.
/// Their descendants survive only a successful launcher exit.
pub async fn launch(command: Command, timeout: Duration, cancel: CancellationToken) -> Result<()> {
    execute(command, vec![], timeout, 0, cancel, true)
        .await
        .map(|_| ())
}
async fn execute(
    mut command: Command,
    input: Vec<u8>,
    timeout: Duration,
    output: usize,
    cancel: CancellationToken,
    detaching: bool,
) -> Result<Vec<u8>> {
    let flag = Arc::new(AtomicUsize::new(0));
    struct CancelOnDrop(Arc<AtomicUsize>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let _cancel_on_drop = CancelOnDrop(flag.clone());
    let worker_flag = flag.clone();
    let mut job = tokio::task::spawn_blocking(move || {
        let limits = seele_runtime::process::Limits { timeout, output };
        let result = if detaching {
            seele_runtime::process::discard_detaching(&mut command, &input, limits, &worker_flag)
                .map(|status| seele_runtime::process::Output {
                    status,
                    stdout: vec![],
                    stderr: vec![],
                })
        } else {
            seele_runtime::process::capture(&mut command, &input, limits, &worker_flag)
        };
        use zeroize::Zeroize;
        let mut input = input;
        input.zeroize();
        result
    });
    let result = tokio::select! {
        result = &mut job => result,
        _ = cancel.cancelled() => {
            flag.store(1, std::sync::atomic::Ordering::Relaxed);
            job.await
        }
    }
    .map_err(|_| "command-failed")?
    .map_err(|_| "command-failed")?;
    if !result.status.success() {
        return Err("command-failed");
    }
    Ok(result.stdout)
}

pub async fn emit(value: &Value) -> io::Result<()> {
    static OUTPUT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = OUTPUT.lock().await;
    write_json(&mut FdIo::stdout()?, value).await
}
