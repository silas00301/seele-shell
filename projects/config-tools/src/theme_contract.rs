//! The theme contract's document rules, as the native helper enforces them.
//!
//! `projects/shared/theme-contract.json` is the one schema. `Theme.qml`
//! implements its slots, `tests/theme-contract.js` holds the two to each other,
//! and this module reads the same file at build time, so every theme document
//! `seele-theme` lists or publishes is checked against the rules the shell was
//! written to, and a preset that is not a valid document never reaches a
//! surface. The messages match the JavaScript check's word for word.
use regex::Regex;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

const CONTRACT: &str = include_str!("../../shared/theme-contract.json");

enum Identity {
    Pattern(Regex),
    Choice(Vec<String>),
}

enum Slot {
    /// A slot a theme may supply, by its contract type.
    Supplied(String),
    /// Computed from the slots it names.
    Derived,
    /// Belongs to the contract, such as the privacy indicator colours.
    Fixed,
}

struct Contract {
    identity: Vec<(String, Identity)>,
    extensions: Vec<String>,
    required: Vec<String>,
    slots: BTreeMap<String, Slot>,
    weights: Vec<String>,
    easings: Vec<String>,
    color: Regex,
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn load() -> Result<Contract, String> {
    let contract: Value = serde_json::from_str(CONTRACT).map_err(|error| error.to_string())?;
    let documents = &contract["documents"];
    let mut identity = Vec::new();
    for (key, rule) in documents["identity"].as_object().ok_or("identity")? {
        let rule = match (
            rule.get("pattern").and_then(Value::as_str),
            rule.get("enum"),
        ) {
            (Some(pattern), _) => {
                Identity::Pattern(Regex::new(pattern).map_err(|error| error.to_string())?)
            }
            (None, Some(choices)) => Identity::Choice(strings(choices)),
            _ => return Err(format!("identity {key} has no rule")),
        };
        identity.push((key.clone(), rule));
    }
    let mut required = Vec::new();
    let mut slots = BTreeMap::new();
    for group in contract["groups"].as_array().ok_or("groups")? {
        for slot in group["slots"].as_array().ok_or("slots")? {
            let name = slot["name"].as_str().ok_or("slot name")?.to_owned();
            let kind = if slot.get("derived").is_some() {
                Slot::Derived
            } else if slot["fixed"] == true {
                Slot::Fixed
            } else {
                Slot::Supplied(slot["type"].as_str().ok_or("slot type")?.to_owned())
            };
            if slot["required"] == true {
                required.push(name.clone());
            }
            slots.insert(name, kind);
        }
    }
    Ok(Contract {
        identity,
        extensions: strings(&documents["extensions"]),
        required,
        slots,
        weights: keys(&contract["enums"]["weight"]),
        easings: keys(&contract["enums"]["easing"]),
        color: Regex::new("^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$").map_err(|e| e.to_string())?,
    })
}

fn contract() -> Result<&'static Contract, String> {
    static CONTRACT: OnceLock<Result<Contract, String>> = OnceLock::new();
    CONTRACT
        .get_or_init(load)
        .as_ref()
        .map_err(|error| format!("Invalid theme contract: {error}"))
}

impl Contract {
    fn is_color(&self, value: &Value) -> bool {
        value.as_str().is_some_and(|text| self.color.is_match(text))
    }
    fn holds(&self, kind: &str, value: &Value) -> bool {
        let text = value.as_str();
        match kind {
            "color" => self.is_color(value),
            "colorList" => value
                .as_array()
                .is_some_and(|items| !items.is_empty() && items.iter().all(|c| self.is_color(c))),
            "int" => {
                value.is_i64()
                    || value.is_u64()
                    || value.as_f64().is_some_and(|number| number.fract() == 0.0)
            }
            "real" => value.is_number(),
            "string" | "url" => text.is_some(),
            "path" => text.is_some_and(|path| path.is_empty() || path.starts_with('/')),
            "outputPaths" => value.as_object().is_some_and(|outputs| {
                outputs.iter().all(|(output, file)| {
                    output_name(output) && file.as_str().is_some_and(|path| path.starts_with('/'))
                })
            }),
            "weight" => text.is_some_and(|name| self.weights.iter().any(|w| w == name)),
            "easing" => text.is_some_and(|name| self.easings.iter().any(|e| e == name)),
            _ => false,
        }
    }
}

