use crate::{Result, MAX_MESSAGE};
use jsonschema::{PatternOptions, Validator};
use serde_json::Value;
use std::sync::Arc;

pub struct Request {
    pub value: Value,
    pub validator: Validator,
}
impl Request {
    pub fn consumer(&self) -> &str {
        self.value["consumer"].as_str().unwrap()
    }
    pub fn item(&self) -> Option<(&str, u64)> {
        self.value.get("item").map(|v| {
            (
                v.as_str().unwrap(),
                self.value["revision"].as_u64().unwrap(),
            )
        })
    }
    pub fn interactive(&self) -> bool {
        self.value["class"] == "interactive"
    }
}
fn inspect(value: &Value, depth: usize, remaining: &mut usize, schema: bool) -> Result<()> {
    if depth > if schema { 32 } else { 64 } || *remaining == 0 {
        return Err("invalid_input");
    }
    *remaining -= 1;
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if schema && matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef") {
                    return Err("invalid_input");
                }
                inspect(child, depth + 1, remaining, schema)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                inspect(child, depth + 1, remaining, schema)?;
            }
        }
        Value::Number(number) => {
            let text = number.to_string();
            if text.len() > 128
                || text.split_once(['e', 'E']).is_some_and(|(_, exponent)| {
                    exponent
                        .parse::<i32>()
                        .map_or(true, |n| n.unsigned_abs() > 308)
                })
            {
                return Err("invalid_input");
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn bounded_instance(value: &Value) -> bool {
    inspect(value, 0, &mut 16384, false).is_ok()
}
fn pattern_count(value: &Value) -> usize {
    match value {
        Value::Object(map) => {
            usize::from(map.contains_key("pattern"))
                + map
                    .get("patternProperties")
                    .and_then(Value::as_object)
                    .map_or(0, |v| v.len())
                + map.values().map(pattern_count).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(pattern_count).sum(),
        _ => 0,
    }
}
fn schema(value: &Value) -> Result<Validator> {
    let version = value["version"].as_str().ok_or("invalid_input")?;
    let document = value.get("schema").ok_or("invalid_input")?;
    if version.len() > 100
        || serde_json::to_vec(document)
            .map_err(|_| "invalid_input")?
            .len()
            > 64 * 1024
    {
        return Err("invalid_input");
    }
    inspect(document, 0, &mut 4096, true)?;
    if pattern_count(document) > 32 {
        return Err("invalid_input");
    }
    // Neither reference retrieval nor regex backtracking is permitted, even if
    // future callers bypass the structural guard above.
    jsonschema::draft202012::options()
        .offline()
        .should_validate_formats(false)
        .with_pattern_options(
            PatternOptions::regex()
                .size_limit(64 * 1024)
                .dfa_size_limit(64 * 1024),
        )
        .build(document)
        .map_err(|_| "invalid_input")
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}
pub fn request(value: Value) -> Result<Arc<Request>> {
    let object = value.as_object().ok_or("invalid_input")?;
    const ALLOWED: [&str; 9] = [
        "consumer", "label", "prompt", "context", "input", "output", "class", "item", "revision",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str()))
        || serde_json::to_vec(&value)
            .map_err(|_| "invalid_input")?
            .len()
            > MAX_MESSAGE
    {
        return Err("invalid_input");
    }
    for key in ["consumer", "label", "prompt"] {
        if value[key].as_str().is_none_or(|v| v.trim().is_empty()) {
            return Err("invalid_input");
        }
    }
    let label = value["label"].as_str().unwrap();
    if !identifier(value["consumer"].as_str().unwrap())
        || label.chars().count() > 120
        || label.chars().any(char::is_control)
    {
        return Err("invalid_input");
    }
    if let Some(class) = value.get("class") {
        if class != "interactive" && class != "background" {
            return Err("invalid_input");
        }
    }
    if object.contains_key("item") != object.contains_key("revision") {
        return Err("invalid_input");
    }
    if let Some(item) = value.get("item") {
        if item.as_str().is_none_or(|s| s.chars().count() > 200)
            || value["revision"].as_u64().is_none_or(|n| n >= (1 << 63))
        {
            return Err("invalid_input");
        }
    }
    inspect(&value["context"], 0, &mut 16384, false)?;
    if !schema(&value["input"])?.is_valid(&value["context"]) {
        return Err("invalid_input");
    }
    let validator = schema(&value["output"])?;
    Ok(Arc::new(Request { value, validator }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    pub fn payload() -> Value {
        json!({"consumer":"fixture","label":"Fixture job","prompt":"Private prompt","context":{"value":1},"input":{"version":"1","schema":{"type":"object","required":["value"]}},"output":{"version":"1","schema":{"type":"integer"}}})
    }
    #[test]
    fn rejects_invalid_inputs_and_all_references() {
        for (key, value) in [
            ("model", json!("other")),
            ("context", Value::Null),
            ("class", json!("urgent")),
            ("label", json!("control\u{7f}")),
            (
                "output",
                json!({"version":"1","schema":{"$ref":"file:///private"}}),
            ),
        ] {
            let mut input = payload();
            input[key] = value;
            assert!(request(input).is_err());
        }
        for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            let mut input = payload();
            input["output"]["schema"] =
                json!({"$defs":{"hidden":{keyword:"https://invalid.test"}}});
            assert!(request(input).is_err());
        }
        let mut input = payload();
        input["item"] = json!("x");
        input["revision"] = json!(true);
        assert!(request(input).is_err());
    }
    #[test]
    fn rejects_numeric_and_pattern_resource_amplification() {
        let huge: Value = serde_json::from_str("1e1000000000").unwrap();
        assert!(!bounded_instance(&huge));
        let mut input = payload();
        input["output"]["schema"] = json!({"type":"string","pattern":"a{1000000}"});
        assert!(request(input).is_err());
        let mut input = payload();
        input["output"]["schema"] =
            json!({"allOf":(0..33).map(|_|json!({"pattern":"a"})).collect::<Vec<_>>()});
        assert!(request(input).is_err());
    }
    #[test]
    fn enforces_202012_and_rejects_backtracking_patterns() {
        let mut input = payload();
        input["output"]["schema"] =
            json!({"type":"array","prefixItems":[{"type":"integer"}],"items":false});
        let request = request(input).unwrap();
        assert!(request.validator.is_valid(&json!([1])));
        assert!(!request.validator.is_valid(&json!([1, 2])));
        let mut input = payload();
        input["output"]["schema"] = json!({"type":"string","pattern":r"(a+)\1"});
        assert!(super::request(input).is_err());
    }
}
