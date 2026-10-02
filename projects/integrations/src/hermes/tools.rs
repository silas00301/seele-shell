use super::{Config, Pending, Shared};
use crate::common::{self, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
pub fn unit(value: &str) -> bool {
    value.len() <= 128
        && value.ends_with(".service")
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.@-".contains(&c))
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
}
async fn command(program: &str, args: &[&str], cancel: CancellationToken) -> Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).env("LC_ALL", "C.UTF-8");
    let bytes = common::command(cmd, vec![], Duration::from_secs(30), 512 * 1024, cancel)
        .await
        .map_err(|_| "The local query failed or exceeded its bounds.")?;
    let raw = String::from_utf8(bytes).map_err(|_| "The local query returned invalid text.")?;
    Ok(seele_runtime::redact::secrets(&raw, true)
        .chars()
        .filter(|c| seele_runtime::redact::visible(*c) || *c == '\n' || *c == '\t')
        .collect())
}
pub async fn revision(config: &Config, cancel: CancellationToken) -> Result<String> {
    let dir = config.flake.to_str().ok_or("Invalid flake directory.")?;
    let revision = command(
        "jj",
        &["-R", dir, "log", "--no-graph", "-r", "@", "-T", "commit_id"],
        cancel,
    )
    .await?;
    let revision = revision.trim();
    if revision.len() != 40 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("The flake revision is unavailable.");
    }
    Ok(revision.into())
}
pub(super) fn immutable_flake(config: &Config, revision: &str) -> Result<String> {
    if revision.len() != 40 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid approved revision.");
    }
    let path =
        url::Url::from_directory_path(&config.flake).map_err(|_| "Invalid flake directory.")?;
    // Git-backed immutable fetch includes the committed submodule identities;
    // neither an edited checkout nor an edited child can change this target.
    Ok(format!("git+{path}?rev={revision}&submodules=1#nerv"))
}
pub fn catalog() -> Value {
    let tool = |name: &str,
                description: &str,
                properties: Value,
                required: Vec<&str>,
                write: bool| json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":!write,"destructiveHint":write,"openWorldHint":false}});
    json!([
        tool("flake_info","Read the configured Seele flake revision and host profile; no source files or credentials.",json!({}),vec![],false),
        tool("host_info","Read NixOS version and architecture; no machine identifiers.",json!({}),vec![],false),
        tool("generations","List retained NixOS generations.",json!({}),vec![],false),
        tool("generation_diff","Read package differences from the running system to a retained generation.",json!({"generation":{"type":"integer","minimum":1}}),vec!["generation"],false),
        tool("service_status","Read selected state fields of an explicitly allowed system service.",json!({"unit":{"type":"string"}}),vec!["unit"],false),
        tool("service_logs","Read up to 100 redacted journal lines for an explicitly allowed system service.",json!({"unit":{"type":"string"}}),vec!["unit"],false),
        tool("request_rebuild","Request a fixed nerv rebuild. Separate permission and local user approval are required; this never executes a rebuild.",json!({}),vec![],true)
    ])
}
fn argument_keys(name: &str, args: &Value) -> Result<()> {
    let object = args
        .as_object()
        .ok_or("Tool arguments must be an object.")?;
    let allowed = match name {
        "generation_diff" => Some("generation"),
        "service_status" | "service_logs" => Some("unit"),
        _ => None,
    };
    if object.keys().any(|k| Some(k.as_str()) != allowed) {
        return Err("Unsupported tool argument.");
    }
    Ok(())
}
fn closure(path: &Path) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path).map_err(|_| "Generation unavailable.")?;
    if path.parent() != Some(Path::new("/nix/store"))
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(seele_runtime::nix::store_name)
    {
        return Err("Generation unavailable.");
    }
    use std::os::unix::fs::MetadataExt;
    let info = std::fs::metadata(&path).map_err(|_| "Generation unavailable.")?;
    if !info.is_dir() || info.uid() != 0 || info.mode() & 0o022 != 0 {
        return Err("Generation unavailable.");
    }
    Ok(path)
}
pub async fn call(
    name: &str,
    args: &Value,
    config: &Config,
    state: &Shared,
    cancel: CancellationToken,
) -> Result<Value> {
    argument_keys(name, args)?;
    match name {
        "flake_info" => Ok(
            json!({"host":"nerv","system":"x86_64-linux","revision":revision(config,cancel).await?}),
        ),
        "host_info" => Ok(
            json!({"version":command("nixos-version",&[],cancel.clone()).await?.trim(),"architecture":command("uname",&["-m"],cancel).await?.trim()}),
        ),
        "generations" => {
            let text = command("seele-control", &["vicinae-generations"], cancel).await?;
            serde_json::from_str(&text).map_err(|_| "Generation query failed.")
        }
        "generation_diff" => {
            let generation = args["generation"]
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or("A positive generation is required.")?;
            let target = closure(&PathBuf::from(format!(
                "/nix/var/nix/profiles/system-{generation}-link"
            )))?;
            let running = closure(Path::new("/run/current-system"))?;
            let diff = command(
                "nvd",
                &[
                    "diff",
                    running.to_str().ok_or("Generation unavailable.")?,
                    target.to_str().ok_or("Generation unavailable.")?,
                ],
                cancel,
            )
            .await?;
            Ok(json!({"generation":generation,"diff":diff}))
        }
        "service_status" | "service_logs" => {
            let unit = args["unit"]
                .as_str()
                .filter(|u| config.services.iter().any(|s| s == u))
                .ok_or("This service is outside the configured read allowlist.")?;
            let output = if name == "service_status" {
                command(
                    "systemctl",
                    &[
                        "show",
                        "--property=Id,LoadState,ActiveState,SubState,Result,ExecMainStatus",
                        "--",
                        unit,
                    ],
                    cancel,
                )
                .await?
            } else {
                command(
                    "journalctl",
                    &[
                        "--unit",
                        unit,
                        "--lines=100",
                        "--no-pager",
                        "--output=cat",
                        "--since=-1h",
                    ],
                    cancel,
                )
                .await?
            };
            Ok(json!({"unit":unit,"output":output}))
        }
        "request_rebuild" => {
            if !config.rebuild {
                return Err(
                    "Rebuild permission is disabled. Enable it locally before requesting approval.",
                );
            }
            let revision = revision(config, cancel).await?;
            let mut state = state.lock().await;
            state.expire();
            if state.pending.len() >= 4 {
                return Err("There are already four pending rebuild approvals.");
            }
            let id = uuid::Uuid::new_v4().to_string();
            state.pending.insert(
                id.clone(),
                Pending {
                    created: Instant::now(),
                    revision: revision.clone(),
                },
            );
            Ok(
                json!({"state":"awaiting-local-approval","id":id,"revision":revision,"expiresIn":120}),
            )
        }
        _ => Err("Unknown Hermes tool."),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approved_flake_is_an_encoded_immutable_git_revision() {
        let config = Config {
            gateway: "http://hermes:9119".into(),
            peer: "hermes".into(),
            port: 8766,
            flake: "/tmp/flake with spaces?#".into(),
            services: vec![],
            rebuild: false,
        };
        let revision = "a".repeat(40);
        let target = immutable_flake(&config, &revision).unwrap();
        assert!(target.starts_with("git+file:///tmp/flake%20with%20spaces%3F%23/"));
        assert!(target.ends_with(&format!("?rev={revision}&submodules=1#nerv")));
        assert!(immutable_flake(&config, "caller&rev=other").is_err());
    }
    #[test]
    fn narrow_arguments_and_units() {
        assert!(!unit("--help.service"));
        assert!(!unit("a.service\n"));
        assert!(unit("nix-daemon.service"));
        assert!(argument_keys("request_rebuild", &json!({"command":"rm"})).is_err());
        assert!(argument_keys("generation_diff", &json!({"generation":1,"path":"/tmp"})).is_err());
    }
    #[tokio::test]
    async fn writes_and_non_allowlisted_reads_fail_closed() {
        let config = Config {
            gateway: "http://hermes:9119".into(),
            peer: "hermes".into(),
            port: 8766,
            flake: "/tmp".into(),
            services: vec!["nix-daemon.service".into()],
            rebuild: false,
        };
        let state = std::sync::Arc::new(tokio::sync::Mutex::new(super::super::State::new()));
        for (name, args) in [
            ("request_rebuild", json!({})),
            ("approve", json!({})),
            ("service_logs", json!({"unit":"sshd.service"})),
            ("generation_diff", json!({"generation":0})),
        ] {
            assert!(call(name, &args, &config, &state, CancellationToken::new())
                .await
                .is_err());
        }
        assert!(state.lock().await.pending.is_empty());
    }
}
