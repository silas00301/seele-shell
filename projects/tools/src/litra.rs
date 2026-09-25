//! The Litra Glows beside the webcam, owned by the resident status monitor.
//!
//! OpenLogi's light commands write settings but cannot read them back, so a
//! panel that only forwarded clicks could never say what a light is doing.
//! Instead this module owns each light's desired state: the mode, brightness
//! and colour temperature the user chose are kept per light identity in one
//! private file, and the monitor is the only writer to the devices. It applies
//! a light's state when that light appears, when the user changes it and, in
//! camera mode, when the webcam starts or stops, then reports what it actually
//! applied. Until the user has chosen anything for a light, nothing is written
//! to it at all.
//!
//! A light is reported only while it is reachable: listed by OpenLogi, and
//! not refusing the last write. A refused write withdraws it until a retry
//! succeeds, so the panel never offers controls that go nowhere.
//!
//! The light's own buttons still change it behind Seele's back. That drift
//! lasts until the next transition; the monitor never polls the light back
//! into line, because that would fight a deliberate press.

use crate::command::{config_home, output, output_with_input, shutdown_signal};
use crate::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

const DEFAULT_BRIGHTNESS: u8 = 50;
const DEFAULT_TEMPERATURE: u16 = 4000;
const TEMPERATURE_RANGE: std::ops::RangeInclusive<u16> = 2700..=6500;
const TEMPERATURE_STEP: u16 = 100;
/// The Glow's HID route as `openlogi light list` prints it.
const GLOW_ROUTE: &str = " (046d:c900 usage ff43:0202)";
/// How often lights are looked for. A probe enumerates HID devices.
const PROBE_INTERVAL: Duration = Duration::from_secs(5);
/// A light that refused a write is retried on this cadence, or sooner on the
/// user's input.
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// A short graph rebuild can momentarily report no running camera source.
/// Wait this long before switching off, but switch on at once.
const CAMERA_SETTLE: Duration = Duration::from_millis(750);
/// A slider being dragged sends a value per pointer move. Persist the last
/// one once the input has paused rather than rewriting the file each time.
const PERSIST_DELAY: Duration = Duration::from_millis(300);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Off,
    On,
    /// On while a camera source runs, off otherwise.
    Camera,
}

impl Mode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            "camera" => Some(Self::Camera),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Camera => "camera",
        }
    }
}

/// What the user asked the light to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    /// `None` until a mode is chosen: power is then left alone.
    mode: Option<Mode>,
    /// Percent of the light's range. The Glow's floor is still lit, so a
    /// brightness that reads 0 % while the light shines is not offered.
    brightness: u8,
    temperature: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: None,
            brightness: DEFAULT_BRIGHTNESS,
            temperature: DEFAULT_TEMPERATURE,
        }
    }
}

fn brightness(value: &str) -> Option<u8> {
    value
        .parse::<u8>()
        .ok()
        .filter(|level| (1..=100).contains(level))
}

fn temperature(value: &str) -> Option<u16> {
    value
        .parse::<u16>()
        .ok()
        .filter(|kelvin| TEMPERATURE_RANGE.contains(kelvin) && kelvin % TEMPERATURE_STEP == 0)
}

impl Settings {
    /// A damaged or hand-edited field falls back on its own; the rest stays.
    fn parse(value: &Value) -> Self {
        let field = |name: &str| match &value[name] {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            _ => String::new(),
        };
        Self {
            mode: Mode::parse(&field("mode")),
            brightness: brightness(&field("brightness")).unwrap_or(DEFAULT_BRIGHTNESS),
            temperature: temperature(&field("temperature")).unwrap_or(DEFAULT_TEMPERATURE),
        }
    }

    fn to_json(self) -> Value {
        let mut value = json!({"brightness":self.brightness,"temperature":self.temperature});
        if let Some(mode) = self.mode {
            value["mode"] = json!(mode.name());
        }
        value
    }

    fn with(self, key: &str, value: &str) -> Result<Self> {
        let mut next = self;
        match key {
            "mode" => next.mode = Some(Mode::parse(value).ok_or("invalid Litra Glow mode")?),
            "brightness" => {
                next.brightness = brightness(value).ok_or("invalid Litra Glow brightness")?
            }
            "temperature" => {
                next.temperature = temperature(value).ok_or("invalid Litra Glow temperature")?
            }
            _ => return Err("invalid Litra Glow setting".into()),
        }
        Ok(next)
    }
}

/// Every light's settings by OpenLogi identity. A light that is unplugged
/// keeps its entry, so it comes back as the user left it.
type Store = BTreeMap<String, Settings>;

