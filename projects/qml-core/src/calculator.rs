//! Bounded, local arithmetic and conversions. No interpreter, IO, or persistence.
use serde_json::{Value, json};

const MAX_INPUT: usize = 512;
const MAX_DEPTH: usize = 32;
const MAX_TAPE: usize = 32;
type CalcResult = Result<f64, &'static str>;

struct Parser<'a> {
    source: &'a [u8],
    cursor: usize,
    answer: Option<f64>,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .source
            .get(self.cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.cursor += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.source.get(self.cursor) == Some(&byte) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    // Pratt binding powers: power is right-associative and binds more tightly
    // than a leading minus, so -2^2 = -4 and 2^-2 = 0.25.
    fn expression(&mut self, minimum: u8, depth: usize) -> CalcResult {
        if depth > MAX_DEPTH {
            return Err("Use fewer nested operations (maximum 32).");
        }
        self.space();
        let mut left = if self.take(b'+') {
            self.expression(5, depth + 1)?
        } else if self.take(b'-') {
            -self.expression(5, depth + 1)?
        } else if self.take(b'(') {
            let value = self.expression(0, depth + 1)?;
            if !self.take(b')') {
                return Err("Close the parenthesis with ).");
            }
            value
        } else if self
            .source
            .get(self.cursor)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            let start = self.cursor;
            while self
                .source
                .get(self.cursor)
                .is_some_and(u8::is_ascii_alphabetic)
            {
                self.cursor += 1;
            }
            match &self.source[start..self.cursor] {
                b"pi" => std::f64::consts::PI,
                b"e" => std::f64::consts::E,
                b"ans" => self.answer.ok_or("Calculate something first to use ans.")?,
                name @ (b"sqrt" | b"abs" | b"round") => {
                    if !self.take(b'(') {
                        return Err("Put the function's value in parentheses.");
                    }
                    let value = self.expression(0, depth + 1)?;
                    if !self.take(b')') {
                        return Err("Close the parenthesis with ).");
                    }
                    match name {
                        b"sqrt" => value.sqrt(),
                        b"abs" => value.abs(),
                        _ => value.round(),
                    }
                }
                _ => return Err("Use numbers, pi, e, ans, sqrt(), abs() or round()."),
            }
        } else {
            let start = self.cursor;
            while self
                .source
                .get(self.cursor)
                .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
            {
                self.cursor += 1;
            }
            if start == self.cursor {
                return Err("Enter a number or an expression.");
            }
            if matches!(self.source.get(self.cursor), Some(b'e' | b'E')) {
                self.cursor += 1;
                if matches!(self.source.get(self.cursor), Some(b'+' | b'-')) {
                    self.cursor += 1;
                }
                while self.source.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                    self.cursor += 1;
                }
            }
            std::str::from_utf8(&self.source[start..self.cursor])
                .ok()
                .and_then(|text| text.parse::<f64>().ok())
                .ok_or("Check the number; use a dot for decimals.")?
        };
        finite(left)?;
        loop {
            self.space();
            if self.source.get(self.cursor) == Some(&b'%') && minimum <= 9 {
                self.cursor += 1;
                left /= 100.0;
                continue;
            }
            let Some(&operator) = self.source.get(self.cursor) else {
                break;
            };
            let (binding, right_binding) = match operator {
                b'+' | b'-' => (1, 2),
                b'*' | b'/' => (3, 4),
                b'^' => (7, 7),
                _ => break,
            };
            if binding < minimum {
                break;
            }
            self.cursor += 1;
            let right = self.expression(right_binding, depth + 1)?;
            left = match operator {
                b'+' => left + right,
                b'-' => left - right,
                b'*' => left * right,
                b'/' if right == 0.0 => return Err("Cannot divide by zero."),
                b'/' => left / right,
                _ => left.powf(right),
            };
            finite(left)?;
        }
        finite(left)
    }
}
fn finite(value: f64) -> CalcResult {
    if value.is_finite() {
        Ok(if value == 0.0 { 0.0 } else { value })
    } else {
        Err("The result is outside the finite real-number range.")
    }
}
fn arithmetic(text: &str, answer: Option<f64>) -> CalcResult {
    let mut parser = Parser {
        source: text.as_bytes(),
        cursor: 0,
        answer,
    };
    let value = parser.expression(0, 0)?;
    parser.space();
    if parser.cursor != parser.source.len() {
        return Err("Check the expression. Write conversions like 12 km to mi.");
    }
    Ok(value)
}

