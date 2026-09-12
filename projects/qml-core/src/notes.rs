//! Editor commands use UTF-16 offsets, matching Qt TextArea's selection and undo
//! APIs. Each command remains one replacement rather than rewriting the note.
use crate::value::{array, number, string, text, truthy, utf16_len};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^(#{{1,6}}){}+", crate::value::SPACE)).unwrap());
static TASK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^({s}*)(?:([-*+]){s}+)?(?:\[([ xX])\]{s}+)?",
        s = crate::value::SPACE
    ))
    .unwrap()
});
static LIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^({s}*)([-*+]|[0-9]+[.)]){s}+(\[[ xX]\]{s}+)?",
        s = crate::value::SPACE
    ))
    .unwrap()
});
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*://").unwrap());

fn position(value: Option<&Value>, length: usize) -> Result<usize, String> {
    let value = number(value);
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > length as f64 {
        return Err("Invalid editor position".into());
    }
    Ok(value as usize)
}
fn boundary(units: &[u16], position: usize) -> Result<(), String> {
    if position > 0
        && position < units.len()
        && (0xd800..=0xdbff).contains(&units[position - 1])
        && (0xdc00..=0xdfff).contains(&units[position])
    {
        return Err("Editor position splits a Unicode character".into());
    }
    Ok(())
}
fn line_start(value: &[u16], position: usize) -> usize {
    // Preserve JS lastIndexOf's inclusive zero position, including its empty
    // first-line edge case. Actual Qt caret positions are integral and bounded.
    value
        .iter()
        .take(position.saturating_sub(1).saturating_add(1))
        .rposition(|c| *c == 10)
        .map_or(0, |i| i + 1)
}
fn line_end(value: &[u16], position: usize) -> usize {
    value[position..]
        .iter()
        .position(|c| *c == 10)
        .map_or(value.len(), |i| position + i)
}
fn slice(value: &[u16], start: usize, end: usize) -> String {
    String::from_utf16_lossy(
        &value[start.min(value.len())..end.min(value.len()).max(start.min(value.len()))],
    )
}
fn command(start: usize, end: usize, text: String, caret: usize) -> Value {
    json!({"start":start,"end":end,"text":text,"caret":caret})
}
fn when(stamp: &Value, today: &Value, midnight: Option<&Value>) -> String {
    let epoch = number(stamp.get("epoch"));
    let midnight = number(midnight);
    let field = |v: &Value, key: &str| number(v.get(key)) as i64;
    if epoch >= midnight {
        return format!("{:02}:{:02}", field(stamp, "hour"), field(stamp, "minute"));
    }
    if epoch >= midnight - 86400000.0 {
        return "Yesterday".into();
    }
    if field(stamp, "year") == field(today, "year") {
        format!("{:02}.{:02}", field(stamp, "day"), field(stamp, "month"))
    } else {
        format!(
            "{}-{:02}-{:02}",
            field(stamp, "year"),
            field(stamp, "month"),
            field(stamp, "day")
        )
    }
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    match function {
        "filter" => {
            let query = text(args.get(1)).to_lowercase();
            let words: Vec<_> = query
                .split(crate::value::is_space)
                .filter(|word| !word.is_empty())
                .collect();
            return Ok(json!(
                array(args.first())
                    .iter()
                    .enumerate()
                    .filter_map(|(index, note)| {
                        let haystack = format!(
                            "{} {} {}",
                            string(note.get("title")),
                            string(note.get("name")),
                            string(note.get("excerpt"))
                        )
                        .to_lowercase();
                        words
                            .iter()
                            .all(|word| haystack.contains(word))
                            .then_some(index)
                    })
                    .collect::<Vec<_>>()
            ));
        }
        "duration" => {
            let value = number(args.first());
            let seconds = if value.is_finite() {
                (value / 1000.0).floor().max(0.0) as u64
            } else {
                0
            };
            return Ok(json!(format!("{}:{:02}", seconds / 60, seconds % 60)));
        }
        "label" => {
            let note = args.first().unwrap_or(&null);
            let title = if truthy(note.get("title")) {
                text(note.get("title"))
            } else {
                text(note.get("name"))
            };
            return Ok(json!(if crate::value::trim(&title).is_empty() {
                "Untitled note"
            } else {
                crate::value::trim(&title)
            }));
        }
        "when" => {
            return Ok(json!(when(
                args.first().unwrap_or(&null),
                args.get(1).unwrap_or(&null),
                args.get(2)
            )));
        }
        "status" => {
            return Ok(json!(match args.first().and_then(Value::as_str) {
                Some("idle") => "Up to date".into(),
                Some("dirty") => "Unsaved changes".into(),
                Some("saving") => "Saving…".into(),
                Some("saved") => format!(
                    "Saved {}",
                    when(
                        args.get(1).unwrap_or(&null),
                        args.get(2).unwrap_or(&null),
                        args.get(3)
                    )
                ),
                Some("conflict") => "Changed on disk while you were editing".into(),
                Some("gone") => "This file was moved or deleted elsewhere".into(),
                Some("failed") => "Could not be saved".into(),
                _ => String::new(),
            }));
        }
        "obsidianUri" => {
            let vault = text(args.first());
            let name = vault.trim_end_matches('/').rsplit('/').next().unwrap_or("");
            let path = text(args.get(1));
            return Ok(json!(if name.is_empty() || path.is_empty() {
                String::new()
            } else {
                format!(
                    "obsidian://open?vault={}&file={}",
                    crate::value::encode_uri_component(name),
                    crate::value::encode_uri_component(&path)
                )
            }));
        }
        _ => {}
    }
    let source = text(args.first());
    let units: Vec<_> = source.encode_utf16().collect();
    if function == "unembed" {
        let name = text(args.get(1));
        if name.len() > 4096 {
            return Err("Attachment name exceeds its limit".into());
        }
        let pattern = Regex::new(&format!(
            r"(?m)^[ \t]*!\[\[{}(\|[^\]]*)?\]\][ \t]*\n?",
            regex::escape(&name)
        ))
        .map_err(|_| "Invalid attachment name")?;
        return Ok(pattern.find(&source).map_or(Value::Null, |found| {
            let start = utf16_len(&source[..found.start()]);
            let end = start + utf16_len(found.as_str());
            command(start, end, String::new(), start)
        }));
    }
    let start = position(args.get(1), units.len())?;
    boundary(&units, start)?;
    Ok(match function {
        "lineStart" => json!(line_start(&units, start)),
        "lineEnd" => json!(line_end(&units, start)),
        "wrap" | "link" => {
            let end = position(args.get(2), units.len())?;
            boundary(&units, end)?;
            if end < start {
                return Err("Reversed editor selection".into());
            }
            let selected = slice(&units, start, end);
            if function == "link" {
                let url = URL.is_match(&selected);
                let replacement = if url {
                    format!("[]({selected})")
                } else {
                    format!("[{selected}]()")
                };
                command(
                    start,
                    end,
                    replacement,
                    start + if url { 1 } else { end - start + 3 },
                )
            } else {
                let marker = text(args.get(3));
                let size = utf16_len(&marker);
                if size > 128 {
                    return Err("Editor marker exceeds its limit".into());
                }
                if start >= size
                    && slice(&units, start - size, start) == marker
                    && slice(&units, end, end.saturating_add(size)) == marker
                {
                    let mut result = command(start - size, end + size, selected, start - size);
                    result["selectionEnd"] = json!(end - size);
                    result
                } else {
                    let mut result = command(
                        start,
                        end,
                        format!("{marker}{selected}{marker}"),
                        start + size,
                    );
                    result["selectionEnd"] = json!(end + size);
                    result
                }
            }
        }
        "heading" | "task" | "newline" => {
            let line_start = line_start(&units, start);
            let end = if function == "newline" {
                start
            } else {
                line_end(&units, start)
            };
            let line = slice(&units, line_start, end);
            let end = if function == "newline" {
                end
            } else {
                line_start + utf16_len(&line)
            };
            if function == "heading" {
                let matched = HEADING.captures(&line);
                let prefix_len = matched.as_ref().map_or(0, |m| utf16_len(&m[0]));
                let current = matched.as_ref().map_or(0, |m| m[1].len());
                let level = number(args.get(2));
                if !level.is_finite() || !(0.0..=6.0).contains(&level) || level.fract() != 0.0 {
                    return Err("Invalid heading level".into());
                }
                let level = level as usize;
                let prefix = if level > 0 && level != current {
                    format!("{} ", "#".repeat(level))
                } else {
                    String::new()
                };
                let body = matched
                    .as_ref()
                    .map_or(line.as_str(), |m| &line[m[0].len()..]);
                command(
                    line_start,
                    end,
                    format!("{prefix}{body}"),
                    (line_start + prefix.len()).max(
                        start
                            .saturating_add(prefix.len())
                            .saturating_sub(prefix_len),
                    ),
                )
            } else if function == "task" {
                let matched = TASK.captures(&line).ok_or("Invalid task line")?;
                let body = &line[matched[0].len()..];
                let indent = &matched[1];
                let bullet = matched.get(2).map(|m| m.as_str());
                let current = matched.get(3).map(|m| m.as_str());
                let check = match current {
                    None => "[ ] ",
                    Some(" ") => "[x] ",
                    _ => "",
                };
                let bullet = if !check.is_empty() {
                    "- ".into()
                } else {
                    bullet.map_or_else(String::new, |b| format!("{b} "))
                };
                let replacement = format!("{indent}{bullet}{check}{body}");
                let caret = line_start + utf16_len(&replacement) - utf16_len(body)
                    + start.saturating_sub(line_start + utf16_len(&matched[0]));
                command(line_start, end, replacement, caret)
            } else {
                let Some(matched) = LIST.captures(&line) else {
                    return Ok(Value::Null);
                };
                if matched[0].len() == line.len() {
                    return Ok(command(line_start, start, String::new(), line_start));
                }
                let marker = &matched[2];
                let digits = marker.bytes().take_while(u8::is_ascii_digit).count();
                let next = if digits > 0 {
                    let number = marker[..digits]
                        .parse::<f64>()
                        .map_err(|_| "Invalid list number")?;
                    if !number.is_finite() {
                        return Err("List number exceeds its limit".into());
                    }
                    format!("{}{} ", number + 1.0, &marker[digits..])
                } else {
                    format!("{marker} ")
                };
                let inserted = format!(
                    "\n{}{next}{}",
                    &matched[1],
                    if matched.get(3).is_some() { "[ ] " } else { "" }
                );
                let caret = start + utf16_len(&inserted);
                command(start, start, inserted, caret)
            }
        }
        "embed" => {
            let name = text(args.get(2));
            let before = slice(&units, 0, start);
            let after = slice(&units, start, units.len());
            let lead = if before.is_empty() || before.ends_with("\n\n") {
                ""
            } else if before.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let trail = if after.starts_with('\n') || after.is_empty() {
                ""
            } else {
                "\n"
            };
            let inserted = format!("{lead}![[{name}]]\n{trail}");
            let caret = start + utf16_len(&inserted);
            command(start, start, inserted, caret)
        }
        _ => return Err(format!("Unknown Notes function: {function}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_preserve_utf16_ranges_and_never_split_surrogates() {
        let command = call("wrap", &[json!("🌸 café"), json!(3), json!(7), json!("**")]).unwrap();
        assert_eq!(
            command,
            json!({"start":3,"end":7,"text":"**café**","caret":5,"selectionEnd":9})
        );
        assert_eq!(
            call(
                "unembed",
                &[
                    json!("🌸\n![[Memo (2).wav|voice]]\ntail"),
                    json!("Memo (2).wav")
                ]
            )
            .unwrap(),
            json!({"start":3,"end":27,"text":"","caret":3})
        );
        for index in [json!(-1), json!(1), json!(999), json!("NaN")] {
            assert!(call("task", &[json!("🌸 café"), index]).is_err());
        }
    }
    #[test]
    fn whitespace_and_undo_commands_match_editor_boundaries() {
        assert_eq!(
            call("heading", &[json!("#\u{feff}🌸"), json!(4), json!(1)]).unwrap(),
            json!({"start":0,"end":4,"text":"🌸","caret":2})
        );
        assert_eq!(
            call("newline", &[json!("- [X] 🌸"), json!(8)]).unwrap(),
            json!({"start":8,"end":8,"text":"\n- [ ] ","caret":15})
        );
        assert!(call("heading", &[json!("plain"), json!(2), json!(1000000)]).is_err());
    }
}
