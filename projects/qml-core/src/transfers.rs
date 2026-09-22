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
        "history" => {
            let groups = array(Some(first));
            if groups.len() > 4096 {
                return Err("transfer group limit exceeded".into());
            }
            let query = string(args.get(1));
            if query.len() > 4096 {
                return Err("transfer search limit exceeded".into());
            }
            let query = query.trim().to_lowercase();
            let direction = string(args.get(2));
            let status = string(args.get(3));
            if !matches!(direction.as_str(), "all" | "incoming" | "outgoing")
                || !matches!(
                    status.as_str(),
                    "all" | "completed" | "failed" | "cancelled"
                )
            {
                return Err("invalid transfer history filter".into());
            }
            let mut running = Vec::new();
            let mut history = Vec::new();
            let mut total = 0;
            for (index, group) in groups.iter().enumerate() {
                let state = group.get("state").and_then(Value::as_str).unwrap_or("");
                // Filtering history must never hide a job's progress or Cancel.
                if active(state) {
                    running.push(index);
                    continue;
                }
                total += 1;
                if direction != "all"
                    && group.get("direction").and_then(Value::as_str) != Some(&direction)
                {
                    continue;
                }
                if status != "all" && state != status {
                    continue;
                }
                let matches = query.is_empty()
                    || string(group.get("device")).to_lowercase().contains(&query)
                    || array(group.get("files"))
                        .iter()
                        .any(|file| string(file.get("name")).to_lowercase().contains(&query));
                if matches {
                    history.push(index);
                }
            }
            let active_count = running.len();
            let matched = history.len();
            running.extend(history);
            // Return indices so Qt keeps the original entry and file indices.
            json!({"indices": running, "active": active_count, "matched": matched, "total": total})
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_matches_only_metadata_and_keeps_active_jobs() {
        let groups = json!([
            {"id":"1", "state":"completed", "direction":"incoming", "device":"Phone", "files":[{"name":"Änderung.pdf", "path":"/private/secret"}]},
            {"id":"2", "state":"failed", "direction":"outgoing", "device":"Tablet", "files":[{"name":"a"},{"name":"Report.txt"}]},
            {"id":"3", "state":"retrying", "direction":"outgoing"},
            {"id":"4", "state":"receiving", "direction":"incoming"},
            {"id":"5", "state":"cancelled", "direction":"incoming", "device":"Tablet"}
        ]);
        let filter = |query: &str, direction: &str, status: &str| {
            call(
                "history",
                &[
                    groups.clone(),
                    json!(query),
                    json!(direction),
                    json!(status),
                ],
            )
            .unwrap()
        };
        assert_eq!(filter("", "all", "all")["indices"], json!([2, 3, 0, 1, 4]));
        assert_eq!(
            filter(" ÄNDERUNG ", "incoming", "completed")["indices"],
            json!([2, 3, 0])
        );
        assert_eq!(
            filter("report", "outgoing", "failed")["indices"],
            json!([2, 3, 1])
        );
        assert_eq!(
            filter("tablet", "incoming", "cancelled")["indices"],
            json!([2, 3, 4])
        );
        let empty = filter("secret", "all", "all");
        assert_eq!(
            empty,
            json!({"indices":[2,3],"active":2,"matched":0,"total":3})
        );
        assert_eq!(groups[0]["files"][0]["name"], "Änderung.pdf");
    }

    #[test]
    fn history_bounds_and_validates_input() {
        assert!(
            call(
                "history",
                &[
                    json!([]),
                    json!("x".repeat(4097)),
                    json!("all"),
                    json!("all")
                ]
            )
            .is_err()
        );
        assert!(
            call(
                "history",
                &[
                    json!(vec![Value::Null; 4097]),
                    json!(""),
                    json!("all"),
                    json!("all")
                ]
            )
            .is_err()
        );
        assert!(
            call(
                "history",
                &[json!([]), json!(""), json!("unknown"), json!("all")]
            )
            .is_err()
        );
        assert!(
            call(
                "history",
                &[json!([]), json!(""), json!("all"), json!("sending")]
            )
            .is_err()
        );
        assert_eq!(
            call(
                "history",
                &[json!([]), json!(""), json!("all"), json!("all")]
            )
            .unwrap()["total"],
            0
        );
    }
}