/// Whether `name` can name an output, as the compositor names one (`DP-1`,
/// `HDMI-A-1`, `eDP-1`): the contract's `outputPaths` keys, and the outputs
/// the host's wallpaper mapping names.
pub fn output_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The required palette roles, in the contract's order.
pub fn roles() -> Result<&'static [String], String> {
    Ok(&contract()?.required)
}

/// The slots a theme supplies a file for. A theme folder names each file
/// relative to itself; registration resolves it to an absolute path.
pub fn paths() -> Result<Vec<&'static str>, String> {
    Ok(contract()?
        .slots
        .iter()
        .filter(|(_, slot)| matches!(slot, Slot::Supplied(kind) if kind == "path"))
        .map(|(name, _)| name.as_str())
        .collect())
}

/// Every rule `document` breaks, or none. A theme document is a JSON object:
/// its identity, every required slot, and any other slot it supplies, of that
/// slot's type. Derived and fixed slots cannot be supplied, and a key the
/// contract does not name is refused unless it is a listed extension.
pub fn violations(document: &Value) -> Result<Vec<String>, String> {
    let contract = contract()?;
    let Some(object) = document.as_object() else {
        return Ok(vec!["a theme document is a JSON object".to_owned()]);
    };
    let mut errors = Vec::new();
    for (key, rule) in &contract.identity {
        let Some(value) = object.get(key) else {
            errors.push(format!("missing {key}"));
            continue;
        };
        let valid = match rule {
            Identity::Pattern(pattern) => value.as_str().is_some_and(|text| pattern.is_match(text)),
            Identity::Choice(choices) => value
                .as_str()
                .is_some_and(|text| choices.iter().any(|choice| choice == text)),
        };
        if !valid {
            errors.push(format!("{key}: {value}"));
        }
    }
    for name in &contract.required {
        if !object.contains_key(name) {
            errors.push(format!("missing required slot {name}"));
        }
    }
    for (key, value) in object {
        if contract.identity.iter().any(|(name, _)| name == key)
            || contract.extensions.contains(key)
        {
            continue;
        }
        match contract.slots.get(key) {
            None => errors.push(format!("{key}: not a contract slot")),
            Some(Slot::Derived) => {
                errors.push(format!("{key}: derived, so a theme cannot supply it"))
            }
            Some(Slot::Fixed) => errors.push(format!("{key}: fixed by the contract")),
            Some(Slot::Supplied(kind)) if !contract.holds(kind, value) => {
                errors.push(format!("{key}: {value} is not a {kind}"))
            }
            Some(Slot::Supplied(_)) => {}
        }
    }
    Ok(errors)
}

