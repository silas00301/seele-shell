//! Keybinding display and native input policy. Locale collation remains in the
//! host, which receives this bounded projection once per explicit refresh.
use crate::value::{trim, utf16_len};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;
static LABELS: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("keybindings.json")).unwrap());
static LUA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?i)^(?:__)?lua{}+[0-9]+$", crate::value::SPACE)).unwrap()
});
const MAX_ROWS: usize = 4096;
fn string(value: Option<&Value>, limit: usize) -> Result<&str, String> {
    match value {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(value)) if value.len() <= limit && !value.contains('\0') => Ok(value),
        _ => Err("Invalid keybinding metadata".into()),
    }
}
fn mask(value: Option<&Value>) -> Result<u32, String> {
    match value {
        None | Some(Value::Null) => Ok(0),
        Some(value) => value
            .as_u64()
            .filter(|n| *n <= u32::MAX as u64)
            .map(|n| n as u32)
            .ok_or("Invalid keybinding modifier mask".into()),
    }
}
fn modifiers(mask: u32) -> Vec<&'static str> {
    [(64, "Super"), (4, "Ctrl"), (8, "Alt"), (1, "Shift")]
        .into_iter()
        .filter_map(|(bit, name)| (mask & bit != 0).then_some(name))
        .collect()
}
fn label<'a>(table: &str, value: &'a str) -> &'a str {
    LABELS[table][value].as_str().unwrap_or(value)
}
pub fn input(row: &Value) -> Result<Value, String> {
    if !row.is_object() {
        return Err("Invalid keybinding selection".into());
    }
    let key = string(row.get("key"), 256)?;
    let modifiers = modifiers(mask(row.get("modmask"))?);
    if key.is_empty() || key.starts_with("mouse:") || key.starts_with("code:") {
        return Ok(json!([]));
    }
    if key.chars().any(char::is_control) {
        return Err("Invalid input key".into());
    }
    let input = LABELS["inputKeyNames"][key]
        .as_str()
        .or_else(|| LABELS["inputKeyNames"][key.to_uppercase()].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if utf16_len(key) == 1 {
                key.to_lowercase()
            } else {
                key.into()
            }
        });
    let mut args = Vec::new();
    for modifier in &modifiers {
        args.extend([
            "-M".to_owned(),
            label("inputModifiers", modifier).to_owned(),
        ]);
    }
    args.extend(["-k".into(), input]);
    for modifier in modifiers.iter().rev() {
        args.extend([
            "-m".to_owned(),
            label("inputModifiers", modifier).to_owned(),
        ]);
    }
    Ok(json!(args))
}
pub fn snapshot(value: &Value) -> Result<Value, String> {
    let rows = value
        .as_array()
        .filter(|r| r.len() <= MAX_ROWS)
        .ok_or("Invalid keybinding list")?;
    let mut result = vec![];
    for row in rows {
        if !row.is_object() {
            return Err("Invalid keybinding entry".into());
        }
        let key = string(row.get("key"), 256)?;
        if key.is_empty() || key.starts_with("mouse:") {
            continue;
        }
        let mask = mask(row.get("modmask"))?;
        let mut parts = modifiers(mask);
        parts.push(label("keyNames", key));
        let shortcut = parts.join(" + ");
        let dispatcher = string(row.get("dispatcher"), 256)?;
        let arg = string(row.get("arg"), 65536)?;
        let action = [dispatcher, arg]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let configured = trim(string(row.get("description"), 65536)?);
        let description = if !configured.is_empty() && !LUA.is_match(configured) {
            configured
        } else {
            LABELS["fallbackDescriptions"][&shortcut]
                .as_str()
                .unwrap_or("Hyprland keybinding")
        };
        let index = result.len();
        result.push(json!({"id":format!("{shortcut}-{action}-{index}"),"shortcut":shortcut,"action":action,"description":description,"key":key,"modmask":mask,"inputAllowed":!key.starts_with("code:")}));
    }
    Ok(json!(result))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_bounded_and_input_is_an_exact_argument_vector() {
        let rows=snapshot(&json!([{"key":"mouse:272"},{"key":"S","modmask":68,"dispatcher":"exec","arg":"private ignored","description":"__lua 42"}])).unwrap();
        assert_eq!(rows[0]["shortcut"], "Super + Ctrl + S");
        assert_eq!(
            rows[0]["description"],
            "Open a visible URI from the frozen screens"
        );
        assert_eq!(
            input(&rows[0]).unwrap(),
            json!([
                "-M", "logo", "-M", "ctrl", "-k", "s", "-m", "ctrl", "-m", "logo"
            ])
        );
        for value in [
            json!({"key":"\0"}),
            json!({"key":"x","modmask":-1}),
            json!({"key":42}),
        ] {
            assert!(input(&value).is_err());
        }
        assert!(snapshot(&json!(vec![json!({}); MAX_ROWS + 1])).is_err());
    }
}
