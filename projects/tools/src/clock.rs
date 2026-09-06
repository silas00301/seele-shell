use crate::command::{atomic_write, output, state_home};
use crate::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{CStr, CString};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

unsafe extern "C" {
    fn tzset();
}
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct ZoneSource {
    id: String,
    zone: String,
    label: String,
    flag: String,
    aliases: String,
    kind: String,
}

#[derive(Serialize)]
struct Zone {
    id: String,
    zone: String,
    label: String,
    flag: String,
    aliases: String,
    kind: String,
    time: String,
    day: String,
    abbreviation: String,
    offset: String,
}

fn zoneinfo() -> PathBuf {
    env::var_os("TZDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/zoneinfo"))
}

fn flag(code: &str) -> String {
    if code.len() != 2 || !code.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return String::new();
    }
    code.bytes()
        .filter_map(|byte| char::from_u32(0x1f1e6 + u32::from(byte - b'A')))
        .collect()
}

fn sources() -> Result<Vec<ZoneSource>> {
    let directory = zoneinfo();
    let countries = fs::read_to_string(directory.join("iso3166.tab"))?;
    let countries: HashMap<&str, &str> = countries
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let table = fs::read_to_string(directory.join("zone1970.tab"))?;
    let mut zones = Vec::new();
    for line in table.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            continue;
        }
        let codes: Vec<&str> = fields[0].split(',').collect();
        let id = fields[2];
        let label = id.rsplit('/').next().unwrap_or(id).replace('_', " ");
        let names = codes
            .iter()
            .filter_map(|code| countries.get(code).copied())
            .collect::<Vec<_>>()
            .join(" ");
        let comment = fields.get(3).copied().unwrap_or("");
        zones.push(ZoneSource {
            id: id.into(),
            zone: id.into(),
            label,
            flag: flag(codes[0]),
            aliases: format!("{} {} {}", id.replace(['_', '/'], " "), names, comment),
            kind: "city".into(),
        });
    }
    zones.sort_by(|left, right| left.label.cmp(&right.label));
    zones.push(ZoneSource {
        id: "UTC".into(),
        zone: "UTC".into(),
        label: "Coordinated Universal Time".into(),
        flag: String::new(),
        aliases: "UTC GMT Zulu Coordinated Universal Time".into(),
        kind: "abbreviation".into(),
    });
    let etc = directory.join("Etc");
    if let Ok(entries) = fs::read_dir(etc) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(suffix) = name.strip_prefix("GMT") else {
                continue;
            };
            if suffix.len() < 2 || !matches!(suffix.as_bytes()[0], b'+' | b'-') {
                continue;
            }
            let Ok(hours) = suffix[1..].parse::<u8>() else {
                continue;
            };
            let sign = if suffix.starts_with('+') { '-' } else { '+' };
            let id = format!("UTC{sign}{hours}");
            let padded = format!("{hours:02}");
            zones.push(ZoneSource {
                id: id.clone(), zone: format!("Etc/{name}"), label: id.clone(), flag: String::new(), kind:"offset".into(),
                aliases: format!("UTC{sign}{hours} UTC{sign}{padded} UTC{sign}{hours}:00 UTC{sign}{padded}:00 GMT{sign}{hours} GMT{sign}{padded} GMT{sign}{hours}:00 GMT{sign}{padded}:00 {sign}{padded}00 {sign}{padded}:00")
            });
        }
    }
    Ok(zones)
}

