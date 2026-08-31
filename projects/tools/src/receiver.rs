use crate::command::json_output;
use crate::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn sources() -> HashSet<String> {
    let dump = json_output("pw-dump", std::iter::empty::<&str>(), json!([]));
    dump.as_array()
        .into_iter()
        .flatten()
        .filter_map(|object| {
            if object.get("type").and_then(Value::as_str) != Some("PipeWire:Interface:Node") {
                return None;
            }
            let props = object.pointer("/info/props")?;
            if props.get("media.class").and_then(Value::as_str) != Some("Audio/Source") {
                return None;
            }
            let api = props
                .get("device.api")
                .and_then(Value::as_str)
                .unwrap_or("");
            let name = props.get("node.name").and_then(Value::as_str).unwrap_or("");
            (api == "bluez5" || name.starts_with("bluez_")).then(|| name.to_owned())
        })
        .collect()
}

fn bridge(node: &str) -> std::io::Result<Child> {
    Command::new("pw-loopback")
        .args([
            "--capture", node,
            &format!("--capture-props=node.name=seele-bluetooth-receiver.{node} seele.role=bluetooth-receiver"),
            &format!("--playback-props=node.name=seele-bluetooth-receiver-out.{node} node.description=Bluetooth Receiver seele.role=bluetooth-receiver"),
        ])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
}

pub fn run() -> Result {
    let interval = env::var("SEELE_BLUETOOTH_RECEIVER_INTERVAL")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(2.0);
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::SeqCst))?;
    let mut bridges: HashMap<String, Child> = HashMap::new();
    while running.load(Ordering::SeqCst) {
        let current = sources();
        bridges.retain(|node, child| {
            let alive = child.try_wait().ok().flatten().is_none();
            if alive && current.contains(node) {
                true
            } else {
                let _ = child.kill();
                false
            }
        });
        for node in current {
            if let std::collections::hash_map::Entry::Vacant(entry) = bridges.entry(node.clone()) {
                if let Ok(child) = bridge(&node) {
                    entry.insert(child);
                }
            }
        }
        thread::sleep(Duration::from_secs_f64(interval.max(0.01)));
    }
    for (_, mut child) in bridges {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}
