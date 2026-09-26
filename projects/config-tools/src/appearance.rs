//! Light and dark: which preset each mode wears, which mode the desktop is in,
//! and when that mode changes by itself.
//!
//! The switcher in `themes.rs` publishes one theme at a time. This module owns
//! the choice of which one: a dark slot, a light slot and the mode that picks
//! between them, plus an optional schedule that flips the mode at fixed times
//! or at sunrise and sunset. The schedule is edge-triggered, so choosing a
//! mode by hand holds until the next boundary rather than being undone at the
//! next check.
//!
//! Sunrise and sunset need a place. It is never asked for and never stored:
//! the system timezone's own reference city in the tz database's
//! `zone1970.tab` stands in for it, which is close enough to decide when the
//! desktop turns dark and says nothing a timezone does not already say.
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dark,
    Light,
}
impl Mode {
    pub fn parse(value: &str) -> Option<Mode> {
        match value {
            "dark" => Some(Mode::Dark),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Off,
    Sun,
    Schedule,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Auto {
    pub source: Source,
    /// Local wall-clock times the schedule switches at, as `HH:MM`.
    pub light_at: String,
    pub dark_at: String,
    /// The boundary the schedule last acted on, as a Unix time. A boundary at
    /// or before it has already had its say.
    pub last: i64,
}
impl Default for Auto {
    fn default() -> Self {
        // The same hours the night light warms the screen at.
        Auto {
            source: Source::Off,
            light_at: "07:00".into(),
            dark_at: "19:00".into(),
            last: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    pub version: u32,
    pub dark: String,
    pub light: String,
    pub mode: Mode,
    pub auto: Auto,
}
impl Preferences {
    pub fn slot(&self, mode: Mode) -> &str {
        match mode {
            Mode::Dark => &self.dark,
            Mode::Light => &self.light,
        }
    }
    pub fn set_slot(&mut self, mode: Mode, id: String) {
        match mode {
            Mode::Dark => self.dark = id,
            Mode::Light => self.light = id,
        }
    }
    pub fn applied(&self) -> &str {
        self.slot(self.mode)
    }
}

/// Minutes after local midnight for a strict `HH:MM`.
pub fn clock(value: &str) -> Option<u32> {
    let (hours, minutes) = value.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    // Digits only: Rust's own integer parser would take "+7" for 7.
    if !hours
        .bytes()
        .chain(minutes.bytes())
        .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

/// Local calendar arithmetic, kept behind a trait so the schedule can be
/// tested against a fixed offset without touching the process's own `TZ`.
pub trait Calendar {
    /// The start of the local day `offset` days from the one holding `epoch`.
    fn day(&self, epoch: i64, offset: i64) -> i64;
    /// `minutes` of wall-clock time into the local day starting at `day`.
    fn at(&self, day: i64, minutes: u32) -> i64;
    /// The local `HH:MM` of `epoch`.
    fn clock(&self, epoch: i64) -> String;
}

/// The system's own local time, through `libc`, as `seele-clock` reads it.
pub struct Local;
impl Local {
    fn broken(epoch: i64) -> libc::tm {
        // SAFETY: localtime_r writes only the provided tm.
        unsafe {
            let raw = epoch as libc::time_t;
            let mut local: libc::tm = std::mem::zeroed();
            libc::localtime_r(&raw, &mut local);
            local
        }
    }
    fn make(mut local: libc::tm) -> i64 {
        local.tm_isdst = -1;
        // SAFETY: mktime normalises and reads only the provided tm.
        unsafe { libc::mktime(&mut local) as i64 }
    }
}
impl Calendar for Local {
    fn day(&self, epoch: i64, offset: i64) -> i64 {
        let mut local = Local::broken(epoch);
        local.tm_mday += offset as i32;
        local.tm_hour = 0;
        local.tm_min = 0;
        local.tm_sec = 0;
        Local::make(local)
    }
    fn at(&self, day: i64, minutes: u32) -> i64 {
        // Through the wall clock rather than by adding seconds, so a schedule
        // time on a daylight-saving day is still that time on the clock.
        let mut local = Local::broken(day);
        local.tm_hour = (minutes / 60) as i32;
        local.tm_min = (minutes % 60) as i32;
        local.tm_sec = 0;
        Local::make(local)
    }
    fn clock(&self, epoch: i64) -> String {
        let local = Local::broken(epoch);
        format!("{:02}:{:02}", local.tm_hour, local.tm_min)
    }
}

/// A boundary: from `at` on, the schedule wants `mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boundary {
    pub at: i64,
    pub mode: Mode,
}

/// What the schedule says about now: the mode it wants and since when, and
/// the next time it will change its mind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub mode: Mode,
    pub since: i64,
    pub next: Option<Boundary>,
}

const WINDOW_DAYS: i64 = 3;

fn plan_from(mut boundaries: Vec<Boundary>, now: i64, fallback: Mode) -> Plan {
    boundaries.sort_by_key(|boundary| boundary.at);
    let last = boundaries.iter().rev().find(|boundary| boundary.at <= now);
    let next = boundaries
        .iter()
        .find(|boundary| boundary.at > now)
        .copied();
    match last {
        Some(last) => Plan {
            mode: last.mode,
            since: last.at,
            next,
        },
        // Nothing has changed within the window: the polar summer or winter,
        // which holds from before the window began.
        None => Plan {
            mode: fallback,
            since: now - WINDOW_DAYS * 86_400,
            next,
        },
    }
}

/// The fixed-time schedule around `now`.
pub fn schedule(calendar: &dyn Calendar, now: i64, light_at: u32, dark_at: u32) -> Plan {
    let mut boundaries = vec![];
    for offset in -WINDOW_DAYS..=WINDOW_DAYS {
        let day = calendar.day(now, offset);
        boundaries.push(Boundary {
            at: calendar.at(day, light_at),
            mode: Mode::Light,
        });
        boundaries.push(Boundary {
            at: calendar.at(day, dark_at),
            mode: Mode::Dark,
        });
    }
    plan_from(boundaries, now, Mode::Dark)
}

/// The sun on one UTC date at one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sun {
    /// Sunrise and sunset, as Unix times.
    Rises(i64, i64),
    AlwaysUp,
    AlwaysDown,
}

fn utc_midnight(year: i32, month: i32, day: i32) -> i64 {
    // SAFETY: timegm normalises and reads only the provided tm.
    unsafe {
        let mut date: libc::tm = std::mem::zeroed();
        date.tm_year = year - 1900;
        date.tm_mon = month - 1;
        date.tm_mday = day;
        libc::timegm(&mut date) as i64
    }
}
fn utc_date(epoch: i64) -> (i32, i32, i32, i32) {
    // SAFETY: gmtime_r writes only the provided tm.
    let date = unsafe {
        let raw = epoch as libc::time_t;
        let mut date: libc::tm = std::mem::zeroed();
        libc::gmtime_r(&raw, &mut date);
        date
    };
    (
        date.tm_year + 1900,
        date.tm_mon + 1,
        date.tm_mday,
        date.tm_yday + 1,
    )
}

/// The sun's declination (radians) and the equation of time (minutes) at a
/// Julian day, by the algorithm behind NOAA's solar calculator, after Meeus.
fn position(julian_day: f64) -> (f64, f64) {
    let t = (julian_day - 2_451_545.0) / 36_525.0;
    let mean_longitude = (280.46646 + t * (36000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let anomaly = 357.52911 + t * (35999.05029 - 0.0001537 * t);
    let eccentricity = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
    let (anomaly_r, longitude_r) = (anomaly.to_radians(), mean_longitude.to_radians());
    let centre = anomaly_r.sin() * (1.914602 - t * (0.004817 + 0.000014 * t))
        + (2.0 * anomaly_r).sin() * (0.019993 - 0.000101 * t)
        + (3.0 * anomaly_r).sin() * 0.000289;
    let node = (125.04 - 1934.136 * t).to_radians();
    let apparent = (mean_longitude + centre - 0.00569 - 0.00478 * node.sin()).to_radians();
    let obliquity = (23.0
        + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0
        + 0.00256 * node.cos())
    .to_radians();
    let declination = (obliquity.sin() * apparent.sin()).asin();
    let y = (obliquity / 2.0).tan().powi(2);
    let equation_of_time = 4.0
        * (y * (2.0 * longitude_r).sin() - 2.0 * eccentricity * anomaly_r.sin()
            + 4.0 * eccentricity * y * anomaly_r.sin() * (2.0 * longitude_r).cos()
            - 0.5 * y * y * (4.0 * longitude_r).sin()
            - 1.25 * eccentricity * eccentricity * (2.0 * anomaly_r).sin())
        .to_degrees();
    (declination, equation_of_time)
}

/// Sunrise and sunset for the UTC date holding `epoch`, by the algorithm
/// behind NOAA's solar calculator. Each event is found from solar noon and
/// refined with the sun's position at the event itself. The standard 90.833°
/// zenith counts the sun as up once its upper limb clears a refracted horizon;
/// the result matches published tables to within a couple of minutes, far
/// closer than a theme needs.
pub fn sun(latitude: f64, longitude: f64, epoch: i64) -> Sun {
    let (year, month, day, _) = utc_date(epoch);
    let midnight = utc_midnight(year, month, day);
    let julian_midnight = midnight as f64 / 86_400.0 + 2_440_587.5;
    let latitude = latitude.to_radians();
    // Minutes after UTC midnight at which the sun crosses the horizon on the
    // side `side` (-1 rising, +1 setting), or which way it never does.
    let event = |side: f64| -> Result<f64, Sun> {
        let mut minutes = 720.0 - 4.0 * longitude;
        for _ in 0..3 {
            let (declination, equation_of_time) = position(julian_midnight + minutes / 1440.0);
            let cosine = 90.833_f64.to_radians().cos() / (latitude.cos() * declination.cos())
                - latitude.tan() * declination.tan();
            if cosine > 1.0 {
                return Err(Sun::AlwaysDown);
            }
            if cosine < -1.0 {
                return Err(Sun::AlwaysUp);
            }
            let hour_angle = cosine.acos().to_degrees();
            minutes = 720.0 - 4.0 * (longitude - side * hour_angle) - equation_of_time;
        }
        Ok(minutes)
    };
    match (event(-1.0), event(1.0)) {
        (Ok(rise), Ok(set)) => Sun::Rises(
            midnight + (rise * 60.0).round() as i64,
            midnight + (set * 60.0).round() as i64,
        ),
        (Err(polar), _) | (_, Err(polar)) => polar,
    }
}

/// The sunrise and sunset schedule around `now`.
pub fn daylight(latitude: f64, longitude: f64, now: i64) -> Plan {
    let mut boundaries = vec![];
    for offset in -WINDOW_DAYS..=WINDOW_DAYS {
        if let Sun::Rises(rise, set) = sun(latitude, longitude, now + offset * 86_400) {
            boundaries.push(Boundary {
                at: rise,
                mode: Mode::Light,
            });
            boundaries.push(Boundary {
                at: set,
                mode: Mode::Dark,
            });
        }
    }
    let fallback = match sun(latitude, longitude, now) {
        Sun::AlwaysUp => Mode::Light,
        _ => Mode::Dark,
    };
    plan_from(boundaries, now, fallback)
}

/// Where the sun is reckoned from: the timezone's reference city.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub zone: String,
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
}

fn zoneinfo() -> PathBuf {
    env::var_os("TZDIR").map(PathBuf::from).unwrap_or_else(|| {
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
    let target = fs::read_link("/etc/localtime").ok()?;
    let target = target.to_string_lossy();
    let (_, zone) = target.rsplit_once("zoneinfo/")?;
    (!zone.is_empty()).then(|| zone.to_owned())
}

/// ISO 6709 `±DDMM±DDDMM` or `±DDMMSS±DDDMMSS`, as the tz tables write it.
fn coordinates(value: &str) -> Option<(f64, f64)> {
    let split = value[1..].find(['+', '-'])? + 1;
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
        let Ok(text) = fs::read_to_string(directory.join(table)) else {
            continue;
        };
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

pub fn place() -> Option<Place> {
    place_in(&zoneinfo(), &zone()?)
}

/// What the schedule says now, or `None` while it is off or has no place.
pub fn plan(auto: &Auto, calendar: &dyn Calendar, place: Option<&Place>, now: i64) -> Option<Plan> {
    match auto.source {
        Source::Off => None,
        Source::Schedule => Some(schedule(
            calendar,
            now,
            clock(&auto.light_at)?,
            clock(&auto.dark_at)?,
        )),
        Source::Sun => place.map(|place| daylight(place.latitude, place.longitude, now)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A calendar with a fixed offset from UTC and no daylight saving.
    struct Fixed(i64);
    impl Calendar for Fixed {
        fn day(&self, epoch: i64, offset: i64) -> i64 {
            let local = epoch + self.0;
            (local - local.rem_euclid(86_400)) - self.0 + offset * 86_400
        }
        fn at(&self, day: i64, minutes: u32) -> i64 {
            day + i64::from(minutes) * 60
        }
        fn clock(&self, epoch: i64) -> String {
            let seconds = (epoch + self.0).rem_euclid(86_400);
            format!("{:02}:{:02}", seconds / 3600, seconds % 3600 / 60)
        }
    }
    fn at(year: i32, month: i32, day: i32, hour: i64, minute: i64) -> i64 {
        utc_midnight(year, month, day) + hour * 3600 + minute * 60
    }
    fn clock_of(epoch: i64) -> String {
        Fixed(0).clock(epoch)
    }

    #[test]
    fn a_schedule_time_is_strictly_hours_and_minutes() {
        assert_eq!(clock("07:00"), Some(420));
        assert_eq!(clock("23:59"), Some(1439));
        assert_eq!(clock("00:00"), Some(0));
        for bad in [
            "7:00", "24:00", "12:60", "12-00", "", "ab:cd", "07:00:00", "+7:00",
        ] {
            assert_eq!(clock(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_fixed_schedule_names_the_mode_now_and_the_next_change() {
        // UTC+2, light from 07:00 and dark from 19:00 local.
        let calendar = Fixed(2 * 3600);
        let morning = at(2026, 9, 26, 6, 30); // 08:30 local
        let plan = schedule(&calendar, morning, 420, 1140);
        assert_eq!(plan.mode, Mode::Light);
        assert_eq!(calendar.clock(plan.since), "07:00");
        let next = plan.next.unwrap();
        assert_eq!(
            (next.mode, calendar.clock(next.at)),
            (Mode::Dark, "19:00".into())
        );

        let late = at(2026, 9, 26, 22, 30); // 00:30 local, the next day
        let plan = schedule(&calendar, late, 420, 1140);
        assert_eq!(
            plan.mode,
            Mode::Dark,
            "after midnight is still the evening's dark"
        );
        assert_eq!(calendar.clock(plan.since), "19:00");
        assert_eq!(calendar.clock(plan.next.unwrap().at), "07:00");

        // A night-owl schedule whose dark starts after midnight.
        let plan = schedule(&calendar, at(2026, 9, 26, 22, 30), 600, 60);
        assert_eq!(plan.mode, Mode::Light, "00:30 is before the 01:00 dark");
    }

    #[test]
    fn sunrise_and_sunset_match_published_tables() {
        // Reference times from api.sunrise-sunset.org, in UTC. The equations
        // are held to three minutes, well inside what a theme can notice.
        let cases = [
            (
                "Berlin midsummer",
                52.52,
                13.405,
                (2026, 6, 21),
                "02:40",
                "19:35",
            ),
            (
                "Berlin midwinter",
                52.52,
                13.405,
                (2026, 12, 21),
                "07:12",
                "14:56",
            ),
            (
                "Berlin equinox",
                52.52,
                13.405,
                (2026, 3, 20),
                "05:06",
                "17:20",
            ),
            (
                "New York autumn",
                40.7128,
                -74.006,
                (2026, 9, 26),
                "10:46",
                "22:48",
            ),
        ];
        let minutes = |clock: &str| i64::from(super::clock(clock).unwrap());
        for (name, latitude, longitude, (year, month, day), rise, set) in cases {
            let Sun::Rises(sunrise, sunset) = sun(latitude, longitude, at(year, month, day, 12, 0))
            else {
                panic!("{name}: the sun rises");
            };
            for (got, want, what) in [(sunrise, rise, "sunrise"), (sunset, set, "sunset")] {
                let drift = (minutes(&clock_of(got)) - minutes(want)).abs();
                assert!(
                    drift <= 3,
                    "{name} {what}: {} against {want}",
                    clock_of(got)
                );
            }
        }
        // East of UTC the local morning is the previous UTC evening.
        let Sun::Rises(sunrise, sunset) = sun(-33.8688, 151.2093, at(2026, 6, 21, 12, 0)) else {
            panic!("Sydney: the sun rises");
        };
        assert!(
            sunrise < at(2026, 6, 21, 0, 0),
            "Sydney's sunrise falls on the previous UTC day"
        );
        assert!((minutes(&clock_of(sunrise)) - minutes("20:58")).abs() <= 3);
        assert!((minutes(&clock_of(sunset)) - minutes("06:55")).abs() <= 3);
    }

    #[test]
    fn the_polar_summer_and_winter_hold_without_a_boundary() {
        let (latitude, longitude) = (69.6496, 18.956); // Tromsø
        assert_eq!(
            sun(latitude, longitude, at(2026, 6, 21, 12, 0)),
            Sun::AlwaysUp
        );
        assert_eq!(
            sun(latitude, longitude, at(2026, 12, 21, 12, 0)),
            Sun::AlwaysDown
        );
        let summer = daylight(latitude, longitude, at(2026, 6, 21, 12, 0));
        assert_eq!((summer.mode, summer.next), (Mode::Light, None));
        let winter = daylight(latitude, longitude, at(2026, 12, 21, 12, 0));
        assert_eq!((winter.mode, winter.next), (Mode::Dark, None));
        // Near the equinox the same place has an ordinary day again.
        let spring = daylight(latitude, longitude, at(2026, 3, 20, 12, 0));
        assert_eq!(spring.mode, Mode::Light);
        assert_eq!(spring.next.map(|next| next.mode), Some(Mode::Dark));
    }

    #[test]
    fn the_sun_plan_follows_the_day() {
        let (latitude, longitude) = (52.52, 13.405);
        let noon = daylight(latitude, longitude, at(2026, 12, 21, 12, 0));
        assert_eq!(noon.mode, Mode::Light);
        assert_eq!(noon.next.unwrap().mode, Mode::Dark);
        let night = daylight(latitude, longitude, at(2026, 12, 21, 20, 0));
        assert_eq!(night.mode, Mode::Dark);
        let next = night.next.unwrap();
        assert_eq!(next.mode, Mode::Light);
        assert!(
            next.at > at(2026, 12, 22, 0, 0),
            "the next change is tomorrow's sunrise"
        );
    }

    #[test]
    fn a_timezone_names_its_reference_city() {
        let directory =
            std::env::temp_dir().join(format!("seele-zone-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("zone1970.tab"),
            "# comment\nDE,DK,NO,SE,SJ\t+5230+01322\tEurope/Berlin\tmost of Germany\n\
             US\t+404251-0740023\tAmerica/New_York\tEastern (most areas)\n\
             AR\t-3436-05827\tAmerica/Argentina/Buenos_Aires\tBuenos Aires (BA, CF)\n",
        )
        .unwrap();
        let berlin = place_in(&directory, "Europe/Berlin").unwrap();
        assert_eq!(berlin.label, "Berlin");
        assert!(
            (berlin.latitude - 52.5).abs() < 1e-9
                && (berlin.longitude - (13.0 + 22.0 / 60.0)).abs() < 1e-9
        );
        let new_york = place_in(&directory, "America/New_York").unwrap();
        assert!((new_york.latitude - (40.0 + 42.0 / 60.0 + 51.0 / 3600.0)).abs() < 1e-9);
        assert!(new_york.longitude < -74.0, "west is negative");
        let buenos_aires = place_in(&directory, "America/Argentina/Buenos_Aires").unwrap();
        assert_eq!(buenos_aires.label, "Buenos Aires");
        assert!(buenos_aires.latitude < 0.0, "south is negative");
        assert_eq!(
            place_in(&directory, "Etc/UTC"),
            None,
            "an offset zone has no city and no sun"
        );
        fs::remove_dir_all(&directory).unwrap();
        assert_eq!(coordinates("+5230"), None);
        assert_eq!(coordinates("5230+01322"), None);
    }

    #[test]
    fn the_plan_needs_a_source_and_for_the_sun_a_place() {
        let calendar = Fixed(0);
        let now = at(2026, 9, 26, 12, 0);
        let mut auto = Auto::default();
        assert_eq!(plan(&auto, &calendar, None, now), None, "off plans nothing");
        auto.source = Source::Schedule;
        assert_eq!(plan(&auto, &calendar, None, now).unwrap().mode, Mode::Light);
        auto.source = Source::Sun;
        assert_eq!(plan(&auto, &calendar, None, now), None, "no place, no sun");
        let place = Place {
            zone: "Europe/Berlin".into(),
            label: "Berlin".into(),
            latitude: 52.52,
            longitude: 13.405,
        };
        assert_eq!(
            plan(&auto, &calendar, Some(&place), now).unwrap().mode,
            Mode::Light
        );
        auto.source = Source::Schedule;
        auto.light_at = "7:00".into();
        assert_eq!(
            plan(&auto, &calendar, None, now),
            None,
            "an unreadable time plans nothing"
        );
    }
}
