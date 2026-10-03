//! Units follow the locale's measurement system: metric unless the locale
//! says otherwise. Forecasts are fetched and cached in metric and converted
//! only for display, so the cache never depends on the locale.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Units {
    Metric,
    Imperial,
}

/// The measurement system the session's locale names. glibc answers that from
/// the locale's own `LC_MEASUREMENT` data, the system's record rather than a
/// list kept here; a locale glibc cannot load falls back to its territory, and
/// anything unknown is metric.
pub(super) fn system() -> Units {
    measurement().unwrap_or_else(|| from_locale(|name| std::env::var(name).ok()))
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn measurement() -> Option<Units> {
    // _NL_ITEM(LC_MEASUREMENT, 0): one byte, 1 for metric and 2 for US units.
    const MEASUREMENT: libc::nl_item = libc::LC_MEASUREMENT << 16;
    // SAFETY: newlocale reads the environment into a fresh locale object that
    // only this function uses; the answer is copied before the object is freed.
    // Neither touches the process-global locale, so this is thread-safe.
    unsafe {
        let locale = libc::newlocale(
            libc::LC_MEASUREMENT_MASK,
            c"".as_ptr(),
            std::ptr::null_mut(),
        );
        if locale.is_null() {
            return None;
        }
        let value = libc::nl_langinfo_l(MEASUREMENT, locale);
        let answer = if value.is_null() {
            None
        } else {
            match *value as u8 {
                1 => Some(Units::Metric),
                2 => Some(Units::Imperial),
                _ => None,
            }
        };
        libc::freelocale(locale);
        answer
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn measurement() -> Option<Units> {
    None
}

/// The POSIX precedence for one category: `LC_ALL`, then `LC_MEASUREMENT`,
/// then `LANG`. Without the locale's own data, the territory decides: the
/// United States, Liberia and Myanmar never officially adopted the metric
/// system, and everything else is metric.
pub(super) fn from_locale(lookup: impl Fn(&str) -> Option<String>) -> Units {
    let locale = ["LC_ALL", "LC_MEASUREMENT", "LANG"]
        .into_iter()
        .filter_map(&lookup)
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    let territory = locale
        .split(['.', '@'])
        .next()
        .and_then(|name| name.split_once('_'))
        .map(|(_, territory)| territory.to_ascii_uppercase());
    match territory.as_deref() {
        Some("US" | "LR" | "MM") => Units::Imperial,
        _ => Units::Metric,
    }
}

impl Units {
    /// A whole-degree reading with the degree sign. The unit letter is left
    /// out, as weather displays do; the locale already says which scale it is.
    pub(super) fn temperature(self, celsius: f64) -> String {
        let value = match self {
            Units::Metric => celsius,
            Units::Imperial => celsius * 9.0 / 5.0 + 32.0,
        };
        let rounded = value.round() as i64;
        // A true minus sign, and no "−0°" from a reading just under zero.
        if rounded < 0 {
            format!("\u{2212}{}°", -rounded)
        } else {
            format!("{rounded}°")
        }
    }

    /// Wind speed in this system's unit, to the whole number shown.
    pub(super) fn speed_value(self, kmh: f64) -> i64 {
        match self {
            Units::Metric => kmh.round() as i64,
            Units::Imperial => (kmh / 1.609_344).round() as i64,
        }
    }

    pub(super) fn speed(self, kmh: f64) -> String {
        let unit = match self {
            Units::Metric => "km/h",
            Units::Imperial => "mph",
        };
        format!("{} {unit}", self.speed_value(kmh))
    }
}

/// The eight-point compass direction wind blows from.
pub(super) fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let index = ((degrees.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8;
    POINTS[index]
}
