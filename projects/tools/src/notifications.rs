use crate::command::{atomic_write, epoch, json_output, state_home};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fs;

fn normalize(active: Value, history: Value, saved: Value, now: i64) -> (Value, Value) {
    let mut stamps = saved.as_object().cloned().unwrap_or_default();
    let active = active.as_array().cloned().unwrap_or_default();
    let history = history.as_array().cloned().unwrap_or_default();
    let mut present = HashSet::new();
    for item in active.iter().chain(&history) {
        let Some(id) = item.get("id") else { continue };
        let key = id.to_string();
        present.insert(key.clone());
        if stamps.get(&key).and_then(Value::as_i64).is_none() {
            stamps.insert(key, json!(now));
        }
    }
    // Keep old timestamps while mako still holds their IDs. Expiring timestamps
    // themselves would make an old notification appear new again every day.
    stamps.retain(|id, _| present.contains(id));
    let stamp = |mut item: Value| -> Option<Value> {
        let id = item.get("id")?.to_string();
        item.as_object_mut()?
            .insert("time".into(), stamps.get(&id)?.clone());
        Some(item)
    };
    let items: Vec<_> = active.into_iter().rev().filter_map(stamp).collect();
    let history: Vec<_> = history
        .into_iter()
        .rev()
        .filter_map(stamp)
        .filter(|item| item["time"].as_i64().unwrap_or(0) > now - 86400)
        .take(40)
        .collect();
    (
        json!({"count": items.len(), "items": items, "history": history}),
        Value::Object(stamps),
    )
}

pub fn state() -> Value {
    let active = json_output("makoctl", ["list", "-j"], json!([]));
    let history = json_output("makoctl", ["history", "-j"], json!([]));
    let path = state_home().join("seele-shell/notification-times.json");
    let saved = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| Value::Object(Map::new()));
    let (result, stamps) = normalize(active, history, saved.clone(), epoch());
    if saved != stamps {
        let _ = atomic_write(&path, stamps.to_string().as_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_toasts_keep_their_first_seen_time_across_polls_and_dismissal() {
        let (first, saved) = normalize(json!([{"id":1}, {"id":2}]), json!([]), json!({}), 100000);
        assert_eq!(first["items"][0]["id"], 2);
        assert_eq!(first["items"][0]["time"], 100000);
        let (next, _) = normalize(json!([{"id":2}]), json!([{"id":1}]), saved, 100005);
        assert_eq!(next["items"][0]["time"], 100000);
        assert_eq!(next["history"][0]["time"], 100000);
        assert_eq!(next["count"], 1);
    }

    #[test]
    fn old_history_never_becomes_new_again_and_missing_ids_are_pruned() {
        let (result, saved) = normalize(json!([]), json!([{"id":1}]), json!({"1":1,"2":10}), 90000);
        assert_eq!(result["history"], json!([]));
        assert_eq!(saved, json!({"1":1}));
        let (result, _) = normalize(json!([]), json!([{"id":1}]), saved, 90005);
        assert_eq!(result["history"], json!([]));
    }

    #[test]
    fn malformed_cache_is_recovered_and_history_is_bounded() {
        let history = (0..60).map(|id| json!({"id":id})).collect::<Vec<_>>();
        let (result, _) = normalize(Value::Null, json!(history), json!([]), 100000);
        assert_eq!(result["history"].as_array().unwrap().len(), 40);
        assert_eq!(result["history"][0]["id"], 59);
    }
}
