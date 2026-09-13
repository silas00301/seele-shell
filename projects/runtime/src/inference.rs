//! The single synchronous broker client lifecycle, shared by CLI integrations.
//! Source payloads remain caller-owned; failures stay available in Activity.
use crate::wire::{rpc, RpcLimits};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
const REQUEST: usize = 256 * 1024;
const RESPONSE: usize = 512 * 1024;
pub fn default_socket() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/nonexistent".into())
        .join("seele-codex.sock")
}
/// Read only the broker's configured model; this never creates inference work.
pub fn configured_model(path: &Path, cancel: &AtomicUsize) -> crate::Result<String> {
    let reply = exchange(
        path,
        &json!({"op":"configuration"}),
        Duration::from_secs(5),
        cancel,
    );
    let model = reply["model"].as_str().filter(|value| {
        !value.is_empty()
            && value.len() <= 200
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
    });
    if reply["ok"] != true || !reply["epoch"].as_str().is_some_and(valid_uuid) {
        return Err("broker configuration unavailable".into());
    }
    model
        .map(str::to_owned)
        .ok_or_else(|| "invalid broker model".into())
}
fn failure(code: &str) -> Value {
    json!({"ok":false,"error":code})
}
fn exchange(path: &Path, message: &Value, timeout: Duration, cancel: &AtomicUsize) -> Value {
    rpc(
        path,
        message,
        RpcLimits {
            timeout,
            request_bytes: REQUEST,
            response_bytes: RESPONSE,
        },
        cancel,
    )
    .ok()
    .filter(|v| v.is_object() && v["ok"].is_boolean())
    .unwrap_or_else(|| failure("broker_unavailable"))
}
pub fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn identity(reply: &Value) -> Option<Value> {
    fn uuid(value: &Value) -> bool {
        value.as_str().is_some_and(valid_uuid)
    }
    if uuid(&reply["epoch"]) && uuid(&reply["job"]["id"]) {
        Some(json!({"epoch":reply["epoch"],"id":reply["job"]["id"]}))
    } else {
        None
    }
}
fn operation(identity: &Value, op: &str) -> Value {
    let mut value = identity.clone();
    value["op"] = json!(op);
    value
}
pub fn call(path: &Path, request: &Value, timeout: Duration, cancel: &AtomicUsize) -> Value {
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        return failure("invalid_input");
    };
    // Validate through the shared bounded serializer before cloning caller
    // data into the envelope. JSON escaping counts toward the actual wire cap.
    const ENVELOPE: usize = b"{\"op\":\"submit\",\"request\":}".len();
    if crate::wire::json_frame(request, REQUEST - ENVELOPE).is_err() {
        return failure("invalid_input");
    }
    let submit = json!({"op":"submit","request":request});
    if cancel.load(Ordering::Relaxed) != 0 {
        return failure("broker_unavailable");
    }
    // Finish receiving a bounded submit acknowledgement after cancellation:
    // once accepted, its returned identity is required to stop the model.
    let acknowledgement = AtomicUsize::new(0);
    let submitted = exchange(
        path,
        &submit,
        timeout.min(Duration::from_secs(10)),
        &acknowledgement,
    );
    if submitted["ok"] != true {
        return submitted;
    }
    let Some(identity) = identity(&submitted) else {
        return failure("broker_unavailable");
    };
    let mut reply = if cancel.load(Ordering::Relaxed) != 0 {
        failure("broker_unavailable")
    } else {
        exchange(
            path,
            &operation(&identity, "wait"),
            deadline.saturating_duration_since(Instant::now()),
            cancel,
        )
    };
    if reply["ok"] == true
        && (reply["epoch"] != identity["epoch"]
            || reply["job"]["id"] != identity["id"]
            || !matches!(
                reply["job"]["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled" | "superseded")
            ))
    {
        reply = failure("broker_unavailable");
    }
    // Cleanup gets its own short deadline and cancellation token: a client
    // SIGTERM must still cancel an already accepted model attempt.
    let cleanup = AtomicUsize::new(0);
    if reply["ok"] != true {
        let cancelled = exchange(
            path,
            &operation(&identity, "cancel"),
            Duration::from_secs(2),
            &cleanup,
        );
        if cancelled["ok"] == true {
            exchange(
                path,
                &operation(&identity, "release"),
                Duration::from_secs(2),
                &cleanup,
            );
        } else if cancelled["error"] == "invalid_state" {
            // Completion may race cancellation. Release successful/abandoned
            // terminal work; preserve a failure for the explicit retry UI.
            let status = exchange(
                path,
                &operation(&identity, "status"),
                Duration::from_secs(2),
                &cleanup,
            );
            if status["ok"] == true
                && matches!(
                    status["job"]["state"].as_str(),
                    Some("succeeded" | "cancelled" | "superseded")
                )
            {
                exchange(
                    path,
                    &operation(&identity, "release"),
                    Duration::from_secs(2),
                    &cleanup,
                );
            }
        }
    } else if reply["job"]["state"] != "failed" {
        exchange(
            path,
            &operation(&identity, "release"),
            Duration::from_secs(2),
            &cleanup,
        );
    }
    reply
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Write};
    use std::os::unix::net::UnixListener;
    fn scenario(state: &str, interrupt: bool) -> Vec<String> {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("broker.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let state = state.to_owned();
        let server = std::thread::spawn(move || {
            let mut operations = vec![];
            let count = if interrupt {
                4
            } else if state == "failed" {
                2
            } else {
                3
            };
            for _ in 0..count {
                let (mut stream, _) = listener.accept().unwrap();
                let mut frame = vec![];
                crate::wire::read_frame(&mut BufReader::new(&stream), &mut frame, REQUEST).unwrap();
                let message: Value = serde_json::from_slice(&frame).unwrap();
                let op = message["op"].as_str().unwrap();
                operations.push(op.to_owned());
                if op == "wait" && interrupt {
                    continue;
                }
                let reply = json!({"ok":true,"epoch":"00000000-0000-4000-8000-000000000001","job":{"id":"00000000-0000-4000-8000-000000000002","state":state},"result":42});
                writeln!(stream, "{reply}").unwrap();
            }
            operations
        });
        let reply = call(
            &path,
            &json!({"prompt":"private"}),
            Duration::from_secs(2),
            &AtomicUsize::new(0),
        );
        assert_eq!(reply["ok"], !interrupt);
        server.join().unwrap()
    }

    #[test]
    fn escaping_and_oversized_input_fail_before_transport() {
        for request in [
            json!({"prompt":"x".repeat(REQUEST)}),
            json!({"prompt":"\0".repeat(REQUEST / 4)}),
        ] {
            let reply = call(
                Path::new("/nonexistent/seele-test.sock"),
                &request,
                Duration::from_secs(1),
                &AtomicUsize::new(0),
            );
            assert_eq!(reply, failure("invalid_input"));
        }
    }

    #[test]
    fn cancellation_during_submit_still_recovers_and_cancels_job() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("broker.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let cancel = std::sync::Arc::new(AtomicUsize::new(0));
        let signal = cancel.clone();
        let server = std::thread::spawn(move || {
            let mut operations = vec![];
            for expected in ["submit", "cancel", "release"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut frame = vec![];
                crate::wire::read_frame(&mut BufReader::new(&stream), &mut frame, REQUEST).unwrap();
                let message: Value = serde_json::from_slice(&frame).unwrap();
                assert_eq!(message["op"], expected);
                operations.push(expected);
                if expected == "submit" {
                    signal.store(15, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_millis(30));
                }
                let reply = json!({"ok":true,"epoch":"00000000-0000-4000-8000-000000000001","job":{"id":"00000000-0000-4000-8000-000000000002","state":"cancelled"}});
                writeln!(stream, "{reply}").unwrap();
            }
            operations
        });
        assert_eq!(
            call(
                &path,
                &json!({"prompt":"private"}),
                Duration::from_secs(2),
                &cancel
            )["ok"],
            false
        );
        assert_eq!(server.join().unwrap(), ["submit", "cancel", "release"]);
    }
    #[test]
    fn completion_releases_failure_retains_and_disconnect_cancels() {
        assert_eq!(scenario("succeeded", false), ["submit", "wait", "release"]);
        assert_eq!(scenario("failed", false), ["submit", "wait"]);
        assert_eq!(
            scenario("running", true),
            ["submit", "wait", "cancel", "release"]
        );
    }
}
