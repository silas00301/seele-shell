//! One lifecycle projection for Hermes' bar, panel and health readout.
use serde_json::{Value, json};
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    if function != "project" {
        return Err("Unknown Hermes projection".into());
    }
    let snapshot = args.first().unwrap_or(&Value::Null);
    let phase = snapshot["state"].as_str().unwrap_or("disconnected");
    let (label, tint, glyph) = match phase {
        "idle" => ("Idle", "green", "󰚩"),
        "listening" => ("Listening", "yellow", "󰍬"),
        "thinking" => ("Thinking", "accent", "󰚩"),
        "speaking" => ("Speaking", "accent", "󰕾"),
        _ => ("Disconnected", "overlay", "󰚩"),
    };
    let connected = matches!(phase, "idle" | "listening" | "thinking" | "speaking");
    let detail = if connected {
        "Connected through Hermes Desktop"
    } else if snapshot["desktop"] == true {
        "Connect in Hermes Desktop · Settings → Gateway"
    } else {
        "Open Hermes Desktop to connect"
    };
    let pending = snapshot["pending"]
        .as_array()
        .filter(|v| v.len() <= 4)
        .cloned()
        .unwrap_or_default();
    Ok(
        json!({"label":label,"tint":tint,"glyph":glyph,"active":matches!(phase,"listening"|"thinking"|"speaking"),"pending":pending,"detail":detail,"health":if connected {"healthy"} else {"disconnected"}}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_connection_does_not_depend_on_anonymous_default_gateway_probe() {
        let view = call(
            "project",
            &[json!({"state":"idle","desktop":true,"reachable":false})],
        )
        .unwrap();
        assert_eq!(view["detail"], "Connected through Hermes Desktop");
    }
    #[test]
    fn phases_are_distinct_and_unknown_is_disconnected() {
        for phase in ["idle", "listening", "thinking", "speaking"] {
            let v = call("project", &[json!({"state":phase})]).unwrap();
            assert_ne!(v["label"], "Disconnected");
        }
        assert_eq!(
            call("project", &[json!({"state":"invalid"})]).unwrap()["label"],
            "Disconnected"
        );
    }
}
