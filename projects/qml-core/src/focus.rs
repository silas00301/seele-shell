use crate::value::{number, text};
use serde_json::{Value, json};

const MAX_SECONDS: f64 = 240.0 * 60.0;
const EXTENSION_SECONDS: f64 = 5.0 * 60.0;

fn custom_minutes(value: Option<&Value>) -> Option<u32> {
    let input = value?.as_str()?.trim();
    if input.is_empty() || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    input
        .parse::<u32>()
        .ok()
        .filter(|minutes| (1..=240).contains(minutes))
}

fn can_extend(state: &Value) -> bool {
    valid(state)
        && matches!(state["status"].as_str(), Some("running" | "paused"))
        && number(state.get("remaining")) > 0.0
        && number(state.get("duration")) + EXTENSION_SECONDS <= MAX_SECONDS
}

fn initial() -> Value {
    json!({"status":"idle","duration":1500,"remaining":1500,"deadline":0})
}
fn valid(value: &Value) -> bool {
    matches!(
        value["status"].as_str(),
        Some("idle" | "running" | "paused" | "done")
    ) && value["duration"]
        .as_f64()
        .is_some_and(|n| (1.0..=MAX_SECONDS).contains(&n))
        && value["remaining"]
            .as_f64()
            .is_some_and(|n| n >= 0.0 && n <= number(value.get("duration")))
        && value["deadline"].as_f64().is_some_and(|n| n >= 0.0)
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match function {
        "initial" => initial(),
        "valid" => Value::Bool(args.first().is_some_and(valid)),
        "customInput" => {
            let minutes = custom_minutes(args.first());
            json!({"valid":minutes.is_some(), "minutes":minutes,
                "hint": if minutes.is_some() || text(args.first()).trim().is_empty() {
                    "1–240 minutes"
                } else { "Enter a whole number from 1 to 240" }})
        }
        "canExtend" => Value::Bool(args.first().is_some_and(can_extend)),
        "extensionHint" => Value::String(
            if args.first().is_some_and(|state| {
                valid(state)
                    && matches!(state["status"].as_str(), Some("running" | "paused"))
                    && !can_extend(state)
            }) {
                "+5 min would exceed the 4-hour session limit"
            } else {
                ""
            }
            .into(),
        ),
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
            if action == "start" || action == "custom" {
                let duration = if action == "custom" {
                    let Some(minutes) = custom_minutes(args.get(3)) else {
                        return Ok(unchanged());
                    };
                    f64::from(minutes) * 60.0
                } else {
                    number(args.get(3)) * 60.0
                };
                if !duration.is_finite() || !(1.0..=MAX_SECONDS).contains(&duration) {
                    return Ok(unchanged());
                }
                let duration = duration.round();
                json!({"status":"running","duration":duration,"remaining":duration,"deadline":now + duration * 1000.0})
            } else if action == "cancel" {
                initial()
            } else if status == "running" && remaining == 0.0 {
                json!({"status":"done","duration":duration,"remaining":0,"deadline":0})
            } else if action == "extend" && can_extend(state) {
                // Keep the absolute deadline, including its subsecond precision.
                // An expired running timer completes above, never restarts here.
                let deadline = if status == "running" {
                    deadline + EXTENSION_SECONDS * 1000.0
                } else {
                    0.0
                };
                json!({"status":status,"duration":duration + EXTENSION_SECONDS,
                    "remaining":remaining + EXTENSION_SECONDS,"deadline":deadline})
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

#[cfg(test)]
mod tests {
    use super::*;

    fn update(state: &Value, action: &str, now: f64, input: Value) -> Value {
        call("update", &[state.clone(), json!(action), json!(now), input]).unwrap()
    }

    #[test]
    fn custom_duration_is_strict_and_bounded() {
        for input in [
            "", "0", "241", "1.5", "-1", "+1", "1e2", "0x10", "１２", "12m",
        ] {
            assert_eq!(custom_minutes(Some(&json!(input))), None, "{input}");
            assert_eq!(update(&initial(), "custom", 0.0, json!(input)), Value::Null);
        }
        for minutes in [1, 37, 240] {
            let state = update(&initial(), "custom", 1000.0, json!(format!(" {minutes} ")));
            assert_eq!(state["duration"], json!(f64::from(minutes) * 60.0));
            assert_eq!(
                state["deadline"],
                json!(1000.0 + f64::from(minutes) * 60000.0)
            );
        }
    }

    #[test]
    fn extension_keeps_absolute_deadline_and_paused_state() {
        let running = update(&initial(), "custom", 123.0, json!("37"));
        let extended = update(&running, "extend", 456.0, Value::Null);
        assert_eq!(extended["deadline"], json!(2520123.0));
        let paused = update(&extended, "pause", 1234.0, Value::Null);
        let extended = update(&paused, "extend", 9000000.0, Value::Null);
        assert_eq!(extended["status"], "paused");
        assert_eq!(extended["deadline"], 0.0);
        assert_eq!(
            number(extended.get("remaining")),
            number(paused.get("remaining")) + 300.0
        );
        let resumed = update(&extended, "resume", 10000000.0, Value::Null);
        assert_eq!(
            number(resumed.get("deadline")),
            10000000.0 + number(extended.get("remaining")) * 1000.0
        );
    }

    #[test]
    fn extension_cannot_exceed_session_limit() {
        let state = update(&initial(), "custom", 0.0, json!("235"));
        assert!(can_extend(&state));
        let state = update(&state, "extend", 0.0, Value::Null);
        assert_eq!(state["duration"], MAX_SECONDS);
        assert!(!can_extend(&state));
        assert_eq!(update(&state, "extend", 0.0, Value::Null), Value::Null);
        let near_limit = update(&initial(), "custom", 0.0, json!("236"));
        assert!(!can_extend(&near_limit));
        assert_eq!(update(&near_limit, "extend", 0.0, Value::Null), Value::Null);
    }

    #[test]
    fn extension_cannot_revive_expired_or_idle_timer() {
        let state = update(&initial(), "custom", 0.0, json!("1"));
        let done = update(&state, "extend", 60000.0, Value::Null);
        assert_eq!(done["status"], "done");
        for action in ["extend", "tick", "resume"] {
            assert_eq!(update(&done, action, 60001.0, Value::Null), Value::Null);
        }
        assert_eq!(update(&initial(), "extend", 0.0, Value::Null), Value::Null);
    }
}
