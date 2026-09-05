use crate::agents;
use crate::command::{
    atomic_write, config_home, detached, json_output, output, process_alive, runtime_home, status, require_status,
};
use crate::nothing;
use crate::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime};

fn runtime_file(name: &str) -> PathBuf {
    runtime_home().join("seele-shell").join(name)
}
fn config_file(name: &str) -> PathBuf {
    config_home().join("seele-shell").join(name)
}
fn no_status() -> bool {
    env::var("SEELE_CONTROL_NO_STATUS").as_deref() == Ok("1")
}
fn data(value: Option<&Value>) -> Option<&Value> {
    value?.get("data")
}
fn bool_data(value: Option<&Value>) -> bool {
    data(value).and_then(Value::as_bool).unwrap_or(false)
}
fn string_data(value: Option<&Value>) -> String {
    data(value).and_then(Value::as_str).unwrap_or("").to_owned()
}

fn pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn cmdline_contains(pid: u32, name: &str) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|bytes| String::from_utf8_lossy(&bytes).contains(name))
        .unwrap_or(false)
}
fn kill_group(pid: u32) {
    unsafe {
        if libc::kill(-(pid as i32), libc::SIGTERM) != 0 {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
}

fn window_address(value: &str) -> Result<String> {
    let address = value.strip_prefix("0x").unwrap_or(value);
    if address.is_empty() || !address.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("valid window address required".into());
    }
    Ok(address.to_ascii_lowercase())
}

fn application_pid(address: &str) -> Option<u32> {
    let clients = json_output("hyprctl", ["clients", "-j"], json!([]));
    clients.as_array()?.iter().find_map(|client| {
        let candidate = client["address"].as_str()?.trim_start_matches("0x");
        if candidate.eq_ignore_ascii_case(address) {
            client["pid"].as_u64()?.try_into().ok()
        } else {
            None
        }
    })
}

fn force_quit_application(address: &str) -> Result {
    let pid = application_pid(address)
        .filter(|pid| *pid > 1)
        .ok_or("application not found")?;
    if unsafe { libc::kill(pid as i32, libc::SIGKILL) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn daemon_active(path: &Path, name: &str) -> bool {
    pid(path)
        .map(|pid| process_alive(pid) && cmdline_contains(pid, name))
        .unwrap_or(false)
}
fn stop_daemon(path: &Path, name: &str) {
    if let Some(pid) = pid(path).filter(|pid| cmdline_contains(*pid, name)) {
        kill_group(pid);
    }
    let _ = fs::remove_file(path);
}
fn start_daemon(
    path: &Path,
    program: &str,
    arguments: &[&str],
    environment: &[(&str, String)],
    log: Option<&Path>,
) -> Result {
    stop_daemon(path, program.trim_start_matches("seele-"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut command = Command::new(program);
    command
        .args(arguments)
        .envs(environment.iter().map(|(key, value)| (*key, value)))
        .stdin(Stdio::null());
    if let Some(log) = log {
        let file = File::create(log)?;
        command.stdout(file.try_clone()?).stderr(file);
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = command.spawn()?;
    atomic_write(path, child.id().to_string().as_bytes())?;
    for _ in 0..20 {
        if process_alive(child.id()) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn nothing_headphones_stop() {
    stop_daemon(
        &runtime_file("nothing-headphones.pid"),
        "nothing-headphones",
    );
    let _ = fs::remove_file(nothing::state_path());
    let _ = fs::remove_file(nothing::command_path());
}

fn nothing_headphones_active(address: &str) -> bool {
    pid(&runtime_file("nothing-headphones.pid"))
        .map(|pid| {
            process_alive(pid)
                && cmdline_contains(pid, "nothing-headphones")
                && cmdline_contains(pid, address)
        })
        .unwrap_or(false)
}

fn nothing_headphones_start(address: &str) -> Result {
    if nothing_headphones_active(address) {
        return Ok(());
    }
    nothing_headphones_stop();
    start_daemon(
        &runtime_file("nothing-headphones.pid"),
        "seele-nothing-headphones",
        &[address],
        &[],
        Some(&runtime_file("nothing-headphones.log")),
    )
}

fn bluetooth_scan_active() -> bool {
    let path = runtime_file("bluetooth-scan.pid");
    pid(&path)
        .map(|pid| {
            process_alive(pid)
                && (cmdline_contains(pid, "bluetoothctl")
                    || fs::read_to_string(format!("/proc/{pid}/comm"))
                        .unwrap_or_default()
                        .contains("bluetoothctl"))
        })
        .unwrap_or(false)
}
fn bluetooth_receiver_active() -> bool {
    daemon_active(&runtime_file("bluetooth-receiver.pid"), "bt-receiver")
}
fn bluetooth_scan_stop() {
    let path = runtime_file("bluetooth-scan.pid");
    if let Some(pid) = pid(&path) {
        kill_group(pid);
    }
    let _ = fs::remove_file(path);
    status("bluetoothctl", ["scan", "off"]);
}
fn bluetooth_scan_start() -> Result {
    bluetooth_scan_stop();
    status("bluetoothctl", ["power", "on"]);
    start_daemon(
        &runtime_file("bluetooth-scan.pid"),
        "bluetoothctl",
        &["--timeout", "120", "scan", "on"],
        &[],
        None,
    )
}
fn bluetooth_receiver_stop() {
    stop_daemon(&runtime_file("bluetooth-receiver.pid"), "bt-receiver");
}
fn bluetooth_receiver_start() -> Result {
    bluetooth_receiver_stop();
    status("bluetoothctl", ["power", "on"]);
    start_daemon(
        &runtime_file("bluetooth-receiver.pid"),
        "seele-bt-receiver",
        &[],
        &[],
        None,
    )
}
fn bluetooth_agent_stop() {
    stop_daemon(&runtime_file("bluetooth-agent.pid"), "bt-agent");
    for name in ["bluetooth-pairing.json", "bluetooth-pairing.answer"] {
        let _ = fs::remove_file(runtime_file(name));
    }
    status("seele-shellctl", ["-q", "bluetooth-pairing-dismiss"]);
}
fn bluetooth_agent_start() -> Result {
    bluetooth_agent_stop();
    start_daemon(
        &runtime_file("bluetooth-agent.pid"),
        "seele-bt-agent",
        &[],
        &[
            ("SEELE_BLUETOOTH_PAIRING_WINDOW", "120".into()),
            ("SEELE_BLUETOOTH_DISCOVERABLE_TIMEOUT", "180".into()),
        ],
        Some(&runtime_file("bluetooth-agent.log")),
    )
}
fn pairing_close() {
    bluetooth_agent_stop();
    status("bluetoothctl", ["discoverable", "off"]);
    status("bluetoothctl", ["pairable", "off"]);
    status("bluetoothctl", ["discoverable-timeout", "180"]);
}
fn pairing_open() -> Result {
    status("bluetoothctl", ["power", "on"]);
    bluetooth_agent_start()?;
    status("bluetoothctl", ["discoverable-timeout", "120"]);
    status("bluetoothctl", ["pairable", "on"]);
    status("bluetoothctl", ["discoverable", "on"]);
    Ok(())
}

fn bluetooth_state() -> Value {
    let raw = output(
        "busctl",
        [
            "--json=short",
            "call",
            "org.bluez",
            "/",
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
        ],
    )
    .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let Some(objects) = raw
        .as_ref()
        .and_then(|value| value.pointer("/data/0"))
        .and_then(Value::as_object)
    else {
        return json!({"available":false,"powered":false,"scanning":false,"receiver":false,"discoverable":false,"connected":0,"devices":[]});
    };
    let mut available = false;
    let mut powered = false;
    let mut discoverable = false;
    let mut transports = HashSet::new();
    for interfaces in objects.values() {
        if let Some(adapter) = interfaces.get("org.bluez.Adapter1") {
            available = true;
            powered |= bool_data(adapter.get("Powered"));
            discoverable |= bool_data(adapter.get("Discoverable"));
        }
        if let Some(transport) = interfaces.get("org.bluez.MediaTransport1") {
            if string_data(transport.get("State")) == "active" {
                transports.insert(string_data(transport.get("Device")));
            }
        }
    }
    let mut devices = Vec::new();
    for (path, interfaces) in objects {
        let Some(device) = interfaces.get("org.bluez.Device1") else {
            continue;
        };
        let address = string_data(device.get("Address"));
        let name = ["Alias", "Name", "Address"]
            .iter()
            .map(|key| string_data(device.get(*key)))
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        let paired = bool_data(device.get("Paired"));
        let connected = bool_data(device.get("Connected"));
        if address.is_empty()
            || name.is_empty()
            || (!paired && !connected && name.eq_ignore_ascii_case(&address.replace(':', "-")))
        {
            continue;
        }
        let source = data(device.get("UUIDs"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|uuid| uuid.to_ascii_lowercase().starts_with("0000110a"))
            })
            .unwrap_or(false);
        let battery = interfaces
            .get("org.bluez.Battery1")
            .and_then(|battery| data(battery.get("Percentage")))
            .cloned()
            .unwrap_or(Value::Null);
        devices.push(json!({"path":path,"address":address,"name":name,"icon":string_data(device.get("Icon")),"paired":paired,"trusted":bool_data(device.get("Trusted")),"connected":connected,"rssi":data(device.get("RSSI")).cloned().unwrap_or(Value::Null),"source":source,"streaming":transports.contains(path),"battery":battery}));
    }
    devices.sort_by(|left, right| {
        let left_key = (
            !left["paired"].as_bool().unwrap_or(false),
            left["name"].as_str().unwrap_or("").to_ascii_lowercase(),
            left["address"].as_str().unwrap_or("").to_owned(),
        );
        let right_key = (
            !right["paired"].as_bool().unwrap_or(false),
            right["name"].as_str().unwrap_or("").to_ascii_lowercase(),
            right["address"].as_str().unwrap_or("").to_owned(),
        );
        left_key.cmp(&right_key)
    });
    json!({"available":available,"powered":powered,"scanning":bluetooth_scan_active(),"receiver":bluetooth_receiver_active(),"discoverable":discoverable,"connected":devices.iter().filter(|device|device["connected"].as_bool()==Some(true)).count(),"devices":devices})
}

fn headphone_kind(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    if name.contains("airpods") || name.contains("beats") {
        Some("airpods")
    } else if name.ends_with("nothing headphone (1)") {
        Some("nothing")
    } else {
        None
    }
}

fn connected_headphone(bluetooth: &Value) -> Option<&Value> {
    bluetooth["devices"].as_array()?.iter().find(|device| {
        device["connected"].as_bool() == Some(true)
            && device["name"].as_str().and_then(headphone_kind).is_some()
    })
}

fn headphone_state(bluetooth: &Value) -> Value {
    let Some(device) = connected_headphone(bluetooth) else {
        if std::env::var("SEELE_NOTHING_HEADPHONES_DISABLE_DAEMON").as_deref() != Ok("1") {
            nothing_headphones_stop();
        }
        return json!({"connected":false,"name":"","kind":"","battery":Value::Null,"controls":false,"noiseMode":""});
    };
    let name = device["name"].as_str().unwrap_or("");
    let kind = headphone_kind(name).unwrap_or("");
    let details = if kind == "nothing" {
        let address = device["address"].as_str().unwrap_or("");
        if std::env::var("SEELE_NOTHING_HEADPHONES_DISABLE_DAEMON").as_deref() != Ok("1") {
            let _ = nothing_headphones_start(address);
        }
        nothing::state(address)
            .map(|state| {
                json!({
                    "battery":state.battery.map(Value::from).unwrap_or_else(||device["battery"].clone()),
                    "controls":state.controls,
                    "noiseMode":state.noise_mode
                })
            })
            .unwrap_or_else(|| json!({"battery":device["battery"],"controls":false,"noiseMode":""}))
    } else {
        if std::env::var("SEELE_NOTHING_HEADPHONES_DISABLE_DAEMON").as_deref() != Ok("1") {
            nothing_headphones_stop();
        }
        json!({"battery":device["battery"],"controls":true,"noiseMode":""})
    };
    json!({
        "connected": true,
        "name": name,
        "kind": kind,
        "battery": details["battery"],
        "controls": details["controls"],
        "noiseMode": details["noiseMode"]
    })
}

fn tailscale_state() -> Value {
    let Some(raw) = output("tailscale", ["status", "--json"])
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return json!({"available":false,"backend":"Unavailable","connected":false,"needsLogin":false,"name":"","ip":"","tailnet":"","peers":0,"onlinePeers":0});
    };
    let backend = raw["BackendState"].as_str().unwrap_or("Unavailable");
    let peers = raw["Peer"].as_object();
    json!({"available":true,"backend":backend,"connected":backend=="Running","needsLogin":backend=="NeedsLogin","name":raw.pointer("/Self/HostName").and_then(Value::as_str).unwrap_or(""),"ip":raw.pointer("/Self/TailscaleIPs/0").and_then(Value::as_str).unwrap_or(""),"tailnet":raw.pointer("/CurrentTailnet/Name").or_else(||raw.get("MagicDNSSuffix")).and_then(Value::as_str).unwrap_or(""),"peers":peers.map(|value|value.len()).unwrap_or(0),"onlinePeers":peers.map(|value|value.values().filter(|peer|peer["Online"].as_bool()==Some(true)).count()).unwrap_or(0)})
}
fn proton_state() -> Value {
    if !env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(path.join("protonvpn"))
                .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        }))
        .unwrap_or(false)
    {
        return json!({"available":false,"connected":false,"connection":""});
    }
    let line = output(
        "nmcli",
        ["-t", "-f", "TYPE,NAME", "connection", "show", "--active"],
    )
    .unwrap_or_default()
    .lines()
    .find(|line| {
        let lower = line.to_ascii_lowercase();
        let kind = line.split(':').next().unwrap_or("");
        matches!(kind, "vpn" | "wireguard" | "tun")
            && (lower.contains("protonvpn")
                || lower.contains("proton vpn")
                || lower.contains("pvpn"))
    })
    .unwrap_or("")
    .to_owned();
    json!({"available":true,"connected":!line.is_empty(),"connection":line.split_once(':').map(|(_,name)|name).unwrap_or("")})
}
fn ssh_state() -> Value {
    let tailscale = output("tailscale", ["debug", "prefs"])
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let tailscale_running = tailscale
        .as_ref()
        .and_then(|prefs| prefs["RunSSH"].as_bool())
        == Some(true);
    let ssh_available = output(
        "systemctl",
        ["show", "--property=LoadState", "--value", "sshd.service"],
    )
    .is_some_and(|state| state.trim() == "loaded");
    let ssh_running =
        ssh_available && status("systemctl", ["is-active", "--quiet", "sshd.service"]);
    let mode = match (tailscale_running, ssh_running) {
        (false, false) => "off",
        (true, false) => "tailscale",
        (false, true) => "ssh",
        (true, true) => "mixed",
    };
    json!({
        "available": tailscale.is_some() || ssh_available,
        "mode": mode,
        "tailscaleAvailable": tailscale.is_some(),
        "sshAvailable": ssh_available
    })
}
fn openlogi_batteries() -> Vec<Value> {
    let cache = runtime_file("openlogi-batteries.json");
    if let Ok(metadata) = fs::metadata(&cache) {
        if metadata
            .modified()
            .ok()
            .and_then(|time| SystemTime::now().duration_since(time).ok())
            .map(|age| age.as_secs() < 30)
            .unwrap_or(false)
        {
            if let Ok(value) = fs::read_to_string(&cache).and_then(|text| {
                serde_json::from_str::<Vec<Value>>(&text).map_err(std::io::Error::other)
            }) {
                return value;
            }
        }
    }
    let text = output("openlogi", ["list"]).unwrap_or_default();
    let mut values = Vec::new();
    for line in text.lines() {
        let Some(slot) = line.find("● ") else {
            continue;
        };
        let rest = &line[slot + 4..];
        let Some((name, details)) = rest.split_once(" (") else {
            continue;
        };
        let kind = details.split(',').next().unwrap_or("");
        let Some(start) = details.find("battery=") else {
            continue;
        };
        let tail = &details[start + 8..];
        let percent = tail
            .split('%')
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let state = tail.rsplit('(').next().unwrap_or("").trim_end_matches(')');
        let state = if state.starts_with("charging") {
            "Charging"
        } else if state == "discharging" {
            "Discharging"
        } else if state == "full" {
            "Full"
        } else {
            state
        };
        let icon = if kind == "mouse" {
            "input-mouse"
        } else if kind == "keyboard" {
            "input-keyboard"
        } else {
            "input-gaming"
        };
        values.push(
            json!({"kind":"logitech","name":name,"percent":percent,"status":state,"icon":icon}),
        );
    }
    let _ = atomic_write(
        &cache,
        serde_json::to_string(&values)
            .unwrap_or_default()
            .as_bytes(),
    );
    values
}
fn system_batteries() -> Vec<Value> {
    let mut values = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let path = entry.path();
            if fs::read_to_string(path.join("type"))
                .unwrap_or_default()
                .trim()
                != "Battery"
            {
                continue;
            }
            let Ok(percent) = fs::read_to_string(path.join("capacity"))
                .unwrap_or_default()
                .trim()
                .parse::<u64>()
            else {
                continue;
            };
            let name = fs::read_to_string(path.join("model_name"))
                .unwrap_or_default()
                .trim()
                .to_owned();
            let name = if name.is_empty() {
                entry.file_name().to_string_lossy().into_owned()
            } else {
                name
            };
            values.push(json!({"kind":"system","name":name,"percent":percent,"status":fs::read_to_string(path.join("status")).unwrap_or_else(|_|"Unknown".into()).trim(),"icon":""}));
        }
    }
    values
}
fn camera_devices() -> Vec<Value> {
    let text = output("v4l2-ctl", ["--list-devices"]).unwrap_or_default();
    let mut name = String::new();
    let mut values = Vec::new();
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            name = line
                .trim_end_matches(':')
                .split(" (usb-")
                .next()
                .unwrap_or("")
                .to_owned();
        } else if let Some(device) = line
            .split_whitespace()
            .find(|part| part.starts_with("/dev/video"))
        {
            values.push(json!({"name":name,"device":device}));
        }
    }
    values
}
fn percent(text: &str) -> u64 {
    text.split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| (value * 100.0) as u64)
        .unwrap_or(0)
}

