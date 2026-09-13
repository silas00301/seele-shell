use crate::value::{array, string, text, truthy};
use serde_json::{Value, json};
use std::collections::HashSet;
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    if function != "addresses" {
        return Err(format!("Unknown network function: {function}"));
    }
    let device = text(args.get(1));
    let valid_device = !device.is_empty()
        && device.len() <= 15
        && device
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte));
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for entry in array(args.first()) {
        let family = entry["family"].as_str().unwrap_or("");
        if !matches!(family, "inet" | "inet6")
            || truthy(entry.get("tentative"))
            || truthy(entry.get("dadfailed"))
            || entry["valid_life_time"].as_f64() == Some(0.0)
            || entry["scope"] == "host"
        {
            continue;
        }
        let mut value = text(entry.get("local"));
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || b":.".contains(&byte))
        {
            continue;
        }
        let ipv6 = family == "inet6";
        if ipv6 && entry["scope"] == "link" {
            if !valid_device {
                continue;
            }
            value.push('%');
            value.push_str(&device);
        }
        if !seen.insert(value.clone()) {
            continue;
        }
        let prefix = entry["prefixlen"].as_f64().filter(|n| n.fract() == 0.0);
        let detail = format!(
            "{value}{}",
            if prefix.is_some() {
                format!("/{}", string(entry.get("prefixlen")))
            } else {
                String::new()
            }
        );
        rows.push(json!({"label":if ipv6 {"IPv6"}else{"IPv4"},"value":value,"detail":detail}));
        if rows.len() == 8 {
            break;
        }
    }
    Ok(json!(rows))
}
