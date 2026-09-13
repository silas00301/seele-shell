//! Home Assistant presentation and preference policy, evaluated on source updates.
use crate::value::{array, number, string, truthy};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn reading(item: &Value) -> bool {
    !truthy(item.get("controllable"))
}
fn present(item: &Value) -> Value {
    let mut item = if item.is_object() {
        item.clone()
    } else {
        json!({})
    };
    let domain = text(&item, "entity_id").split('.').next().unwrap_or("");
    let glyph = match domain {
        "light" => "󰌵",
        "fan" => "󰈐",
        "switch" | "input_boolean" => "󰐥",
        _ => match text(&item, "device_class") {
            "temperature" => "󰔏",
            "humidity" | "moisture" => "󰖎",
            "battery" => "󰁹",
            "power" | "energy" | "voltage" | "current" => "󱐋",
            "door" | "window" | "opening" | "garage_door" => "󰠡",
            "motion" | "occupancy" | "presence" => "󰛕",
            _ if matches!(text(&item, "unit"), "°C" | "°F") => "󰔏",
            _ => "󰓅",
        },
    };
    let label = match text(&item, "state") {
        _ if !truthy(item.get("available")) => "Unavailable",
        "on" => match text(&item, "device_class") {
            "door" | "window" | "opening" | "garage_door" => "Open",
            "motion" | "occupancy" | "presence" => "Detected",
            _ => "On",
        },
        "off" => match text(&item, "device_class") {
            "door" | "window" | "opening" | "garage_door" => "Closed",
            "motion" | "occupancy" | "presence" => "Clear",
            _ => "Off",
        },
        state => state,
    }
    .to_owned();
    item["glyph"] = json!(glyph);
    item["state_label"] = json!(label);
    item
}
fn entries(value: Option<&Value>) -> Result<&[Value], String> {
    let values = array(value);
    if values.len() > 16384 {
        Err("Home Assistant list exceeds its limit".into())
    } else {
        Ok(values)
    }
}
fn project(entities: &[Value], preferences: &[Value]) -> Value {
    // Groups are keyed by identity, never by state. Readouts remain named and
    // may be favorites too; no selected sensor disappears into a room heading.
    let mut groups: Vec<Value> = Vec::new();
    let mut indexes = HashMap::new();
    if entities.iter().any(|item| truthy(item.get("favorite"))) {
        groups.push(json!({"key":"favorites","heading":"Favorites","readings":[],"controls":[]}));
        indexes.insert("favorites".to_owned(), 0);
    }
    for item in entities {
        let favorite = truthy(item.get("favorite"));
        let room = text(item, "room");
        let key = if favorite {
            "favorites".to_owned()
        } else {
            format!("room:{room}")
        };
        let next = groups.len();
        let index = *indexes.entry(key.clone()).or_insert_with(|| {
            groups.push(json!({"key":key,"heading":if room.is_empty() { "Other" } else { room },"readings":[],"controls":[]}));
            next
        });
        let kind = if reading(item) {
            "readings"
        } else {
            "controls"
        };
        groups[index][kind]
            .as_array_mut()
            .unwrap()
            .push(present(item));
    }
    let mut known = HashMap::new();
    for item in entities {
        known.entry(text(item, "entity_id")).or_insert(item);
    }
    let mut preference_map = Map::new();
    let mut moves = Map::new();
    let mut last = HashMap::new();
    for (index, preference) in preferences.iter().enumerate() {
        let id = text(preference, "entity_id");
        preference_map
            .entry(id)
            .or_insert_with(|| preference.clone());
        let Some(item) = known.get(id) else {
            continue;
        };
        let reading = reading(item);
        let favorite = truthy(item.get("favorite"));
        let category = (
            reading,
            favorite,
            if favorite { "" } else { text(item, "room") },
        );
        let previous = last.insert(category, (id, index));
        moves.insert(
            id.to_owned(),
            json!({"before":previous.map_or(-1,|(_,index)|index as i64),"after":-1}),
        );
        if let Some((previous, _)) = previous {
            moves[previous]["after"] = json!(index);
        }
    }
    json!({"groups":groups,"preferences":preference_map,"moves":moves})
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let values = entries(args.first())?;
    let id = args.get(1).and_then(Value::as_str).unwrap_or("");
    Ok(match function {
        "project" => project(values, entries(args.get(1))?),
        "edit" => {
            let field = args.get(2).and_then(Value::as_str).unwrap_or("");
            if !matches!(field, "name" | "room" | "favorite") {
                return Ok(Value::Null);
            }
            let Some(index) = values.iter().position(|item| text(item, "entity_id") == id) else {
                return Ok(Value::Null);
            };
            let mut next = values.to_vec();
            next[index][field] = args.get(3).cloned().unwrap_or(Value::Null);
            json!(next)
        }
        "select" => {
            let selected = values.iter().any(|item| text(item, "entity_id") == id);
            let mut next: Vec<_> = values
                .iter()
                .filter(|item| text(item, "entity_id") != id)
                .cloned()
                .collect();
            if !selected {
                next.push(json!({"entity_id":id,"name":"","room":"","favorite":false}));
            }
            let summary = args.get(2).cloned().unwrap_or(Value::Null);
            json!({"entries":next,"summary":if selected && summary==id {json!("")} else {summary}})
        }
        "move" => {
            let target = number(args.get(2));
            let Some(index) = values.iter().position(|item| text(item, "entity_id") == id) else {
                return Ok(Value::Null);
            };
            if !target.is_finite()
                || target < 0.0
                || target.fract() != 0.0
                || target >= values.len() as f64
            {
                return Ok(Value::Null);
            }
            let mut next = values.to_vec();
            next.swap(index, target as usize);
            json!(next)
        }
        "devices" => {
            let catalog = entries(args.get(1))?;
            let query = string(args.get(2)).trim().to_lowercase();
            let selected_only = truthy(args.get(3));
            let mut known = HashSet::new();
            let mut result = Vec::new();
            for item in values
                .iter()
                .chain(if selected_only { &[] } else { catalog })
            {
                if !known.insert(text(item, "entity_id")) {
                    continue;
                }
                let searchable = format!(
                    "{} {} {}",
                    string(item.get("name")),
                    string(item.get("entity_id")),
                    string(item.get("room"))
                )
                .to_lowercase();
                if searchable.contains(&query) {
                    result.push(present(item));
                }
            }
            json!(result)
        }
        _ => return Err("unknown Home Assistant UI function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selected_entity_has_one_named_home() {
        let entities = json!([
            {"entity_id":"sensor.office","name":"Office temperature","room":"Office","state":"21.4","unit":"°C","available":true,"favorite":true},
            {"entity_id":"light.desk","name":"Desk","room":"Office","state":"on","available":true,"controllable":true,"favorite":true},
            {"entity_id":"sensor.humidity","name":"Humidity","room":"Office","state":"unavailable","available":false},
            {"entity_id":"binary_sensor.door","name":"Door","room":"Favorites","state":"off","device_class":"door","available":true},
            {"entity_id":"sensor.other","name":"Other","room":"","state":"12","available":true}
        ]);
        let result = project(entities.as_array().unwrap(), &[]);
        let groups = result["groups"].as_array().unwrap();
        assert_eq!(
            groups.iter().map(|g| text(g, "key")).collect::<Vec<_>>(),
            ["favorites", "room:Office", "room:Favorites", "room:"]
        );
        assert_eq!(groups[0]["readings"][0]["name"], "Office temperature");
        assert_eq!(groups[0]["controls"][0]["entity_id"], "light.desk");
        assert_eq!(groups[1]["readings"][0]["state_label"], "Unavailable");
        assert_eq!(groups[2]["readings"][0]["state_label"], "Closed");
        let ids: Vec<_> = groups
            .iter()
            .flat_map(|g| {
                g["readings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .chain(g["controls"].as_array().unwrap())
            })
            .map(|e| text(e, "entity_id"))
            .collect();
        assert_eq!(ids.len(), 5);
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 5);
    }

    #[test]
    fn updates_do_not_reorder_groups_or_controls() {
        let mut entities = json!([
            {"entity_id":"light.a","room":"Office","controllable":true,"state":"off"},
            {"entity_id":"light.b","room":"Office","controllable":true,"state":"on"},
            {"entity_id":"fan.c","room":"Bedroom","controllable":true,"state":"on"}
        ]);
        let before = project(entities.as_array().unwrap(), &[]);
        entities[0]["state"] = json!("on");
        entities[1]["available"] = json!(false);
        let after = project(entities.as_array().unwrap(), &[]);
        for (before, after) in before["groups"]
            .as_array()
            .unwrap()
            .iter()
            .zip(after["groups"].as_array().unwrap())
        {
            assert_eq!(before["key"], after["key"]);
            assert_eq!(
                before["controls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| text(v, "entity_id"))
                    .collect::<Vec<_>>(),
                after["controls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| text(v, "entity_id"))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn picker_filters_without_losing_saved_unavailable_entities() {
        let selected =
            json!([{"entity_id":"sensor.a","name":"Saved name","room":"Office","available":false}]);
        let catalog = json!([{"entity_id":"sensor.a","name":"Old name"},{"entity_id":"light.b","name":"Desk","room":"Office"}]);
        let results = call(
            "devices",
            &[
                selected.clone(),
                catalog.clone(),
                json!(" OFFICE "),
                json!(false),
            ],
        )
        .unwrap();
        assert_eq!(results.as_array().unwrap().len(), 2);
        assert_eq!(results[0]["name"], "Saved name");
        let results = call("devices", &[selected, catalog, json!(""), json!(true)]).unwrap();
        assert_eq!(results.as_array().unwrap().len(), 1);
        assert_eq!(results[0]["state_label"], "Unavailable");
    }

    #[test]
    fn favorite_readouts_order_with_readouts_and_controls_with_controls() {
        let entities = json!([
            {"entity_id":"sensor.a","favorite":true,"room":"Office"},
            {"entity_id":"light.b","favorite":true,"room":"Office","controllable":true},
            {"entity_id":"sensor.c","favorite":true,"room":"Bedroom"}
        ]);
        let result = project(entities.as_array().unwrap(), entities.as_array().unwrap());
        assert_eq!(result["moves"]["sensor.a"]["after"], 2);
        assert_eq!(result["moves"]["sensor.c"]["before"], 0);
        assert_eq!(result["moves"]["light.b"]["after"], -1);
    }
}
