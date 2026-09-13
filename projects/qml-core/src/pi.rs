//! Pi footer policy; the host measures/truncates using its terminal engine and
//! adds its own theme escape sequences only after this native sanitization.
use crate::value::{array, fixed, number, number_text, text, trim};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;
const MAX_PILLS: usize = 256;
const MAX_TEXT: usize = 65536;
static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?:\x1b\]|\x{9d})[^\x07\x1b\x{9c}]*(?:\x07|\x1b\\|\x{9c}|$)|",
        r"(?:\x1b[P^_X]|[\x{90}\x{98}\x{9e}\x{9f}])[^\x1b\x{9c}]*(?:\x1b\\|\x{9c}|$)|",
        r"(?:\x1b\[|\x{9b})[0-?]*[ -/]*[@-~]|\x1b[ -/]*[@-~]"
    ))
    .unwrap()
});
fn sanitize(value: &str) -> Result<String, String> {
    if value.len() > MAX_TEXT {
        return Err("Footer text exceeds its limit".into());
    }
    let clean = ANSI.replace_all(value, "");
    let mut result = String::with_capacity(clean.len());
    let mut space = false;
    for mut c in clean.chars() {
        if matches!(c,'\0'..='\u{8}'|'\u{b}'|'\u{c}'|'\u{e}'..='\u{1f}'|'\u{7f}'..='\u{9f}'|'\u{ad}'|'\u{61c}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}')
        {
            continue;
        }
        if matches!(c, '\r' | '\n' | '\t') {
            c = ' ';
        }
        if c != ' ' || !space {
            result.push(c);
        }
        space = c == ' ';
    }
    Ok(trim(&result).to_owned())
}
fn tokens(count: f64) -> String {
    if count < 1000.0 {
        number_text(count)
    } else if count < 1_000_000.0 {
        format!("{}k", fixed(count / 1000.0, u32::from(count < 10_000.0)))
    } else {
        format!(
            "{}M",
            fixed(count / 1_000_000.0, u32::from(count < 10_000_000.0))
        )
    }
}
fn pill(color: &str, priority: Option<u32>, value: String) -> Result<Value, String> {
    Ok(json!({"color":color,"priority":priority,"text":sanitize(&value)?}))
}
fn pills(data: &Value) -> Result<Value, String> {
    let mut cwd = text(data.get("cwd"));
    let home = text(data.get("home"));
    if !home.is_empty() {
        if cwd == home {
            cwd = "~".into();
        } else if let Some(rest) = cwd.strip_prefix(&format!("{home}/")) {
            cwd = format!("~/{rest}");
        }
    }
    let mut left = vec![pill("accent", None, format!(" {cwd}"))?];
    if data.get("jj").is_some_and(|v| !v.is_null()) {
        left.push(pill(
            "success",
            Some(2),
            format!("jj {}", text(data.get("jj"))),
        )?);
    } else if !text(data.get("branch")).is_empty() {
        left.push(pill(
            "success",
            Some(2),
            format!(" {}", text(data.get("branch"))),
        )?);
    }
    let session = text(data.get("sessionName"));
    if !session.is_empty() {
        left.push(pill("syntaxString", Some(0), session)?);
    }
    let statuses = array(data.get("statuses"));
    if statuses.len() > MAX_PILLS {
        return Err("Too many footer statuses".into());
    }
    for status in statuses {
        let value = sanitize(&text(Some(status)))?;
        if !value.is_empty() {
            left.push(pill("muted", Some(0), value)?);
        }
    }
    let mut right = vec![];
    let input = number(data.get("inputTokens"));
    let output = number(data.get("outputTokens"));
    if input > 0.0 || output > 0.0 {
        right.push(pill(
            "muted",
            Some(0),
            format!("↑{} ↓{}", tokens(input), tokens(output)),
        )?);
    }
    let percent = data
        .get("contextPercent")
        .filter(|v| !v.is_null())
        .map(|v| number(Some(v)));
    let color = if percent.is_some_and(|p| p > 90.0) {
        "error"
    } else if percent.is_some_and(|p| p > 70.0) {
        "warning"
    } else {
        "success"
    };
    right.push(pill(
        color,
        None,
        percent.map_or("󰍛 ?".into(), |p| format!("󰍛 {}%", fixed(p, 0))),
    )?);
    if data["reasoning"] == true {
        let level = data["thinkingLevel"].as_str().unwrap_or("off");
        // Only Pi's actual theme enum may select a color. Unknown metadata is
        // shown safely using the neutral off color, never used as a theme key.
        let color = match level {
            "off" => "thinkingOff",
            "minimal" => "thinkingMinimal",
            "low" => "thinkingLow",
            "medium" => "thinkingMedium",
            "high" => "thinkingHigh",
            "xhigh" => "thinkingXhigh",
            _ => "thinkingOff",
        };
        right.push(pill(color, Some(1), format!("󰔛 {level}"))?);
    }
    right.push(pill(
        "mdHeading",
        None,
        format!("󰚩 {}", data["modelId"].as_str().unwrap_or("no model")),
    )?);
    Ok(json!({"left":left,"right":right}))
}
#[derive(Clone)]
struct Measure {
    index: usize,
    width: u64,
    priority: Option<u64>,
}
fn measures(value: Option<&Value>) -> Result<Vec<Measure>, String> {
    let rows = value
        .and_then(Value::as_array)
        .filter(|v| v.len() <= MAX_PILLS + 8)
        .ok_or("Invalid footer measurements")?;
    rows.iter()
        .enumerate()
        .map(|(index, value)| {
            Ok(Measure {
                index,
                width: value["width"]
                    .as_u64()
                    .filter(|w| *w <= MAX_TEXT as u64)
                    .ok_or("Invalid footer width")?,
                priority: if value["priority"].is_null() {
                    None
                } else {
                    Some(
                        value["priority"]
                            .as_u64()
                            .filter(|p| *p <= 2)
                            .ok_or("Invalid footer priority")?,
                    )
                },
            })
        })
        .collect()
}
fn group_width(values: &[Measure]) -> u64 {
    values.iter().map(|m| m.width + 4).sum::<u64>() + values.len().saturating_sub(1) as u64
}
fn total(left: &[Measure], right: &[Measure]) -> u64 {
    group_width(left) + group_width(right) + u64::from(!left.is_empty() && !right.is_empty())
}
fn layout(args: &[Value]) -> Result<Value, String> {
    let mut left = measures(args.first())?;
    let mut right = measures(args.get(1))?;
    let width = args
        .get(2)
        .and_then(Value::as_u64)
        .filter(|w| *w <= MAX_TEXT as u64)
        .ok_or("Invalid terminal width")?;
    let stage = args.get(3).and_then(Value::as_u64).unwrap_or(0);
    if stage > 2 || left.is_empty() || right.is_empty() {
        return Err("Invalid footer layout stage".into());
    }
    if stage == 0 {
        while total(&left, &right) > width {
            let candidate = left
                .iter()
                .map(|m| (false, m))
                .chain(right.iter().map(|m| (true, m)))
                .filter_map(|(side, m)| m.priority.map(|priority| (priority, side, m.index)))
                .min_by_key(|c| c.0);
            let Some((_, side, index)) = candidate else {
                break;
            };
            if side {
                right.retain(|m| m.index != index);
            } else {
                left.retain(|m| m.index != index);
            }
        }
    }
    let excess = total(&left, &right).saturating_sub(width);
    let shrink = if excess > 0 && stage < 2 {
        let (side, index, measure) = if stage == 0 {
            ("left", 0, left.first().ok_or("Missing primary footer")?)
        } else {
            (
                "right",
                right.len() - 1,
                right.last().ok_or("Missing model footer")?,
            )
        };
        Some(json!({"side":side,"index":index,"width":measure.width.saturating_sub(excess).max(4)}))
    } else {
        None
    };
    let padding = width
        .saturating_sub(group_width(&left) + group_width(&right))
        .max(u64::from(!left.is_empty() && !right.is_empty()));
    Ok(
        json!({"left":left.iter().map(|m|m.index).collect::<Vec<_>>(),"right":right.iter().map(|m|m.index).collect::<Vec<_>>(),"shrink":shrink,"nextStage":stage+1,"padding":padding}),
    )
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    match function {
        "sanitize" => Ok(json!(sanitize(&text(args.first()))?)),
        "formatTokens" => Ok(json!(tokens(number(args.first())))),
        "pills" => pills(args.first().ok_or("Missing footer metadata")?),
        "layout" => layout(args),
        "usage" => {
            let rows = args
                .first()
                .and_then(Value::as_array)
                .filter(|r| r.len() <= 100_000)
                .ok_or("Invalid footer usage entries")?;
            let (mut input, mut output) = (0.0, 0.0);
            for row in rows {
                if row["type"] == "message" && row["role"] == "assistant" {
                    input += number(row.get("input"));
                    output += number(row.get("output"));
                }
            }
            Ok(
                json!({"input":if input.is_finite(){json!(input)}else{json!(number_text(input))},"output":if output.is_finite(){json!(output)}else{json!(number_text(output))}}),
            )
        }
        _ => Err("Unknown Pi footer function".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_programs_and_directions_never_reach_measurement() {
        for input in [
            "a\u{1b}]52;c;secret\u{7}b",
            "a\u{9d}52;c;secret\u{9c}b",
            "a\u{1b}Psecret\u{1b}\\b",
        ] {
            assert_eq!(sanitize(input).unwrap(), "ab");
        }
        assert_eq!(sanitize("a\u{202e}\u{1b}[31mb").unwrap(), "ab");
        assert_eq!(sanitize("a\u{1b}]52;c;secret").unwrap(), "a");
    }
    #[test]
    fn usage_is_metadata_only_and_bounds_are_enforced() {
        assert_eq!(call("usage",&[json!([{"type":"message","role":"assistant","input":2,"output":3},{"type":"message","role":"user","input":999,"output":999}])]).unwrap(),json!({"input":2.0,"output":3.0}));
        assert!(sanitize(&"x".repeat(MAX_TEXT + 1)).is_err());
        assert_eq!(tokens(2250.0), "2.3k");
        assert_eq!(tokens(1150.0), "1.1k");
    }
}
