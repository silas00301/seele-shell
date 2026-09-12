use crate::value::{number, text};
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"status":"idle","duration":1500,"remaining":1500,"deadline":0})
}
fn valid(value: &Value) -> bool {
    matches!(
        value["status"].as_str(),
        Some("idle" | "running" | "paused" | "done")
    ) && value["duration"]
        .as_f64()
        .is_some_and(|n| (1.0..=14400.0).contains(&n))
        && value["remaining"]
            .as_f64()
            .is_some_and(|n| n >= 0.0 && n <= number(value.get("duration")))
        && value["deadline"].as_f64().is_some_and(|n| n >= 0.0)
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match function {
        "initial" => initial(),
        "valid" => Value::Bool(args.first().is_some_and(valid)),
        "label" => {
            let seconds = number(args.first());
            let seconds = if seconds.is_finite() {
                seconds.ceil().max(0.0) as u64
            } else {
                0
            };
            Value::String(format!("{:02}:{:02}", seconds / 60, seconds % 60))
        }
        "update" => {
            let saved = args.first().filter(|value| valid(value));
            let default = initial();
            let state = saved.unwrap_or(&default);
            let action = text(args.get(1));
            let now = number(args.get(2));
            let duration = number(state.get("duration"));
            let status = state["status"].as_str().unwrap_or("idle");
            let mut remaining = number(state.get("remaining"));
            let deadline = number(state.get("deadline"));
            // Null means reuse the caller's object and avoid a spurious Qt
            // change signal. Invalid persisted states return the safe default.
            let unchanged = || saved.map_or_else(|| default.clone(), |_| Value::Null);
            if !now.is_finite() || now < 0.0 {
                return Ok(unchanged());
            }
            if status == "running" {
                remaining = ((deadline - now) / 1000.0).ceil().clamp(0.0, duration);
            }
            if action == "start" {
                let duration = number(args.get(3)) * 60.0;
                if !duration.is_finite() || !(1.0..=14400.0).contains(&duration) {
                    return Ok(unchanged());
                }
                let duration = duration.round();
                json!({"status":"running","duration":duration,"remaining":duration,"deadline":now + duration * 1000.0})
            } else if action == "cancel" {
                initial()
            } else if status == "running" && remaining == 0.0 {
                json!({"status":"done","duration":duration,"remaining":0,"deadline":0})
            } else if action == "pause" && status == "running" {
                json!({"status":"paused","duration":duration,"remaining":remaining,"deadline":0})
            } else if action == "resume" && status == "paused" {
                json!({"status":"running","duration":duration,"remaining":remaining,"deadline":now + remaining * 1000.0})
            } else if status == "running" && number(state.get("remaining")) != remaining {
                json!({"status":"running","duration":duration,"remaining":remaining,"deadline":deadline})
            } else {
                unchanged()
            }
        }
        _ => return Err(format!("Unknown focus function: {function}")),
    })
}
