//! Provider-neutral transfer presentation and bounded explicit action policy.
use crate::value::{array, decode_uri_component, number, string, truthy};
use serde_json::{Value, json};

fn active(state: &str) -> bool {
    matches!(state, "sending" | "receiving" | "retrying")
}
fn failure(code: &str) -> &'static str {
    match code {
        "provider-unavailable" => "Tailscale is unavailable. Connect it and try again.",
        "service-unavailable" => "Transfers service is unavailable.",
        "target-unavailable" => "This device is unavailable. Bring it online, then retry.",
        "source-missing" => "A source file is missing. Choose the file again.",
        "source-changed" => "A source file changed. Choose it again.",
        "not-a-file" => "Choose files; folders are not supported.",
        "cancel-at-sender" => "Stop this incoming transfer on the sending device.",
        "interrupted" => "Transfer interrupted. Retry when the device is available.",
        "choose-files" => "Choose files before selecting a device.",
        "cancelled" => "Cancelled",
        "already-active" => "This transfer is already active.",
        "" => "",
        _ => "The action failed. Try again.",
    }
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let first = args.first().unwrap_or(&null);
    Ok(match function {
        "project" => {
            let groups = array(Some(first));
            if groups.len() > 4096 {
                return Err("transfer group limit exceeded".into());
            }
            let (mut attention, mut failed, mut running, mut unseen, mut size, mut bytes) =
                (false, false, false, 0usize, 0.0, 0.0);
            for group in groups {
                let state = group.get("state").and_then(Value::as_str).unwrap_or("");
                let seen = truthy(group.get("seen"));
                let active = active(state);
                failed |= state == "failed";
                attention |= active || state == "failed" || !seen;
                if !seen {
                    unseen += 1;
                }
                if active {
                    running = true;
                    size += number(group.get("size"));
                    bytes += number(group.get("bytes"));
                }
            }
            let detail = if running {
                if size > 0.0 {
                    format!("{}%", string(Some(&json!((bytes / size * 100.0).floor()))))
                } else {
                    "…".into()
                }
            } else if failed {
                "!".into()
            } else {
                unseen.to_string()
            };
            json!({"attention":attention,"barText":format!("󰇚 {detail}")})
        }
        "selectUrls" => {
            let urls = array(Some(first));
            if urls.len() > 256 {
                return Ok(json!({"error":"Choose at most 256 files."}));
            }
            let mut paths = Vec::new();
            let mut bytes = 0usize;
            for value in urls {
                let value = string(Some(value));
                let Some(value) = value.strip_prefix("file:///") else {
                    return Ok(json!({"error":"Only local files can be sent."}));
                };
                if value.len() > 32768 {
                    return Ok(json!({"error":"Invalid file."}));
                }
                let Ok(path) = decode_uri_component(value) else {
                    return Ok(json!({"error":"Invalid file."}));
                };
                bytes = bytes.saturating_add(path.len() * 6 + 4);
                if bytes > 250 * 1024 {
                    return Ok(json!({"error":"The file selection is too large."}));
                }
                paths.push(format!("/{path}"));
            }
            json!({"paths":paths})
        }
        "enqueue" => {
            let queue = array(Some(first));
            let value = args.get(1).unwrap_or(&null);
            if queue.contains(value) {
                Value::Null
            } else if queue.len() >= 128 {
                json!({"error":"Too many pending actions. Wait for the current action to finish."})
            } else {
                let mut queue = queue.to_vec();
                queue.push(value.clone());
                json!({"queue":queue})
            }
        }
        "failure" => json!(failure(&string(Some(first)))),
        _ => return Err("unknown Transfers UI function".into()),
    })
}
