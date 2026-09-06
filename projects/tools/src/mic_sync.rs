use crate::command::{detached, output};
use crate::Result;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

fn read(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn walk(directory: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth - 1, found);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("number")
            && path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("sound"))
                .unwrap_or(false)
        {
            found.push(path);
        }
    }
}

fn card_index(root: &Path, vendor: &str, product: &str) -> Option<u32> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if read(path.join("idVendor")).as_deref() != Some(vendor)
            || read(path.join("idProduct")).as_deref() != Some(product)
        {
            continue;
        }
        let mut numbers = Vec::new();
        walk(&path, 5, &mut numbers);
        numbers.sort();
        if let Some(number) = numbers
            .into_iter()
            .find_map(|path| read(path)?.parse().ok())
        {
            return Some(number);
        }
    }
    None
}

fn switch_numid(card: u32) -> Option<u32> {
    let listing = output("amixer", ["-c", &card.to_string(), "controls"])?;
    listing.lines().find_map(|line| {
        if !line.contains("Capture Switch'") {
            return None;
        }
        line.strip_prefix("numid=")?.split(',').next()?.parse().ok()
    })
}

fn switch_muted(card: u32, numid: u32) -> Option<bool> {
    let listing = output(
        "amixer",
        ["-c", &card.to_string(), "cget", &format!("numid={numid}")],
    )?;
    listing.lines().find_map(|line| {
        line.trim()
            .strip_prefix(": values=")
            .map(|value| value == "off")
    })
}

fn set_switch(card: u32, numid: u32, muted: bool) {
    let _ = output(
        "amixer",
        [
            "-q",
            "-c",
            &card.to_string(),
            "cset",
            &format!("numid={numid}"),
            if muted { "off" } else { "on" },
        ],
    );
}

fn node_muted(node: u64) -> Option<bool> {
    output("wpctl", ["get-volume", &node.to_string()]).map(|value| value.contains("MUTED"))
}

fn set_node(node: u64, muted: bool) {
    let _ = output(
        "wpctl",
        ["set-mute", &node.to_string(), if muted { "1" } else { "0" }],
    );
}

struct Session {
    card: u32,
    numid: u32,
    node: Option<u64>,
    applied: Option<bool>,
    node_dirty: bool,
}
impl Session {
    fn graph(&mut self, object: &Value) {
        if self.node == object.get("id").and_then(Value::as_u64) && object.get("info").is_none() {
            self.node = None;
            self.applied = None;
            return;
        }
        let props = object.pointer("/info/props");
        let card = props
            .and_then(|value| value.get("alsa.card"))
            .and_then(Value::as_u64);
        let known_node =
            self.node.is_some() && self.node == object.get("id").and_then(Value::as_u64);
        if !known_node
            && (props
                .and_then(|value| value.get("media.class"))
                .and_then(Value::as_str)
                != Some("Audio/Source")
                || card != Some(self.card as u64))
        {
            return;
        }
        if !known_node {
            self.applied = None;
            self.node = object.get("id").and_then(Value::as_u64);
        }
        let carries_mute = object
            .pointer("/info/params/Props")
            .and_then(Value::as_array)
            .map(|items| items.iter().any(|item| item.get("mute").is_some()))
            .unwrap_or(false);
        if carries_mute {
            self.node_dirty = true;
        }
    }
    fn node_change(&mut self) {
        let Some(node) = self.node else { return };
        let Some(muted) = node_muted(node) else {
            return;
        };
        if self.applied.is_none() {
            let Some(device) = switch_muted(self.card, self.numid) else {
                return;
            };
            self.applied = Some(device);
            if muted != device {
                println!("adopting device mute={device} on node {node}");
                set_node(node, device);
            }
        } else if self.applied != Some(muted) {
            self.applied = Some(muted);
            println!("desktop mute={muted}, following on card {}", self.card);
            set_switch(self.card, self.numid, muted);
        }
    }
    fn switch_event(&mut self) {
        let (Some(previous), Some(node)) = (self.applied, self.node) else {
            return;
        };
        let Some(muted) = switch_muted(self.card, self.numid) else {
            return;
        };
        if muted == previous {
            return;
        }
        self.applied = Some(muted);
        println!("device mute={muted}, following on node {node}");
        set_node(node, muted);
        let _ = detached(
            "seele-shellctl",
            &[
                "-q".into(),
                "microphone-state".into(),
                if muted { "muted".into() } else { "live".into() },
            ],
        );
    }
}

enum Event {
    Alsa,
    Graph(String),
    Closed,
}

fn stream(mut child: Child, graph: bool, sender: mpsc::Sender<Event>) -> Child {
    let mut stdout = child.stdout.take().unwrap();
    thread::spawn(move || {
        let mut bytes = [0_u8; 65536];
        loop {
            match stdout.read(&mut bytes) {
                Ok(0) | Err(_) => {
                    let _ = sender.send(Event::Closed);
                    break;
                }
                Ok(count) if graph => {
                    let _ = sender.send(Event::Graph(
                        String::from_utf8_lossy(&bytes[..count]).into_owned(),
                    ));
                }
                Ok(_) => {
                    let _ = sender.send(Event::Alsa);
                }
            }
        }
    });
    child
}

