//! Sanitized integration health metadata and stale-state policy.
use crate::value::{array, number, truthy, utf16_len};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

const STATES: &[&str] = &["healthy", "degraded", "disconnected", "setup-required"];
const ACTIONS: &[&str] = &["retry", "restart", "reconnect", "settings", "diagnostics"];
static PRIVATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(bearer\s|token[=:]|password[=:]|secret[=:]|gh[pousr]_|sk-[A-Za-z0-9])")
        .unwrap()
});
static SERVICE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9@_.-]{0,100}\.service$").unwrap());

fn identifier(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|value| {
        !value.is_empty()
            && value.len() <= 64
            && value.as_bytes()[0].is_ascii_lowercase()
            && value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    })
}
fn clean(value: Option<&Value>, limit: usize) -> Result<Value, String> {
    let text = value.and_then(Value::as_str).ok_or("Invalid metadata")?;
    if utf16_len(text) > limit || text.chars().any(|c| c <= '\u{1f}') {
        return Err("Invalid metadata".into());
    }
    if PRIVATE.is_match(text) {
        return Err("Private metadata".into());
    }
    Ok(json!(text))
}
fn fallback<'a>(value: Option<&'a Value>, default: &'a Value) -> &'a Value {
    value.filter(|value| truthy(Some(value))).unwrap_or(default)
}
fn registration(value: &Value) -> Result<Value, String> {
    if !identifier(value.get("id")) {
        return Err("Invalid provider".into());
    }
    let defaults = json!(["settings", "diagnostics"]);
    let allowed = fallback(value.get("actions"), &defaults);
    if !allowed.is_array()
        || array(Some(allowed))
            .iter()
            .any(|v| !v.as_str().is_some_and(|s| ACTIONS.contains(&s)))
    {
        return Err("Invalid action".into());
    }
    if truthy(value.get("service"))
        && !value["service"]
            .as_str()
            .is_some_and(|s| SERVICE.is_match(s))
    {
        return Err("Invalid managed service".into());
    }
    if truthy(value.get("setup")) && !identifier(value.get("setup")) {
        return Err("Invalid setup destination".into());
    }
    let deadline = number(value.get("deadline"));
    let deadline = if deadline == 0.0 || deadline.is_nan() {
        90000.0
    } else {
        deadline
    }
    .clamp(5000.0, 3600000.0);
    let empty = json!("");
    let disruptive = fallback(value.get("disruptive"), &Value::Null);
    if !disruptive.is_null() && !disruptive.is_array() {
        return Err("Invalid disruptive actions".into());
    }
    Ok(
        json!({"id":value["id"], "name":clean(value.get("name"),80)?, "deadline":deadline, "actions":allowed,
        "service":fallback(value.get("service"),&empty), "setup":fallback(value.get("setup"),&empty),
        "disruptive":array(Some(disruptive)).iter().filter(|action| array(Some(allowed)).contains(action)).collect::<Vec<_>>() }),
    )
}
fn publication(reg: &Value, value: &Value, now: f64) -> Result<Value, String> {
    if !value["state"].as_str().is_some_and(|s| STATES.contains(&s)) {
        return Err("Invalid state".into());
    }
    let defaults = json!([]);
    let offered = fallback(value.get("actions"), &defaults);
    if !offered.is_array()
        || array(Some(offered))
            .iter()
            .any(|action| !array(reg.get("actions")).contains(action))
    {
        return Err("Unsupported action".into());
    }
    let last = number(value.get("lastSuccess"));
    let last = if last.is_nan() || last == 0.0 {
        0.0
    } else {
        last
    };
    let empty = json!("");
    Ok(
        json!({"id":reg["id"],"name":reg["name"],"state":value["state"], "summary":clean(value.get("summary"),240)?,
        "detail":clean(Some(fallback(value.get("detail"),&empty)),1200)?,"lastSuccess":last.min(now).max(0.0),"actions":offered,"updated":now}),
    )
}
fn row_groups(registrations: &Value, values: &Value, now: f64, keys: &[Value]) -> Value {
    let mut attention = Vec::new();
    let mut healthy = Vec::new();
    for key in keys.iter().filter_map(Value::as_str) {
        let Some(reg) = registrations.get(key) else {
            continue;
        };
        let fallback_actions = |diagnostics| {
            array(reg.get("actions"))
                .iter()
                .filter(|action| {
                    matches!(action.as_str(), Some("settings" | "restart"))
                        || diagnostics && action.as_str() == Some("diagnostics")
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut row = values.get(key).filter(|v| truthy(Some(v))).cloned().unwrap_or_else(|| json!({"id":key,"name":reg["name"],"state":"stale","summary":"Waiting for provider status","lastSuccess":0,"detail":"","actions":fallback_actions(false),"updated":0}));
        if now - number(row.get("updated")) > number(reg.get("deadline")) {
            row["state"] = json!("stale");
            row["summary"] = json!("Provider stopped updating");
            row["actions"] = json!(fallback_actions(true));
        }
        if row["state"] == "healthy" {
            healthy.push(row);
        } else {
            attention.push(row);
        }
    }
    json!([attention, healthy])
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let first = args.first().unwrap_or(&Value::Null);
    match function {
        "identifier" => Ok(json!(identifier(args.first()))),
        "clean" => clean(args.first(), number(args.get(1)).max(0.0) as usize),
        "registration" => registration(first),
        "publication" => publication(
            first,
            args.get(1).unwrap_or(&Value::Null),
            number(args.get(2)),
        ),
        "rowGroups" => Ok(row_groups(
            first,
            args.get(1).unwrap_or(&Value::Null),
            number(args.get(2)),
            array(args.get(3)),
        )),
        _ => Err("unknown health function".into()),
    }
}
