//! Explicit, bounded clipboard handoff. Contents never appear in argv or logs.
use seele_runtime::process::{capture, discard_detaching, termination_signal, Limits};
use serde_json::json;
use std::{
    io::{self, Read},
    process::Command,
    time::Duration,
};

fn run() -> Result<String, &'static str> {
    let cancelled = termination_signal().map_err(|_| "Could not initialize clipboard handoff.")?;
    match std::env::args().nth(1).as_deref() {
        Some("paste") => {
            let result = capture(
                Command::new("wl-paste").args(["--no-newline", "--type", "text"]),
                b"",
                Limits {
                    timeout: Duration::from_secs(3),
                    output: 64 * 1024,
                },
                &cancelled,
            )
            .map_err(|_| "Clipboard unavailable, timed out, or exceeds 64 KiB.")?;
            if !result.status.success() {
                return Err("Clipboard does not contain readable text.");
            }
            let text = String::from_utf8(result.stdout)
                .map_err(|_| "Clipboard bytes are not valid UTF-8 text.")?;
            if text.contains('\0') {
                return Err("Clipboard contains a NUL byte; binary data is not supported.");
            }
            Ok(text)
        }
        Some("copy") => {
            let mut bytes = Vec::new();
            io::stdin()
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Could not read text.")?;
            if bytes.len() > 256 * 1024 {
                return Err("Result exceeds 256 KiB.");
            }
            let text = String::from_utf8(bytes).map_err(|_| "Result is not valid UTF-8 text.")?;
            if text.contains('\0') {
                return Err("Result contains a NUL byte.");
            }
            let status = discard_detaching(
                Command::new("wl-copy").args(["--type", "text/plain;charset=utf-8"]),
                text.as_bytes(),
                Limits {
                    timeout: Duration::from_secs(3),
                    output: 0,
                },
                &cancelled,
            )
            .map_err(|_| "Could not copy the result.")?;
            if !status.success() {
                return Err("Could not copy the result.");
            }
            Ok(String::new())
        }
        _ => Err("Choose paste or copy."),
    }
}
fn main() {
    // Structured, bounded stdout is the sole return channel. Never print input.
    println!(
        "{}",
        match run() {
            Ok(text) => json!({"ok":true,"text":text}),
            Err(error) => json!({"ok":false,"error":error}),
        }
    );
}
