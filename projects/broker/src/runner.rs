//! Codex runs from a fresh private workspace, with all tools disabled. Each
//! attempt owns and reaps its process group through the shared runtime library.
use crate::lifecycle::Runner;
use crate::validation::Request;
use crate::{Result, MAX_MESSAGE};
use seele_runtime::process::{capture, Limits};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

pub struct Codex {
    policy: seele_runtime::codex::Toolless,
    runtime: PathBuf,
}
impl Codex {
    pub fn new(binary: PathBuf, runtime: PathBuf) -> Self {
        Self {
            policy: seele_runtime::codex::Toolless::new(binary, runtime.clone()),
            runtime,
        }
    }
    fn command(
        &self,
        model: &str,
        context: &seele_runtime::codex::Context,
        workspace: &Path,
        schema: &File,
        cancel: &AtomicUsize,
    ) -> Result<Command> {
        let mut command = self
            .policy
            .command(
                context,
                seele_runtime::codex::Options {
                    workspace,
                    resume: false,
                    ephemeral: true,
                    instructions: "Return only the requested JSON.",
                },
                cancel,
            )
            .map_err(|_| "runtime_failure")?;
        let schema_fd = seele_runtime::process::inherit_file(&mut command, schema)
            .map_err(|_| "runtime_failure")?;
        command
            .arg("--output-schema")
            .arg(format!("/proc/self/fd/{schema_fd}"))
            .arg("--model")
            .arg(model)
            .arg("-");
        Ok(command)
    }
}
impl Runner for Codex {
    fn infer(
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
        let temp = tempfile::Builder::new()
            .prefix("seele-inference-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&self.runtime)
            .map_err(|_| "runtime_failure")?;
        let workspace = temp.path().join("workspace");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&workspace)
            .map_err(|_| "runtime_failure")?;
        // SAFETY: literal name is terminated. A successful call returns a new
        // descriptor uniquely owned by File; it stays live until capture ends.
        let fd = unsafe {
            libc::memfd_create(
                c"seele-output-schema".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if fd < 0 {
            return Err("runtime_failure");
        }
        let mut schema = unsafe { File::from_raw_fd(fd) };
        schema
            .write_all(
                &serde_json::to_vec(&request.value["output"]["schema"])
                    .map_err(|_| "invalid_input")?,
            )
            .map_err(|_| "runtime_failure")?;
        schema
            .seek(SeekFrom::Start(0))
            .map_err(|_| "runtime_failure")?;
        if unsafe {
            libc::fcntl(
                fd,
                libc::F_ADD_SEALS,
                libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
            )
        } < 0
        {
            return Err("runtime_failure");
        }
        let input = serde_json::to_vec(
            &json!({"task":request.value["prompt"],"context":request.value["context"]}),
        )
        .map_err(|_| "invalid_input")?;
        let context = self
            .policy
            .context()
            .map_err(|_| "authentication_unavailable")?;
        let mut command = self.command(model, &context, &workspace, &schema, cancel)?;
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
        parse_events(&output.stdout)
    }
}
pub fn parse_events(bytes: &[u8]) -> Result<(Value, Value)> {
    let mut answer = None;
    let mut usage = json!({});
    let mut completed = false;
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        if line.len() > MAX_MESSAGE * 2 {
            return Err("runtime_failure");
        }
        let event: Value = serde_json::from_slice(line).map_err(|_| "runtime_failure")?;
        if !event.is_object() {
            return Err("runtime_failure");
        }
        if seele_runtime::codex::tool_event(&event) {
            return Err("isolation_failure");
        }
        if let Some(item) = event.get("item") {
            if item["type"] == "agent_message" && event["type"] == "item.completed" {
                answer = Some(item["text"].as_str().ok_or("invalid_output")?.to_owned());
            }
        }
        if event["type"] == "turn.completed" {
            completed = true;
            usage = json!({"input":event["usage"]["input_tokens"],"output":event["usage"]["output_tokens"]});
        }
        if event["type"] == "turn.failed" || event["type"] == "error" {
            return Err("model_failure");
        }
    }
    if !completed {
        return Err("model_failure");
    }
    let answer = answer.ok_or("model_failure")?;
    if answer.len() > MAX_MESSAGE {
        return Err("invalid_output");
    }
    let result = serde_json::from_str(&answer).map_err(|_| "invalid_output")?;
    Ok((result, usage))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_tool_events_and_requires_completed_valid_json() {
        let success=b"{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"42\"}}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}\n";
        assert_eq!(
            parse_events(success).unwrap(),
            (json!(42), json!({"input":2,"output":1}))
        );
        assert_eq!(
            parse_events(b"{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\"}}")
                .unwrap_err(),
            "isolation_failure"
        );
        assert!(parse_events(
            b"{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"42\"}}"
        )
        .is_err());
    }
}
