//! Home Assistant presentation and preference policy, evaluated on source updates.
use crate::value::{array, number, string, truthy};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn reading(item: &Value) -> bool {
    text(item, "entity_id").starts_with("sensor.")
        && (matches!(text(item, "device_class"), "temperature" | "humidity")
            || matches!(text(item, "unit"), "°C" | "°F"))
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
    let mut rows = Vec::new();
    let favorites: Vec<_> = entities
        .iter()
        .filter(|item| truthy(item.get("favorite")) && !reading(item))
        .collect();
    if !favorites.is_empty() {
        rows.push(json!({"heading":"Favorites"}));
        rows.extend(favorites.into_iter().cloned());
    }
    let mut room_indexes = HashMap::new();
    let mut rooms: Vec<(&str, Vec<&Value>, Vec<String>)> = Vec::new();
    for item in entities {
        let reading = reading(item);
        if truthy(item.get("favorite")) && !reading {
            continue;
        }
        let room = text(item, "room");
        let next = rooms.len();
        let index = *room_indexes.entry(room).or_insert_with(|| {
            rooms.push((room, Vec::new(), Vec::new()));
            next
        });
        if reading {
            rooms[index].2.push(if truthy(item.get("available")) {
                format!("{}{}", string(item.get("state")), string(item.get("unit")))
            } else {
                format!(
                    "{} unavailable",
                    if text(item, "device_class") == "humidity" {
                        "Humidity"
                    } else {
                        "Temperature"
                    }
                )
            });
        } else {
            rooms[index].1.push(item);
        }
    }
    for (room, members, readings) in rooms {
        rows.push(json!({"heading":room,"detail":readings.join(" · ")}));
        rows.extend(members.into_iter().cloned());
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
        let favorite = !reading && truthy(item.get("favorite"));
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
    json!({"rows":rows,"preferences":preference_map,"moves":moves})
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
            let query = string(args.get(2)).to_lowercase();
            let mut known = HashSet::new();
            let mut result = Vec::new();
            for item in values.iter().chain(catalog) {
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
                    result.push(item);
                }
            }
            json!(result)
        }
        _ => return Err("unknown Home Assistant UI function".into()),
    })
}
