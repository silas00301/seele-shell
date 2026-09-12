//! Pure native preparation of Vicinae snapshots. React and locale collation
//! remain in the extension; no formatter spawns a process during rendering.
mod keybindings;
use crate::value::{array, is_space, string, text, utf16_len};
use regex::Regex;
use seele_runtime::nix::store_basename;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;
const PROFILE_ROOT: &str = "/nix/var/nix/profiles";
const MAX_ROWS: usize = 4096;
const MAX_BYTES: usize = 4 * 1024 * 1024;
static CSI: LazyLock<Regex> = LazyLock::new(|| Regex::new("\u{1b}\\[[0-?]*[ -/]*[@-~]").unwrap());
fn integer(value: &Value, positive: bool) -> Result<u64, String> {
    let value = value
        .as_f64()
        .filter(|n| {
            n.fract() == 0.0
                && *n >= if positive { 1.0 } else { 0.0 }
                && *n <= 9_007_199_254_740_991.0
        })
        .ok_or("Invalid numeric identity")?;
    Ok(value as u64)
}
fn one_line(value: Option<&Value>, fallback: &str) -> Result<String, String> {
    let Some(value) = value.and_then(Value::as_str) else {
        return Ok(fallback.into());
    };
    if utf16_len(value) > 4096 {
        return Err("Generation field exceeds its limit".into());
    }
    let value = value
        .chars()
        .map(|c| {
            if c <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect::<String>();
    let value = value
        .split(is_space)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(if value.is_empty() {
        fallback.into()
    } else {
        value
    })
}
fn escape(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        if "\\`*_{}[]<>#+.!|~-".contains(c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
fn generation(value: &Value) -> Result<Value, String> {
    if !value.is_object() {
        return Err("Generation entry must be an object".into());
    }
    let number = integer(&value["generation"], true)?;
    let specials = array(value.get("specialisations"));
    if specials.len() > 64 {
        return Err("Too many specialisations".into());
    }
    let specialisations = specials
        .iter()
        .map(|v| one_line(Some(v), "Unknown"))
        .collect::<Result<Vec<_>, _>>()?;
    let date = one_line(value.get("date"), "Unknown")?;
    let nixos = one_line(value.get("nixosVersion"), "Unknown")?;
    let kernel = one_line(value.get("kernelVersion"), "Unknown")?;
    let revision = one_line(value.get("configurationRevision"), "")?;
    Ok(
        json!({"generation":number,"date":date,"nixosVersion":nixos,"kernelVersion":kernel,"configurationRevision":revision,
        "specialisations":specialisations,"profilePath":format!("{PROFILE_ROOT}/system-{number}-link"),"active":false,
        "escaped":{"date":escape(&date),"nixosVersion":escape(&nixos),"kernelVersion":escape(&kernel),"configurationRevision":escape(&revision),
            "specialisations":specialisations.iter().map(|s|escape(s)).collect::<Vec<_>>()}}),
    )
}
fn parse_generations(payload: &str) -> Result<Value, String> {
    if payload.len() > MAX_BYTES {
        return Err("Generation list exceeds its limit".into());
    }
    let values: Value = serde_json::from_str(payload).map_err(|_| "Invalid generation list")?;
    let rows = values
        .as_array()
        .filter(|rows| rows.len() <= MAX_ROWS)
        .ok_or("Invalid generation list")?;
    let mut seen = HashSet::new();
    let mut generations = rows
        .iter()
        .map(|value| {
            let next = generation(value)?;
            if !seen.insert(next["generation"].as_u64().unwrap_or_default()) {
                return Err("Duplicate generation number".into());
            }
            Ok(next)
        })
        .collect::<Result<Vec<_>, String>>()?;
    generations.sort_by_key(|g| std::cmp::Reverse(g["generation"].as_u64()));
    Ok(json!(generations))
}
fn switch_arguments(generation: &Value, target: &Value, running: &Value) -> Result<Value, String> {
    let number = integer(generation, true)?;
    let target = target
        .as_str()
        .and_then(store_basename)
        .ok_or("Reviewed target is unavailable")?;
    let running = running
        .as_str()
        .and_then(store_basename)
        .ok_or("Reviewed running system is unavailable")?;
    Ok(json!([number.to_string(), target, running]))
}
fn package_diff(output: &str, maximum: usize) -> String {
    let clean = CSI
        .replace_all(output, "")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let clean = clean
        .chars()
        .filter(|c| {
            !('\0'..='\u{8}').contains(c)
                && !matches!(c, '\u{b}' | '\u{c}')
                && !('\u{e}'..='\u{1f}').contains(c)
                && !('\u{7f}'..='\u{9f}').contains(c)
        })
        .collect::<String>();
    let mut text = crate::value::trim(&clean).to_owned();
    if text.is_empty() {
        return "_No package changes reported._".into();
    }
    if utf16_len(&text) > maximum {
        // Never split a Unicode scalar while enforcing Qt/JS UTF-16 limits.
        let mut units = 0;
        let bytes = text
            .char_indices()
            .take_while(|(_, c)| {
                units += c.len_utf16();
                units <= maximum
            })
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(0);
        text.truncate(bytes);
        text.push_str("\n… diff truncated");
    }
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
fn clients(values: &[Value]) -> Result<Value, String> {
    if values.len() > MAX_ROWS {
        return Err("Too many windows".into());
    }
    let mut groups = BTreeMap::<i64, Vec<Value>>::new();
    for value in values {
        if value["mapped"] != true
            || value["hidden"] == true
            || text(value.get("class")).to_lowercase() == "vicinae"
        {
            continue;
        }
        let address = value["address"].as_str().ok_or("Invalid window address")?;
        if !address.starts_with("0x")
            || address.len() < 3
            || address.len() > 18
            || !address[2..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Invalid window address".into());
        }
        let mut entry = serde_json::Map::new();
        for key in ["address", "title", "class"] {
            let field = value[key].as_str().unwrap_or("");
            if field.len() > 16384 {
                return Err("Window field exceeds its limit".into());
            }
            entry.insert(key.into(), json!(field));
        }
        let workspace = &value["workspace"];
        entry.insert(
            "workspace".into(),
            json!({"id":workspace["id"],"name":text(workspace.get("name"))}),
        );
        entry.insert("monitor".into(), value["monitor"].clone());
        let order = value["focusHistoryID"].as_i64().unwrap_or(0);
        entry.insert("focusHistoryID".into(), json!(order));
        groups.entry(order).or_default().push(Value::Object(entry));
    }
    // The host's localeCompare is the exact tie-breaker; it sees only one
    // equal-focus group at a time and cannot change native policy ordering.
    Ok(json!(groups.into_values().collect::<Vec<_>>()))
}
fn audio_selection(wanted: &Value, current: &[Value], toggle: bool) -> Result<Value, String> {
    integer(&wanted["id"], false)?;
    if !wanted["profile"].is_null() {
        integer(&wanted["profile"], false)?;
    }
    if current.len() > 512 {
        return Err("Too many audio devices".into());
    }
    let found = current
        .iter()
        .find(|candidate| {
            candidate["kind"] == wanted["kind"]
                && if !text(wanted.get("node")).is_empty() {
                    candidate["node"] == wanted["node"]
                } else {
                    candidate["id"] == wanted["id"]
                        && candidate["profile"] == wanted["profile"]
                        && candidate["name"] == wanted["name"]
                }
        })
        .ok_or("Device disconnected")?;
    let node = text(found.get("node"));
    if toggle && found["kind"] == "output" && !node.is_empty() {
        let mut selected = current
            .iter()
            .filter(|device| {
                device["kind"] == "output"
                    && !text(device.get("node")).is_empty()
                    && (device["selected"] == true || device["default"] == true)
            })
            .map(|device| text(device.get("node")))
            .collect::<Vec<_>>();
        if selected.contains(&node) {
            selected.retain(|value| *value != node);
        } else {
            selected.push(node);
        }
        if selected.is_empty() {
            return Ok(json!([]));
        }
        Ok(json!([
            "audio-outputs",
            serde_json::to_string(&selected).map_err(|_| "Invalid selected outputs")?
        ]))
    } else {
        let mut arguments = vec![
            "audio-device".to_owned(),
            integer(&found["id"], false)?.to_string(),
        ];
        if !found["profile"].is_null() {
            arguments.push(integer(&found["profile"], false)?.to_string());
        }
        Ok(json!(arguments))
    }
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    match function {
        "keybindings" => keybindings::snapshot(args.first().unwrap_or(&null)),
        "keybindingInput" => keybindings::input(args.first().unwrap_or(&null)),
        "generationPath" => Ok(json!(format!(
            "{PROFILE_ROOT}/system-{}-link",
            integer(args.first().unwrap_or(&null), true)?
        ))),
        "switchGenerationArguments" => switch_arguments(
            args.first().unwrap_or(&null),
            args.get(1).unwrap_or(&null),
            args.get(2).unwrap_or(&null),
        ),
        "parseGenerations" => parse_generations(
            args.first()
                .and_then(Value::as_str)
                .ok_or("Invalid generation payload")?,
        ),
        "isStorePath" => Ok(json!(
            args.first()
                .and_then(Value::as_str)
                .and_then(store_basename)
                .is_some()
        )),
        "markActiveGenerations" => {
            let running = args
                .get(1)
                .and_then(Value::as_str)
                .filter(|s| store_basename(s).is_some())
                .ok_or("Running system is unavailable")?;
            let paths = args
                .get(2)
                .and_then(Value::as_object)
                .ok_or("Invalid generation identities")?;
            let rows = array(args.first());
            if rows.len() > MAX_ROWS {
                return Err("Too many generations".into());
            }
            rows.iter()
                .map(|row| {
                    let mut row = row.clone();
                    let number = integer(&row["generation"], true)?;
                    let target = paths
                        .get(&number.to_string())
                        .and_then(Value::as_str)
                        .filter(|s| store_basename(s).is_some());
                    row["storePath"] = json!(target);
                    row["runningStorePath"] = json!(running);
                    row["active"] = json!(target == Some(running));
                    if let Some(target) = target {
                        row["switchArguments"] =
                            switch_arguments(&row["generation"], &json!(target), &json!(running))?;
                    }
                    Ok(row)
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|rows| json!(rows))
        }
        "escapeMarkdown" => Ok(json!(escape(&one_line(args.first(), "Unknown")?))),
        "formatPackageDiff" => {
            let output = string(args.first());
            if output.len() > MAX_BYTES {
                return Err("Package diff exceeds its limit".into());
            }
            let maximum = args
                .get(1)
                .map(|v| integer(v, false))
                .transpose()?
                .unwrap_or(120000)
                .min(120000) as usize;
            Ok(json!(package_diff(&output, maximum)))
        }
        "desktop" => {
            let groups = clients(
                args.first()
                    .and_then(Value::as_array)
                    .ok_or("Invalid window snapshot")?,
            )?;
            let values = args
                .get(1)
                .and_then(Value::as_array)
                .ok_or("Invalid workspace snapshot")?;
            if values.len() > MAX_ROWS {
                return Err("Too many workspaces".into());
            }
            let mut workspaces=values.iter().filter(|w|w["id"].as_i64().is_some_and(|n|n>0)).map(|w|json!({"id":w["id"],"name":text(w.get("name")),"monitor":text(w.get("monitor")),"windows":w["windows"]})).collect::<Vec<_>>();
            workspaces.sort_by_key(|w| w["id"].as_i64());
            Ok(json!({"clientGroups":groups,"workspaces":workspaces}))
        }
        "audioSelection" => audio_selection(
            args.first().unwrap_or(&null),
            array(args.get(1)),
            args.get(2) == Some(&Value::Bool(true)),
        ),
        _ => Err(format!("Unknown Vicinae function: {function}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_selection_resolves_stable_nodes_and_preserves_last_output() {
        let old = json!({"id":1,"profile":null,"kind":"output","node":"speaker","name":"Speaker"});
        let current = json!({"id":99,"profile":null,"kind":"output","node":"speaker","name":"Speaker","selected":true});
        assert_eq!(
            call(
                "audioSelection",
                &[old.clone(), json!([current.clone()]), json!(false)]
            )
            .unwrap(),
            json!(["audio-device", "99"])
        );
        assert_eq!(
            call(
                "audioSelection",
                &[old.clone(), json!([current]), json!(true)]
            )
            .unwrap(),
            json!([])
        );
        assert!(call("audioSelection", &[old, json!([]), json!(false)]).is_err());
    }
    #[test]
    fn generation_snapshots_bound_rows_and_escape_only_display_metadata() {
        let rows=call("parseGenerations",&[json!(r#"[{"generation":42,"date":"bad *date*","kernelVersion":"6.18\npreview","configurationRevision":"<untrusted>"}]"#)]).unwrap();
        assert_eq!(rows[0]["kernelVersion"], "6.18 preview");
        assert_eq!(
            rows[0]["escaped"]["configurationRevision"],
            "\\<untrusted\\>"
        );
        assert_eq!(rows[0]["escaped"]["date"], "bad \\*date\\*");
        assert!(call("parseGenerations", &[json!("[{},{}]")]).is_err());
        assert!(call("desktop", &[json!({}), json!([])]).is_err());
    }
    #[test]
    #[ignore = "reports local native snapshot throughput; no timing threshold"]
    fn snapshot_cost() {
        let clients=(0..100).map(|i|json!({"address":format!("0x{:x}",i+1),"class":"ghostty","title":format!("Window {i}"),"mapped":true,"hidden":false,"focusHistoryID":i,"workspace":{"id":1,"name":"main"}})).collect::<Vec<_>>();
        let args = [json!(clients), json!([])];
        let start = std::time::Instant::now();
        for _ in 0..2000 {
            std::hint::black_box(call("desktop", &args).unwrap());
        }
        eprintln!(
            "100-window native snapshot: {:.1} microseconds/call",
            start.elapsed().as_secs_f64() * 1e6 / 2000.0
        );
    }
}
