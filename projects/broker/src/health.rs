//! Health metadata contains no authentication output or private job fields.
use crate::{now, MAX_REPLY};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

pub fn probe(path: &Path, cancel: &AtomicUsize) -> bool {
    seele_runtime::wire::rpc(
        path,
        &json!({"op":"list"}),
        seele_runtime::wire::RpcLimits {
            timeout: Duration::from_secs(5),
            request_bytes: 64,
            response_bytes: MAX_REPLY,
        },
        cancel,
    )
    .is_ok_and(|v| v["ok"] == true)
}
pub fn run(command: &mut Command, payload: &[u8], cancel: &AtomicUsize) -> Option<i32> {
    seele_runtime::process::discard(
        command,
        payload,
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(8),
            output: 0,
        },
        cancel,
    )
    .ok()?
    .code()
}
pub fn snapshot(
    available: bool,
    execute: impl FnOnce() -> Option<i32>,
    clock: impl FnOnce() -> f64,
) -> Value {
    let mut value = json!({"state":"disconnected","summary":"Broker unavailable","detail":"The inference service is not responding.","lastSuccess":0,"actions":["restart","diagnostics"]});
    if !available {
        return value;
    }
    match execute() {
        None => {
            value["summary"] = json!("Codex unavailable");
            value["detail"] = json!("The configured Codex application could not be checked.");
        }
        Some(0) => {
            value["state"] = json!("healthy");
            value["summary"] = json!("Ready for requests");
            value["detail"] = json!("");
            value["lastSuccess"] = json!((clock() * 1000.0) as u64);
        }
        Some(_) => {
            value["state"] = json!("setup-required");
            value["summary"] = json!("Sign in to Codex");
            value["detail"] = json!("Open a terminal and run codex login to connect your account.");
        }
    }
    value
}
pub fn publish(cancel: &AtomicUsize) {
    let path = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/nonexistent".into())
        .join("seele-codex.sock");
    let binary = std::env::var_os("SEELE_BROKER_CODEX").unwrap_or_else(|| "codex".into());
    let mut command = Command::new(binary);
    seele_runtime::codex::authentication_environment(&mut command, path.parent().unwrap());
    command.args(["login", "status"]);
    let value = snapshot(probe(&path, cancel), || run(&mut command, b"", cancel), now);
    if let Ok(payload) = serde_json::to_vec(&value) {
        run(
            Command::new("seele-shellctl").args(["health-publish", "codex"]),
            &payload,
            cancel,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_broker_never_probes_authentication() {
        assert_eq!(
            snapshot(false, || panic!("auth probe"), || 0.0)["lastSuccess"],
            0
        );
    }
    #[test]
    fn auth_missing_and_application_missing_stay_distinct() {
        assert_eq!(snapshot(true, || None, || 123.0)["state"], "disconnected");
        let value = snapshot(true, || Some(1), || 123.0);
        assert_eq!(value["state"], "setup-required");
        assert_eq!(value["actions"], json!(["restart", "diagnostics"]));
        let value = snapshot(true, || Some(0), || 123.0);
        assert_eq!(value["state"], "healthy");
        assert_eq!(value["lastSuccess"], 123000);
    }
}
