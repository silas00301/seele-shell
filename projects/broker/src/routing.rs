//! Known exhausted quota uses Claude Code's tool-free bare mode. Authentication
//! is a dedicated broker wallet entry, never the user's CLI credential file.
use crate::lifecycle::{Runner, Selection};
use crate::runner::Codex;
use crate::validation::Request;
use crate::{Result, MAX_MESSAGE};
use seele_runtime::process::{capture, Limits};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

pub struct Routing {
    codex: Codex,
    runtime: PathBuf,
    quota: PathBuf,
    claude: PathBuf,
}
impl Routing {
    pub fn new(codex: Codex, runtime: PathBuf) -> Self {
        Self {
            codex,
            runtime,
            quota: std::env::var_os("SEELE_BROKER_CODEXBAR")
                .unwrap_or_else(|| "codexbar".into())
                .into(),
            claude: std::env::var_os("SEELE_BROKER_CLAUDE")
                .unwrap_or_else(|| "claude".into())
                .into(),
        }
    }
    fn remaining(&self, cancel: &AtomicUsize) -> Option<bool> {
        let mut command = Command::new(&self.quota);
        // CodexBar owns provider authentication; no task content enters it.
        command.env_clear();
        for key in ["HOME", "PATH", "CODEX_HOME", "SSL_CERT_FILE"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.args(["usage", "--provider", "codex", "--json"]);
        let output = capture(
            &mut command,
            b"",
            Limits {
                timeout: Duration::from_secs(8),
                output: 512 * 1024,
            },
            cancel,
        )
        .ok()?;
        if !output.status.success() {
            return None;
        }
        quota_remaining(&serde_json::from_slice::<Value>(&output.stdout).ok()?)
    }
    fn claude(
        &self,
        request: &Request,
        model: &str,
        cancel: &AtomicUsize,
    ) -> Result<(Value, Value)> {
        let metadata = fs::symlink_metadata(&self.runtime).map_err(|_| "runtime_failure")?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err("runtime_failure");
        }
        let temporary = tempfile::Builder::new()
            .prefix("seele-claude-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&self.runtime)
            .map_err(|_| "runtime_failure")?;
        let workspace = temporary.path().join("workspace");
        let home = temporary.path().join("home");
        for path in [&workspace, &home] {
            seele_runtime::fs::private_directory(path).map_err(|_| "runtime_failure")?;
        }
        let environment = |command: &mut Command| {
            command
                .env_clear()
                .env("HOME", &home)
                .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
                .env("XDG_RUNTIME_DIR", &self.runtime)
                .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
                .env("CLAUDE_CODE_SKIP_PROMPT_HISTORY", "1")
                .current_dir(&workspace);
            for key in [
                "PATH",
                "SSL_CERT_FILE",
                "DBUS_SESSION_BUS_ADDRESS",
                "SEELE_BROKER_SECRET_TOOL",
            ] {
                if let Some(value) = std::env::var_os(key) {
                    command.env(key, value);
                }
            }
        };
        // Unsupported CLIs fail before receiving a private task or wallet key.
        let mut help = Command::new(&self.claude);
        environment(&mut help);
        help.arg("--help");
        let output = capture(
            &mut help,
            b"",
            Limits {
                timeout: Duration::from_secs(10),
                output: 128 * 1024,
            },
            cancel,
        )
        .map_err(|_| "isolation_failure")?;
        let text = std::str::from_utf8(&output.stdout).map_err(|_| "isolation_failure")?;
        if !output.status.success()
            || [
                "--bare",
                "--tools",
                "--disallowedTools",
                "--strict-mcp-config",
                "--no-session-persistence",
                "--setting-sources",
            ]
            .iter()
            .any(|flag| {
                !text.lines().any(|line| {
                    line.trim_start()
                        .strip_prefix(flag)
                        .is_some_and(|tail| tail.is_empty() || tail.starts_with([' ', ',']))
                })
            })
        {
            return Err("isolation_failure");
        }
        let helper = std::env::current_exe()
            .map_err(|_| "runtime_failure")?
            .with_file_name("seele-claude-key");
        let mut check = Command::new(&helper);
        environment(&mut check);
        check.arg("--check");
        let output = capture(
            &mut check,
            b"",
            Limits {
                timeout: Duration::from_secs(10),
                output: 1024,
            },
            cancel,
        )
        .map_err(|_| "authentication_unavailable")?;
        if !output.status.success() {
            return Err("authentication_unavailable");
        }
        let helper = helper.to_str().ok_or("runtime_failure")?;
        let helper = format!("'{}'", helper.replace('\'', "'\\''"));
        let mut command = Command::new(&self.claude);
        environment(&mut command);
        command.args([
            "-p",
            "--bare",
            "--tools",
            "",
            "--disallowedTools",
            "*",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--setting-sources",
            "",
            "--no-session-persistence",
            "--permission-mode",
            "dontAsk",
            "--output-format",
            "stream-json",
            "--verbose",
            "--max-turns",
            "1",
            "--model",
            model,
            "--system-prompt",
            seele_runtime::codex::POLICY,
        ]);
        command
            .arg("--settings")
            .arg(json!({"apiKeyHelper":helper}).to_string());
        // Schema stays on stdin. Native broker validation avoids enabling a
        // structured-output tool solely to constrain the response format.
        let input = serde_json::to_vec(&json!({"task":request.value["prompt"],"context":request.value["context"],"outputSchema":request.value["output"]["schema"],"instruction":"Return only JSON satisfying outputSchema."})).map_err(|_| "invalid_input")?;
        let output = capture(
            &mut command,
            &input,
            Limits {
                timeout: Duration::from_secs(180),
                output: MAX_MESSAGE * 8,
            },
            cancel,
        )
        .map_err(|_| "runtime_failure")?;
        if !output.status.success() {
            return Err("model_failure");
        }
        claude_events(&output.stdout)
    }
}
impl Runner for Routing {
    fn select(&self, model: &str, cancel: &AtomicUsize) -> Selection {
        match self.remaining(cancel) {
            Some(false) => Selection {
                model: "claude:haiku".into(),
                reason: "codex_quota_exhausted",
            },
            Some(true) => Selection {
                model: model.into(),
                reason: "codex_quota_available",
            },
            None => Selection {
                model: model.into(),
                reason: "codex_quota_unknown",
            },
        }
    }
    fn infer(
        &self,
        request: &Request,
        model: &str,
        cancel: &AtomicUsize,
    ) -> Result<(Value, Value)> {
        if model != "claude:haiku" {
            return self.codex.infer(request, model, cancel);
        }
        for tier in ["haiku", "sonnet", "opus"] {
            match self.claude(request, tier, cancel) {
                Ok((value, mut usage)) => {
                    usage["model"] = json!(format!("claude:{tier}"));
                    return Ok((value, usage));
                }
                Err("model_failure") => (),
                failure => return failure,
            }
        }
        Err("model_failure")
    }
}
fn quota_remaining(value: &Value) -> Option<bool> {
    let records = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(value));
    let mut remaining = None;
    for record in records {
        if record["provider"] != "codex" || record.get("error").is_some() {
            continue;
        }
        for key in ["primary", "secondary", "tertiary"] {
            let window = &record["usage"][key];
            if window["usageKnown"] == false {
                continue;
            }
            let Some(used) = window["usedPercent"]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 100.0)
            else {
                continue;
            };
            if used == 100.0 {
                return Some(false);
            }
            remaining = Some(true);
        }
    }
    remaining
}
fn claude_events(bytes: &[u8]) -> Result<(Value, Value)> {
    let mut answer = None;
    let mut isolated = false;
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        if line.len() > MAX_MESSAGE * 2 {
            return Err("runtime_failure");
        }
        let event: Value = serde_json::from_slice(line).map_err(|_| "runtime_failure")?;
        if event["type"] == "system" && event["subtype"] == "init" {
            if event["tools"]
                .as_array()
                .is_none_or(|tools| !tools.is_empty())
                || event["mcp_servers"]
                    .as_array()
                    .is_none_or(|servers| !servers.is_empty())
            {
                return Err("isolation_failure");
            }
            isolated = true;
        }
        if event["message"]["content"].as_array().is_some_and(|parts| {
            parts.iter().any(|part| {
                matches!(
                    part["type"].as_str(),
                    Some("tool_use" | "server_tool_use" | "tool_result")
                )
            })
        }) {
            return Err("isolation_failure");
        }
        if event["type"] == "result" {
            if event["is_error"] != false || event["subtype"] != "success" {
                return Err("model_failure");
            }
            let text = event["result"].as_str().ok_or("invalid_output")?;
            if text.len() > MAX_MESSAGE {
                return Err("invalid_output");
            }
            answer = Some((
                serde_json::from_str(text).map_err(|_| "invalid_output")?,
                json!({"input":event["usage"]["input_tokens"],"output":event["usage"]["output_tokens"]}),
            ));
        }
    }
    if !isolated {
        return Err("isolation_failure");
    }
    answer.ok_or("model_failure")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quota_requires_known_valid_provider_windows() {
        assert_eq!(
            quota_remaining(
                &json!([{"provider":"codex","usage":{"primary":{"usedPercent":50},"secondary":{"usedPercent":100}}}])
            ),
            Some(false)
        );
        assert_eq!(
            quota_remaining(&json!({"provider":"codex","usage":{"primary":{"usedPercent":0}}})),
            Some(true)
        );
        for value in [
            json!([]),
            json!({"provider":"claude","usage":{"primary":{"usedPercent":100}}}),
            json!({"provider":"codex","usage":{"primary":{"usedPercent":100,"usageKnown":false}}}),
            json!({"provider":"codex","usage":{"primary":{"usedPercent":-1}}}),
        ] {
            assert_eq!(quota_remaining(&value), None);
        }
    }
    #[test]
    fn claude_requires_success_and_rejects_tools() {
        let success = b"{\"type\":\"system\",\"subtype\":\"init\",\"tools\":[],\"mcp_servers\":[]}\n{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"42\",\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}\n";
        assert_eq!(claude_events(success).unwrap().0, json!(42));
        assert_eq!(
            claude_events(b"{\"type\":\"system\",\"subtype\":\"init\",\"tools\":[\"Bash\"]}")
                .unwrap_err(),
            "isolation_failure"
        );
        assert_eq!(
            claude_events(
                b"{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\"}]}}"
            )
            .unwrap_err(),
            "isolation_failure"
        );
        assert!(claude_events(b"{}").is_err());
    }
}
