//! Pure Caffeinate presentation and duration policy. One implementation serves
//! the menu bar item, the compact panel and the launcher's native endpoint, so
//! both surfaces name the same session in the same words.
use crate::value::{number, string, trim, truthy};
use serde_json::{Value, json};

/// A session shorter than a minute is indistinguishable from doing nothing, and
/// a day is the longest deliberate stretch worth an indefinite-looking timer.
const MIN_SECONDS: u64 = 60;
const MAX_SECONDS: u64 = 24 * 60 * 60;

fn failure(code: &str) -> &'static str {
    match code {
        "service-unavailable" => "Caffeinate is unavailable. Check its user service.",
        "inhibit-unavailable" => "The session manager refused the idle inhibitor.",
        "task-unavailable" => "That task has already ended. Refresh and choose another.",
        "task-unknown" => "That task is no longer listed. Refresh and choose another.",
        "invalid-duration" => "Enter a duration such as 45m, 2h or 1h30.",
        "duration-out-of-range" => "Choose between 1 minute and 24 hours.",
        "no-session" => "No Caffeinate session is active.",
        "invalid-request" => "That request is not valid.",
        "" => "",
        _ => "The action failed. Try again.",
    }
}

fn seconds(value: Option<&Value>) -> u64 {
    let value = number(value);
    if value.is_finite() && value > 0.0 {
        (value as u64).min(MAX_SECONDS * 2)
    } else {
        0
    }
}

/// Spelled out for the panel, where the line has room and the reading is slow.
fn long_label(total: u64) -> String {
    let (hours, minutes) = (total / 3600, total % 3600 / 60);
    if hours > 0 && minutes > 0 {
        format!("{hours} h {minutes} min")
    } else if hours > 0 {
        format!("{hours} h")
    } else if minutes > 0 {
        format!("{minutes} min")
    } else {
        format!("{total} s")
    }
}

/// Compact for the bar, where the item sits between others and pays for width.
fn short_label(total: u64) -> String {
    let (hours, minutes) = (total / 3600, total % 3600 / 60);
    if hours > 0 {
        format!("{hours}h{minutes:02}")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{total}s")
    }
}

fn bounded(total: Option<u64>) -> Result<u64, &'static str> {
    let total = total.ok_or("duration-out-of-range")?;
    if (MIN_SECONDS..=MAX_SECONDS).contains(&total) {
        Ok(total)
    } else {
        Err("duration-out-of-range")
    }
}

/// `1h30`, `90`, `90m`, `2 hours` and `1:30` all name the same hour and a half.
/// A bare number is minutes, which is what a typed duration almost always means.
fn parse_duration(value: &str) -> Result<u64, &'static str> {
    let value = trim(value).to_ascii_lowercase();
    if value.is_empty() || value.len() > 32 {
        return Err("invalid-duration");
    }
    if let Some((hours, minutes)) = value.split_once(':') {
        let minutes = minutes.trim();
        if minutes.len() != 2 || !minutes.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("invalid-duration");
        }
        let hours: u64 = hours.trim().parse().map_err(|_| "invalid-duration")?;
        let minutes: u64 = minutes.parse().map_err(|_| "invalid-duration")?;
        if minutes > 59 {
            return Err("invalid-duration");
        }
        return bounded(
            hours
                .checked_mul(3600)
                .and_then(|total| total.checked_add(minutes * 60)),
        );
    }
    let bytes = value.as_bytes();
    let (mut index, mut total, mut parts) = (0usize, 0u64, 0usize);
    while index < bytes.len() {
        while bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start || index - start > 6 {
            return Err("invalid-duration");
        }
        let count: u64 = value[start..index]
            .parse()
            .map_err(|_| "invalid-duration")?;
        while bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        let unit = index;
        while bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
            index += 1;
        }
        // An unqualified count is minutes, so `1h30` reads as an hour and a half
        // rather than thirty more hours.
        let step = match &value[unit..index] {
            "h" | "hr" | "hrs" | "hour" | "hours" => count.checked_mul(3600),
            "" | "m" | "min" | "mins" | "minute" | "minutes" => count.checked_mul(60),
            "s" | "sec" | "secs" | "second" | "seconds" => Some(count),
            _ => return Err("invalid-duration"),
        };
        total = total
            .checked_add(step.ok_or("duration-out-of-range")?)
            .ok_or("duration-out-of-range")?;
        parts += 1;
        if parts > 3 {
            return Err("invalid-duration");
        }
    }
    if parts == 0 {
        return Err("invalid-duration");
    }
    bounded(Some(total))
}

