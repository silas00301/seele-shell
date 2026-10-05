//! Projection of CodexBar's reported quota windows, without guessing cadence from a slot.
use serde_json::{json, Value};

fn label(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").chars()
        .filter(|c| !c.is_control() && !matches!(*c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'))
        .take(120).collect::<String>().trim().to_owned()
}

fn cadence(window: &Value, fallback: &str) -> String {
    match window.get("windowMinutes").and_then(Value::as_u64) {
        Some(300) => "Session".into(),
        Some(1440) => "Daily".into(),
        Some(10080) => "Weekly".into(),
        Some(40320..=44640) => "Monthly".into(),
        Some(minutes) if minutes > 0 => format!("{minutes} min window"),
        _ => fallback.into(),
    }
}

fn project(window: &Value, name: String) -> Option<Value> {
    if window
        .get("isSyntheticPlaceholder")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return None;
    }
    let used = window.get("usedPercent").and_then(Value::as_f64)?;
    if !used.is_finite() {
        return None;
    }
    Some(json!({"name":name,"usedPercent":used,
        "resetsAt":label(window.get("resetsAt")),
        "resetDescription":label(window.get("resetDescription"))}))
}

pub(super) fn limits(record: &Value) -> Vec<Value> {
    let mut limits = Vec::new();
    for (slot, fallback) in [
        ("primary", "Primary limit"),
        ("secondary", "Secondary limit"),
        ("tertiary", "Additional limit"),
    ] {
        let window = &record["usage"][slot];
        let name = label(
            record
                .get("rateWindowLabels")
                .and_then(|labels| labels.get(slot)),
        );
        let name = if name.is_empty() {
            cadence(window, fallback)
        } else {
            name
        };
        if let Some(limit) = project(window, name) {
            limits.push(limit);
        }
    }
    if let Some(extras) = record["usage"]["extraRateWindows"].as_array() {
        for extra in extras.iter().take(32) {
            if extra.get("usageKnown").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let window = &extra["window"];
            let title = label(extra.get("title"));
            let name = if title.is_empty() {
                cadence(window, "Additional limit")
            } else {
                title
            };
            if let Some(limit) = project(window, name) {
                limits.push(limit);
            }
        }
    }
    limits
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_names_win_and_named_extras_keep_their_identity() {
        let rows = limits(
            &json!({"provider":"cursor", "rateWindowLabels":{"primary":"Plan", "secondary":"Cursor models"}, "usage":{
            "primary":{"usedPercent":60,"windowMinutes":43200},
            "secondary":{"usedPercent":20,"windowMinutes":43200},
            "extraRateWindows":[{"title":"Grok Bot Weekly", "window":{"usedPercent":10,"windowMinutes":10080}},
                {"title":"Unknown", "usageKnown":false,"window":{"usedPercent":100}}]}}),
        );
        assert_eq!(
            rows.iter()
                .map(|r| r["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["Plan", "Cursor models", "Grok Bot Weekly"]
        );
    }
    #[test]
    fn cadence_comes_from_duration_and_absent_metadata_stays_unknown() {
        for (minutes, name) in [
            (300, "Session"),
            (1440, "Daily"),
            (10080, "Weekly"),
            (43200, "Monthly"),
            (44640, "Monthly"),
            (60, "60 min window"),
        ] {
            assert_eq!(
                limits(&json!({"usage":{"secondary":{"usedPercent":20,"windowMinutes":minutes}}}))
                    [0]["name"],
                name
            );
        }
        assert_eq!(
            limits(&json!({"usage":{"secondary":{"usedPercent":20}}}))[0]["name"],
            "Secondary limit"
        );
    }
    #[test]
    fn absent_and_synthetic_windows_are_not_free_quota() {
        assert!(limits(&json!({"usage":{"primary":{"usedPercent":null}, "secondary":{"usedPercent":0,"isSyntheticPlaceholder":true}}})).is_empty());
        assert_eq!(
            limits(&json!({"usage":{"primary":{"usedPercent":0}}}))[0]["usedPercent"].as_f64(),
            Some(0.0)
        );
    }
    #[test]
    fn provider_text_is_bounded_and_reset_fallback_is_retained() {
        let rows = limits(
            &json!({"rateWindowLabels":{"primary":"Plan\u{202e}\n"},"usage":{"primary":{"usedPercent":25,"resetDescription":"Tomorrow\u{200b}"}}}),
        );
        assert_eq!(rows[0]["name"], "Plan");
        assert_eq!(rows[0]["resetDescription"], "Tomorrow");
        assert_eq!(label(Some(&json!("x".repeat(1000)))).len(), 120);
    }
}
