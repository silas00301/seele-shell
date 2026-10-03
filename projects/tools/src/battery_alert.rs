//! Warn once when a device's battery runs low.
//!
//! The status monitor already reads every battery the shell draws: system
//! power supplies, OpenLogi devices and connected Bluetooth peripherals. This
//! policy watches the same list and says so when one of them crosses into low
//! or almost-empty territory, once per crossing. A device re-arms only after it
//! is seen charging or back at a comfortable level, so a reading that wobbles
//! around the threshold never repeats itself. What has already been said lives
//! in the private runtime directory: a shell reload does not repeat a warning,
//! and a reboot starts fresh.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// At or below this, a discharging device is low.
pub(crate) const LOW: u64 = 15;
/// At or below this it is almost empty, and the warning stays on screen.
pub(crate) const CRITICAL: u64 = 5;
/// A device has to climb back to here, or be seen charging, before it can warn
/// again.
pub(crate) const RECOVERED: u64 = 25;
const MAX_DEVICES: usize = 64;
const STATE_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Low = 1,
    Critical = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Alert {
    pub name: String,
    pub percent: u64,
    pub level: Level,
    pub icon: String,
}

impl Alert {
    /// `notify-send` arguments; the device name travels as data, never as a
    /// command line.
    pub(crate) fn arguments(&self) -> Vec<String> {
        let (urgency, title) = match self.level {
            Level::Low => ("normal", "Battery low"),
            Level::Critical => ("critical", "Battery almost empty"),
        };
        vec![
            "--app-name=Battery".into(),
            format!(
                "--icon={}",
                if self.icon.is_empty() {
                    "battery-caution"
                } else {
                    &self.icon
                }
            ),
            format!("--urgency={urgency}"),
            "--".into(),
            title.into(),
            format!("{} · {}%", self.name, self.percent),
        ]
    }
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[derive(Default, Debug)]
pub(crate) struct Watch {
    /// The deepest level already announced for each device since it last
    /// recovered.
    announced: BTreeMap<String, Level>,
}

impl Watch {
    pub(crate) fn load(path: &Path) -> Self {
        let announced = seele_runtime::fs::read_private(path, STATE_LIMIT)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Map<String, Value>>(&bytes).ok())
            .map(|entries| {
                entries
                    .into_iter()
                    .take(MAX_DEVICES)
                    .filter_map(|(key, level)| {
                        let level = match level.as_u64()? {
                            1 => Level::Low,
                            2 => Level::Critical,
                            _ => return None,
                        };
                        Some((clean(&key, 160), level))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { announced }
    }

    pub(crate) fn save(&self, path: &Path) -> crate::Result {
        let entries: Map<String, Value> = self
            .announced
            .iter()
            .map(|(key, level)| (key.clone(), json!(*level as u8)))
            .collect();
        crate::command::atomic_write(path, serde_json::to_string(&entries)?.as_bytes())
    }

    /// Compare one battery list with what has been said and return what is
    /// newly worth saying. The second value reports whether memory changed.
    pub(crate) fn observe(&mut self, batteries: &[Value]) -> (Vec<Alert>, bool) {
        let mut alerts = Vec::new();
        let mut changed = false;
        for battery in batteries.iter().take(MAX_DEVICES) {
            let name = clean(battery["name"].as_str().unwrap_or(""), 64);
            // The OpenLogi reading falls back to zero when it cannot parse a
            // level, so zero is unknown rather than empty.
            let Some(percent) = battery["percent"]
                .as_u64()
                .filter(|p| (1..=100).contains(p))
            else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let key = format!(
                "{}:{name}",
                clean(battery["kind"].as_str().unwrap_or(""), 16)
            );
            let status = battery["status"].as_str().unwrap_or("");
            let charging = matches!(status, "Charging" | "Full");
            if charging || percent >= RECOVERED {
                changed |= self.announced.remove(&key).is_some();
                continue;
            }
            let level = if percent <= CRITICAL {
                Level::Critical
            } else if percent <= LOW {
                Level::Low
            } else {
                continue;
            };
            if self.announced.get(&key).is_some_and(|said| *said >= level) {
                continue;
            }
            if !self.announced.contains_key(&key) && self.announced.len() >= MAX_DEVICES {
                continue;
            }
            self.announced.insert(key, level);
            changed = true;
            alerts.push(Alert {
                name,
                percent,
                level,
                icon: clean(battery["icon"].as_str().unwrap_or(""), 64),
            });
        }
        (alerts, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(name: &str, percent: u64, status: &str) -> Value {
        json!({"kind":"device","name":name,"percent":percent,"status":status,"icon":"input-mouse"})
    }

    fn levels(watch: &mut Watch, name: &str, percents: &[u64]) -> Vec<(u64, Level)> {
        percents
            .iter()
            .flat_map(|percent| watch.observe(&[battery(name, *percent, "")]).0)
            .map(|alert| (alert.percent, alert.level))
            .collect()
    }

    #[test]
    fn one_warning_per_crossing_with_hysteresis() {
        let mut watch = Watch::default();
        assert_eq!(
            levels(
                &mut watch,
                "Mouse",
                &[40, 20, 15, 14, 16, 15, 12, 5, 4, 6, 3]
            ),
            [(15, Level::Low), (5, Level::Critical)]
        );
        // Wobbling below the recovery point never re-arms.
        assert!(levels(&mut watch, "Mouse", &[24, 14, 24, 4]).is_empty());
        assert_eq!(levels(&mut watch, "Mouse", &[25, 15]), [(15, Level::Low)]);
    }

    #[test]
    fn charging_rearms_and_never_warns() {
        let mut watch = Watch::default();
        assert_eq!(levels(&mut watch, "Keys", &[10]).len(), 1);
        let (alerts, changed) = watch.observe(&[battery("Keys", 10, "Charging")]);
        assert!(alerts.is_empty() && changed);
        assert!(watch.observe(&[battery("Keys", 3, "Full")]).0.is_empty());
        assert_eq!(levels(&mut watch, "Keys", &[10]), [(10, Level::Low)]);
    }

    #[test]
    fn a_device_that_appears_low_warns_once_at_its_deepest_level() {
        let mut watch = Watch::default();
        assert_eq!(levels(&mut watch, "Pods", &[3, 12]), [(3, Level::Critical)]);
    }

    #[test]
    fn unknown_readings_and_devices_are_ignored() {
        let mut watch = Watch::default();
        let (alerts, changed) = watch.observe(&[
            json!({"kind":"logitech","name":"Parse failure","percent":0,"status":""}),
            json!({"kind":"device","name":"","percent":3}),
            json!({"kind":"device","name":"Text","percent":"3"}),
            json!({"kind":"device","name":"Over","percent":130}),
        ]);
        assert!(alerts.is_empty() && !changed);
    }

    #[test]
    fn devices_are_independent_and_bounded() {
        let mut watch = Watch::default();
        let many: Vec<_> = (0..100)
            .map(|i| battery(&format!("Device {i}"), 10, ""))
            .collect();
        let (alerts, _) = watch.observe(&many);
        assert_eq!(alerts.len(), MAX_DEVICES);
        assert_eq!(watch.announced.len(), MAX_DEVICES);
    }

    #[test]
    fn names_are_cleaned_and_passed_as_data() {
        let mut watch = Watch::default();
        let (alerts, _) = watch.observe(&[json!({"kind":"device","name":"Evil\u{202e}\n--icon=x","percent":9,"status":"","icon":""})]);
        let arguments = alerts[0].arguments();
        assert_eq!(arguments[1], "--icon=battery-caution");
        assert_eq!(arguments[3], "--");
        assert_eq!(arguments[5], "Evil--icon=x · 9%");
        assert_eq!(arguments[2], "--urgency=normal");
        let (alerts, _) = watch.observe(&[battery("Mouse", 2, "Discharging")]);
        assert_eq!(alerts[0].arguments()[2], "--urgency=critical");
        assert_eq!(alerts[0].arguments()[1], "--icon=input-mouse");
    }

    #[test]
    fn memory_survives_a_reload_and_rejects_foreign_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("battery-alerts.json");
        let mut watch = Watch::default();
        assert_eq!(levels(&mut watch, "Mouse", &[10]).len(), 1);
        watch.save(&path).unwrap();
        let mut reloaded = Watch::load(&path);
        assert!(levels(&mut reloaded, "Mouse", &[9]).is_empty());
        assert_eq!(levels(&mut reloaded, "Mouse", &[4]), [(4, Level::Critical)]);
        // Private files only, so these fail on their content, not their mode.
        let private = |bytes: &[u8]| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&path, bytes).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        private(b"{\"device:Mouse\": 2}");
        assert_eq!(Watch::load(&path).announced.len(), 1);
        private(b"{\"device:Mouse\": 7, \"x\": \"y\"}");
        assert!(Watch::load(&path).announced.is_empty());
        private(b"not json");
        assert!(Watch::load(&path).announced.is_empty());
        private(b"{\"device:Mouse\": 2}");
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o644))
            .unwrap();
        assert!(Watch::load(&path).announced.is_empty());
        assert!(Watch::load(&dir.path().join("absent")).announced.is_empty());
    }
}
