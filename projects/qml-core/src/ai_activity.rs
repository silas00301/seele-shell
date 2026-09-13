use crate::value::{array, number, string, text};
use serde_json::{Value, json};
fn active(state: &str) -> bool {
    matches!(state, "queued" | "running" | "retrying")
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match function {
        "actions" => match args.first().and_then(Value::as_str) {
            Some("queued") => {
                json!([{"op":"cancel","label":"Cancel"},{"op":"next","label":"Do next"}])
            }
            Some("running" | "retrying") => json!([{"op":"cancel","label":"Cancel"}]),
            Some("failed") => {
                json!([{"op":"retry","label":"Retry"},{"op":"release","label":"Dismiss"}])
            }
            _ => json!([]),
        },
        "rows" => {
            let now = number(args.get(1));
            json!(array(args.first()).iter().filter(|job| {
                let state = job["state"].as_str().unwrap_or("");
                active(state) || state == "failed" || matches!(state,"succeeded"|"cancelled"|"superseded") && now - number(job.get("updated")) < 5.0
            }).map(|job| json!({
                "id":string(job.get("id")),"consumer":string(job.get("consumer")),"label":string(job.get("label")),
                "state":string(job.get("state")),"created":number(job.get("created")),"updated":number(job.get("updated")),
                "model":string(job.get("model")),"attempts":number(job.get("attempts")),"queueDuration":number(job.get("queueDuration")),
                "tokens":{"input":number(Some(&job["tokens"]["input"])),"output":number(Some(&job["tokens"]["output"]))},
                "error":text(job.get("error"))
            })).collect::<Vec<_>>())
        }
        "indicator" => {
            let jobs = array(args.first());
            json!(if jobs.iter().any(|job| job["state"] == "failed") {
                "failed"
            } else if jobs
                .iter()
                .any(|job| active(job["state"].as_str().unwrap_or("")))
            {
                "active"
            } else {
                ""
            })
        }
        _ => return Err(format!("Unknown activity function: {function}")),
    })
}
