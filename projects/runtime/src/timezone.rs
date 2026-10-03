//! The system timezone and its reference city in the tz database.
//!
//! Some features need a place without asking for one: the theme switcher
//! reckons sunrise and sunset from it, and the weather worker uses it as its
//! default location. The tz database's `zone1970.tab` names one reference city
//! for each zone with its coordinates to the arc minute, which is close enough
//! for either and says nothing a timezone does not already say. Nothing here
//! asks for, derives or stores a more precise location.
use std::{
    env,
    path::{Path, PathBuf},
};

/// The tz tables are a few dozen kilobytes; anything far larger is not one.
const MAX_TABLE: usize = 1024 * 1024;

/// A timezone's reference city.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// The IANA zone, such as `Europe/Berlin`.
    pub zone: String,
    /// The zone's city, such as `Berlin`.
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// The directory holding the tz tables: `TZDIR`, else the first standard
/// location that carries `zone1970.tab`.
pub fn zoneinfo() -> PathBuf {
    env::var_os("TZDIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            ["/etc/zoneinfo", "/usr/share/zoneinfo"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.join("zone1970.tab").is_file())
                .unwrap_or_else(|| PathBuf::from("/usr/share/zoneinfo"))
        })
}

/// The system timezone's name: `TZ` when it names a zone, else the zone
/// `/etc/localtime` links into.
pub fn zone() -> Option<String> {
    if let Some(value) = env::var_os("TZ") {
        let value = value.to_string_lossy();
        let value = value.trim_start_matches(':');
        if !value.is_empty() && !value.starts_with('/') {
            return Some(value.to_owned());
        }
    }
    let target = std::fs::read_link("/etc/localtime").ok()?;
    let target = target.to_string_lossy();
    let (_, zone) = target.rsplit_once("zoneinfo/")?;
    (!zone.is_empty()).then(|| zone.to_owned())
}

/// ISO 6709 `±DDMM±DDDMM` or `±DDMMSS±DDDMMSS`, as the tz tables write it.
fn coordinates(value: &str) -> Option<(f64, f64)> {
    let split = value.get(1..)?.find(['+', '-'])? + 1;
    let angle = |part: &str, degree_digits: usize| -> Option<f64> {
        let sign = match part.as_bytes().first()? {
            b'+' => 1.0,
            b'-' => -1.0,
            _ => return None,
        };
        let digits = &part[1..];
        if !digits.bytes().all(|byte| byte.is_ascii_digit())
            || !(digits.len() == degree_digits + 2 || digits.len() == degree_digits + 4)
        {
            return None;
        }
        let degrees: f64 = digits[..degree_digits].parse().ok()?;
        let minutes: f64 = digits[degree_digits..degree_digits + 2].parse().ok()?;
        let seconds: f64 = digits
            .get(degree_digits + 2..)
            .filter(|rest| !rest.is_empty())
            .map_or(Some(0.0), |rest| rest.parse().ok())?;
        Some(sign * (degrees + minutes / 60.0 + seconds / 3600.0))
    };
    Some((angle(&value[..split], 2)?, angle(&value[split..], 3)?))
}

/// The reference city of `zone` from the tz tables under `directory`.
pub fn place_in(directory: &Path, zone: &str) -> Option<Place> {
    for table in ["zone1970.tab", "zone.tab"] {
        let Ok(bytes) = crate::fs::read_bounded(&directory.join(table), MAX_TABLE, false) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() >= 3 && fields[2] == zone {
                let (latitude, longitude) = coordinates(fields[1])?;
                let label = zone.rsplit('/').next().unwrap_or(zone).replace('_', " ");
                return Some(Place {
                    zone: zone.to_owned(),
                    label,
                    latitude,
                    longitude,
                });
            }
        }
    }
    None
}

/// The system timezone's reference city, or `None` for a zone without one
/// such as `Etc/UTC`.
pub fn place() -> Option<Place> {
    place_in(&zoneinfo(), &zone()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_timezone_names_its_reference_city() {
        let directory = tempfile::tempdir().unwrap();
        let directory = directory.path();
        fs::write(
            directory.join("zone1970.tab"),
            "# comment\nDE,DK,NO,SE,SJ\t+5230+01322\tEurope/Berlin\tmost of Germany\n\
             US\t+404251-0740023\tAmerica/New_York\tEastern (most areas)\n\
             AR\t-3436-05827\tAmerica/Argentina/Buenos_Aires\tBuenos Aires (BA, CF)\n",
        )
        .unwrap();
        let berlin = place_in(directory, "Europe/Berlin").unwrap();
        assert_eq!(berlin.label, "Berlin");
        assert!(
            (berlin.latitude - 52.5).abs() < 1e-9
                && (berlin.longitude - (13.0 + 22.0 / 60.0)).abs() < 1e-9
        );
        let new_york = place_in(directory, "America/New_York").unwrap();
        assert!((new_york.latitude - (40.0 + 42.0 / 60.0 + 51.0 / 3600.0)).abs() < 1e-9);
        assert!(new_york.longitude < -74.0, "west is negative");
        let buenos_aires = place_in(directory, "America/Argentina/Buenos_Aires").unwrap();
        assert_eq!(buenos_aires.label, "Buenos Aires");
        assert!(buenos_aires.latitude < 0.0, "south is negative");
        assert_eq!(
            place_in(directory, "Etc/UTC"),
            None,
            "an offset zone has no city and no sun"
        );
        assert_eq!(coordinates("+5230"), None);
        assert_eq!(coordinates("5230+01322"), None);
        assert_eq!(coordinates(""), None);
    }

    #[test]
    fn the_older_table_answers_when_the_newer_one_lacks_a_zone() {
        let directory = tempfile::tempdir().unwrap();
        let directory = directory.path();
        fs::write(
            directory.join("zone1970.tab"),
            "DE\t+5230+01322\tEurope/Berlin\n",
        )
        .unwrap();
        fs::write(
            directory.join("zone.tab"),
            "IS\t+6409-02151\tAtlantic/Reykjavik\n",
        )
        .unwrap();
        assert_eq!(
            place_in(directory, "Atlantic/Reykjavik").unwrap().label,
            "Reykjavik"
        );
        let missing = tempfile::tempdir().unwrap();
        assert_eq!(place_in(missing.path(), "Europe/Berlin"), None);
    }
}
