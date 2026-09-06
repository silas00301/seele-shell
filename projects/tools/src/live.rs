use crate::{control, pipewire::Graph, Result};
use dbus::{blocking::Connection, message::MatchRule, MessageType};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const NETWORK: usize = 0;
const BLUETOOTH: usize = 1;
const NOTIFICATIONS: usize = 2;
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
type Dirty = Arc<[GroupFlag; 3]>;
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
        "notifications" => NOTIFICATIONS,
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
        NOTIFICATIONS => control::notification_status(),
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
    loop {
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
            let mut notifications: Option<crate::notifications::Snapshot> = None;
            loop {
                // History ages out after 24h even if mako emits no signal.
                // Refilter the cached lists, without launching another probe.
                if group == NOTIFICATIONS
                    && last.elapsed() >= Duration::from_secs(5)
                    && !dirty[group].dirty.load(Ordering::Acquire)
                {
                    if let Some(snapshot) = notifications.as_mut() {
                        if sender.send(Event::Patch(snapshot.patch())).is_err() {
                            return Ok(());
                        }
                        last = Instant::now();
                    }
                }
                if last.elapsed() >= Duration::from_millis(50) {
                    if let Some(force) = dirty[group].take() {
                        if group == NOTIFICATIONS {
                            let mut snapshot = crate::notifications::Snapshot::read();
                            let patch = snapshot.patch();
                            notifications = Some(snapshot);
                            let event = if force {
                                Event::Refresh(patch)
                            } else {
                                Event::Patch(patch)
                            };
                            if sender.send(event).is_err() {
                                return Ok(());
                            }
                        } else if !send_snapshot(group, force, &bluetooth, &sender) {
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
    let mut force = false;
    loop {
        let auxiliary = control::auxiliary_status();
        {
            let cached = bluetooth.lock().unwrap();
            let patch = if let Some(bluetooth) = cached.as_ref() {
                control::merge_status([auxiliary, control::bluetooth_status(bluetooth)])
            } else {
                auxiliary
            };
            let event = if force {
                Event::Refresh(patch)
            } else {
                Event::Patch(patch)
            };
            if sender.send(event).is_err() {
                return;
            }
        }
        force = match wake.recv_timeout(Duration::from_secs(5)) {
            Ok(()) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
    }
}

fn watch_pipewire(sender: SyncSender<Event>, audio: Arc<Mutex<()>>) {
    loop {
        let mut command = Command::new("pw-dump");
        command
            .args(["-m", "-N", "-i", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // This reader can block while idle. Ensure the child dies with this
        // controller, including when Quickshell closes its stdin or is killed.
        let parent = std::process::id();
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() as u32 != parent {
                    return Err(io::Error::other("status controller exited"));
                }
                Ok(())
            });
        }
        if let Ok(mut child) = command.spawn() {
            let mut graph = Graph::default();
            let mut volume_key = Value::Null;
            let stream =
                serde_json::Deserializer::from_reader(BufReader::new(child.stdout.take().unwrap()))
                    .into_iter::<Value>();
            let mut closed = false;
            for update in stream {
                let Ok(update) = update else { break };
                if !update.is_array() {
                    break;
                }
                graph.update(update);
                let key = graph.volume_key();
                let patch = control::graph_status(graph.snapshot());
                let guard = audio.lock().unwrap();
                let patch = if key != volume_key {
                    volume_key = key;
                    control::merge_status([patch, control::volumes()])
                } else {
                    patch
                };
                if sender.send(Event::Patch(patch)).is_err() {
                    closed = true;
                    break;
                }
                drop(guard);
            }
            let _ = child.kill();
            let _ = child.wait();
            if closed {
                return;
            }
        }
        // A restarted PipeWire instance has a new registry and may reuse IDs.
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
    let dirty: Dirty = Arc::new(std::array::from_fn(|_| GroupFlag::default()));
    let bluetooth: Bluetooth = Arc::new(Mutex::new(None));
    // Bounded backpressure: status floods cannot grow an unbounded JSON queue.
    let (sender, receiver) = mpsc::sync_channel(16);
    for (service, session, group) in [
        ("org.freedesktop.NetworkManager", false, NETWORK),
        ("org.bluez", false, BLUETOOTH),
        ("org.freedesktop.Notifications", true, NOTIFICATIONS),
    ] {
        let (dirty, bluetooth, sender) = (dirty.clone(), bluetooth.clone(), sender.clone());
        thread::spawn(move || watch_bus(service, session, group, dirty, bluetooth, sender));
    }
    let (auxiliary, wake) = mpsc::sync_channel(1);
    let (cached, updates) = (bluetooth, sender.clone());
    thread::spawn(move || watch_auxiliary(wake, cached, updates));
    let audio = Arc::new(Mutex::new(()));
    let (updates, guard) = (sender.clone(), audio.clone());
    thread::spawn(move || watch_pipewire(updates, guard));
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let command = line.trim();
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
    let mut previous = json!({});
    let mut stdout = io::stdout().lock();
    while let Ok(event) = receiver.recv() {
        let (patch, force) = match event {
            Event::Patch(patch) => (patch, false),
            Event::Refresh(patch) => (patch, true),
            Event::Closed => break,
        };
        let delta = changed(&mut previous, patch.clone());
        if let Some(delta) = if force { Some(patch) } else { delta } {
            writeln!(stdout, "{delta}")?;
            stdout.flush()?;
        }
    }
    Ok(())
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
        for index in 0..3 {
            assert_eq!(
                dirty[index].dirty.load(Ordering::Relaxed),
                index == NOTIFICATIONS
            );
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
