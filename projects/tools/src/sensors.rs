//! Temperatures and fan speeds from the kernel's hwmon class, read only while
//! one Sensors panel is open. One process is one observation session.
use crate::Result;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, BufRead, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

const CADENCE: Duration = Duration::from_secs(2);
const DEVICE_LIMIT: usize = 64;
const ENTRY_LIMIT: usize = 1024;
const CHANNEL_LIMIT: usize = 64;
const LABEL_LIMIT: usize = 48;
// Millidegrees Celsius. A reading outside this span is not a temperature any
// driver means, so it is reported as unreadable rather than drawn.
const COLDEST: i64 = -273_150;
const HOTTEST: i64 = 500_000;
const FASTEST: i64 = 100_000;

fn small(path: &Path, limit: u64) -> io::Result<String> {
    let mut value = String::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut value)?;
    if value.len() as u64 > limit {
        return Err(io::Error::other("attribute exceeds its bound"));
    }
    Ok(value.trim().to_owned())
}
fn number(path: &Path) -> io::Result<i64> {
    small(path, 24)?
        .parse()
        .map_err(|_| io::Error::other("attribute is not a number"))
}
fn flag(path: &Path) -> bool {
    number(path).ok() == Some(1)
}
/// A driver or firmware string reaches the panel as one plain line: control
/// and direction characters that could forge a row of interface are dropped.
fn clean(text: &str) -> Option<String> {
    let words = text
        .chars()
        .filter(|c| seele_runtime::redact::visible(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let line = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(LABEL_LIMIT)
        .collect::<String>();
    (!line.is_empty()).then_some(line)
}
fn label(path: &Path) -> Option<String> {
    clean(&small(path, 128).ok()?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Temperature,
    Fan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum State {
    Normal,
    Stopped,
    High,
    Critical,
    Alarm,
    Fault,
    Disabled,
    Unavailable,
}
#[derive(Clone, Debug, PartialEq)]
struct Channel {
    kind: Kind,
    index: u32,
    label: Option<String>,
    value: Option<i64>,
    high: Option<i64>,
    critical: Option<i64>,
    minimum: Option<i64>,
    state: State,
}
#[derive(Clone, Debug)]
struct Device {
    id: String,
    inode: u64,
    chip: String,
    device: Option<String>,
    title: Option<String>,
    channels: Vec<Channel>,
}
#[derive(Default)]
struct Discovery {
    devices: Vec<Device>,
    limited: bool,
    skipped: usize,
}

/// A limit only counts as one when it is a temperature a part could reach;
/// drivers publish 0 or −273.15 °C for a limit they do not have.
fn limit(path: &Path) -> Option<i64> {
    number(path).ok().filter(|v| *v > 0 && *v <= HOTTEST)
}
fn channel(root: &Path, kind: Kind, index: u32) -> Channel {
    let prefix = match kind {
        Kind::Temperature => format!("temp{index}"),
        Kind::Fan => format!("fan{index}"),
    };
    let at = |suffix: &str| root.join(format!("{prefix}_{suffix}"));
    let label = label(&at("label"));
    if number(&at("enable")).ok() == Some(0) {
        return Channel {
            kind,
            index,
            label,
            value: None,
            high: None,
            critical: None,
            minimum: None,
            state: State::Disabled,
        };
    }
    let (value, high, critical, minimum) = match kind {
        Kind::Temperature => (
            number(&at("input"))
                .ok()
                .filter(|v| (COLDEST..=HOTTEST).contains(v)),
            limit(&at("max")),
            limit(&at("crit")),
            None,
        ),
        Kind::Fan => (
            number(&at("input"))
                .ok()
                .filter(|v| (0..=FASTEST).contains(v)),
            None,
            None,
            number(&at("min")).ok().filter(|v| *v > 0 && *v <= FASTEST),
        ),
    };
    // The driver's own verdict leads: an alarm flag is raised against the
    // limit the chip compares in hardware, which may differ from the one it
    // shows. The comparison only fills in where a driver keeps no flag.
    let state = if flag(&at("fault")) {
        State::Fault
    } else if let Some(value) = value {
        match kind {
            Kind::Temperature => {
                if flag(&at("crit_alarm")) || critical.is_some_and(|c| value >= c) {
                    State::Critical
                } else if flag(&at("max_alarm"))
                    || flag(&at("alarm"))
                    || high.is_some_and(|h| value >= h)
                {
                    State::High
                } else {
                    State::Normal
                }
            }
            Kind::Fan => {
                if flag(&at("alarm")) || flag(&at("min_alarm")) {
                    State::Alarm
                } else if value == 0 {
                    State::Stopped
                } else {
                    State::Normal
                }
            }
        }
    } else {
        State::Unavailable
    };
    Channel {
        kind,
        index,
        label,
        value: if state == State::Fault { None } else { value },
        high,
        critical,
        minimum,
        state,
    }
}
fn channels(root: &Path) -> io::Result<(Vec<Channel>, bool)> {
    let mut found = BTreeSet::new();
    let mut entries = 0;
    for entry in fs::read_dir(root)? {
        entries += 1;
        if entries > ENTRY_LIMIT {
            break;
        }
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Some(stem) = name.strip_suffix("_input") else {
            continue;
        };
        let (kind, digits) = if let Some(digits) = stem.strip_prefix("temp") {
            (Kind::Temperature, digits)
        } else if let Some(digits) = stem.strip_prefix("fan") {
            (Kind::Fan, digits)
        } else {
            continue;
        };
        if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(index) = digits.parse::<u32>() {
            found.insert((kind, index));
        }
    }
    let mut limited = entries > ENTRY_LIMIT;
    let mut out = Vec::new();
    let mut per_kind = BTreeMap::<Kind, usize>::new();
    for (kind, index) in found {
        let count = per_kind.entry(kind).or_default();
        if *count == CHANNEL_LIMIT {
            limited = true;
            continue;
        }
        *count += 1;
        out.push(channel(root, kind, index));
    }
    Ok((out, limited))
}
fn discover(root: &Path) -> io::Result<Discovery> {
    // Sorting before truncating keeps the same devices on every pass when
    // there are more than the panel lists, rather than whichever the
    // directory happened to yield first.
    let mut entries = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .take(ENTRY_LIMIT)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    let mut discovery = Discovery {
        limited: entries.len() > DEVICE_LIMIT,
        ..Discovery::default()
    };
    entries.truncate(DEVICE_LIMIT);
    for entry in entries {
        let path = entry.path();
        let Ok(chip) = small(&path.join("name"), 64) else {
            continue;
        };
        // The ABI promises a short lowercase word; anything else is not a chip
        // name this panel can print or key on.
        if chip.is_empty()
            || !chip
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        {
            continue;
        }
        // drivetemp answers each read with an ATA or SCSI command to the disk,
        // and its documentation warns that this can reset a drive's spin-down
        // timer. An inspector that keeps a disk awake is touching what it
        // inspects, so those drives are named as left alone instead.
        if chip == "drivetemp" {
            discovery.skipped += 1;
            continue;
        }
        let Ok(inode) = fs::metadata(&path).map(|m| m.ino()) else {
            continue;
        };
        let device = fs::canonicalize(path.join("device")).ok();
        let Ok((channels, limited)) = channels(&path) else {
            continue;
        };
        discovery.limited |= limited;
        if channels.is_empty() {
            continue;
        }
        // Recheck identity after the multi-file read: a driver that unbound
        // and rebound mid-pass cannot join two devices' readings.
        if fs::metadata(&path).map(|m| m.ino()).ok() != Some(inode) {
            continue;
        }
        let title = label(&path.join("label")).or_else(|| {
            device
                .as_ref()
                .and_then(|device| label(&device.join("model")))
        });
        let id = match &device {
            Some(device) => format!("{chip}@{}", device.display()),
            None => format!("{chip}@{}", entry.file_name().to_string_lossy()),
        };
        discovery.devices.push(Device {
            id,
            inode,
            chip,
            device: device
                .as_ref()
                .and_then(|d| d.file_name())
                .and_then(|n| clean(&n.to_string_lossy())),
            title,
            channels,
        });
    }
    Ok(discovery)
}

/// A family name for the chips a desktop commonly carries, and the order the
/// panel lists them in. The order is fixed by what a device is, never by its
/// reading, so a row does not move while the pointer is on its way to it.
fn family(chip: &str) -> (u8, &'static str) {
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| chip.starts_with(p));
    if starts(&["k10temp", "k8temp", "zenpower", "coretemp", "fam15h_power"]) {
        (0, "CPU")
    } else if starts(&["amdgpu", "radeon", "nouveau", "i915", "xe"]) {
        (1, "GPU")
    } else if starts(&[
        "nct", "it87", "it86", "w83", "f71", "asus", "gigabyte", "dell_smm", "thinkpad", "applesmc",
    ]) {
        (2, "Mainboard")
    } else if starts(&["spd5118", "jc42", "ee1004"]) {
        (3, "Memory")
    } else if starts(&["nvme"]) {
        (4, "NVMe drive")
    } else if starts(&[
        "iwlwifi", "mt79", "mt76", "ath1", "r8169", "igc", "ixgbe", "atlantic", "aquantia",
    ]) {
        (5, "Network adapter")
    } else if starts(&["acpitz", "pch_", "cpu_thermal", "soc_thermal"]) {
        (6, "Thermal zone")
    } else if starts(&["BAT", "battery", "ucsi", "hidpp", "logitech"]) {
        (7, "Battery")
    } else {
        (8, "")
    }
}

/// One decimal, the trailing `.0` dropped, the way the shell prints any
/// measured reading.
fn celsius(millidegrees: i64) -> String {
    let tenths = (millidegrees as f64 / 100.0).round() as i64;
    let sign = if tenths < 0 { "−" } else { "" };
    let tenths = tenths.abs();
    if tenths % 10 == 0 {
        format!("{sign}{} °C", tenths / 10)
    } else {
        format!("{sign}{}.{} °C", tenths / 10, tenths % 10)
    }
}
fn rpm(value: i64) -> String {
    format!("{value} RPM")
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Reading {
    id: String,
    kind: Kind,
    label: String,
    value: String,
    peak: String,
    limits: String,
    state: State,
    status: &'static str,
    ratio: Option<f64>,
}
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    title: String,
    detail: String,
    readings: Vec<Reading>,
}
#[derive(Default)]
struct Track {
    inode: u64,
    peaks: BTreeMap<(Kind, u32), i64>,
}
#[derive(Default)]
struct Session {
    tracks: BTreeMap<String, Track>,
}
struct Projection {
    rows: Vec<Row>,
    summary: String,
    attention: usize,
}
impl Session {
    fn reset(&mut self) {
        self.tracks.clear();
    }
    fn sample(&mut self, mut devices: Vec<Device>) -> Projection {
        self.tracks
            .retain(|id, _| devices.iter().any(|device| &device.id == id));
        devices.sort_by(|a, b| (family(&a.chip).0, &a.id).cmp(&(family(&b.chip).0, &b.id)));
        let mut hottest: Option<(i64, String)> = None;
        let mut attention = 0;
        let rows = devices
            .into_iter()
            .map(|device| {
                let track = self.tracks.entry(device.id.clone()).or_default();
                // A new sysfs directory under the same device is a new driver
                // instance; its peaks belong to the one that went away.
                if track.inode != device.inode {
                    *track = Track {
                        inode: device.inode,
                        ..Track::default()
                    };
                }
                let (_, family_name) = family(&device.chip);
                let title = device.title.clone().unwrap_or_else(|| {
                    if family_name.is_empty() {
                        device.chip.clone()
                    } else {
                        family_name.to_owned()
                    }
                });
                let detail = match &device.device {
                    Some(name) => format!("{} · {name}", device.chip),
                    None => device.chip.clone(),
                };
                let readings = device
                    .channels
                    .iter()
                    .map(|channel| {
                        let key = (channel.kind, channel.index);
                        let peak = channel.value.map(|value| {
                            let peak = track.peaks.entry(key).or_insert(value);
                            *peak = (*peak).max(value);
                            *peak
                        });
                        let peak = peak.or_else(|| track.peaks.get(&key).copied());
                        let label = channel.label.clone().unwrap_or_else(|| match channel.kind {
                            Kind::Temperature => format!("Temperature {}", channel.index),
                            Kind::Fan => format!("Fan {}", channel.index),
                        });
                        if matches!(
                            channel.state,
                            State::High | State::Critical | State::Alarm | State::Fault
                        ) {
                            attention += 1;
                        }
                        if channel.kind == Kind::Temperature {
                            if let Some(value) = channel.value {
                                if hottest.as_ref().is_none_or(|(best, _)| value > *best) {
                                    hottest = Some((value, format!("{title} {label}")));
                                }
                            }
                        }
                        let format = |value: i64| match channel.kind {
                            Kind::Temperature => celsius(value),
                            Kind::Fan => rpm(value),
                        };
                        let limits = [
                            channel.high.map(|v| format!("High {}", celsius(v))),
                            channel.critical.map(|v| format!("critical {}", celsius(v))),
                            channel.minimum.map(|v| format!("Minimum {}", rpm(v))),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                        // The meter spans zero to the hardest limit the driver
                        // states; with none stated there is nothing to measure
                        // a reading against, so no meter is drawn.
                        let ceiling = channel.critical.or(channel.high);
                        let ratio = match (channel.value, ceiling) {
                            (Some(value), Some(ceiling)) => {
                                Some((value as f64 / ceiling as f64).clamp(0.0, 1.0))
                            }
                            _ => None,
                        };
                        Reading {
                            id: format!(
                                "{}{}",
                                match channel.kind {
                                    Kind::Temperature => "temp",
                                    Kind::Fan => "fan",
                                },
                                channel.index
                            ),
                            kind: channel.kind,
                            label,
                            value: channel.value.map_or_else(|| "—".into(), format),
                            peak: peak.map_or_else(|| "—".into(), format),
                            limits,
                            state: channel.state,
                            status: match channel.state {
                                State::Normal => "",
                                State::Stopped => "Stopped",
                                State::High => "High",
                                State::Critical => "Critical",
                                State::Alarm => "Alarm",
                                State::Fault => "Fault",
                                State::Disabled => "Disabled",
                                State::Unavailable => "No reading",
                            },
                            ratio,
                        }
                    })
                    .collect();
                Row {
                    id: device.id,
                    title,
                    detail,
                    readings,
                }
            })
            .collect();
        let hottest = match hottest {
            Some((value, name)) => format!("Hottest {} · {name}", celsius(value)),
            None => "No temperature readings".into(),
        };
        // The header is tinted when anything needs attention, so it also has
        // to say so: the hottest reading is not necessarily the one at fault.
        let summary = match attention {
            0 => hottest,
            1 => format!("1 reading needs attention · {hottest}"),
            n => format!("{n} readings need attention · {hottest}"),
        };
        Projection {
            rows,
            summary,
            attention,
        }
    }
}
fn emit(session: &mut Session, root: &Path, elapsed: Duration) -> Result {
    let common = serde_json::json!({
        "version": 1,
        "elapsed": elapsed.as_secs(),
        "cadenceSeconds": CADENCE.as_secs(),
        "deviceLimit": DEVICE_LIMIT,
    });
    let mut value = match discover(root) {
        Ok(discovery) => {
            let projection = session.sample(discovery.devices);
            serde_json::json!({
                "rows": projection.rows,
                "summary": projection.summary,
                "attention": projection.attention,
                "limited": discovery.limited,
                "skipped": discovery.skipped,
                "error": "",
            })
        }
        Err(_) => serde_json::json!({
            "rows": [],
            "summary": "",
            "attention": 0,
            "limited": false,
            "skipped": 0,
            "error": "The kernel's sensor interface is unavailable",
        }),
    };
    if let (Some(value), Some(common)) = (value.as_object_mut(), common.as_object()) {
        value.extend(common.clone());
    }
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}
pub fn run(arguments: &[String]) -> Result {
    if !arguments.is_empty() {
        return Err("usage: seele-sensors (JSON lines on stdin: reset)".into());
    }
    let root = std::env::var_os("SEELE_SENSORS_SYSFS")
        .map_or_else(|| PathBuf::from("/sys/class/hwmon"), PathBuf::from);
    let (sender, receiver) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        loop {
            let mut line = Vec::new();
            match input.by_ref().take(1025).read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if line.len() > 1024 => break,
                Ok(_) => {
                    if serde_json::from_slice::<serde_json::Value>(&line)
                        .ok()
                        .as_ref()
                        .and_then(|v| v.get("op"))
                        .and_then(|v| v.as_str())
                        == Some("reset")
                        && sender.send(()).is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    let mut session = Session::default();
    let mut start = Instant::now();
    emit(&mut session, &root, start.elapsed())?;
    let mut next = Instant::now() + CADENCE;
    loop {
        match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(()) => {
                session.reset();
                start = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        emit(&mut session, &root, start.elapsed())?;
        next = Instant::now() + CADENCE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn write(path: &Path, value: impl ToString) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value.to_string()).unwrap();
    }
    fn chip(root: &Path, dir: &str, name: &str) -> PathBuf {
        let path = root.join(dir);
        write(&path.join("name"), name);
        path
    }
    fn only(root: &Path) -> Device {
        let mut devices = discover(root).unwrap().devices;
        assert_eq!(devices.len(), 1);
        devices.remove(0)
    }

    #[test]
    fn celsius_keeps_one_decimal_and_drops_a_trailing_zero() {
        assert_eq!(celsius(54_250), "54.3 °C");
        assert_eq!(celsius(61_000), "61 °C");
        assert_eq!(celsius(60_960), "61 °C");
        assert_eq!(celsius(-5_500), "−5.5 °C");
        assert_eq!(celsius(-40), "0 °C");
    }

    #[test]
    fn limits_and_alarm_flags_decide_the_state() {
        let root = tempfile::tempdir().unwrap();
        let path = chip(root.path(), "hwmon0", "nvme");
        write(&path.join("temp1_input"), 50_000);
        write(&path.join("temp1_max"), 80_000);
        write(&path.join("temp1_crit"), 85_000);
        write(&path.join("temp1_min"), -273_150);
        write(&path.join("temp2_input"), 81_000);
        write(&path.join("temp2_max"), 80_000);
        write(&path.join("temp3_input"), 90_000);
        write(&path.join("temp3_crit"), 85_000);
        // The chip's own flag is believed over a displayed limit it has not reached.
        write(&path.join("temp4_input"), 40_000);
        write(&path.join("temp4_max"), 80_000);
        write(&path.join("temp4_max_alarm"), 1);
        // No limits: nothing is invented and no meter is drawn.
        write(&path.join("temp5_input"), 99_000);
        write(&path.join("temp5_max"), 0);
        let states = only(root.path())
            .channels
            .iter()
            .map(|c| c.state)
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            [
                State::Normal,
                State::High,
                State::Critical,
                State::High,
                State::Normal
            ]
        );
        let device = only(root.path());
        let projection = Session::default().sample(vec![device]);
        let readings = &projection.rows[0].readings;
        assert_eq!(readings[0].limits, "High 80 °C · critical 85 °C");
        assert_eq!(readings[0].ratio, Some(50.0 / 85.0));
        assert_eq!(readings[4].limits, "");
        assert_eq!(readings[4].ratio, None);
        assert_eq!(projection.attention, 3);
        assert_eq!(
            projection.summary,
            "3 readings need attention · Hottest 99 °C · NVMe drive Temperature 5"
        );
    }

    #[test]
    fn broken_channels_stay_visible_as_what_they_are() {
        let root = tempfile::tempdir().unwrap();
        let path = chip(root.path(), "hwmon0", "nct6798");
        write(&path.join("temp1_input"), 30_000);
        write(&path.join("temp1_fault"), 1);
        write(&path.join("temp2_input"), 30_000);
        write(&path.join("temp2_enable"), 0);
        // A read error, as ENODATA from a sleeping sensor gives, is not a zero.
        fs::create_dir_all(path.join("temp3_input")).unwrap();
        write(&path.join("temp4_input"), "hot");
        write(&path.join("temp5_input"), 900_000);
        write(&path.join("fan1_input"), 0);
        write(&path.join("fan2_input"), 1_240);
        write(&path.join("fan2_min"), 300);
        write(&path.join("fan3_input"), 200);
        write(&path.join("fan3_alarm"), 1);
        let device = only(root.path());
        let states = device.channels.iter().map(|c| c.state).collect::<Vec<_>>();
        assert_eq!(
            states,
            [
                State::Fault,
                State::Disabled,
                State::Unavailable,
                State::Unavailable,
                State::Unavailable,
                State::Stopped,
                State::Normal,
                State::Alarm,
            ]
        );
        assert!(device.channels.iter().take(5).all(|c| c.value.is_none()));
        let rows = Session::default().sample(vec![device]).rows;
        let readings = &rows[0].readings;
        assert_eq!(rows[0].title, "Mainboard");
        assert_eq!(readings[0].value, "—");
        assert_eq!(readings[2].status, "No reading");
        assert_eq!(readings[5].value, "0 RPM");
        assert_eq!(readings[5].status, "Stopped");
        assert_eq!(readings[6].limits, "Minimum 300 RPM");
        assert_eq!(readings[6].ratio, None);
    }

    #[test]
    fn names_come_from_the_kernel_and_are_plain_text() {
        let root = tempfile::tempdir().unwrap();
        let devices = root.path().join("devices/pci0000:00/nvme/nvme0");
        write(
            &devices.join("model"),
            "Samsung SSD\u{202e} 990 PRO\n  2TB\u{7}",
        );
        let path = chip(root.path(), "hwmon3", "nvme");
        symlink(&devices, path.join("device")).unwrap();
        write(&path.join("temp1_input"), 41_850);
        write(&path.join("temp1_label"), "Composite\u{1b}[31m");
        write(&path.join("temp2_input"), 38_000);
        let device = only(root.path());
        assert_eq!(device.title.as_deref(), Some("Samsung SSD 990 PRO 2TB"));
        assert_eq!(device.device.as_deref(), Some("nvme0"));
        assert!(device.id.starts_with("nvme@") && device.id.ends_with("/nvme0"));
        let rows = Session::default().sample(vec![device]).rows;
        assert_eq!(rows[0].detail, "nvme · nvme0");
        assert_eq!(rows[0].readings[0].label, "Composite [31m");
        assert_eq!(rows[0].readings[1].label, "Temperature 2");
        // A chip name outside the ABI's alphabet is not keyed on or printed.
        chip(root.path(), "hwmon4", "bad name");
        write(&root.path().join("hwmon4/temp1_input"), 1_000);
        assert_eq!(discover(root.path()).unwrap().devices.len(), 1);
    }

    #[test]
    fn drivetemp_disks_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let path = chip(root.path(), "hwmon1", "drivetemp");
        write(&path.join("temp1_input"), 35_000);
        let discovery = discover(root.path()).unwrap();
        assert!(discovery.devices.is_empty());
        assert_eq!(discovery.skipped, 1);
    }

    #[test]
    fn order_follows_identity_never_the_reading() {
        let root = tempfile::tempdir().unwrap();
        for (dir, name, value) in [
            ("hwmon0", "nvme", 70_000),
            ("hwmon1", "acpitz", 20_000),
            ("hwmon2", "k10temp", 40_000),
            ("hwmon3", "mystery", 10_000),
        ] {
            write(&chip(root.path(), dir, name).join("temp1_input"), value);
        }
        let mut session = Session::default();
        let titles = |session: &mut Session| {
            session
                .sample(discover(root.path()).unwrap().devices)
                .rows
                .into_iter()
                .map(|row| row.title)
                .collect::<Vec<_>>()
        };
        let first = titles(&mut session);
        assert_eq!(first, ["CPU", "NVMe drive", "Thermal zone", "mystery"]);
        write(&root.path().join("hwmon2/temp1_input"), 95_000);
        write(&root.path().join("hwmon0/temp1_input"), 10_000);
        assert_eq!(titles(&mut session), first);
    }

    #[test]
    fn peaks_hold_until_reset_and_follow_the_driver_instance() {
        let root = tempfile::tempdir().unwrap();
        let path = chip(root.path(), "hwmon0", "k10temp");
        write(&path.join("temp1_input"), 50_000);
        let mut session = Session::default();
        let peak = |session: &mut Session| {
            session.sample(discover(root.path()).unwrap().devices).rows[0].readings[0]
                .peak
                .clone()
        };
        assert_eq!(peak(&mut session), "50 °C");
        write(&path.join("temp1_input"), 72_500);
        assert_eq!(peak(&mut session), "72.5 °C");
        write(&path.join("temp1_input"), 45_000);
        assert_eq!(peak(&mut session), "72.5 °C");
        // A reading that fails keeps the peak it already had.
        fs::remove_file(path.join("temp1_input")).unwrap();
        fs::create_dir(path.join("temp1_input")).unwrap();
        let rows = session.sample(discover(root.path()).unwrap().devices).rows;
        assert_eq!(rows[0].readings[0].value, "—");
        assert_eq!(rows[0].readings[0].peak, "72.5 °C");
        fs::remove_dir(path.join("temp1_input")).unwrap();
        write(&path.join("temp1_input"), 45_000);
        session.reset();
        assert_eq!(peak(&mut session), "45 °C");
        write(&path.join("temp1_input"), 60_000);
        assert_eq!(peak(&mut session), "60 °C");
        // The driver rebinds: same chip and hwmon name, a new directory. The
        // replacement exists before the original goes, so no inode is reused.
        write(
            &chip(root.path(), "next", "k10temp").join("temp1_input"),
            41_000,
        );
        fs::remove_dir_all(&path).unwrap();
        fs::rename(root.path().join("next"), &path).unwrap();
        assert_eq!(peak(&mut session), "41 °C");
        // A device that goes away takes its session with it.
        fs::remove_dir_all(root.path().join("hwmon0")).unwrap();
        session.sample(discover(root.path()).unwrap().devices);
        assert!(session.tracks.is_empty());
    }

    #[test]
    fn discovery_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..DEVICE_LIMIT + 3 {
            write(
                &chip(root.path(), &format!("hwmon{index:03}"), "k10temp").join("temp1_input"),
                40_000,
            );
        }
        let path = root.path().join("hwmon000");
        for index in 1..=CHANNEL_LIMIT + 5 {
            write(&path.join(format!("temp{index}_input")), 40_000);
            write(&path.join(format!("fan{index}_input")), 900);
        }
        // Oversized attributes are refused rather than truncated into a number.
        write(&path.join("temp1_input"), "4".repeat(400));
        write(&path.join("temp1_label"), "x".repeat(4096));
        let discovery = discover(root.path()).unwrap();
        assert!(discovery.limited);
        assert_eq!(discovery.devices.len(), DEVICE_LIMIT);
        let first = &discovery.devices[0];
        let temperatures = first
            .channels
            .iter()
            .filter(|c| c.kind == Kind::Temperature);
        assert_eq!(temperatures.count(), CHANNEL_LIMIT);
        assert_eq!(first.channels.len(), CHANNEL_LIMIT * 2);
        assert_eq!(first.channels[0].state, State::Unavailable);
        assert_eq!(first.channels[0].label, None);
        assert!(clean(&"y".repeat(200)).unwrap().chars().count() == LABEL_LIMIT);
    }
}
