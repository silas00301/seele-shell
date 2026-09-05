use serde_json::{json, Value};
use std::collections::HashSet;

fn contains_sink(value: &Value) -> bool {
    match value {
        Value::String(value) => value == "Audio/Sink",
        Value::Array(values) => values.iter().any(contains_sink),
        Value::Object(values) => values.values().any(contains_sink),
        _ => false,
    }
}

pub fn devices(dump: &Value) -> Vec<Value> {
    let mut default_sink = String::new();
    let mut default_source = String::new();
    for object in dump.as_array().into_iter().flatten().filter(|object| {
        object.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Metadata")
            && object
                .pointer("/props/metadata.name")
                .and_then(Value::as_str)
                == Some("default")
    }) {
        for item in object
            .get("metadata")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = item["key"].as_str().unwrap_or("");
            let name = item
                .pointer("/value/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            if key == "default.audio.sink" {
                default_sink = name.into()
            }
            if key == "default.audio.source" {
                default_source = name.into()
            }
        }
    }
    let mut values = Vec::new();
    for object in dump.as_array().into_iter().flatten() {
        let class = object
            .pointer("/info/props/media.class")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !matches!(class, "Audio/Sink" | "Audio/Source") {
            continue;
        }
        let props = object.pointer("/info/props").unwrap_or(&Value::Null);
        let node = props["node.name"].as_str().unwrap_or("");
        let kind = if class == "Audio/Sink" {
            "output"
        } else {
            "input"
        };
        values.push(json!({"id":object["id"],"kind":kind,"name":props["node.description"].as_str().or_else(||props["node.nick"].as_str()).or_else(||props["node.name"].as_str()).unwrap_or(""),"node":node,"profile":Value::Null,"default":if kind=="output"{node==default_sink}else{node==default_source}}));
    }
    let sink_devices: HashSet<u64> = dump
        .as_array()
        .into_iter()
        .flatten()
        .filter(|object| {
            object
                .pointer("/info/props/media.class")
                .and_then(Value::as_str)
                == Some("Audio/Sink")
        })
        .filter_map(|object| {
            object
                .pointer("/info/props/device.id")
                .and_then(Value::as_u64)
        })
        .collect();
    for device in dump.as_array().into_iter().flatten() {
        if device["type"].as_str() != Some("PipeWire:Interface:Device")
            || device
                .pointer("/info/props/media.class")
                .and_then(Value::as_str)
                != Some("Audio/Device")
            || device["id"]
                .as_u64()
                .is_some_and(|id| sink_devices.contains(&id))
        {
            continue;
        }
        let props = &device["info"]["props"];
        let name = props["device.description"]
            .as_str()
            .or_else(|| props["device.name"].as_str())
            .unwrap_or("");
        for profile in device
            .pointer("/info/params/EnumProfile")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if profile["available"].as_str() != Some("yes")
                || !contains_sink(&profile["classes"])
                || profile["index"].as_u64().is_none()
            {
                continue;
            }
            let description = profile["description"].as_str().unwrap_or("");
            values.push(json!({"id":device["id"],"kind":"output","name":format!("{name} · {description}"),"node":"","profile":profile["index"],"default":false}));
        }
    }
    values.sort_by_key(|value| {
        (
            value["kind"].as_str().unwrap_or("").to_owned(),
            value["name"].as_str().unwrap_or("").to_ascii_lowercase(),
        )
    });
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: u64) -> Value {
        json!({"id":id,"type":"PipeWire:Interface:Device","info":{
        "props":{"media.class":"Audio/Device","device.description":"Built-in Audio"},
        "params":{"EnumProfile":[
            {"index":0,"description":"Off","available":"yes","classes":[]},
            {"index":1,"description":"Analog Stereo","available":"yes","classes":[["Audio/Sink",1]]},
            {"index":2,"description":"Unplugged HDMI","available":"no","classes":[["Audio/Sink",1]]},
            {"index":3,"description":"Pro Audio","available":"unknown","classes":[["Audio/Sink",1]]},
            {"index":4,"description":"Input","available":"yes","classes":[["Audio/Source",1]]}
        ]}}})
    }

    #[test]
    fn inactive_card_offers_only_available_output_profiles() {
        let entries = devices(&json!([card(10)]));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["id"], 10);
        assert_eq!(entries[0]["profile"], 1);
        assert_eq!(entries[0]["default"], false);
    }

    #[test]
    fn active_sink_hides_profiles_and_retains_default_selection() {
        let sink = json!({"id":20,"type":"PipeWire:Interface:Node","info":{"props":{
            "media.class":"Audio/Sink","device.id":10,"node.name":"analog","node.description":"Speakers"
        }}});
        let metadata = json!({"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
            "metadata":[{"key":"default.audio.sink","value":{"name":"analog"}}]});
        let entries = devices(&json!([card(10), sink, metadata]));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["id"], 20);
        assert!(entries[0]["profile"].is_null());
        assert_eq!(entries[0]["default"], true);
    }
}
