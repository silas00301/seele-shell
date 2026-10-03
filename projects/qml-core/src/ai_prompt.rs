use crate::value::{array, number, text, truthy};
use serde_json::{Value, json};

struct Mention {
    kind: &'static str,
    glyph: &'static str,
    caption: &'static str,
}

// The same five controls the mention scanner accepts. Captions state when a
// source is read, because choosing one from the list must not read it.
const MENTIONS: [Mention; 5] = [
    Mention {
        kind: "clip",
        glyph: "\u{f014c}",
        caption: "Included once when you press Send",
    },
    Mention {
        kind: "select",
        glyph: "\u{f0485}",
        caption: "Included once when you press Send",
    },
    Mention {
        kind: "window",
        glyph: "\u{f05b2}",
        caption: "Focused application and title",
    },
    Mention {
        kind: "dir",
        glyph: "\u{f024b}",
        caption: "Resolved when you press Send",
    },
    Mention {
        kind: "screen",
        glyph: "\u{f0e51}",
        caption: "Captured once when you press Send",
    },
];

// Qt Quick key codes. The field keeps the caret, so it reports these values
// and the completion list never takes focus of its own.
const KEY_ESCAPE: i64 = 0x0100_0000;
const KEY_TAB: i64 = 0x0100_0001;
const KEY_RETURN: i64 = 0x0100_0004;
const KEY_ENTER: i64 = 0x0100_0005;
const KEY_UP: i64 = 0x0100_0013;
const KEY_DOWN: i64 = 0x0100_0015;

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// QML reports the caret in UTF-16 code units. A cursor that lands inside one
/// scalar is placed after it, which is the only boundary Qt will use.
fn byte_at_utf16(text: &str, cursor: usize) -> usize {
    let mut units = 0usize;
    for (index, ch) in text.char_indices() {
        if units >= cursor {
            return index;
        }
        let next = units + ch.len_utf16();
        if cursor < next {
            return index + ch.len_utf8();
        }
        units = next;
    }
    text.len()
}

fn cursor_units(value: Option<&Value>, text: &str) -> Option<usize> {
    let number = number(value);
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 {
        return None;
    }
    let units = number as u64;
    let len = u64::try_from(utf16_len(text)).unwrap_or(u64::MAX);
    usize::try_from(units.min(len)).ok()
}

fn key_code(value: Option<&Value>) -> Option<i64> {
    let number = number(value);
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > i64::MAX as f64 {
        return None;
    }
    Some(number as i64)
}

fn mention_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn mention_blocked(ch: char) -> bool {
    mention_word(ch) || ch == '@'
}

struct ActiveMention<'a> {
    start: usize,
    end: usize,
    query: &'a str,
}

/// The caret is completing a mention when it sits at the end of an `@` token
/// that is a proper prefix of one of the five controls. A finished control is
/// absent, so Enter sends instead of inserting it again. Email addresses and
/// `@@` escapes use the same boundary as the mention scanner.
fn active_mention(text: &str, cursor: usize) -> Option<ActiveMention<'_>> {
    let end = byte_at_utf16(text, cursor);
    if !text.is_char_boundary(end) || text[end..].chars().next().is_some_and(mention_word) {
        return None;
    }
    let mut query_at = end;
    for (index, ch) in text[..end].char_indices().rev() {
        if mention_word(ch) {
            query_at = index;
        } else {
            break;
        }
    }
    let head = &text[..query_at];
    let (at, ch) = head.char_indices().next_back()?;
    if ch != '@' || head[..at].chars().next_back().is_some_and(mention_blocked) {
        return None;
    }
    let query = &text[query_at..end];
    if MENTIONS.iter().any(|mention| mention.kind == query)
        || !MENTIONS
            .iter()
            .any(|mention| mention.kind.starts_with(query))
    {
        return None;
    }
    Some(ActiveMention {
        start: at,
        end,
        query,
    })
}

fn inactive_completion() -> Value {
    json!({"active": false, "start": 0, "end": 0, "query": "", "options": []})
}