fn parse_pending(pending: &mut String, session: &mut Session) {
    parse_values(pending, |value| {
        if let Value::Array(objects) = value {
            for object in objects {
                session.graph(&object);
            }
        }
    });
    // pw-dump can batch many snapshots into one read. Query the current
    // value once, rather than running wpctl for every obsolete snapshot.
    if std::mem::take(&mut session.node_dirty) {
        session.node_change();
    }
}

fn parse_values(pending: &mut String, mut consume: impl FnMut(Value)) {
    let mut consumed = 0;
    loop {
        let remaining = &pending[consumed..];
        let trimmed = remaining.trim_start();
        consumed += remaining.len() - trimmed.len();
        if trimmed.is_empty() {
            break;
        }
        let mut stream = serde_json::Deserializer::from_str(trimmed).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) => {
                consumed += stream.byte_offset();
                consume(value);
            }
            Some(Err(error)) if error.is_eof() => break,
            Some(Err(_)) => {
                consumed += trimmed.chars().next().unwrap().len_utf8();
            }
            None => {
                consumed = pending.len();
                break;
            }
        }
    }
    pending.drain(..consumed);
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[test]
    fn partial_updates_keep_identity_and_replacement_resets_agreement() {
        let mut session = Session {
            card: 9,
            numid: 3,
            node: Some(63),
            applied: Some(true),
            node_dirty: false,
        };
        session.graph(&serde_json::json!({"id":63,"info":{"params":{"Props":[{"mute":false}]}}}));
        assert!(session.node_dirty);
        assert_eq!(session.node, Some(63));
        assert_eq!(session.applied, Some(true));
        session.node_dirty = false;
        session.graph(&serde_json::json!({"id":64,"info":{"params":{"Props":[{"mute":false}]}}}));
        assert!(!session.node_dirty);
        session.graph(&serde_json::json!({"id":64,"info":{"props":{"media.class":"Audio/Source","alsa.card":9},"params":{"Props":[{"mute":false}]}}}));
        assert_eq!(session.node, Some(64));
        assert_eq!(session.applied, None);
        assert!(session.node_dirty);
        session.graph(&serde_json::json!({"id":64}));
        assert_eq!(session.node, None);
    }

    #[test]
    fn split_values_and_invalid_utf8_characters_preserve_stream_order() {
        let mut pending = String::new();
        let mut values = Vec::new();
        for chunk in [" \n[", "1] 🦀 [2,", "3] tr", "ue {\"x\":", "4}"] {
            pending.push_str(chunk);
            parse_values(&mut pending, |value| values.push(value));
        }
        assert_eq!(values, [serde_json::json!([1]), serde_json::json!([2, 3]), Value::Bool(true), serde_json::json!({"x":4})]);
        assert!(pending.is_empty());
        pending.push_str("[1][2][3");
        values.clear();
        parse_values(&mut pending, |value| values.push(value));
        assert_eq!(values, [serde_json::json!([1]), serde_json::json!([2])]);
        assert_eq!(pending, "[3");
    }
}

fn watch(card: u32, numid: u32, running: &AtomicBool) -> Result {
    let (sender, receiver) = mpsc::channel();
    let mut alsa = Command::new("alsactl")
        .args(["monitor", &format!("hw:{card}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let graph = match Command::new("pw-dump")
        .arg("-m")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(graph) => graph,
        Err(error) => {
            let _ = alsa.kill();
            let _ = alsa.wait();
            return Err(error.into());
        }
    };
    let mut alsa = stream(alsa, false, sender.clone());
    let mut graph = stream(graph, true, sender);
    let mut session = Session {
        card,
        numid,
        node: None,
        applied: None,
        node_dirty: false,
    };
    let mut pending = String::new();
    while running.load(Ordering::SeqCst) {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(Event::Alsa) => session.switch_event(),
            Ok(Event::Graph(chunk)) => {
                pending.push_str(&chunk);
                parse_pending(&mut pending, &mut session);
            }
            Ok(Event::Closed) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if alsa.try_wait().ok().flatten().is_some()
                    || graph.try_wait().ok().flatten().is_some()
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = alsa.kill();
    let _ = graph.kill();
    let _ = alsa.wait();
    let _ = graph.wait();
    Ok(())
}

pub fn run(arguments: &[String]) -> Result {
    let device = arguments
        .first()
        .ok_or("device must be vendor:product, such as 14ed:1019")?;
    let (vendor, product) = device
        .split_once(':')
        .ok_or("device must be vendor:product, such as 14ed:1019")?;
    let vendor = vendor.to_ascii_lowercase();
    let product = product.to_ascii_lowercase();
    let root = env::var_os("SEELE_MIC_SYNC_SYSFS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sys/bus/usb/devices"));
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::SeqCst))?;
    while running.load(Ordering::SeqCst) {
        if let Some(card) = card_index(&root, &vendor, &product) {
            if let Some(numid) = switch_numid(card) {
                println!("watching card {card} control {numid} for {vendor}:{product}");
                watch(card, numid, &running)?;
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}
