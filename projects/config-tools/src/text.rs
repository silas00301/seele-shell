use unicode_general_category::{get_general_category, GeneralCategory as C};

/// Encode terminal control and layout characters while preserving printable
/// Unicode. This is for labels; opaque file-selection tokens keep exact bytes.
pub fn terminal(value: &str) -> String {
    let mut output = String::new();
    for c in value.chars() {
        if c != ' '
            && matches!(
                get_general_category(c),
                C::Control
                    | C::Format
                    | C::Surrogate
                    | C::Unassigned
                    | C::SpaceSeparator
                    | C::LineSeparator
                    | C::ParagraphSeparator
            )
        {
            let escaped = serde_json::to_string(&c.to_string()).unwrap();
            if escaped.len() == c.len_utf8() + 2 {
                output.push_str(&format!("\\u{:04x}", c as u32));
            } else {
                output.push_str(&escaped[1..escaped.len() - 1]);
            }
        } else {
            output.push(c);
        }
    }
    output
}

pub fn path(bytes: &[u8]) -> String {
    let mut remaining = bytes;
    let mut output = String::new();
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                output.push_str(&terminal(text));
                break;
            }
            Err(error) => {
                let (valid, tail) = remaining.split_at(error.valid_up_to());
                output.push_str(&terminal(std::str::from_utf8(valid).unwrap()));
                let invalid = error.error_len().unwrap_or(tail.len());
                for byte in &tail[..invalid] {
                    output.push_str(&format!("\\udc{byte:02x}"));
                }
                remaining = &tail[invalid..];
            }
        }
    }
    output
}
