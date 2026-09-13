//! Stable Qt ListModel edit plans using only IDs, without copying row payloads.
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

struct Remaining(Vec<usize>);
impl Remaining {
    fn new(count: usize) -> Self {
        Self(
            (0..=count)
                .map(|index| index.isolate_lowest_one())
                .collect(),
        )
    }
    fn before(&self, mut index: usize) -> usize {
        let mut count = 0;
        while index > 0 {
            count += self.0[index];
            index -= index.isolate_lowest_one();
        }
        count
    }
    fn remove(&mut self, mut index: usize) {
        index += 1;
        while index < self.0.len() {
            self.0[index] -= 1;
            index += index.isolate_lowest_one();
        }
    }
}
fn ids(value: Option<&Value>) -> Result<Vec<&str>, String> {
    let array = value
        .and_then(Value::as_array)
        .filter(|items| items.len() <= 16384)
        .ok_or("invalid model IDs")?;
    array
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| value.len() <= 4096)
                .ok_or_else(|| "invalid model ID".into())
        })
        .collect()
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    if function != "plan" {
        return Err("unknown model function".into());
    }
    let previous = ids(args.first())?;
    let next = ids(args.get(1))?;
    let mut positions: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (index, id) in previous.iter().enumerate() {
        positions.entry(id).or_default().push_back(index);
    }
    let mut remaining = Remaining::new(previous.len());
    let mut operations = Vec::new();
    for (index, id) in next.iter().enumerate() {
        if let Some(original) = positions.get_mut(id).and_then(VecDeque::pop_front) {
            let from = index + remaining.before(original);
            if from != index {
                operations.push(json!({"move":from,"to":index}));
            }
            remaining.remove(original);
        } else {
            operations.push(json!({"insert":index}));
        }
    }
    Ok(json!(operations))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_preserve_stable_rows_with_insert_delete_reorder_and_duplicate_ids() {
        let mut seed = 42u32;
        for _ in 0..1000 {
            let mut random = || {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                seed as usize
            };
            let old: Vec<_> = (0..random() % 32)
                .map(|_| format!("id{}", random() % 20))
                .collect();
            let next: Vec<_> = (0..random() % 32)
                .map(|_| format!("id{}", random() % 20))
                .collect();
            let mut rows = old.clone();
            for operation in call("plan", &[json!(old), json!(next)])
                .unwrap()
                .as_array()
                .unwrap()
            {
                if let Some(index) = operation["insert"].as_u64() {
                    rows.insert(index as usize, next[index as usize].clone());
                } else {
                    let row = rows.remove(operation["move"].as_u64().unwrap() as usize);
                    rows.insert(operation["to"].as_u64().unwrap() as usize, row);
                }
            }
            rows.truncate(next.len());
            assert_eq!(rows, next);
        }
    }
}