fn tray_hidden() -> Value {
    fs::read_to_string(config_file("tray.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("hidden").cloned())
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]))
}
fn bar_modules() -> Value {
    fs::read_to_string(config_file("bar.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("modules").cloned())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn airpods_config() -> PathBuf {
    config_home().join("AirPodsTrayApp/AirPodsTrayApp.conf")
}

fn airpods_ear_detection() -> bool {
    let text = fs::read_to_string(airpods_config()).unwrap_or_default();
    let mut section = "";
    let mut setting = None;
    for line in text.lines() {
        if line.starts_with('[') {
            section = line;
        } else if section == "[earDetection]" {
            if let Some(value) = line.strip_prefix("setting=") {
                setting = value.trim().parse::<u8>().ok();
            }
        }
    }
    setting != Some(2)
}

fn set_airpods_ear_detection(action: &str) -> Result {
    let enabled = airpods_ear_detection();
    let value = match action {
        "on" => 0,
        "off" => 2,
        "toggle" if enabled => 2,
        "toggle" => 0,
        _ => return Err("invalid ear-detection action".into()),
    };
    status("systemctl", ["--user", "stop", "librepods.service"]);
    let path = airpods_config();
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut result = String::new();
    let mut skipping = false;
    for line in text.lines() {
        if line.starts_with('[') {
            skipping = line == "[earDetection]";
        }
        if !skipping {
            result.push_str(line);
            result.push('\n');
        }
    }
    result.push_str(&format!("[earDetection]\nsetting={value}\n"));
    atomic_write(&path, result.as_bytes())?;
    status("systemctl", ["--user", "start", "librepods.service"]);
    Ok(())
}

fn status_value() -> Value {
    let audio = output("wpctl", ["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .unwrap_or_else(|| "Volume: 0.00".into());
    let microphone = output("wpctl", ["get-volume", "@DEFAULT_AUDIO_SOURCE@"])
        .unwrap_or_else(|| "Volume: 0.00".into());
    let connections = output(
        "nmcli",
        ["-t", "-f", "TYPE,NAME", "connection", "show", "--active"],
    )
    .unwrap_or_default();
    let wired = |kind: &str| kind.contains("ethernet");
    let wireless = |kind: &str| kind.contains("wireless") || kind == "wifi";
    let (connection_type, connection_name) = connections
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(kind, _)| wired(kind) || wireless(kind))
        .min_by_key(|(kind, _)| u8::from(!wired(kind)))
        .unwrap_or(("", "Disconnected"));
    let wifi_available = output("nmcli", ["-t", "-f", "TYPE", "device"])
        .unwrap_or_default()
        .lines()
        .any(|line| line == "wifi");
    let wifi_enabled = output("nmcli", ["-t", "-f", "WIFI", "general"])
        .unwrap_or_default()
        .trim()
        == "enabled";
    let route = json_output("ip", ["-json", "route", "get", "1.1.1.1"], json!([]));
    let bluetooth = bluetooth_state();
    let headphones = headphone_state(&bluetooth);
    let mut batteries = system_batteries();
    batteries.extend(openlogi_batteries());
    for device in bluetooth["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|device| {
            device["connected"].as_bool() == Some(true) && !device["battery"].is_null()
        })
    {
        batteries.push(json!({"kind":"device","name":device["name"],"percent":device["battery"],"status":"","icon":device["icon"]}));
    }
    if headphones["connected"].as_bool() == Some(true) && !headphones["battery"].is_null() {
        batteries.push(json!({"kind":"device","name":headphones["name"],"percent":headphones["battery"],"status":"","icon":"audio-headphones"}));
    }
    let mut names = HashSet::new();
    batteries.retain(|value| names.insert(value["name"].as_str().unwrap_or("").to_owned()));
    let cameras = camera_devices();
    let dump = json_output("pw-dump", std::iter::empty::<&str>(), json!([]));
    let array = dump.as_array().cloned().unwrap_or_default();
    let microphone_active = array.iter().any(|object| {
        object
            .pointer("/info/props/media.class")
            .and_then(Value::as_str)
            == Some("Stream/Input/Audio")
            && object.pointer("/info/state").and_then(Value::as_str) == Some("running")
            && object
                .pointer("/info/props/seele.role")
                .and_then(Value::as_str)
                != Some("bluetooth-receiver")
    });
    let screen_recording = array.iter().any(|object| {
        object
            .pointer("/info/props/media.class")
            .and_then(Value::as_str)
            == Some("Stream/Output/Video")
            && object.pointer("/info/state").and_then(Value::as_str) == Some("running")
    });
    let camera_active = array.iter().any(|object| {
        object
            .pointer("/info/props/media.class")
            .and_then(Value::as_str)
            == Some("Video/Source")
            && object.pointer("/info/state").and_then(Value::as_str) == Some("running")
    });
    let tailscale = tailscale_state();
    let proton = proton_state();
    let ssh = ssh_state();
    json!({"volume":percent(&audio),"muted":audio.contains("MUTED"),"microphoneVolume":percent(&microphone),"microphoneMuted":microphone.contains("MUTED"),"microphoneActive":microphone_active,"connection":connection_name,"connectionType":connection_type,"connectivity":output("nmcli",["networking","connectivity"]).unwrap_or_else(||"unknown".into()).trim(),"wifiEnabled":wifi_enabled,"wifiAvailable":wifi_available,"ipAddress":route.pointer("/0/prefsrc").and_then(Value::as_str).unwrap_or(""),"gateway":route.pointer("/0/gateway").and_then(Value::as_str).unwrap_or(""),"tailscale":tailscale,"protonVpn":proton,"sshServer":ssh,"bluetoothAvailable":bluetooth["available"],"bluetoothPowered":bluetooth["powered"],"bluetoothScanning":bluetooth["scanning"],"bluetoothReceiver":bluetooth["receiver"],"bluetoothDiscoverable":bluetooth["discoverable"],"bluetoothConnected":bluetooth["connected"],"bluetoothDevices":bluetooth["devices"],"headphones":headphones,"airpodsEarDetection":airpods_ear_detection(),"trayHidden":tray_hidden(),"barModules":bar_modules(),"batteries":batteries,"voxtypeStatus":output("voxtype",["status"]).unwrap_or_else(||"unavailable".into()).lines().next().unwrap_or("unavailable"),"cameraDevices":cameras,"cameraDevice":cameras.first().and_then(|value|value["device"].as_str()).unwrap_or(""),"cameraActive":camera_active,"screenRecording":screen_recording,"audioDevices":crate::audio::devices(&dump),"agentStates":agents::aggregate_states(),"notifications":crate::notifications::state(),"dnd":output("makoctl",["mode"]).unwrap_or_default().lines().any(|line|line=="do-not-disturb")})
}
fn print_status() {
    if !no_status() {
        println!("{}", status_value())
    }
}

fn speedtest() -> Result {
    let mut child = Command::new("speedtest")
        .args([
            "--accept-license",
            "--accept-gdpr",
            "--format=jsonl",
            "--progress=yes",
            "--progress-update-interval=250",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    for line in BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(std::result::Result::ok)
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let result = match value["type"].as_str().unwrap_or("") {
            "ping" => {
                json!({"phase":"ping","ping":value.pointer("/ping/latency").and_then(Value::as_f64).unwrap_or(0.0),"jitter":value.pointer("/ping/jitter").and_then(Value::as_f64).unwrap_or(0.0)})
            }
            "download" => {
                json!({"phase":"download","download":value.pointer("/download/bandwidth").and_then(Value::as_f64).unwrap_or(0.0)*8.0/1_000_000.0})
            }
            "upload" => {
                json!({"phase":"upload","upload":value.pointer("/upload/bandwidth").and_then(Value::as_f64).unwrap_or(0.0)*8.0/1_000_000.0})
            }
            "result" => {
                json!({"ping":value.pointer("/ping/latency").and_then(Value::as_f64).unwrap_or(0.0),"jitter":value.pointer("/ping/jitter").and_then(Value::as_f64).unwrap_or(0.0),"download":value.pointer("/download/bandwidth").and_then(Value::as_f64).unwrap_or(0.0)*8.0/1_000_000.0,"upload":value.pointer("/upload/bandwidth").and_then(Value::as_f64).unwrap_or(0.0)*8.0/1_000_000.0,"server":value.pointer("/server/name").and_then(Value::as_str).unwrap_or("Ookla Speedtest")})
            }
            _ => continue,
        };
        println!("{result}");
    }
    let result = child.wait()?;
    if result.success() {
        Ok(())
    } else {
        Err("speedtest failed".into())
    }
}
fn set_json_list(path: &Path, key: &str, id: &str, action: &str) -> Result {
    let mut values = if key == "hidden" {
        tray_hidden().as_array().cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    if action == "hide"
        || (action == "toggle" && !values.iter().any(|value| value.as_str() == Some(id)))
    {
        values.push(json!(id));
        values.sort_by_key(|value| value.as_str().unwrap_or("").to_owned());
        values.dedup();
    } else {
        values.retain(|value| value.as_str() != Some(id));
    }
    atomic_write(
        path,
        serde_json::to_string(&json!({key:values}))?.as_bytes(),
    )
}
fn camera_settings(device: &str) -> Result {
    let properties = output("udevadm", ["info", "--query=property", "--name", device])
        .ok_or("camera not found")?;
    let fields: HashMap<&str, &str> = properties
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    let vendor = fields
        .get("ID_VENDOR_ID")
        .copied()
        .unwrap_or("")
        .to_ascii_lowercase();
    let product = fields
        .get("ID_MODEL_ID")
        .copied()
        .unwrap_or("")
        .to_ascii_lowercase();
    if vendor != "046d" || product.is_empty() {
        return Err("camera is not supported by openlogi".into());
    }
    let serial = fields
        .get("ID_SERIAL_SHORT")
        .copied()
        .unwrap_or("")
        .to_ascii_lowercase();
    let key = if serial.is_empty() || serial == "0" {
        format!("camera:{vendor}:{product}")
    } else {
        format!("camera:{vendor}:{product}:serial:{serial}")
    };
    let path = config_home().join("openlogi/config.toml");
    let old = fs::read_to_string(&path).unwrap_or_else(|_| "schema_version = 2\n".into());
    let selected = format!("selected_device = {}", serde_json::to_string(&key)?);
    let mut result = String::new();
    let mut written = false;
    for line in old.lines() {
        if !written && line.trim_start().starts_with("selected_device") {
            result.push_str(&selected);
            result.push('\n');
            written = true;
        } else if !written && line.starts_with('[') {
            result.push_str(&selected);
            result.push_str("\n\n");
            result.push_str(line);
            result.push('\n');
            written = true;
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    if !written {
        result.push_str(&selected);
        result.push('\n');
    }
    atomic_write(&path, result.as_bytes())?;
    if !status("pgrep", ["-x", "openlogi-gui"]) {
        detached("openlogi-gui", &[])?;
    }
    Ok(())
}

pub fn run(arguments: &[String]) -> Result {
    let command = arguments.first().map(String::as_str).unwrap_or("status");
    let arg = |index: usize| arguments.get(index).map(String::as_str).unwrap_or("");
    match command {
        "status" => println!("{}", status_value()),
        "agent-status" => println!("{}", agents::aggregate_states()),
        "bluetooth-status" => println!("{}", bluetooth_state()),
        "speedtest" => return speedtest(),
        "launcher-toggle" => {
            if status("vicinae", ["state", "open"]) {
                require_status("vicinae", ["close"])?;
            } else {
                require_status("vicinae", ["open"])?;
            }
        }
        "application" => {
            let address = window_address(arg(2))?;
            match arg(1) {
                "quit" => require_status(
                    "hyprctl",
                    ["dispatch", "closewindow", &format!("address:0x{address}")],
                )?,
                "force-quit" => force_quit_application(&address)?,
                _ => return Err("invalid application action".into()),
            }
        }
        "bluetooth-pairing-answer" => {
            if !matches!(arg(2), "accept" | "reject") {
                return Err("accept or reject required".into());
            }
            let path = runtime_file("bluetooth-pairing.answer");
            atomic_write(
                &path,
                format!("{} {} {}\n", arg(1), arg(2), arg(3)).as_bytes(),
            )?;
        }
        "bluetooth-pair-worker" => {
            require_status("timeout", ["90", "bluetoothctl", "pair", arg(1)])?;
            require_status("timeout", ["10", "bluetoothctl", "trust", arg(1)])?;
            require_status("timeout", ["20", "bluetoothctl", "connect", arg(1)])?;
        }
        "volume" | "microphone" => {
            let (target, maximum, limit) = if command == "volume" {
                ("@DEFAULT_AUDIO_SINK@", 150, "1.5")
            } else {
                ("@DEFAULT_AUDIO_SOURCE@", 100, "1.0")
            };
            match arg(1) {
                "up" => {
                    require_status("wpctl", ["set-volume", "-l", limit, target, "5%+"])?;
                }
                "down" => {
                    require_status("wpctl", ["set-volume", target, "5%-"])?;
                }
                "mute" => {
                    require_status("wpctl", ["set-mute", target, "toggle"])?;
                }
                value if value.parse::<u8>().is_ok_and(|value| value <= maximum) => {
                    require_status(
                        "wpctl",
                        ["set-volume", "-l", limit, target, &format!("{value}%")],
                    )?;
                }
                _ => return Err("invalid audio value".into()),
            }
        }
        "audio-device" => {
            let wanted = arg(1).parse::<u64>().map_err(|_| "device id required")?;
            if arg(2).is_empty() {
                require_status("wpctl", ["set-default", arg(1)])?;
            } else {
                arg(2).parse::<u64>().map_err(|_| "profile id required")?;
                require_status("wpctl", ["set-profile", arg(1), arg(2)])?;
                let mut selected = false;
                for _ in 0..20 {
                    let dump = json_output("pw-dump", std::iter::empty::<&str>(), json!([]));
                    let node = dump
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|object| {
                            object.get("type").and_then(Value::as_str)
                                == Some("PipeWire:Interface:Node")
                                && object
                                    .pointer("/info/props/media.class")
                                    .and_then(Value::as_str)
                                    == Some("Audio/Sink")
                                && object
                                    .pointer("/info/props/device.id")
                                    .and_then(Value::as_u64)
                                    == Some(wanted)
                        })
                        .and_then(|object| object.get("id"))
                        .and_then(Value::as_u64);
                    if let Some(node) = node {
                        require_status("wpctl", ["set-default", &node.to_string()])?;
                        selected = true;
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                if !selected {
                    return Err("audio profile did not create an output".into());
                }
            }
        }
        "voxtype" => {
            require_status("voxtype", ["record", "toggle"])?;
        }
        "wifi" => {
            let mut target = arg(1);
            if target.is_empty() || target == "toggle" {
                target = if output("nmcli", ["-t", "-f", "WIFI", "general"])
                    .unwrap_or_default()
                    .trim()
                    == "enabled"
                {
                    "off"
                } else {
                    "on"
                };
            }
            require_status("nmcli", ["radio", "wifi", target])?;
        }
        "bluetooth" => match arg(1) {
            "" | "toggle" => {
                if bluetooth_state()["powered"].as_bool() == Some(true) {
                    bluetooth_scan_stop();
                    bluetooth_receiver_stop();
                    pairing_close();
                    require_status("bluetoothctl", ["power", "off"])?;
                } else {
                    require_status("bluetoothctl", ["power", "on"])?;
                }
            }
            "scan" => match arg(2) {
                "on" => {
                    bluetooth_scan_start()?;
                    pairing_open()?
                }
                "off" => {
                    bluetooth_scan_stop();
                    pairing_close()
                }
                "toggle" | "" => {
                    if bluetooth_scan_active() {
                        bluetooth_scan_stop();
                        pairing_close()
                    } else {
                        bluetooth_scan_start()?;
                        pairing_open()?
                    }
                }
                _ => return Err("invalid Bluetooth scan action".into()),
            },
            "receiver" => match arg(2) {
                "on" => bluetooth_receiver_start()?,
                "off" => {
                    bluetooth_receiver_stop();
                    pairing_close()
                }
                "toggle" | "" => {
                    if bluetooth_receiver_active() {
                        bluetooth_receiver_stop();
                        pairing_close()
                    } else {
                        bluetooth_receiver_start()?
                    }
                }
                _ => return Err("invalid receiver action".into()),
            },
            "pairing" => match arg(2) {
                "open" | "" => pairing_open()?,
                "close" => pairing_close(),
                _ => return Err("invalid pairing action".into()),
            },
            "connect" => {
                require_status("bluetoothctl", ["power", "on"])?;
                let paired = bluetooth_state()["devices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|device| {
                        device["address"].as_str() == Some(arg(2))
                            && device["paired"].as_bool() == Some(true)
                    });
                if paired {
                    bluetooth_scan_stop();
                    require_status("timeout", ["20", "bluetoothctl", "connect", arg(2)])?;
                } else {
                    if !daemon_active(&runtime_file("bluetooth-agent.pid"), "bt-agent") {
                        bluetooth_agent_start()?;
                    }
                    detached(
                        "seele-control",
                        &["bluetooth-pair-worker".into(), arg(2).into()],
                    )?;
                }
            }
            "pair" => {
                require_status("bluetoothctl", ["power", "on"])?;
                if !daemon_active(&runtime_file("bluetooth-agent.pid"), "bt-agent") {
                    bluetooth_agent_start()?;
                }
                detached(
                    "seele-control",
                    &["bluetooth-pair-worker".into(), arg(2).into()],
                )?;
            }
            "forget" => {
                require_status("timeout", ["20", "bluetoothctl", "remove", arg(2)])?;
            }
            "disconnect" => {
                require_status("timeout", ["20", "bluetoothctl", "disconnect", arg(2)])?;
            }
            "trust" => {
                let trusted = bluetooth_state()["devices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|device| {
                        device["address"].as_str() == Some(arg(2))
                            && device["trusted"].as_bool() == Some(true)
                    });
                let action = match arg(3) {
                    "on" => "trust",
                    "off" => "untrust",
                    "toggle" | "" if trusted => "untrust",
                    "toggle" | "" => "trust",
                    _ => return Err("invalid Bluetooth trust action".into()),
                };
                require_status("timeout", ["10", "bluetoothctl", action, arg(2)])?;
            }
            _ => return Err("invalid Bluetooth action".into()),
        },
        "tailscale" => match arg(1) {
            "up" => {
                require_status("tailscale", ["up"])?;
            }
            "down" => {
                require_status("tailscale", ["down"])?;
            }
            "login" => detached("ghostty", &["-e".into(), "tailscale".into(), "up".into()])?,
            _ => return Err("invalid Tailscale action".into()),
        },
        "proton-vpn" => match arg(1) {
            "connect" => {
                require_status("protonvpn", ["connect"])?;
            }
            "disconnect" => {
                require_status("protonvpn", ["disconnect"])?;
            }
            "open" => detached("protonvpn-app", &[])?,
            _ => return Err("invalid Proton VPN action".into()),
        },
        "ssh-server" => {
            let mode = arg(1);
            if !matches!(mode, "off" | "tailscale" | "ssh") {
                return Err("invalid SSH mode".into());
            }
            let state = ssh_state();
            let tailscale_available = state["tailscaleAvailable"].as_bool() == Some(true);
            let ssh_available = state["sshAvailable"].as_bool() == Some(true);

            if mode != "tailscale"
                && tailscale_available
                && !status("tailscale", ["set", "--ssh=false"])
            {
                return Err("could not disable Tailscale SSH".into());
            }
            if mode != "ssh" && ssh_available && !status("systemctl", ["stop", "sshd.service"]) {
                return Err("could not stop OpenSSH".into());
            }

            match mode {
                "tailscale" if !tailscale_available => {
                    return Err("Tailscale SSH is unavailable".into());
                }
                "tailscale" if !status("tailscale", ["set", "--ssh=true"]) => {
                    return Err("could not enable Tailscale SSH".into());
                }
                "ssh" if !ssh_available => return Err("OpenSSH is unavailable".into()),
                "ssh" if !status("systemctl", ["start", "sshd.service"]) => {
                    return Err("could not start OpenSSH".into());
                }
                _ => {}
            }
        }
        "headphones" | "airpods" => {
            let bluetooth = bluetooth_state();
            let device =
                connected_headphone(&bluetooth).ok_or("no supported headphones connected")?;
            let kind = headphone_kind(device["name"].as_str().unwrap_or(""))
                .ok_or("unsupported headphones")?;
            match (kind, arg(1)) {
                ("airpods", "off" | "anc" | "transparency" | "adaptive") => {
                    require_status("librepods-ctl", [&format!("noise:{}", arg(1))])?;
                }
                ("airpods", "ear-detection") => {
                    set_airpods_ear_detection(if arg(2).is_empty() { "toggle" } else { arg(2) })?
                }
                ("airpods", "open") => {
                    if !status("librepods-ctl", ["reopen"]) {
                        detached("librepods", &[])?;
                    }
                }
                ("nothing", "off" | "anc" | "transparency" | "adaptive") => {
                    let address = device["address"]
                        .as_str()
                        .ok_or("Bluetooth address missing")?;
                    if std::env::var("SEELE_NOTHING_HEADPHONES_DISABLE_DAEMON").as_deref()
                        != Ok("1")
                        && !nothing_headphones_active(address)
                    {
                        nothing_headphones_start(address)?;
                    }
                    nothing::queue_noise(address, arg(1))?
                }
                _ => return Err("invalid headphone action".into()),
            }
        }
        "bar" => {
            let path = config_file("bar.json");
            let mut modules = bar_modules().as_object().cloned().unwrap_or_default();
            modules.insert(arg(2).into(), json!(arg(1) == "show"));
            atomic_write(
                &path,
                serde_json::to_string(&json!({"modules":modules}))?.as_bytes(),
            )?;
        }
        "tray" => set_json_list(&config_file("tray.json"), "hidden", arg(2), arg(1))?,
        "tray-menu" => {
            let cursor = json_output("hyprctl", ["cursorpos", "-j"], json!({}));
            let x = cursor["x"].as_i64().unwrap_or(0).to_string();
            let y = cursor["y"].as_i64().unwrap_or(0).to_string();
            let registered = json_output(
                "busctl",
                [
                    "--user",
                    "--json=short",
                    "get-property",
                    "org.kde.StatusNotifierWatcher",
                    "/StatusNotifierWatcher",
                    "org.kde.StatusNotifierWatcher",
                    "RegisteredStatusNotifierItems",
                ],
                json!({"data":[]}),
            );
            let mut opened = false;
            for address in registered["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let Some((service, object)) = address.split_once('/') else {
                    continue;
                };
                let object = format!("/{object}");
                let item = json_output(
                    "busctl",
                    [
                        "--user",
                        "--json=short",
                        "get-property",
                        service,
                        &object,
                        "org.kde.StatusNotifierItem",
                        "Id",
                    ],
                    json!({}),
                );
                if item["data"].as_str() == Some(arg(1)) {
                    opened = status(
                        "busctl",
                        [
                            "--user",
                            "call",
                            service,
                            &object,
                            "org.kde.StatusNotifierItem",
                            "ContextMenu",
                            "ii",
                            &x,
                            &y,
                        ],
                    );
                    break;
                }
            }
            if !opened {
                return Err("tray item not found".into());
            }
        }
        "camera-settings" => {
            let device = if arg(1).is_empty() {
                camera_devices()
                    .first()
                    .and_then(|value| value["device"].as_str())
                    .unwrap_or("")
                    .to_owned()
            } else {
                arg(1).into()
            };
            camera_settings(&device)?;
        }
        "camera-preview" => {
            let device = if arg(1).is_empty() {
                camera_devices()
                    .first()
                    .and_then(|value| value["device"].as_str())
                    .unwrap_or("")
                    .to_owned()
            } else {
                arg(1).into()
            };
            detached("cameraview", &["-d".into(), device])?;
        }
        "notifications" => match arg(1) {
            "restore" => {
                require_status("makoctl", ["restore"])?;
            }
            "dismiss" => {
                require_status("makoctl", ["dismiss", "-n", arg(2)])?;
            }
            "invoke" => {
                require_status("makoctl", ["invoke", "-n", arg(2)])?;
            }
            "clear" => {
                require_status("makoctl", ["dismiss", "--all", "--no-history"])?;
            }
            _ => return Err("invalid notification action".into()),
        },
        "dnd" => {
            require_status("makoctl", ["mode", "-t", "do-not-disturb"])?;
        }
        "network-settings" => detached("nm-connection-editor", &[])?,
        "outages" => detached("xdg-open", &["https://xn--allestrungen-9ib.de/".to_owned()])?,
        "copy-ip" => {
            let ip = json_output("ip", ["-json", "route", "get", "1.1.1.1"], json!([]))
                .pointer("/0/prefsrc")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let mut child = Command::new("wl-copy").stdin(Stdio::piped()).spawn()?;
            child.stdin.take().unwrap().write_all(ip.as_bytes())?;
            child.wait()?;
        }
        "lock" | "lock-suspend" => {
            if !status(
                &env::var("SEELE_LOCK").unwrap_or_else(|_| "seele-lock".into()),
                std::iter::empty::<&str>(),
            ) {
                return Err("could not secure the session".into());
            }
            if command == "lock-suspend" && !status("systemctl", ["suspend"]) {
                return Err("could not suspend the session".into());
            }
        }
        "logout" => {
            require_status("hyprctl", ["dispatch", "exit"])?;
        }
        "reboot" => {
            require_status("systemctl", ["reboot"])?;
        }
        "shutdown" => {
            require_status("systemctl", ["poweroff"])?;
        }
        "reboot-windows" => {
            require_status(
                "systemctl",
                ["--no-block", "start", "reboot-windows.service"],
            )?;
        }
        _ => return Err("unknown Seele control command".into()),
    }
    if !matches!(
        command,
        "status"
            | "agent-status"
            | "bluetooth-status"
            | "speedtest"
            | "application"
            | "bluetooth-pair-worker"
            | "tray-menu"
    ) {
        print_status()
    }
    Ok(())
}