fn mode_label(mode: &str) -> &'static str {
    match mode {
        "duration" => "For a set time",
        "task" => "Until a task ends",
        "manual" => "Until stopped",
        _ => "Caffeinate",
    }
}

/// The bar item, the panel and the launcher all read one projection, so a
/// session cannot be described two ways at once.
fn project(snapshot: &Value) -> Value {
    let active = truthy(snapshot.get("active"));
    let mode = string(snapshot.get("mode"));
    let remaining = seconds(snapshot.get("remaining"));
    let elapsed = seconds(snapshot.get("elapsed"));
    let task = snapshot.get("task").cloned().unwrap_or(Value::Null);
    let label = string(task.get("label"));
    let detail = if !active {
        String::new()
    } else if mode == "duration" {
        format!("{} remaining", long_label(remaining))
    } else if mode == "task" && !label.is_empty() {
        label
    } else {
        format!("Active for {}", long_label(elapsed))
    };
    let bar = if !active {
        String::new()
    } else if mode == "duration" {
        format!("\u{f0176} {}", short_label(remaining))
    } else {
        "\u{f0176}".into()
    };
    let headline = if active {
        mode_label(&mode)
    } else {
        "Caffeinate"
    };
    let hover = if !active {
        "Caffeinate".to_owned()
    } else if detail.is_empty() {
        format!("Caffeinate · {headline}")
    } else {
        format!("Caffeinate · {headline} · {detail}")
    };
    json!({
        "active": active,
        "barText": bar,
        "headline": headline,
        "detail": detail,
        "task": task,
        "hoverText": hover,
    })
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let first = args.first().unwrap_or(&null);
    Ok(match function {
        "project" => project(first),
        "parseDuration" => match parse_duration(&string(Some(first))) {
            Ok(seconds) => json!({ "seconds": seconds }),
            Err(error) => json!({ "error": error }),
        },
        "duration" => json!(long_label(seconds(Some(first)))),
        "failure" => json!(failure(&string(Some(first)))),
        _ => return Err("unknown Caffeinate UI function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_durations_agree_on_one_hour_and_a_half() {
        for text in [
            "90",
            "90m",
            "90 minutes",
            "1h30",
            "1h 30m",
            "1:30",
            " 1H30M ",
        ] {
            assert_eq!(parse_duration(text), Ok(5400), "{text}");
        }
        assert_eq!(parse_duration("2h"), Ok(7200));
        assert_eq!(parse_duration("24h"), Ok(MAX_SECONDS));
        assert_eq!(parse_duration("60s"), Ok(MIN_SECONDS));
    }

    #[test]
    fn unbounded_or_unreadable_durations_never_start_a_session() {
        for text in [
            "",
            "   ",
            "h",
            "1x",
            "1:5",
            "1:60",
            "1:305",
            "1h2m3m4m",
            "99999999999999",
        ] {
            assert!(parse_duration(text).is_err(), "{text}");
        }
        assert_eq!(parse_duration("30s"), Err("duration-out-of-range"));
        assert_eq!(parse_duration("25h"), Err("duration-out-of-range"));
        assert_eq!(parse_duration("0"), Err("duration-out-of-range"));
    }

    #[test]
    fn an_inactive_session_draws_no_bar_item() {
        let idle = project(&json!({"active": false}));
        assert_eq!(idle["barText"], "");
        assert_eq!(idle["detail"], "");
        let timed = project(&json!({"active": true, "mode": "duration", "remaining": 4320}));
        assert_eq!(timed["barText"], "\u{f0176} 1h12");
        assert_eq!(timed["detail"], "1 h 12 min remaining");
        let build = json!({"label": "nix build", "kind": "build"});
        let task = project(&json!({"active": true, "mode": "task", "task": build}));
        assert_eq!(task["barText"], "\u{f0176}");
        assert_eq!(task["detail"], "nix build");
        let manual = project(&json!({"active": true, "mode": "manual", "elapsed": 780}));
        assert_eq!(manual["detail"], "Active for 13 min");
    }
}