// Units are explicit and case-sensitive: MB is decimal, MiB is binary, and
// temperatures use their affine conversion instead of pretending to be ratios.
fn unit(name: &str) -> Option<(&'static str, f64, f64)> {
    let (dimension, scale, offset) = match name {
        "mm" => ("length", 0.001, 0.0),
        "cm" => ("length", 0.01, 0.0),
        "m" => ("length", 1.0, 0.0),
        "km" => ("length", 1000.0, 0.0),
        "in" => ("length", 0.0254, 0.0),
        "ft" => ("length", 0.3048, 0.0),
        "yd" => ("length", 0.9144, 0.0),
        "mi" => ("length", 1609.344, 0.0),
        "mg" => ("mass", 0.000001, 0.0),
        "g" => ("mass", 0.001, 0.0),
        "kg" => ("mass", 1.0, 0.0),
        "oz" => ("mass", 0.028349523125, 0.0),
        "lb" => ("mass", 0.45359237, 0.0),
        "ms" => ("time", 0.001, 0.0),
        "s" => ("time", 1.0, 0.0),
        "min" => ("time", 60.0, 0.0),
        "h" => ("time", 3600.0, 0.0),
        "day" => ("time", 86400.0, 0.0),
        "mL" => ("volume", 0.001, 0.0),
        "L" => ("volume", 1.0, 0.0),
        "B" => ("data", 1.0, 0.0),
        "kB" => ("data", 1000.0, 0.0),
        "MB" => ("data", 1e6, 0.0),
        "GB" => ("data", 1e9, 0.0),
        "TB" => ("data", 1e12, 0.0),
        "KiB" => ("data", 1024.0, 0.0),
        "MiB" => ("data", 1048576.0, 0.0),
        "GiB" => ("data", 1073741824.0, 0.0),
        "TiB" => ("data", 1099511627776.0, 0.0),
        "C" | "°C" => ("temperature", 1.0, 273.15),
        "F" | "°F" => ("temperature", 5.0 / 9.0, 273.15 - 32.0 * 5.0 / 9.0),
        "K" => ("temperature", 1.0, 0.0),
        _ => return None,
    };
    Some((dimension, scale, offset))
}
fn format(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    // Twelve significant decimal digits, with scientific notation for extremes.
    let exponent = value.abs().log10().floor() as i32;
    if !(-6..12).contains(&exponent) {
        let raw = format!("{value:.11e}");
        let (mantissa, exponent) = raw.split_once('e').unwrap();
        format!(
            "{}e{exponent}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        )
    } else {
        let digits = (11 - exponent).max(0) as usize;
        let raw = format!("{value:.digits$}");
        if raw.contains('.') {
            raw.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            raw
        }
    }
}
fn evaluate(text: &str, answer: Option<f64>) -> Result<Value, &'static str> {
    if text.len() > MAX_INPUT {
        return Err("Keep the expression within 512 bytes.");
    }
    let text = text.trim();
    if text.is_empty() {
        return Ok(json!({"empty": true}));
    }
    let (value, suffix) = if let Some((source, target)) = text.split_once(" to ") {
        let target = target.trim();
        let (expression, from) = source
            .trim()
            .rsplit_once(char::is_whitespace)
            .ok_or("Write a conversion like 12 km to mi.")?;
        let (dimension, scale, offset) =
            unit(from).ok_or("Unknown source unit. Open the unit guide for supported symbols.")?;
        let (destination, target_scale, target_offset) =
            unit(target).ok_or("Unknown target unit. Unit symbols are case-sensitive.")?;
        if dimension != destination {
            return Err("Choose two units of the same kind.");
        }
        let mut base = finite(arithmetic(expression, answer)? * scale + offset)?;
        if dimension == "temperature" {
            if base < -1e-10 {
                return Err("Temperature cannot be below absolute zero.");
            }
            // The exact Fahrenheit lower bound can land a few ulps below zero
            // after the affine transform; never display a negative Kelvin.
            base = base.max(0.0);
        }
        (finite((base - target_offset) / target_scale)?, target)
    } else {
        (arithmetic(text, answer)?, "")
    };
    let number = format(value);
    let result = if suffix.is_empty() {
        number.clone()
    } else {
        format!("{number} {suffix}")
    };
    Ok(
        json!({"value": value, "number": number, "result": result, "expression": text, "unit": suffix}),
    )
}
fn answer(state: &Value) -> Option<f64> {
    state
        .get("answer")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}