/// OpenLogi prefers a serial number for a light's identity, so settings follow
/// the physical light across ports. It selects by substring, so an identity
/// must be non-empty and free of line breaks to name one light.
fn valid_identity(identity: &str) -> bool {
    !identity.is_empty() && identity.len() <= 256 && !identity.chars().any(char::is_control)
}

fn parse_store(text: &str) -> Store {
    let value: Value = serde_json::from_str(text).unwrap_or_default();
    value["lights"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(identity, _)| valid_identity(identity))
        .map(|(identity, settings)| (identity.clone(), Settings::parse(settings)))
        .collect()
}

fn store_json(store: &Store) -> Value {
    let lights: serde_json::Map<String, Value> = store
        .iter()
        .map(|(identity, settings)| (identity.clone(), settings.to_json()))
        .collect();
    json!({ "lights": lights })
}

fn settings_path() -> PathBuf {
    config_home().join("seele-shell").join("litra-glow.json")
}

fn load() -> Store {
    fs::read_to_string(settings_path())
        .map(|text| parse_store(&text))
        .unwrap_or_default()
}

fn save(store: &Store) -> Result {
    crate::command::atomic_write(&settings_path(), store_json(store).to_string().as_bytes())
}

fn stamp() -> Option<SystemTime> {
    fs::metadata(settings_path())
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Applies one `<field> <value>` change to a light's stored settings.
fn update(store: &mut Store, identity: &str, key: &str, value: &str) -> Result {
    if !valid_identity(identity) {
        return Err("invalid Litra Glow identity".into());
    }
    let current = store.get(identity).copied().unwrap_or_default();
    store.insert(identity.to_owned(), current.with(key, value)?);
    Ok(())
}

/// `seele-control litra-glow <identity> <mode|brightness|temperature> <value>`.
/// The monitor notices the file change and applies it.
pub(crate) fn set(identity: &str, key: &str, value: &str) -> Result {
    let mut store = load();
    update(&mut store, identity, key, value)?;
    save(&store)
}

fn glow_identities(list: &str) -> Vec<String> {
    let mut identities: Vec<String> = list
        .lines()
        .filter_map(|line| {
            let (_, route) = line.rsplit_once(" — ")?;
            let identity = route.strip_suffix(GLOW_ROUTE)?;
            valid_identity(identity).then(|| identity.to_owned())
        })
        .collect();
    // A stable order keeps each light in its place in the panel.
    identities.sort();
    identities.dedup();
    identities
}

/// `None` when OpenLogi could not be asked at all, which is not the same as
/// no lights: an unknown inventory leaves the known lights as they were.
fn detect() -> Option<Vec<String>> {
    output("openlogi", ["light", "list"]).map(|list| glow_identities(&list))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Write {
    Power(bool),
    Brightness(u8),
    Temperature(u16),
}

impl Write {
    fn arguments(self, identity: &str) -> Vec<String> {
        let mut arguments = vec!["light".to_owned()];
        match self {
            Self::Power(on) => arguments.push(if on { "on" } else { "off" }.to_owned()),
            Self::Brightness(level) => {
                arguments.extend(["brightness".into(), "--percent".into(), level.to_string()])
            }
            Self::Temperature(kelvin) => {
                arguments.extend(["temperature".into(), "--kelvin".into(), kelvin.to_string()])
            }
        }
        arguments.extend(["--device".into(), identity.to_owned()]);
        arguments
    }

    /// `--device` selects by identity, so a light that went away or was
    /// replaced fails the write rather than redirecting it.
    fn send(self, identity: &str) -> Result {
        output_with_input(
            "openlogi",
            self.arguments(identity),
            b"",
            WRITE_TIMEOUT,
            64 * 1024,
        )
        .map(|_| ())
        .ok_or_else(|| "Litra Glow write failed".into())
    }
}

/// What this monitor has written to one light since it last appeared. A
/// value is `None` until written, and forgotten when the light reconnects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Applied {
    power: Option<bool>,
    brightness: Option<u8>,
    temperature: Option<u16>,
}

impl Applied {
    fn record(&mut self, write: Write) {
        match write {
            Write::Power(on) => {
                self.power = Some(on);
                // Turning on always lands on the levels the panel shows, even
                // if the light's own buttons moved them while it was off.
                if on {
                    self.brightness = None;
                    self.temperature = None;
                }
            }
            Write::Brightness(level) => self.brightness = Some(level),
            Write::Temperature(kelvin) => self.temperature = Some(kelvin),
        }
    }
}

/// The power the mode asks for. `camera` is `None` while the camera's state
/// is unknown or still settling, which leaves power where it is.
fn target_power(mode: Option<Mode>, camera: Option<bool>) -> Option<bool> {
    match mode? {
        Mode::Off => Some(false),
        Mode::On => Some(true),
        Mode::Camera => camera,
    }
}

/// The next write that brings a light to `settings`. Power goes first, so
/// the levels land on a light that is on. A light that is off keeps its
/// levels pending until it is turned on again.
fn next_write(settings: Settings, power: Option<bool>, applied: Applied) -> Option<Write> {
    if let Some(on) = power.filter(|on| applied.power != Some(*on)) {
        return Some(Write::Power(on));
    }
    if applied.power == Some(false) {
        return None;
    }
    if applied.brightness != Some(settings.brightness) {
        return Some(Write::Brightness(settings.brightness));
    }
    if applied.temperature != Some(settings.temperature) {
        return Some(Write::Temperature(settings.temperature));
    }
    None
}

/// One attached light as the monitor sees it.
#[derive(Debug)]
struct Light {
    applied: Applied,
    /// False after a refused write, until a retry succeeds.
    reachable: bool,
    retry_at: Instant,
}

impl Light {
    fn new() -> Self {
        Self {
            applied: Applied::default(),
            reachable: true,
            retry_at: Instant::now(),
        }
    }
}

fn status(lights: &BTreeMap<String, Light>, store: &Store) -> Value {
    let lights: Vec<Value> = lights
        .iter()
        .filter(|(_, light)| light.reachable)
        .map(|(identity, light)| {
            let chosen = store.get(identity).copied();
            let settings = chosen.unwrap_or_default();
            json!({
                "device":identity,
                "mode":settings.mode.map_or("", Mode::name),
                "brightness":settings.brightness,
                "temperature":settings.temperature,
                "power":light.applied.power,
            })
        })
        .collect();
    json!({ "litraGlows": lights })
}

/// Settings sent over the status monitor's input, coalesced per light and
/// field so a dragged slider costs one pending value rather than a queue.
#[derive(Clone, Default)]
pub(crate) struct Requests(Arc<(Mutex<Pending>, Condvar)>);

/// Pending values by light identity and field.
type Pending = BTreeMap<(String, &'static str), String>;

impl Requests {
    /// Accepts `<identity> <mode|brightness|temperature> <value>`. The field
    /// and value are the last two words, so an identity may contain spaces;
    /// the value is checked when it is applied.
    pub(crate) fn submit(&self, request: &str) -> bool {
        let mut words = request.trim().rsplitn(3, ' ');
        let (Some(value), Some(key), Some(identity)) = (words.next(), words.next(), words.next())
        else {
            return false;
        };
        let Some(key) = ["mode", "brightness", "temperature"]
            .into_iter()
            .find(|name| *name == key)
        else {
            return false;
        };
        if !valid_identity(identity) {
            return false;
        }
        let (pending, ready) = &*self.0;
        let mut pending = pending.lock().unwrap();
        // Bounded: at most three fields for each light a request names.
        if pending.len() >= 64 && !pending.contains_key(&(identity.to_owned(), key)) {
            return false;
        }
        pending.insert((identity.to_owned(), key), value.to_owned());
        ready.notify_one();
        true
    }

    fn take(&self, timeout: Duration) -> Pending {
        let (pending, ready) = &*self.0;
        let pending = pending.lock().unwrap();
        let (mut pending, _) = ready
            .wait_timeout_while(pending, timeout, |pending| pending.is_empty())
            .unwrap();
        std::mem::take(&mut *pending)
    }
}

/// The lights' owner. `camera` is the PipeWire watcher's camera activity,
/// `None` while the graph is unknown; `publish` returns false once the
/// status stream has closed.
pub(crate) fn watch(
    camera: Arc<Mutex<Option<bool>>>,
    requests: Requests,
    publish: impl Fn(Value) -> bool,
) {
    let stop = shutdown_signal();
    let mut store = load();
    let mut stored_at = stamp();
    let mut unsaved: Option<Instant> = None;
    let mut lights: BTreeMap<String, Light> = BTreeMap::new();
    let mut probed: Option<Instant> = None;
    let mut observed: Option<bool> = None;
    let mut observed_since = Instant::now();
    let mut published = Value::Null;
    let mut announce = |value: Value| {
        if value == published {
            return true;
        }
        published = value.clone();
        publish(value)
    };
    while stop.load(Ordering::Relaxed) == 0 {
        let requested = requests.take(Duration::from_millis(100));
        for ((identity, key), value) in &requested {
            if update(&mut store, identity, key, value).is_ok() {
                unsaved.get_or_insert_with(Instant::now);
            }
            // The user is acting on this light now; do not make them wait
            // out a retry.
            if let Some(light) = lights.get_mut(identity) {
                light.retry_at = Instant::now();
            }
        }
        if unsaved.is_some_and(|since| since.elapsed() >= PERSIST_DELAY) {
            let _ = save(&store);
            stored_at = stamp();
            unsaved = None;
        } else if unsaved.is_none() && stamp() != stored_at {
            // Changed through `seele-control litra-glow` or by hand.
            stored_at = stamp();
            store = load();
            for light in lights.values_mut() {
                light.retry_at = Instant::now();
            }
        }
        if probed.is_none_or(|at| at.elapsed() >= PROBE_INTERVAL) {
            if let Some(found) = detect() {
                lights.retain(|identity, _| found.contains(identity));
                for identity in found {
                    lights.entry(identity).or_insert_with(Light::new);
                }
            }
            probed = Some(Instant::now());
        }
        let active = *camera.lock().unwrap();
        if active != observed {
            observed = active;
            observed_since = Instant::now();
        }
        let settled =
            observed.filter(|active| *active || observed_since.elapsed() >= CAMERA_SETTLE);
        if !announce(status(&lights, &store)) {
            return;
        }
        for (identity, light) in &mut lights {
            let Some(wanted) = store.get(identity).copied() else {
                continue;
            };
            if Instant::now() < light.retry_at {
                continue;
            }
            let power = target_power(wanted.mode, settled);
            while let Some(write) = next_write(wanted, power, light.applied) {
                if write.send(identity).is_err() {
                    light.reachable = false;
                    light.retry_at = Instant::now() + RETRY_INTERVAL;
                    break;
                }
                light.applied.record(write);
                light.reachable = true;
            }
        }
        if !announce(status(&lights, &store)) {
            return;
        }
    }
    if unsaved.is_some() {
        let _ = save(&store);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: Option<Mode>) -> Settings {
        Settings {
            mode,
            brightness: 60,
            temperature: 4500,
        }
    }

    /// Replays the monitor's write loop against a light that accepts every
    /// write, returning what it would send.
    fn converge(settings: Settings, power: Option<bool>, applied: &mut Applied) -> Vec<Write> {
        let mut writes = Vec::new();
        while let Some(write) = next_write(settings, power, *applied) {
            applied.record(write);
            writes.push(write);
            assert!(writes.len() <= 3, "writes never converge: {writes:?}");
        }
        writes
    }

    #[test]
    fn inventory_selects_every_glow_hid_route_in_a_stable_order() {
        let listing = "Litra Glow — serial:b (046d:c900 usage ff43:0202)\n  power: yes\nLitra Beam — beam-id (046d:c901 usage ff43:0202)\nLitra Glow — serial:a (046d:c900 usage ff43:0202)\n  brightness: 20–250 Lumens\nOther — wrong-interface (046d:c900 usage 0001:0001)\n";
        assert_eq!(glow_identities(listing), ["serial:a", "serial:b"]);
        assert!(glow_identities("No supported standalone lights found.\n").is_empty());
    }

    #[test]
    fn writes_name_the_light_and_stay_inside_its_ranges() {
        assert_eq!(
            Write::Brightness(60).arguments("serial:a"),
            [
                "light",
                "brightness",
                "--percent",
                "60",
                "--device",
                "serial:a"
            ]
        );
        assert_eq!(
            Write::Power(false).arguments("serial:a"),
            ["light", "off", "--device", "serial:a"]
        );
        let base = Settings::default();
        for (key, value) in [
            ("mode", "camera"),
            ("brightness", "1"),
            ("brightness", "100"),
            ("temperature", "2700"),
            ("temperature", "6500"),
        ] {
            assert!(base.with(key, value).is_ok(), "{key} {value}");
        }
        for (key, value) in [
            ("mode", "auto"),
            ("brightness", "0"),
            ("brightness", "101"),
            ("temperature", "2600"),
            ("temperature", "6550"),
            ("temperature", "6501"),
            ("power", "on"),
        ] {
            assert!(base.with(key, value).is_err(), "{key} {value}");
        }
    }

    #[test]
    fn each_light_keeps_its_own_settings() {
        let mut store = Store::new();
        update(&mut store, "serial:a", "mode", "camera").unwrap();
        update(&mut store, "serial:b", "brightness", "20").unwrap();
        assert!(update(&mut store, "", "mode", "on").is_err());
        assert!(update(&mut store, "serial:\nb", "mode", "on").is_err());
        assert_eq!(store["serial:a"].mode, Some(Mode::Camera));
        assert_eq!(store["serial:a"].brightness, DEFAULT_BRIGHTNESS);
        assert_eq!(store["serial:b"].mode, None);
        assert_eq!(store["serial:b"].brightness, 20);
        assert_eq!(parse_store(&store_json(&store).to_string()), store);
    }

    #[test]
    fn a_damaged_file_keeps_its_valid_fields() {
        let parsed = parse_store(
            r#"{"lights":{"serial:a":{"mode":"camera","brightness":400,"temperature":3000},"":{"mode":"on"}}}"#,
        );
        assert_eq!(parsed.len(), 1);
        let light = parsed["serial:a"];
        assert_eq!(light.mode, Some(Mode::Camera));
        assert_eq!(light.brightness, DEFAULT_BRIGHTNESS);
        assert_eq!(light.temperature, 3000);
        assert!(parse_store("not json").is_empty());
        assert!(parse_store(r#"{"mode":"on"}"#).is_empty());
    }

    #[test]
    fn an_unreachable_light_is_withdrawn_from_the_panel() {
        let mut lights = BTreeMap::new();
        lights.insert("serial:a".to_owned(), Light::new());
        let mut refused = Light::new();
        refused.reachable = false;
        lights.insert("serial:b".to_owned(), refused);
        let reported = status(&lights, &Store::new());
        let reported = reported["litraGlows"].as_array().unwrap();
        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0]["device"], "serial:a");
        assert_eq!(reported[0]["mode"], "");
    }

    #[test]
    fn turning_on_lands_on_the_shown_levels() {
        let mut applied = Applied::default();
        let wanted = settings(Some(Mode::On));
        assert_eq!(
            converge(wanted, Some(true), &mut applied),
            [
                Write::Power(true),
                Write::Brightness(60),
                Write::Temperature(4500)
            ]
        );
        assert!(converge(wanted, Some(true), &mut applied).is_empty());
    }

    #[test]
    fn levels_set_while_off_wait_for_the_light_to_come_on() {
        let mut applied = Applied::default();
        let mut wanted = settings(Some(Mode::Off));
        assert_eq!(
            converge(wanted, Some(false), &mut applied),
            [Write::Power(false)]
        );
        wanted.brightness = 80;
        assert!(converge(wanted, Some(false), &mut applied).is_empty());
        wanted.mode = Some(Mode::On);
        assert_eq!(
            converge(wanted, Some(true), &mut applied),
            [
                Write::Power(true),
                Write::Brightness(80),
                Write::Temperature(4500)
            ]
        );
    }

    #[test]
    fn an_unchosen_mode_leaves_power_alone() {
        assert_eq!(target_power(None, Some(true)), None);
        let mut applied = Applied::default();
        assert_eq!(
            converge(settings(None), None, &mut applied),
            [Write::Brightness(60), Write::Temperature(4500)]
        );
    }

    #[test]
    fn camera_mode_follows_only_a_known_camera() {
        assert_eq!(target_power(Some(Mode::Camera), Some(true)), Some(true));
        assert_eq!(target_power(Some(Mode::Camera), Some(false)), Some(false));
        // An unknown or settling camera never switches the light off.
        assert_eq!(target_power(Some(Mode::Camera), None), None);
        let mut applied = Applied {
            power: Some(true),
            brightness: Some(60),
            temperature: Some(4500),
        };
        assert!(converge(settings(Some(Mode::Camera)), None, &mut applied).is_empty());
        assert_eq!(applied.power, Some(true));
    }

    #[test]
    fn requests_coalesce_per_light_and_field_and_refuse_unknown_ones() {
        let requests = Requests::default();
        assert!(requests.submit("serial:a brightness 20"));
        assert!(requests.submit("serial:a brightness 70"));
        assert!(requests.submit("serial:b brightness 30"));
        assert!(requests.submit("serial:with space mode on"));
        assert!(!requests.submit("serial:a power on"));
        assert!(!requests.submit("mode on"));
        let taken = requests.take(Duration::ZERO);
        let value =
            |identity: &str, key| taken.get(&(identity.to_owned(), key)).map(String::as_str);
        assert_eq!(value("serial:a", "brightness"), Some("70"));
        assert_eq!(value("serial:b", "brightness"), Some("30"));
        assert_eq!(value("serial:with space", "mode"), Some("on"));
        assert_eq!(taken.len(), 3);
        assert!(requests.take(Duration::ZERO).is_empty());
    }
}
