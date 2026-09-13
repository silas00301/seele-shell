//! GitHub snapshot presentation and refresh policy.
use crate::value::{number, truthy};
use serde_json::{Value, json};
fn initial(host: Option<&Value>) -> Value {
    json!({"state":"idle","message":"Open this panel to load pull requests.","host":host.filter(|v|truthy(Some(v))).cloned().unwrap_or(json!("github.com")),"viewer":"","updatedAt":"","reviews":[],"authored":[],"reviewTotal":0,"authoredTotal":0,"stale":false})
}
fn receive(current: &Value, next: &Value) -> Value {
    let mut next = if next["state"]
        .as_str()
        .is_some_and(|s| ["ready", "error", "auth-required", "rate-limited"].contains(&s))
    {
        next.clone()
    } else {
        json!({"state":"error","message":"GitHub returned an unreadable response."})
    };
    if next["state"] == "ready" && next["reviews"].is_array() && next["authored"].is_array() {
        next["stale"] = json!(false);
        return next;
    }
    let mut value = if next["state"] == "auth-required" {
        initial(current.get("host"))
    } else {
        if current.is_object() {
            current.clone()
        } else {
            initial(None)
        }
    };
    value["state"] = if next["state"] == "ready" {
        json!("error")
    } else {
        next["state"].clone()
    };
    value["message"] = next
        .get("message")
        .filter(|v| truthy(Some(v)))
        .cloned()
        .unwrap_or(json!(
            "GitHub returned incomplete data. Try refreshing again."
        ));
    value["stale"] = json!(value["updatedAt"] != "");
    value
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let first = args.first().unwrap_or(&Value::Null);
    Ok(match function {
        "initial" => initial(args.first()),
        "receive" => receive(first, args.get(1).unwrap_or(&Value::Null)),
        "due" => json!(
            number(args.get(1)) - number(args.first())
                >= if truthy(args.get(3)) {
                    5000.0
                } else if args.get(2).and_then(Value::as_str) == Some("rate-limited") {
                    300000.0
                } else {
                    60000.0
                }
        ),
        "safeUrl" => json!(
            args.get(1)
                .and_then(Value::as_str)
                .is_some_and(|host| seele_runtime::github::safe_url(first, host).is_some())
        ),
        "checksLabel" => json!(match first.as_str() {
            Some("SUCCESS") => "Checks passing",
            Some("FAILURE") => "Checks failing",
            Some("ERROR") => "Checks errored",
            Some("PENDING") => "Checks running",
            Some("EXPECTED") => "Checks expected",
            _ => "No check status",
        }),
        "reviewLabel" => json!(if truthy(first.get("draft")) {
            "Draft"
        } else {
            match first["review"].as_str() {
                Some("APPROVED") => "Approved",
                Some("CHANGES_REQUESTED") => "Changes requested",
                Some("REVIEW_REQUIRED") => "Review required",
                _ => "Open",
            }
        }),
        _ => return Err("unknown github function".into()),
    })
}
