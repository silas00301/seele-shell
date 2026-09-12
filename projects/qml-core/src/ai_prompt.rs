use crate::value::{array, number, text, truthy};
use serde_json::{Value, json};
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
        _ => return Err(format!("Unknown AI prompt function: {function}")),
    })
}
