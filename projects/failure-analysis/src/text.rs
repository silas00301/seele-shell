use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;
static ANSI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))").unwrap());
pub fn bounded(text: &str, limit: usize, tail: bool) -> String {
    if text.len() <= limit {
        return text.into();
    }
    const MARKER: &str = "\n… output truncated …\n";
    if limit <= MARKER.len() {
        return String::new();
    }
    let size = limit - MARKER.len();
    if tail {
        let mut start = text.len() - size;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        format!("{MARKER}{}", &text[start..])
    } else {
        let mut end = size;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}{MARKER}", &text[..end])
    }
}
pub fn clean(text: &str, limit: usize) -> String {
    let stripped = ANSI.replace_all(text, "");
    let plain: String = stripped
        .chars()
        .map(|c| {
            if c == '\t' || c == '\n' || !c.is_control() {
                c
            } else {
                ' '
            }
        })
        .collect();
    bounded(plain.trim(), limit, false)
}
pub fn clean_value(value: &Value, limit: usize) -> String {
    let text = if let Some(text) = value.as_str() {
        text.to_owned()
    } else if let Some(bytes) = value.as_array().and_then(|items| {
        items
            .iter()
            .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect::<Option<Vec<_>>>()
    }) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        value.to_string()
    };
    clean(&text, limit)
}
pub fn timestamp(micros: i64) -> String {
    let seconds = micros.div_euclid(1_000_000);
    let millis = micros.rem_euclid(1_000_000) / 1000;
    seele_runtime::time::format_timestamp(seconds as libc::time_t)
        .map(|v| format!("{}.{millis:03}+00:00", v.trim_end_matches('Z')))
        .unwrap_or_else(|| micros.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_bounds_and_progress_cleaning() {
        assert_eq!(clean("\r\x1b[2Kbuilding ✓\r\n", 100), "building ✓");
        let text = "✓".repeat(100);
        for tail in [true, false] {
            let result = bounded(&text, 100, tail);
            assert!(result.len() <= 100);
            assert!(result.contains("truncated"));
        }
        assert_eq!(timestamp(2_001_000), "1970-01-01T00:00:02.001+00:00");
    }
}