fn completion_value(text: &str, cursor: Option<usize>) -> Value {
    let Some(active) = cursor.and_then(|cursor| active_mention(text, cursor)) else {
        return inactive_completion();
    };
    let options = MENTIONS
        .iter()
        .filter(|mention| mention.kind.starts_with(active.query))
        .map(|mention| {
            json!({
                "kind": mention.kind,
                "glyph": mention.glyph,
                "label": format!("@{}", mention.kind),
                "caption": mention.caption,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "active": true,
        "start": utf16_len(&text[..active.start]),
        "end": utf16_len(&text[..active.end]),
        "query": active.query,
        "options": options,
    })
}

fn apply_completion(text: &str, cursor: Option<usize>, kind: &str) -> Value {
    let Some(cursor) = cursor else {
        return json!({"text": text, "cursor": 0, "applied": false});
    };
    let Some(active) = active_mention(text, cursor) else {
        return json!({"text": text, "cursor": cursor, "applied": false});
    };
    if !MENTIONS
        .iter()
        .any(|mention| mention.kind == kind && mention.kind.starts_with(active.query))
    {
        return json!({"text": text, "cursor": cursor, "applied": false});
    }
    let mut next = String::new();
    next.push_str(&text[..active.start]);
    let insertion = format!("@{kind}");
    next.push_str(&insertion);
    let rest = &text[active.end..];
    // Keep one existing space, or add one, so the caret lands where typing continues.
    let kept = rest.chars().next().filter(|ch| ch.is_whitespace());
    if kept.is_none() {
        next.push(' ');
    }
    next.push_str(rest);
    let cursor_byte = active.start + insertion.len() + kept.map_or(1, char::len_utf8);
    json!({
        "text": next,
        "cursor": utf16_len(&next[..cursor_byte]),
        "applied": true,
    })
}

/// Escape closes the list before the panel. Enter and Tab insert the
/// highlighted mention; Shift+Enter still adds a line. Arrow keys move only
/// while the list is open, and every other key reaches the field.
fn completion_key(active: bool, key: Option<i64>, shift: bool) -> &'static str {
    if !active {
        return "passthrough";
    }
    match key {
        Some(KEY_ESCAPE) => "dismiss",
        Some(KEY_UP) => "up",
        Some(KEY_DOWN) => "down",
        Some(KEY_TAB) if !shift => "accept",
        Some(KEY_RETURN | KEY_ENTER) if !shift => "accept",
        _ => "passthrough",
    }
}

fn has(values: Option<&Value>, kind: &str) -> bool {
    array(values)
        .iter()
        .any(|value| value.as_str() == Some(kind))
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match function {
        "mentions" => {
            // ASCII word boundaries match the original JavaScript context
            // syntax and avoid interpreting email addresses or @@ escapes.
            let value = text(args.first());
            let bytes = value.as_bytes();
            let mut found = Vec::new();
            for (index, byte) in bytes.iter().enumerate() {
                if *byte != b'@'
                    || index > 0
                        && (bytes[index - 1].is_ascii_alphanumeric()
                            || b"_@".contains(&bytes[index - 1]))
                {
                    continue;
                }
                for kind in ["clip", "select", "window", "dir", "screen"] {
                    let end = index + 1 + kind.len();
                    if bytes.get(index + 1..end) == Some(kind.as_bytes())
                        && bytes
                            .get(end)
                            .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
                        && !found.contains(&kind)
                    {
                        found.push(kind);
                    }
                }
            }
            json!(found)
        }
        "has" => json!(has(args.first(), &text(args.get(1)))),
        "permissions" => json!(
            [("clip", 1), ("select", 2)]
                .into_iter()
                .filter(|(kind, index)| has(args.first(), kind) && truthy(args.get(*index)))
                .map(|(kind, _)| kind)
                .collect::<Vec<_>>()
        ),
        "canSubmit" => json!(
            !truthy(args.get(6))
                && !crate::value::trim(&text(args.first())).is_empty()
                && [("clip", 2), ("select", 3), ("dir", 4), ("screen", 5)]
                    .into_iter()
                    .all(|(kind, index)| !has(args.get(1), kind) || truthy(args.get(index)))
        ),
        "preview" => {
            let raw = text(args.first());
            let mut value = if raw.is_empty() {
                "Empty text".into()
            } else {
                raw
            };
            value = value
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\n', "  ↵  ");
            let count = number(args.get(1));
            if count > 0.0 && count.is_finite() {
                value.push_str(&format!(
                    " · {count} character{}",
                    if count == 1.0 { "" } else { "s" }
                ));
            }
            if truthy(args.get(2)) {
                value.push_str(" · first 65,536 characters");
            }
            json!(value)
        }
        "completion" => {
            let value = text(args.first());
            completion_value(&value, cursor_units(args.get(1), &value))
        }
        "applyCompletion" => {
            let value = text(args.first());
            apply_completion(
                &value,
                cursor_units(args.get(1), &value),
                &text(args.get(2)),
            )
        }
        "completionKey" => json!(completion_key(
            truthy(args.first()),
            key_code(args.get(1)),
            truthy(args.get(2))
        )),
        _ => return Err(format!("Unknown AI prompt function: {function}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn completion(text: &str, cursor: usize) -> Value {
        call("completion", &[json!(text), json!(cursor)]).unwrap()
    }

    #[test]
    fn at_opens_every_source_and_a_prefix_narrows_it() {
        let opened = completion("Ask @", 5);
        assert_eq!(opened["active"], true);
        assert_eq!(opened["query"], "");
        assert_eq!(
            opened["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["clip", "select", "window", "dir", "screen"]
        );
        assert_eq!(completion("Ask @sc", 7)["options"][0]["kind"], "screen");
        assert_eq!(
            completion("Ask @sc", 7)["options"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn finished_mentions_email_and_escapes_stay_closed() {
        assert_eq!(completion("Ask @clip", 9)["active"], false);
        assert_eq!(completion("mail me@example.org", 18)["active"], false);
        assert_eq!(completion("quote @@clip", 12)["active"], false);
        assert_eq!(completion("@cli", 2)["active"], false);
        assert_eq!(completion("Ask @clipboard", 14)["active"], false);
        assert_eq!(completion("🙂 @", 4)["active"], true);
    }

    #[test]
    fn applying_inserts_only_a_matching_source() {
        assert_eq!(
            call(
                "applyCompletion",
                &[json!("Explain @cl please"), json!(11), json!("clip")]
            )
            .unwrap(),
            json!({"text": "Explain @clip please", "cursor": 14, "applied": true})
        );
        assert_eq!(
            call(
                "applyCompletion",
                &[json!("Explain @"), json!(9), json!("screen")]
            )
            .unwrap(),
            json!({"text": "Explain @screen ", "cursor": 16, "applied": true})
        );
        assert_eq!(
            call(
                "applyCompletion",
                &[json!("@cl-next"), json!(3), json!("clip")]
            )
            .unwrap(),
            json!({"text": "@clip -next", "cursor": 6, "applied": true})
        );
        assert_eq!(
            call(
                "applyCompletion",
                &[json!("@cl\nmore"), json!(3), json!("clip")]
            )
            .unwrap(),
            json!({"text": "@clip\nmore", "cursor": 6, "applied": true})
        );
        assert_eq!(
            call("applyCompletion", &[json!("🙂 @"), json!(4), json!("dir")]).unwrap(),
            json!({"text": "🙂 @dir ", "cursor": 8, "applied": true})
        );
        assert_eq!(
            call(
                "applyCompletion",
                &[json!("Explain @cl"), json!(11), json!("screen")]
            )
            .unwrap()["applied"],
            false
        );
    }

    #[test]
    fn open_list_takes_navigation_and_leaves_shift_enter() {
        let key = |active, code, shift| {
            call("completionKey", &[json!(active), json!(code), json!(shift)]).unwrap()
        };
        assert_eq!(key(true, 16777237, false), "down");
        assert_eq!(key(true, 16777235, false), "up");
        assert_eq!(key(true, 16777220, false), "accept");
        assert_eq!(key(true, 16777221, true), "passthrough");
        assert_eq!(key(true, 16777217, false), "accept");
        assert_eq!(key(true, 16777216, false), "dismiss");
        assert_eq!(key(false, 16777216, false), "passthrough");
        assert_eq!(key(false, 16777220, false), "passthrough");
    }
}
