//! Bounded, text-only transforms. No clipboard, filesystem or network access.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

const INPUT: usize = 64 * 1024;
const OUTPUT: usize = 256 * 1024;
const COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    if function != "transform" {
        return Err("unknown text workbench operation".into());
    }
    let input = args
        .first()
        .and_then(Value::as_str)
        .ok_or("text required")?;
    let mode = args
        .get(1)
        .and_then(Value::as_str)
        .ok_or("transform required")?;
    let transformed = transform(input, mode);
    Ok(match transformed {
        Ok(output) => {
            json!({"valid":true,"output":output,"error":"","inputBytes":input.len(),"outputBytes":output.len(),"characters":output.chars().count(),"lines":if output.is_empty() {0} else {output.split('\n').count()}})
        }
        Err(error) => {
            json!({"valid":false,"output":"","error":error,"inputBytes":input.len(),"outputBytes":0})
        }
    })
}

fn transform(input: &str, mode: &str) -> Result<String, String> {
    if input.len() > INPUT {
        return Err("Input exceeds 64 KiB. Nothing was transformed.".into());
    }
    // NUL is valid Unicode but cannot be represented faithfully by many text
    // clipboard consumers. Refuse it rather than silently changing binary data.
    if input.contains('\0') {
        return Err("NUL bytes are not supported in this text workbench.".into());
    }
    let output = match mode {
        "json-format" | "json-minify" => json_layout(input, mode == "json-format")?,
        "url-encode" => percent_encoding::utf8_percent_encode(input, COMPONENT).to_string(),
        "url-decode" => {
            let bytes = input.as_bytes();
            for (index, byte) in bytes.iter().enumerate() {
                if *byte == b'%'
                    && (index + 2 >= bytes.len()
                        || !bytes[index + 1].is_ascii_hexdigit()
                        || !bytes[index + 2].is_ascii_hexdigit())
                {
                    return Err(
                        "Invalid percent escape. Use % followed by two hexadecimal digits.".into(),
                    );
                }
            }
            percent_encoding::percent_decode_str(input)
                .decode_utf8()
                .map_err(|_| "Decoded bytes are not valid UTF-8 text.")?
                .into_owned()
        }
        "base64-encode" => STANDARD.encode(input),
        "base64-decode" => String::from_utf8(STANDARD.decode(input).map_err(
            |_| "Invalid Base64. Use the standard alphabet and padding, without whitespace.",
        )?)
        .map_err(|_| "Decoded bytes are not valid UTF-8 text.")?,
        "lines-clean" => input
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        "lines-unique" => {
            let mut seen = HashSet::new();
            input
                .split('\n')
                .filter(|line| seen.insert(*line))
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => return Err("Choose a supported transform.".into()),
    };
    if output.len() > OUTPUT {
        return Err("Result exceeds 256 KiB. Nothing was truncated.".into());
    }
    if output.contains('\0') {
        return Err("Decoded text contains a NUL byte and cannot be copied safely.".into());
    }
    Ok(output)
}

// Validate the grammar, then lay out the original tokens. Re-serializing a
// serde_json::Value would silently round large integers and discard duplicate
// object keys. This preserves every number, escape, key and string exactly.
fn json_layout(input: &str, pretty: bool) -> Result<String, String> {
    let mut parser = serde_json::Deserializer::from_str(input);
    serde::de::IgnoredAny::deserialize(&mut parser)
        .and_then(|_| parser.end())
        .map_err(|e| format!("Invalid JSON at line {}, column {}.", e.line(), e.column()))?;
    // IgnoredAny validates JSON grammar without coercing numeric lexemes, but
    // deliberately skips string decoding. Validate each string separately so
    // unpaired UTF-16 escapes are rejected too.
    let mut start = None;
    let mut escape = false;
    for (index, byte) in input.bytes().enumerate() {
        if let Some(begin) = start {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                serde_json::from_str::<String>(&input[begin..=index])
                    .map_err(|_| "Invalid Unicode escape in JSON string.")?;
                start = None;
            }
        } else if byte == b'"' {
            start = Some(index);
        }
    }
    let mut output = String::new();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else {
            match ch {
                '"' => {
                    quoted = true;
                    output.push(ch);
                }
                ' ' | '\n' | '\r' | '\t' => (),
                '{' | '[' if pretty => {
                    output.push(ch);
                    depth += 1;
                    while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                        chars.next();
                    }
                    if chars.peek().is_some_and(|c| *c == '}' || *c == ']') {
                        output.push(chars.next().unwrap());
                        depth -= 1;
                    } else {
                        newline(&mut output, depth);
                    }
                }
                '}' | ']' if pretty => {
                    depth -= 1;
                    newline(&mut output, depth);
                    output.push(ch);
                }
                ',' if pretty => {
                    output.push(ch);
                    newline(&mut output, depth);
                }
                ':' if pretty => output.push_str(": "),
                _ => output.push(ch),
            }
        }
        if output.len() > OUTPUT {
            return Err("Result exceeds 256 KiB. Nothing was truncated.".into());
        }
    }
    Ok(output)
}
fn newline(output: &mut String, depth: usize) {
    output.push('\n');
    for _ in 0..depth {
        output.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_roundtrips_and_strict_binary_rejection() {
        for (encode, decode) in [
            ("url-encode", "url-decode"),
            ("base64-encode", "base64-decode"),
        ] {
            let text = "Grüße 🦀 日本語\n +/%";
            assert_eq!(
                transform(&transform(text, encode).unwrap(), decode).unwrap(),
                text
            );
        }
        for (text, mode) in [
            ("%FF", "url-decode"),
            ("%0", "url-decode"),
            ("%GG", "url-decode"),
            ("/w==", "base64-decode"),
            ("AA==", "base64-decode"),
            ("YQ", "base64-decode"),
            ("YQ==\n", "base64-decode"),
        ] {
            assert!(transform(text, mode).is_err(), "{text}");
        }
        assert_eq!(transform("a+b", "url-decode").unwrap(), "a+b");
        assert_eq!(
            transform("a-z_~. /+", "url-encode").unwrap(),
            "a-z_~.%20%2F%2B"
        );
    }
    #[test]
    fn json_preserves_values_keys_and_escapes() {
        let text = r#"{ "n": 184467440737095516160, "n": 1e999, "decimal": 0.12345678901234567890123456789, "s": "\u0061", "a": [true, {}] }"#;
        let compact = transform(text, "json-minify").unwrap();
        assert_eq!(
            compact,
            r#"{"n":184467440737095516160,"n":1e999,"decimal":0.12345678901234567890123456789,"s":"\u0061","a":[true,{}]}"#
        );
        assert_eq!(
            transform(&transform(text, "json-format").unwrap(), "json-minify").unwrap(),
            compact
        );
        for invalid in ["{", "[1,]", "{} x", "\"\\ud800\""] {
            assert!(transform(invalid, "json-format").is_err());
        }
    }
    #[test]
    fn exact_limits_and_no_partial_result() {
        assert!(transform(&"x".repeat(INPUT), "base64-encode").is_ok());
        assert!(transform(&"x".repeat(INPUT + 1), "lines-clean").is_err());
        assert!(transform(&"é".repeat(INPUT / 2), "lines-clean").is_ok());
        assert!(transform(&"é".repeat(INPUT / 2 + 1), "lines-clean").is_err());
        let huge = format!(
            "{}{}{}",
            "[".repeat(100),
            (0..4000).map(|_| "1").collect::<Vec<_>>().join(","),
            "]".repeat(100)
        );
        assert!(transform(&huge, "json-format").is_err());
        let reply = call("transform", &[json!("%ff"), json!("url-decode")]).unwrap();
        assert_eq!(reply["valid"], false);
        assert_eq!(reply["output"], "");
    }
    #[test]
    fn line_tools_are_explicit_and_stable() {
        assert_eq!(transform(" a \r\n\n b\t\n", "lines-clean").unwrap(), "a\nb");
        assert_eq!(
            transform("a\nb\na\nA\n", "lines-unique").unwrap(),
            "a\nb\nA\n"
        );
        assert_eq!(transform("", "lines-clean").unwrap(), "");
    }
}
