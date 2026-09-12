use crate::{control, pipewire::Graph, Result};
use dbus::{blocking::Connection, message::MatchRule, MessageType};
use serde_json::{json, Value};
use std::io;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const NETWORK: usize = 0;
const BLUETOOTH: usize = 1;
#[derive(Default)]
struct GroupFlag {
    dirty: AtomicBool,
    force: AtomicBool,
}
impl GroupFlag {
    fn mark(&self, force: bool) {
        if force {
            self.force.store(true, Ordering::Relaxed);
        }
        self.dirty.store(true, Ordering::Release);
    }
    fn take(&self) -> Option<bool> {
        self.dirty
            .swap(false, Ordering::AcqRel)
            .then(|| self.force.swap(false, Ordering::Relaxed))
    }
}
type Dirty = Arc<[GroupFlag; 2]>;
type Bluetooth = Arc<Mutex<Option<Value>>>;

enum Event {
    Patch(Value),
    Refresh(Value),
    Closed,
}

fn refresh(dirty: &Dirty, command: &str) -> bool {
    let group = match command {
        "network" => NETWORK,
        "bluetooth" => BLUETOOTH,
        "aux" => return true,
        "all" => {
            for flag in dirty.iter() {
                flag.mark(true);
            }
            return true;
        }
        _ => return false,
    };
    dirty[group].mark(true);
    false
}

fn send_snapshot(
    group: usize,
    force: bool,
    bluetooth: &Bluetooth,
    sender: &SyncSender<Event>,
) -> bool {
    let event = |patch| {
        if force {
            Event::Refresh(patch)
        } else {
            Event::Patch(patch)
        }
    };
    let patch = match group {
        NETWORK => control::network_status(),
        BLUETOOTH => {
            let snapshot = control::bluetooth_state();
            let mut cached = bluetooth.lock().unwrap();
            let patch = control::bluetooth_status(&snapshot);
            *cached = Some(snapshot);
            // Keep related auxiliary updates ordered with this snapshot.
            return sender.send(event(patch)).is_ok();
        }
        _ => unreachable!(),
    };
    sender.send(event(patch)).is_ok()
}