/// Refuses a document that breaks any rule, naming the first few.
pub fn check(document: &Value) -> crate::Result {
    let errors = violations(document)?;
    if errors.is_empty() {
        return Ok(());
    }
    let id = document.get("id").and_then(Value::as_str).unwrap_or("?");
    let shown: Vec<&str> = errors.iter().take(4).map(String::as_str).collect();
    Err(format!("Invalid theme document {id}: {}", shown.join("; ")).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal() -> Value {
        json!({
            "id": "example", "name": "Example", "mode": "dark",
            "base": "#1e1e2e", "mantle": "#181825", "crust": "#11111b",
            "surface": "#313244", "overlay": "#6c7086", "text": "#cdd6f4",
            "subtext": "#a6adc8", "accent": "#b4befe", "red": "#f38ba8",
            "green": "#a6e3a1", "yellow": "#f9e2af",
        })
    }
    fn with(key: &str, value: Value) -> Value {
        let mut document = minimal();
        document[key] = value;
        document
    }
    fn without(key: &str) -> Value {
        let mut document = minimal();
        document.as_object_mut().unwrap().remove(key);
        document
    }
    fn rejects(document: Value, message: &str) {
        let errors = violations(&document).unwrap();
        assert!(
            errors.iter().any(|error| error.starts_with(message)),
            "expected {message:?}, got {errors:?}"
        );
        assert!(check(&document).is_err());
    }

    #[test]
    fn the_embedded_contract_names_the_eleven_roles() {
        let contract = contract().unwrap();
        assert_eq!(
            contract.required,
            [
                "base", "mantle", "crust", "surface", "overlay", "text", "subtext", "accent",
                "red", "green", "yellow"
            ]
        );
        assert!(contract.slots.len() > contract.required.len());
        assert!(["palette", "terminal", "version"]
            .iter()
            .all(|key| contract.extensions.iter().any(|extension| extension == key)));
        assert_eq!(roles().unwrap(), contract.required.as_slice());
    }

    #[test]
    fn an_output_is_named_as_the_compositor_names_it() {
        for name in ["DP-1", "HDMI-A-1", "eDP-1", "Virtual-1", "HEADLESS_2.0"] {
            assert!(output_name(name), "{name}");
        }
        for name in ["", "DP 1", "DP-1\n", "../DP-1", "Ü-1", &"D".repeat(65)] {
            assert!(!output_name(name), "{name:?}");
        }
    }

    #[test]
    fn the_file_slots_are_the_sound_cues_and_the_wallpaper() {
        let paths = paths().unwrap();
        assert!(paths.contains(&"wallpaper") && paths.contains(&"soundNotification"));
        assert!(paths
            .iter()
            .all(|name| *name == "wallpaper" || name.starts_with("sound")));
    }

    #[test]
    fn the_required_slots_alone_make_a_theme() {
        assert_eq!(violations(&minimal()).unwrap(), Vec::<String>::new());
        check(&minimal()).unwrap();
    }

    #[test]
    fn supplied_slots_and_extensions_are_accepted_by_type() {
        let mut document = minimal();
        for (key, value) in [
            ("fontFamily", json!("Maple Mono NF CN")),
            ("radius", json!(12)),
            ("radius", json!(12.0)),
            ("lineHeightProse", json!(1.25)),
            ("weightStrong", json!("Bold")),
            ("easingStandard", json!("OutCubic")),
            ("wallpaper", json!("/etc/wallpaper/wallpaper.jpg")),
            ("wallpaper", json!("")),
            ("wallpaperOutputs", json!({})),
            (
                "wallpaperOutputs",
                json!({"DP-1": "/x.png", "HDMI-A-1": "/y.jpg"}),
            ),
            ("accent", json!("#ffb4befe")),
            ("palette", json!({"base00": "#1e1e2e"})),
            ("terminal", json!(["#1e1e2e"])),
            ("version", json!(2)),
        ] {
            document[key] = value;
            assert_eq!(
                violations(&document).unwrap(),
                Vec::<String>::new(),
                "{key}"
            );
        }
    }

    #[test]
    fn every_rule_the_javascript_check_states_is_enforced() {
        rejects(with("id", json!("Not An Id")), "id");
        rejects(with("mode", json!("dim")), "mode");
        rejects(with("name", json!(" leading space")), "name");
        rejects(without("name"), "missing name");
        rejects(without("accent"), "missing required slot accent");
        rejects(with("radius", json!("round")), "radius");
        rejects(with("radius", json!(8.5)), "radius");
        rejects(with("accent", json!("lavender")), "accent");
        rejects(with("panelColor", json!("#000000")), "panelColor: derived");
        rejects(with("iosRed", json!("#000000")), "iosRed: fixed");
        rejects(with("sparkle", json!(true)), "sparkle: not a contract slot");
        rejects(with("easingStandard", json!("Bounce")), "easingStandard");
        rejects(with("weightStrong", json!(600)), "weightStrong");
        rejects(with("wallpaper", json!("relative.jpg")), "wallpaper");
        for outputs in [
            json!({"DP-1": "relative.png"}),
            json!({"DP 1": "/x.png"}),
            json!({"": "/x.png"}),
            json!({"DP-1": 7}),
            json!(["/x.png"]),
            json!("/x.png"),
        ] {
            rejects(with("wallpaperOutputs", outputs), "wallpaperOutputs");
        }
        rejects(
            with("soundNotification", json!("ping.oga")),
            "soundNotification",
        );
        rejects(json!([]), "a theme document is a JSON object");
    }

    #[test]
    fn a_refusal_names_the_theme_and_what_it_broke() {
        let error = check(&with("radius", json!("round")))
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "Invalid theme document example: radius: \"round\" is not a int"
        );
    }
}
