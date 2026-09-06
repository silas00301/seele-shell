use serde_json::{json, Value};

// pw-dump -m emits an initial array followed by arrays of partial updates.
// Keep registry order for the device list's stable, case-insensitive sort.
pub(crate) struct Graph {
    objects: Value,
}

impl Default for Graph {
    fn default() -> Self {
        Self { objects: json!([]) }
    }
}

fn merge(current: &mut Value, patch: Value) {
    match (current, patch) {
        (Value::Object(current), Value::Object(patch)) => {
            for (key, value) in patch {
                if key == "props" {
                    current.insert(key, value);
                } else {
                    merge(current.entry(key).or_insert(Value::Null), value);
                }
            }
        }
        (current, patch) => *current = patch,
    }
}

impl Graph {
    pub(crate) fn update(&mut self, update: Value) {
        let Value::Array(updates) = update else {
            return;
        };
        let objects = self.objects.as_array_mut().unwrap();
        for mut patch in updates {
            let Some(id) = patch.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let position = objects.iter().position(|object| object["id"] == id);
            if patch.get("info") == Some(&Value::Null)
                || (patch.as_object().is_some_and(|object| object.len() == 1))
            {
                if let Some(position) = position {
                    objects.remove(position);
                }
                continue;
            }
            if let Some(position) = position {
                // Metadata events replace individual keys, not the entire
                // default-device table. A null value removes that key.
                if let Some(Value::Array(entries)) = patch
                    .as_object_mut()
                    .and_then(|object| object.remove("metadata"))
                {
                    let object = &mut objects[position];
                    if !object["metadata"].is_array() {
                        object["metadata"] = json!([]);
                    }
                    let metadata = object["metadata"].as_array_mut().unwrap();
                    for entry in entries {
                        metadata.retain(|old| {
                            old["subject"] != entry["subject"] || old["key"] != entry["key"]
                        });
                        if !entry["value"].is_null() {
                            metadata.push(entry);
                        }
                    }
                }
                merge(&mut objects[position], patch);
            } else {
                objects.push(patch);
            }
        }
    }

    pub(crate) fn snapshot(&self) -> &Value {
        &self.objects
    }

    // Stream activity changes need no wpctl calls. Only device properties and
    // default routing can change the values get-volume reports.
    pub(crate) fn volume_key(&self) -> Value {
        json!(self
            .objects
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|object| {
                if object
                    .pointer("/props/metadata.name")
                    .and_then(Value::as_str)
                    == Some("default")
                {
                    Some(json!({"id":object["id"],"metadata":object["metadata"]}))
                } else if matches!(
                    object
                        .pointer("/info/props/media.class")
                        .and_then(Value::as_str),
                    Some("Audio/Sink" | "Audio/Source")
                ) {
                    Some(json!({"id":object["id"],"props":object.pointer("/info/params/Props")}))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_updates_and_removals_preserve_identity_and_clear_old_properties() {
        let mut graph = Graph::default();
        graph.update(json!([{"id":1,"type":"Node","info":{"state":"idle","props":{"media.class":"Audio/Sink","old":1},"params":{"Props":[{"mute":false}]}}}]));
        let original = graph.volume_key();
        graph.update(json!([{"id":1,"info":{"state":"running"}}]));
        assert_eq!(graph.volume_key(), original);
        assert_eq!(graph.snapshot()[0]["type"], "Node");
        graph.update(json!([{"id":1,"info":{"params":{"Props":[{"mute":true}]}}}]));
        assert_ne!(graph.volume_key(), original);
        graph.update(json!([{"id":1,"info":{"props":{"media.class":"Audio/Sink"}}}]));
        assert!(graph.snapshot()[0]["info"]["props"]["old"].is_null());
        graph.update(json!([{"id":1,"info":null}]));
        assert_eq!(graph.snapshot(), &json!([]));
    }

    #[test]
    fn metadata_updates_preserve_the_other_default_and_remove_null_entries() {
        let mut graph = Graph::default();
        graph.update(
            json!([{"id":2,"props":{"metadata.name":"default"},"metadata":[
                {"subject":0,"key":"default.audio.sink","value":{"name":"speakers"}},
                {"subject":0,"key":"default.audio.source","value":{"name":"mic"}}
            ]}]),
        );
        graph.update(json!([{"id":2,"metadata":[{"subject":0,"key":"default.audio.sink","value":{"name":"headphones"}}]}]));
        assert_eq!(graph.snapshot()[0]["metadata"].as_array().unwrap().len(), 2);
        graph.update(
            json!([{"id":2,"metadata":[{"subject":0,"key":"default.audio.sink","value":null}]}]),
        );
        assert_eq!(
            graph.snapshot()[0]["metadata"][0]["key"],
            "default.audio.source"
        );
    }
}