fn watch_bus(
    service: &'static str,
    session: bool,
    group: usize,
    dirty: Dirty,
    bluetooth: Bluetooth,
    sender: SyncSender<Event>,
) {
    let stop = crate::command::shutdown_signal();
    while stop.load(Ordering::Relaxed) == 0 {
        let connect = if session {
            Connection::new_session
        } else {
            Connection::new_system
        };
        let connected = (|| -> Result {
            let connection = connect()?;
            let mut changes = MatchRule::new();
            changes.msg_type = Some(MessageType::Signal);
            changes.sender = Some(service.into());
            let flag = dirty.clone();
            connection.add_match(changes, move |_: (), _, _| {
                flag[group].mark(false);
                true
            })?;
            let flag = dirty.clone();
            connection.add_match(
                MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
                    .with_sender("org.freedesktop.DBus"),
                move |(name, _, _): (String, String, String), _, _| {
                    if name == service {
                        flag[group].mark(false);
                    }
                    true
                },
            )?;
            // Subscribe before querying: a change during the initial snapshot
            // stays queued. Owner changes also bootstrap a restarted daemon.
            dirty[group].mark(false);
            let mut last = Instant::now() - Duration::from_secs(1);
            loop {
                if stop.load(Ordering::Relaxed) != 0 {
                    return Ok(());
                }
                if last.elapsed() >= Duration::from_millis(50) {
                    if let Some(force) = dirty[group].take() {
                        if !send_snapshot(group, force, &bluetooth, &sender) {
                            return Ok(());
                        }
                        last = Instant::now();
                    }
                }
                connection.process(Duration::from_millis(250))?;
            }
        })();
        if connected.is_ok() {
            return;
        }
        // No bus: publish the existing probes' unavailable values rather than
        // leaving stale devices on screen, then reconnect without a busy loop.
        if !send_snapshot(group, true, &bluetooth, &sender) {
            return;
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn watch_auxiliary(wake: mpsc::Receiver<()>, bluetooth: Bluetooth, sender: SyncSender<Event>) {
    let stop = crate::command::shutdown_signal();
    let mut force = false;
    while stop.load(Ordering::Relaxed) == 0 {
        let auxiliary = control::auxiliary_status();
        {
            let cached = bluetooth.lock().unwrap();
            let mut patch = if let Some(bluetooth) = cached.as_ref() {
                control::merge_status([auxiliary, control::bluetooth_status(bluetooth)])
            } else {
                auxiliary
            };
            // A completed probe heartbeats even when its semantic state is unchanged.
            // The common delta filter must not make a quiet healthy provider stale.
            patch["healthHeartbeat"] = json!({"tailscale": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64});
            let event = if force {
                Event::Refresh(patch)
            } else {
                Event::Patch(patch)
            };
            if sender.send(event).is_err() {
                return;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        force = loop {
            if stop.load(Ordering::Relaxed) != 0 {
                return;
            }
            match wake.recv_timeout(Duration::from_millis(100)) {
                Ok(()) => break true,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => break false,
                Err(mpsc::RecvTimeoutError::Timeout) => (),
            }
        };
    }
}

fn watch_pipewire(sender: SyncSender<Event>, audio: Arc<Mutex<()>>) {
    let stop = crate::command::shutdown_signal();
    while stop.load(Ordering::Relaxed) == 0 {
        let mut command = Command::new("pw-dump");
        command.args(["-m", "-N", "-i", "0"]);
        let parent = std::process::id();
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0
                    || libc::getppid() as u32 != parent
                {
                    return Err(io::Error::other("status controller exited"));
                }
                Ok(())
            });
        }
        let mut graph = Graph::default();
        let mut volume_key = Value::Null;
        let mut pending = Vec::new();
        let mut closed = false;
        let _ = seele_runtime::process::stream_stdout(
            &mut command,
            b"",
            seele_runtime::process::Limits {
                timeout: Duration::from_secs(365 * 86400),
                output: 65536,
            },
            &stop,
            |chunk| {
                if chunk.len() > 16 * 1024 * 1024 - pending.len() {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                pending.extend_from_slice(chunk);
                let mut invalid = false;
                crate::pipewire::parse_values(&mut pending, |update| {
                    if invalid || closed {
                        return;
                    }
                    if !graph.update(update) {
                        invalid = true;
                        return;
                    }
                    let key = graph.volume_key();
                    let patch = control::graph_status(graph.snapshot());
                    let _guard = audio.lock().unwrap();
                    let patch = if key != volume_key {
                        volume_key = key;
                        control::merge_status([patch, control::volumes()])
                    } else {
                        patch
                    };
                    closed = sender.send(Event::Patch(patch)).is_err();
                });
                if invalid || closed {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Ok(())
            },
        );
        if closed || stop.load(Ordering::Relaxed) != 0 {
            return;
        }
        {
            let _guard = audio.lock().unwrap();
            let reset = control::merge_status([
                control::graph_status(&json!([])),
                json!({"volume":0,"muted":false,"microphoneVolume":0,"microphoneMuted":false}),
            ]);
            if sender.send(Event::Patch(reset)).is_err() {
                return;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn changed(previous: &mut Value, patch: Value) -> Option<Value> {
    let mut delta = serde_json::Map::new();
    if let Value::Object(fields) = patch {
        for (key, value) in fields {
            if previous.get(&key) != Some(&value) {
                previous[&key] = value.clone();
                delta.insert(key, value);
            }
        }
    }
    (!delta.is_empty()).then_some(Value::Object(delta))
}

pub(crate) fn run() -> Result {
    let stop = crate::command::shutdown_signal();
    let mut workers = Vec::new();
    let dirty: Dirty = Arc::new(std::array::from_fn(|_| GroupFlag::default()));
    let bluetooth: Bluetooth = Arc::new(Mutex::new(None));
    // Bounded backpressure: status floods cannot grow an unbounded JSON queue.
    let (sender, receiver) = mpsc::sync_channel(16);
    for (service, session, group) in [
        ("org.freedesktop.NetworkManager", false, NETWORK),
        ("org.bluez", false, BLUETOOTH),
    ] {
        let (dirty, bluetooth, sender) = (dirty.clone(), bluetooth.clone(), sender.clone());
        workers.push(thread::spawn(move || {
            watch_bus(service, session, group, dirty, bluetooth, sender)
        }));
    }
    let (auxiliary, wake) = mpsc::sync_channel(1);
    let (cached, updates) = (bluetooth, sender.clone());
    workers.push(thread::spawn(move || {
        watch_auxiliary(wake, cached, updates)
    }));
    let audio = Arc::new(Mutex::new(()));
    let (updates, guard) = (sender.clone(), audio.clone());
    workers.push(thread::spawn(move || watch_pipewire(updates, guard)));
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        let mut frame = Vec::new();
        while let Ok(true) = seele_runtime::wire::read_frame(&mut input, &mut frame, 4096) {
            let Ok(command) = std::str::from_utf8(&frame) else {
                continue;
            };
            let command = command.trim();
            if matches!(command, "audio" | "all") {
                let _guard = audio.lock().unwrap();
                if sender.send(Event::Refresh(control::volumes())).is_err() {
                    return;
                }
            }
            if refresh(&dirty, command) {
                let _ = auxiliary.try_send(());
            }
        }
        let _ = sender.send(Event::Closed);
    });
    let result = (|| -> Result {
        let mut previous = json!({});
        let mut stdout = seele_runtime::wire::nonblocking_stdout()?;
        while stop.load(Ordering::Relaxed) == 0 {
            let event = match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let (patch, force) = match event {
                Event::Patch(patch) => (patch, false),
                Event::Refresh(patch) => (patch, true),
                Event::Closed => break,
            };
            let delta = changed(&mut previous, patch.clone());
            if let Some(delta) = if force { Some(patch) } else { delta } {
                let bytes = seele_runtime::wire::json_frame(&delta, 4 * 1024 * 1024)?;
                seele_runtime::wire::write_bytes(
                    &mut stdout,
                    &bytes,
                    Duration::from_secs(5),
                    &stop,
                )?;
            }
        }
        Ok(())
    })();
    stop.store(1, Ordering::Relaxed);
    drop(receiver);
    for worker in workers {
        let _ = worker.join();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_refreshes_cannot_request_device_probes() {
        let dirty = Arc::new(std::array::from_fn(|_| GroupFlag::default()));
        for _ in 0..100 {
            assert!(!refresh(&dirty, "notifications"));
        }
        for flag in dirty.iter() {
            assert!(!flag.dirty.load(Ordering::Relaxed));
        }
        assert!(refresh(&dirty, "all"));
        assert!(dirty
            .iter()
            .all(|flag| flag.dirty.load(Ordering::Relaxed) && flag.force.load(Ordering::Relaxed)));
    }

    #[test]
    fn output_is_a_field_delta_and_reconnections_can_clear_previous_values() {
        let mut state = json!({});
        assert!(changed(
            &mut state,
            json!({"volume":50,"notifications":{"items":[]}})
        )
        .is_some());
        assert!(changed(&mut state, json!({"volume":50})).is_none());
        assert_eq!(
            changed(&mut state, json!({"volume":0})),
            Some(json!({"volume":0}))
        );
        assert_eq!(state["notifications"], json!({"items":[]}));
    }
}