fn format_time(epoch: i64, format: &str) -> String {
    unsafe {
        let raw = epoch as libc::time_t;
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&raw, &mut local);
        let pattern = CString::new(format).unwrap();
        let mut buffer = [0_i8; 128];
        libc::strftime(buffer.as_mut_ptr(), buffer.len(), pattern.as_ptr(), &local);
        CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

fn zone_time(epoch: i64) -> [String; 4] {
    let combined = format_time(epoch, "%H:%M\n%a %d %b\n%Z\n%z");
    let fields: Vec<_> = combined.split('\n').map(str::to_owned).collect();
    fields.try_into().unwrap_or_else(|_| {
        // Preserve the individual conversions for unusually long locale data
        // or a timezone abbreviation containing the separator.
        ["%H:%M", "%a %d %b", "%Z", "%z"].map(|pattern| format_time(epoch, pattern))
    })
}

fn seasonal_aliases(zones: &mut [ZoneSource], now: i64) {
    let directory = zoneinfo();
    let year = format_time(now, "%Y");
    let winter = output("date", ["-d", &format!("{year}-01-15 12:00"), "+%s"])
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(now);
    let summer = output("date", ["-d", &format!("{year}-07-15 12:00"), "+%s"])
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(now);
    let previous_tz = env::var_os("TZ");
    for source in zones {
        let timezone = if source.zone.contains('/') {
            format!(":{}", directory.join(&source.zone).display())
        } else {
            source.zone.clone()
        };
        env::set_var("TZ", timezone);
        unsafe {
            tzset();
        }
        let standard = format_time(winter, "%Z");
        let daylight = format_time(summer, "%Z");
        let mut aliases = Vec::new();
        let mut seen = HashSet::new();
        for word in format!("{} {standard} {daylight}", source.aliases).split_whitespace() {
            if seen.insert(word.to_ascii_lowercase()) {
                aliases.push(word.to_owned());
            }
        }
        source.aliases = aliases.join(" ");
    }
    restore_timezone(previous_tz);
}

fn restore_timezone(previous_tz: Option<std::ffi::OsString>) {
    if let Some(value) = previous_tz {
        env::set_var("TZ", value)
    } else {
        env::remove_var("TZ")
    }
    unsafe {
        tzset();
    }
}

// The packaged TZDIR is immutable. Also notice table/version replacements in
// a mutable database, and refresh seasonal aliases at a year/locale change.
#[derive(PartialEq)]
struct CatalogKey {
    directory: PathBuf,
    files: Vec<Option<(u64, SystemTime)>>,
    context: Vec<Option<std::ffi::OsString>>,
    year: String,
}

fn catalog_key(now: i64) -> CatalogKey {
    let directory = fs::canonicalize(zoneinfo()).unwrap_or_else(|_| zoneinfo());
    CatalogKey {
        files: ["", "iso3166.tab", "zone1970.tab", "tzdata.zi", "Etc"]
            .iter()
            .map(|name| {
                fs::metadata(directory.join(name))
                    .ok()
                    .and_then(|info| Some((info.len(), info.modified().ok()?)))
            })
            .collect(),
        directory,
        context: ["TZ", "LC_ALL", "LC_TIME", "LANG"]
            .iter()
            .map(env::var_os)
            .collect(),
        year: format_time(now, "%Y"),
    }
}

#[derive(Default)]
struct Catalog {
    key: Option<CatalogKey>,
    zones: Vec<ZoneSource>,
}

impl Catalog {
    fn snapshot(&mut self, now: i64) -> Result<Value> {
        let key = catalog_key(now);
        if self.key.as_ref() != Some(&key) {
            let mut zones = sources()?;
            seasonal_aliases(&mut zones, now);
            self.zones = zones;
            self.key = Some(key);
        }
        let previous_tz = env::var_os("TZ");
        let directory = zoneinfo();
        let zones: Vec<_> = self
            .zones
            .iter()
            .map(|source| {
                let timezone = if source.zone.contains('/') {
                    format!(":{}", directory.join(&source.zone).display())
                } else {
                    source.zone.clone()
                };
                env::set_var("TZ", timezone);
                unsafe {
                    tzset();
                }
                let [time, day, abbreviation, offset] = zone_time(now);
                Zone {
                    id: source.id.clone(),
                    zone: source.zone.clone(),
                    label: source.label.clone(),
                    flag: source.flag.clone(),
                    aliases: source.aliases.clone(),
                    kind: source.kind.clone(),
                    time,
                    day,
                    abbreviation,
                    offset,
                }
            })
            .collect();
        restore_timezone(previous_tz);
        Ok(json!({"pinned":pins(&self.zones),"zones":zones}))
    }
}

fn print_snapshot(catalog: &mut Catalog) -> Result {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", catalog.snapshot(now)?)?;
    stdout.flush()?;
    Ok(())
}

fn pin_path() -> PathBuf {
    state_home().join("seele-shell/timezone")
}

fn resolve(wanted: &str, zones: &[ZoneSource]) -> Option<String> {
    zones
        .iter()
        .find(|zone| zone.id.eq_ignore_ascii_case(wanted))
        .or_else(|| {
            zones
                .iter()
                .find(|zone| zone.zone.eq_ignore_ascii_case(wanted))
        })
        .map(|zone| zone.id.clone())
}

fn pins(zones: &[ZoneSource]) -> Vec<String> {
    let Ok(text) = fs::read_to_string(pin_path()) else {
        return Vec::new();
    };
    if let Ok(values) = serde_json::from_str::<Vec<Value>>(&text) {
        let mut result = Vec::new();
        for value in values.iter().filter_map(Value::as_str) {
            if !result.iter().any(|item| item == value) {
                result.push(value.to_owned());
            }
        }
        result
    } else {
        resolve(text.trim(), zones).into_iter().collect()
    }
}

fn write_pins(values: &[String]) -> Result {
    let path = pin_path();
    if values.is_empty() {
        let _ = fs::remove_file(path);
    } else {
        atomic_write(&path, serde_json::to_string(values)?.as_bytes())?;
    }
    Ok(())
}

pub fn run(arguments: &[String]) -> Result {
    let command = arguments.first().map(String::as_str).unwrap_or("list");
    match command {
        "list" => print_snapshot(&mut Catalog::default())?,
        "watch" => {
            let mut catalog = Catalog::default();
            print_snapshot(&mut catalog)?;
            for line in io::stdin().lock().lines() {
                if line?.trim() == "refresh" {
                    print_snapshot(&mut catalog)?;
                }
            }
        }
        "pin" | "unpin" => {
            let available = sources()?;
            let wanted = arguments.get(1).map(String::as_str).unwrap_or("");
            let id =
                resolve(wanted, &available).ok_or_else(|| format!("Unknown timezone: {wanted}"))?;
            let mut values = pins(&available);
            if command == "pin" {
                if !values.contains(&id) {
                    values.push(id);
                }
            } else {
                values.retain(|value| value != &id);
            }
            write_pins(&values)?;
        }
        _ => return Err("Usage: seele-clock [list|watch|pin TIMEZONE|unpin TIMEZONE]".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch(year: i32, month: i32, day: i32, hour: i32, minute: i32) -> i64 {
        let mut date: libc::tm = unsafe { std::mem::zeroed() };
        date.tm_year = year - 1900;
        date.tm_mon = month - 1;
        date.tm_mday = day;
        date.tm_hour = hour;
        date.tm_min = minute;
        unsafe { libc::timegm(&mut date) as i64 }
    }

    #[test]
    fn cached_metadata_keeps_live_dst_times_and_invalidates_with_database_and_year() {
        // Clock runs in its own single-threaded process. No other unit test
        // uses localtime/TZ; timestamps elsewhere use gmtime_r instead.
        let previous_tz = env::var_os("TZ");
        let previous_dir = env::var_os("TZDIR");
        let dir = env::temp_dir().join(format!("seele-clock-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("Etc")).unwrap();
        fs::write(dir.join("iso3166.tab"), "XX\tFixture\n").unwrap();
        fs::write(dir.join("zone1970.tab"), "XX\t+0000+00000\tUTC\tOriginal\n").unwrap();
        env::set_var("TZDIR", &dir);
        env::set_var("TZ", "UTC");
        unsafe {
            tzset();
        }
        let before = epoch(2026, 3, 8, 6, 59);
        let after = epoch(2026, 3, 8, 7, 0);
        let mut catalog = Catalog::default();
        let first = catalog.snapshot(before).unwrap();
        let allocation = catalog.zones.as_ptr();
        assert_eq!(first["zones"][0]["time"], "06:59");
        let next = catalog.snapshot(after).unwrap();
        assert_eq!(next["zones"][0]["time"], "07:00");
        assert_eq!(
            catalog.zones.as_ptr(),
            allocation,
            "metadata was rebuilt without a key change"
        );
        fs::write(
            dir.join("zone1970.tab"),
            "XX\t+0000+00000\tUTC\tReplacement comment\n",
        )
        .unwrap();
        assert!(catalog.snapshot(after).unwrap()["zones"][0]["aliases"]
            .as_str()
            .unwrap()
            .contains("Replacement comment"));
        catalog.snapshot(epoch(2027, 1, 1, 0, 0)).unwrap();
        assert_eq!(catalog.key.as_ref().unwrap().year, "2027");
        // POSIX rules give the test a real DST transition without depending
        // on which timezone files the test machine has installed.
        catalog.key = Some(catalog_key(before));
        catalog.zones = vec![ZoneSource {
            id: "fixture".into(),
            zone: "EST5EDT,M3.2.0,M11.1.0".into(),
            label: "Fixture".into(),
            flag: String::new(),
            aliases: "EST EDT".into(),
            kind: "city".into(),
        }];
        let first = catalog.snapshot(before).unwrap();
        let next = catalog.snapshot(after).unwrap();
        assert_eq!(first["zones"][0]["time"], "01:59");
        assert_eq!(next["zones"][0]["time"], "03:00");
        assert_eq!(first["zones"][0]["offset"], "-0500");
        assert_eq!(next["zones"][0]["offset"], "-0400");
        assert_eq!(first["zones"][0]["aliases"], next["zones"][0]["aliases"]);
        assert_eq!(env::var("TZ").unwrap(), "UTC");
        restore_timezone(previous_tz);
        if let Some(value) = previous_dir {
            env::set_var("TZDIR", value);
        } else {
            env::remove_var("TZDIR");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