fn tape(state: &Value) -> Vec<Value> {
    state.get("tape").and_then(Value::as_array).into_iter().flatten().take(MAX_TAPE)
        .filter(|entry| entry.get("expression").and_then(Value::as_str).is_some_and(|s| s.len() <= MAX_INPUT)
            && entry.get("result").and_then(Value::as_str).is_some_and(|s| s.len() <= 128)
            && entry.get("number").and_then(Value::as_str).is_some_and(|s| s.len() <= 64))
        .map(|entry| json!({"expression":entry["expression"], "result":entry["result"], "number":entry["number"]})).collect()
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let text = args.first().and_then(Value::as_str).unwrap_or_default();
    let state = args.get(1).unwrap_or(&Value::Null);
    match function {
        "preview" | "commit" => {
            let result = match evaluate(text, answer(state)) {
                Ok(result) => result,
                Err(error) => return Ok(json!({"error":error})),
            };
            if function == "preview" || result["empty"] == true {
                return Ok(result);
            }
            let mut entries = tape(state);
            entries.insert(0, json!({"expression": result["expression"], "result":result["result"], "number":result["number"]}));
            entries.truncate(MAX_TAPE);
            Ok(json!({"answer":result["value"], "last":result, "tape":entries}))
        }
        _ => Err("unknown calculator function".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value(text: &str) -> f64 {
        evaluate(text, None).unwrap()["value"].as_f64().unwrap()
    }
    #[test]
    fn precedence_and_number_grammar() {
        for (text, expected) in [
            ("2+3*4", 14.0),
            ("(2+3)*4", 20.0),
            ("-2^2", -4.0),
            ("2^-2", 0.25),
            ("2^3^2", 512.0),
            ("200*15%", 30.0),
            ("sqrt(81)+abs(-2)", 11.0),
            ("1.2e3 + .5", 1200.5),
            ("round(2.6)", 3.0),
        ] {
            assert_eq!(value(text), expected, "{text}");
        }
        assert_eq!(arithmetic("ans*2", Some(7.0)), Ok(14.0));
        assert!(arithmetic("ans", None).is_err());
    }
    #[test]
    fn conversions_preserve_dimensions_and_temperature_offsets() {
        for (text, expected) in [
            ("1 mi to km", 1.609344),
            ("2.5 kg to g", 2500.0),
            ("90 min to h", 1.5),
            ("1 GiB to MiB", 1024.0),
            ("1 GB to MB", 1000.0),
            ("32 F to C", 0.0),
            ("100 C to F", 212.0),
            ("-273.15 C to K", 0.0),
            ("-459.67 F to K", 0.0),
            ("(2+3) ft to in", 60.0),
        ] {
            assert!((value(text) - expected).abs() < 1e-9, "{text}");
        }
        for text in [
            "1 kg to m",
            "1 MB to mb",
            "-1 K to C",
            "1 m to",
            "1 foo to m",
        ] {
            assert!(evaluate(text, None).is_err(), "{text}");
        }
    }
    #[test]
    fn unsafe_invalid_and_unbounded_input_is_data_only() {
        for text in [
            "1/0",
            "sqrt(-1)",
            "10^999",
            "1e999",
            "1.2.3",
            "1e",
            "2(3)",
            "1; id",
            "$(id)",
            "Math.random()",
            "NaN",
            "<img>",
            "((((1)",
            "1,5",
            "π",
        ] {
            assert!(evaluate(text, None).is_err(), "{text}");
        }
        assert!(evaluate(&"1".repeat(513), None).is_err());
        assert!(evaluate(&format!("{}1{}", "(".repeat(100), ")".repeat(100)), None).is_err());
        assert!(evaluate(&format!("{}1", "-".repeat(100)), None).is_err());
        assert_eq!(format(value("0.1+0.2")), "0.3");
        assert_eq!(format(value("1e-20")), "1e-20");
        assert_eq!(format(value("-0")), "0");
    }
    #[test]
    fn tape_is_bounded_errors_do_not_commit_and_answer_chains() {
        let mut state = json!({});
        for index in 0..40 {
            state = call("commit", &[json!(index.to_string()), state]).unwrap();
        }
        assert_eq!(state["tape"].as_array().unwrap().len(), 32);
        assert_eq!(state["tape"][31]["expression"], "8");
        let next = call("commit", &[json!("ans+1"), state.clone()]).unwrap();
        assert_eq!(next["answer"], 40.0);
        assert!(
            call("commit", &[json!("1/0"), state])
                .unwrap()
                .get("error")
                .is_some()
        );
        let hostile =
            json!({"tape":[{"expression":"1","result":"1","number":"1","extra":"never retained"}]});
        let next = call("commit", &[json!("2"), hostile]).unwrap();
        assert!(next["tape"][1].get("extra").is_none());
    }
}
