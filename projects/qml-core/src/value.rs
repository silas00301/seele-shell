//! JavaScript primitive compatibility at the QML value boundary.
use serde_json::Value;

pub const SPACE: &str = r"[\t\n\x0b\f\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]";
pub fn is_space(value: char) -> bool {
    matches!(
        value,
        '\t' | '\n' | '\u{000b}' | '\u{000c}' | '\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
pub fn trim(value: &str) -> &str {
    value.trim_matches(is_space)
}

pub fn number(value: Option<&Value>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(value)) => f64::from(u8::from(*value)),
        Some(Value::Number(value)) => value.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(value)) => {
            let value = trim(value);
            if value.is_empty() {
                return 0.0;
            }
            for (prefix, radix) in [
                ("0x", 16),
                ("0X", 16),
                ("0o", 8),
                ("0O", 8),
                ("0b", 2),
                ("0B", 2),
            ] {
                if let Some(digits) = value.strip_prefix(prefix) {
                    if digits.is_empty() {
                        return f64::NAN;
                    }
                    return digits
                        .chars()
                        .try_fold(0.0, |total, c| {
                            c.to_digit(radix)
                                .map(|digit| total * f64::from(radix) + f64::from(digit))
                        })
                        .unwrap_or(f64::NAN);
                }
            }
            // Rust also accepts "inf" and case-insensitive infinity spellings;
            // ECMAScript accepts only this signed decimal/Infinity grammar.
            static DECIMAL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
                regex::Regex::new(
                    r"^[+-]?(?:(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?|Infinity)$",
                )
                .unwrap()
            });
            if DECIMAL.is_match(value) {
                value.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
        Some(Value::Array(values)) => {
            if values.is_empty() {
                0.0
            } else if values.len() == 1 {
                number(Some(&Value::String(if values[0].is_null() {
                    String::new()
                } else {
                    string(values.first())
                })))
            } else {
                f64::NAN
            }
        }
        _ => f64::NAN,
    }
}

pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Number(value)) => value
            .as_f64()
            .is_some_and(|value| value != 0.0 && !value.is_nan()),
        _ => true,
    }
}

pub fn string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                if value.is_null() {
                    String::new()
                } else {
                    string(Some(value))
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
        Some(Value::Number(value)) => number_text(value.as_f64().unwrap_or(f64::NAN)),
        Some(value) => value.to_string(),
    }
}

pub fn text(value: Option<&Value>) -> String {
    if truthy(value) {
        string(value)
    } else {
        String::new()
    }
}

pub fn array(value: Option<&Value>) -> &[Value] {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

pub fn finite(value: Option<&Value>, fallback: f64) -> f64 {
    let value = number(value);
    if value.is_finite() { value } else { fallback }
}

pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// encodeURIComponent's exact unescaped ASCII set, shared by native UI links.
pub fn encode_uri_component(value: &str) -> String {
    const SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'!')
        .remove(b'~')
        .remove(b'*')
        .remove(b'\'')
        .remove(b'(')
        .remove(b')');
    percent_encoding::utf8_percent_encode(value, SET).to_string()
}

/// URI components retain '+' literally; malformed escapes and UTF-8 fail closed.
pub fn decode_uri_component(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1..index + 3]
                    .iter()
                    .all(u8::is_ascii_hexdigit)
            {
                return Err("Invalid URI escape".into());
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .map(|value| value.into_owned())
        .map_err(|_| "Invalid UTF-8 URI component".into())
}

/// ECMAScript's decimal/scientific threshold and finite/nonfinite spelling.
pub fn number_text(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        }
        .into();
    }
    if value == 0.0 {
        return "0".into();
    }
    if value.abs() >= 1e21 || value.abs() < 1e-6 {
        let text = format!("{value:e}");
        let (mantissa, exponent) = text.split_once('e').expect("formatted exponent");
        return format!(
            "{mantissa}e{}{exponent}",
            if exponent.starts_with('-') { "" } else { "+" }
        );
    }
    value.to_string()
}

/// Exact JS toFixed rounding for the bounded precisions used by UI policies.
/// Supports 0..=6; precision is supplied by native code, never an input payload.
pub fn fixed(value: f64, digits: u32) -> String {
    assert!(digits <= 6, "unsupported fixed precision");
    if !value.is_finite() || value.abs() >= 1e21 {
        return number_text(value);
    }
    let scale = 10_u128.pow(digits);
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa =
        u128::from(bits & ((1_u64 << 52) - 1)) + if exponent == 0 { 0 } else { 1_u128 << 52 };
    let binary_shift = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let numerator = mantissa * scale;
    let rounded = if binary_shift >= 0 {
        numerator << binary_shift
    } else if binary_shift <= -128 {
        0
    } else {
        let shift = (-binary_shift) as u32;
        (numerator + (1_u128 << (shift - 1))) >> shift
    };
    let sign = if value < 0.0 { "-" } else { "" };
    if digits == 0 {
        format!("{sign}{rounded}")
    } else {
        format!(
            "{sign}{}.{:0width$}",
            rounded / scale,
            rounded % scale,
            width = digits as usize
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decimal_fixed_uses_exact_binary_rounding_and_js_number_thresholds() {
        use super::*;
        assert_eq!(fixed(2.25, 1), "2.3");
        assert_eq!(fixed(1.15, 1), "1.1");
        assert_eq!(fixed(-2.25, 1), "-2.3");
        assert_eq!(fixed(-0.0, 1), "0.0");
        assert_eq!(fixed(-f64::from_bits(1), 1), "-0.0");
        assert_eq!(fixed(1e21, 1), "1e+21");
        assert_eq!(fixed(f64::INFINITY, 0), "Infinity");
        assert_eq!(number_text(1e-7), "1e-7");
        assert_eq!(number_text(1e-6), "0.000001");
    }

    use super::*;
    use serde_json::json;
    #[test]
    fn uri_components_preserve_plus_and_reject_invalid_utf8_or_escapes() {
        let value = "🌸 +/?!()";
        assert_eq!(
            decode_uri_component(&encode_uri_component(value)).unwrap(),
            value
        );
        assert_eq!(decode_uri_component("one+two").unwrap(), "one+two");
        for value in ["%", "%1", "%xy", "%ff", "%c0%80", "%ed%a0%80"] {
            assert!(decode_uri_component(value).is_err());
        }
    }
    #[test]
    fn coercion_uses_ecma_whitespace_radices_and_arrays() {
        assert_eq!(number(Some(&json!("\u{feff}42\u{feff}"))), 42.0);
        assert!(number(Some(&json!("\u{0085}42"))).is_nan());
        for (input, expected) in [("0xff", 255.0), ("0O17", 15.0), ("0b11", 3.0)] {
            assert_eq!(number(Some(&json!(input))), expected);
        }
        for input in [
            "0x", "0o8", "-0x1", "0b2", "inf", "INF", "infinity", "1_000",
        ] {
            assert!(number(Some(&json!(input))).is_nan());
        }
        assert_eq!(number(Some(&json!("+Infinity"))), f64::INFINITY);
        assert_eq!(number(Some(&json!("-.125e+2"))), -12.5);
        assert!(number(None).is_nan());
        assert_eq!(number(Some(&Value::Null)), 0.0);
        assert_eq!(number(Some(&json!([]))), 0.0);
        assert_eq!(number(Some(&json!([null]))), 0.0);
        assert_eq!(string(Some(&json!(1.0))), "1");
    }
}
