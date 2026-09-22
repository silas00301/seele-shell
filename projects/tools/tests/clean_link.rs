//! Exercise actual executable stdin, bounds, errors and cancellation.
use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
fn run(bytes: &[u8], extra: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seele-control"))
        .arg("vicinae-clean-link")
        .args(extra)
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(bytes);
    child.wait_with_output().unwrap()
}
#[test]
fn stdin_only_preview_without_external_tools() {
    let result = run(
        b"https://example.test/a?unknown=%2f&utm_source=secret&fbclid=secret#frag",
        &[],
    );
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["cleaned"], "https://example.test/a?unknown=%2f#frag");
    assert_eq!(
        value["removed"],
        serde_json::json!(["utm_source", "fbclid"])
    );
    assert!(value["markdown"].as_str().unwrap().contains("    https://"));
    let result = run(b"", &["https://example.test/private-secret"]);
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("private-secret"));
}
#[test]
fn errors_are_bounded_and_do_not_echo_payloads() {
    for bytes in [
        b"https://example.test/private-secret\nhttps://example.test/".to_vec(),
        vec![255],
        vec![b'x'; 32 * 1024],
    ] {
        let result = run(&bytes, &[]);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(result.stderr.len() < 300);
        assert!(!String::from_utf8_lossy(&result.stderr).contains("private-secret"));
    }
}
#[test]
fn missing_eof_times_out_and_sigterm_cancels() {
    for cancel in [false, true] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_seele-control"))
            .arg("vicinae-clean-link")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Keep the writer open: Child::wait would otherwise close its own stdin.
        let _input = child.stdin.take().unwrap();
        let started = Instant::now();
        if cancel {
            // Cancellation remains safe even before handlers are installed.
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
        }
        let status = child.wait().unwrap();
        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(4));
        if !cancel {
            assert!(started.elapsed() >= Duration::from_millis(1500));
        }
    }
}
