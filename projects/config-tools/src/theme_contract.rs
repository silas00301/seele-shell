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
    /// Computed from the slots it names unless the theme gives a value of
    /// its contract type in its place, as a Material theme's tonal surfaces
    /// and springs are.
    Suppliable(String),
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
    capitalizations: Vec<String>,
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
            let kind = if slot.get("derived").is_some() && slot["suppliable"] == true {
                Slot::Suppliable(slot["type"].as_str().ok_or("slot type")?.to_owned())
            } else if slot.get("derived").is_some() {
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
        capitalizations: keys(&contract["enums"]["capitalization"]),
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
            "capitalization" => {
                text.is_some_and(|name| self.capitalizations.iter().any(|c| c == name))
            }
            // A spline ends where the transition does, at (1, 1); one that
            // ends anywhere else would leave its property short of the target.
            "curve" => value.as_array().is_some_and(|numbers| {
                let point = |index: usize| numbers[index].as_f64();
                numbers.len() % 6 == 0
                    && numbers.len() <= 48
                    && numbers
                        .iter()
                        .all(|number| number.as_f64().is_some_and(f64::is_finite))
                    && (numbers.is_empty()
                        || (point(numbers.len() - 2) == Some(1.0)
                            && point(numbers.len() - 1) == Some(1.0)))
            }),
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

/// The integer defaults of the slots in `group`, in the contract's order.
pub fn int_defaults(group: &str) -> Result<Vec<(String, i64)>, String> {
    let contract: Value = serde_json::from_str(CONTRACT).map_err(|error| error.to_string())?;
    let slots = contract["groups"]
        .as_array()
        .and_then(|groups| groups.iter().find(|entry| entry["name"] == group))
        .and_then(|entry| entry["slots"].as_array())
        .ok_or_else(|| format!("no contract group {group}"))?;
    Ok(slots
        .iter()
        .filter(|slot| slot["type"] == "int")
        .filter_map(|slot| Some((slot["name"].as_str()?.to_owned(), slot["default"].as_i64()?)))
        .collect())
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
            Some(Slot::Suppliable(kind) | Slot::Supplied(kind)) if !contract.holds(kind, value) => {
                errors.push(format!("{key}: {value} is not a {kind}"))
            }
            Some(Slot::Suppliable(_) | Slot::Supplied(_)) => {}
        }
    }
    // An easing is BezierSpline exactly when its curve is supplied, so a
    // transition never runs on a spline it was not given. A suppliable easing
    // and its curve are supplied together, or their derivation stands for
    // both.
    for (name, slot) in &contract.slots {
        let (Slot::Supplied(kind) | Slot::Suppliable(kind)) = slot else {
            continue;
        };
        if kind != "curve" {
            continue;
        }
        let easing = name.trim_end_matches("Curve");
        if matches!(slot, Slot::Suppliable(_)) {
            if object.contains_key(easing) != object.contains_key(name) {
                errors.push(format!("{name}: {easing} and {name} are supplied together"));
                continue;
            }
            if !object.contains_key(easing) {
                continue;
            }
        }
        let spline = object
            .get(easing)
            .and_then(Value::as_str)
            .is_some_and(|easing| easing == "BezierSpline");
        let Some(curve) = object
            .get(name)
            .map_or(Some(0), |curve| curve.as_array().map(Vec::len))
        else {
            continue;
        };
        if spline && curve == 0 {
            errors.push(format!(
                "{name}: {easing} is BezierSpline, so it needs a curve"
            ));
        } else if !spline && curve > 0 {
            errors.push(format!("{name}: a curve needs {easing} to be BezierSpline"));
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
    fn the_file_slots_are_the_sound_cues_the_wallpaper_and_the_assets() {
        let paths = paths().unwrap();
        for slot in [
            "wallpaper",
            "soundNotification",
            "fontFileRegular",
            "fontFileStrong",
            "surfaceTexture",
        ] {
            assert!(paths.contains(&slot), "{slot}");
        }
        assert!(paths.iter().all(|name| *name == "wallpaper"
            || *name == "surfaceTexture"
            || name.starts_with("sound")
            || name.starts_with("fontFile")));
    }

    #[test]
    fn a_suppliable_spline_comes_whole() {
        let mut document = with("easingSpatial", json!("BezierSpline"));
        document["easingSpatialCurve"] = json!([0.2, 0, 0, 1, 1, 1]);
        assert_eq!(violations(&document).unwrap(), Vec::<String>::new());
        document["easingSpatial"] = json!("OutCubic");
        document["easingSpatialCurve"] = json!([]);
        assert_eq!(violations(&document).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_spline_easing_comes_with_its_curve() {
        let mut document = with("easingExit", json!("BezierSpline"));
        document["easingExitCurve"] = json!([0.3, 0, 0.8, 0.15, 1, 1]);
        assert_eq!(violations(&document).unwrap(), Vec::<String>::new());
        let long: Vec<f64> = [0.3, 0.0, 0.8, 0.15, 1.0, 1.0].repeat(9);
        document["easingExitCurve"] = json!(long);
        assert!(!violations(&document).unwrap().is_empty());
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
            ("material", json!({"seed": "#6750a4", "contrast": 0.0})),
            ("cardColor", json!("#202020")),
            ("panelBorder", json!("#00000000")),
            ("radiusPressed", json!(8)),
            ("focusRingGap", json!(-1)),
            ("durationSpatial", json!(421)),
            ("titleCapitalization", json!("AllUppercase")),
            ("labelCapitalization", json!("MixedCase")),
            ("panelCut", json!(12)),
            ("panelTrimColor", json!("#ff3355")),
            (
                "fontFileRegular",
                json!("/themes/cyberpunk/Face-Regular.ttf"),
            ),
            ("surfaceTexture", json!("")),
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
        rejects(with("colorLensSize", json!(120)), "colorLensSize: derived");
        rejects(with("mutedText", json!("#000000")), "mutedText: derived");
        rejects(with("panelColor", json!("ink")), "panelColor");
        rejects(
            with("easingStandard", json!("BezierSpline")),
            "easingStandardCurve: easingStandard is BezierSpline",
        );
        rejects(
            with("easingStandardCurve", json!([0.2, 0, 0, 1, 1, 1])),
            "easingStandardCurve: a curve needs",
        );
        let mut short = with("easingStandard", json!("BezierSpline"));
        short["easingStandardCurve"] = json!([0.2, 0, 0, 1]);
        rejects(short, "easingStandardCurve");
        let mut unfinished = with("easingStandard", json!("BezierSpline"));
        unfinished["easingStandardCurve"] = json!([0.2, 0, 0, 1, 0.5, 0.5]);
        rejects(unfinished, "easingStandardCurve");
        let mut midway = with("easingStandard", json!("BezierSpline"));
        midway["easingStandardCurve"] = json!([0.1, 0, 0.2, 0.6, 1, 1, 0.3, 0.7, 0.5, 1, 0.9, 1]);
        rejects(midway, "easingStandardCurve");
        rejects(with("durationSpatial", json!("fast")), "durationSpatial");
        rejects(with("radiusPressed", json!(7.5)), "radiusPressed");
        rejects(
            with("easingSpatial", json!("BezierSpline")),
            "easingSpatialCurve: easingSpatial and easingSpatialCurve are supplied together",
        );
        rejects(
            with("easingSpatialCurve", json!([0.2, 0, 0, 1, 1, 1])),
            "easingSpatialCurve: easingSpatial and easingSpatialCurve are supplied together",
        );
        let mut named = with("easingSpatial", json!("OutBack"));
        named["easingSpatialCurve"] = json!([0.2, 0, 0, 1, 1, 1]);
        rejects(named, "easingSpatialCurve: a curve needs");
        let mut short_spring = with("easingSpatial", json!("BezierSpline"));
        short_spring["easingSpatialCurve"] = json!([0.2, 0, 0, 1, 0.5, 0.5]);
        rejects(short_spring, "easingSpatialCurve");
        rejects(with("iosRed", json!("#000000")), "iosRed: fixed");
        rejects(with("sparkle", json!(true)), "sparkle: not a contract slot");
        rejects(with("easingStandard", json!("Bounce")), "easingStandard");
        rejects(
            with("titleCapitalization", json!("Shouting")),
            "titleCapitalization",
        );
        rejects(with("labelCapitalization", json!(4)), "labelCapitalization");
        rejects(
            with("fontFileRegular", json!("Face-Regular.ttf")),
            "fontFileRegular",
        );
        rejects(with("panelTrimColor", json!("neon")), "panelTrimColor");
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
