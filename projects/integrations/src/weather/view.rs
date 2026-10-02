//! What the popup draws, decided here: conditions and their glyphs, local
//! times at the place, the strip of coming hours, the week's rows and their
//! shared temperature scale. QML receives finished labels and ratios.
use super::*;
use chrono::{FixedOffset, Offset};
use chrono_tz::Tz;

/// Hours in the strip, starting with the current one.
pub(super) const HOURS_SHOWN: usize = 8;
pub(super) const DAYS_SHOWN: usize = 7;
/// A chance of rain below this is not worth a number.
pub(super) const RAIN_SHOWN: f64 = 20.0;
/// A reading this old stops standing for "now"; the hourly forecast does.
pub(super) const CURRENT_FOR: i64 = 3600;
/// The narrowest span a day's bar is drawn with, so a day whose low and high
/// round together still shows where on the week's scale it sits.
pub(super) const MIN_SPAN: f64 = 0.04;

/// A WMO weather interpretation code as Open-Meteo reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Condition {
    pub(super) label: &'static str,
    pub(super) glyph: &'static str,
    /// `sun` lights the glyph in the theme's warm colour; everything else is quiet.
    pub(super) tone: &'static str,
}

pub(super) fn condition(code: u8, day: bool) -> Condition {
    const SUN: &str = "\u{f0599}";
    const MOON: &str = "\u{f0594}";
    const PARTLY: &str = "\u{f0595}";
    const PARTLY_NIGHT: &str = "\u{f0f31}";
    const CLOUD: &str = "\u{f0590}";
    const FOG: &str = "\u{f0591}";
    const RAIN: &str = "\u{f0597}";
    const POURING: &str = "\u{f0596}";
    const SHOWERS: &str = "\u{f0f33}";
    const SLEET: &str = "\u{f067f}";
    const SNOW: &str = "\u{f0598}";
    const HEAVY_SNOW: &str = "\u{f0f36}";
    const SNOW_SHOWERS: &str = "\u{f0f34}";
    const STORM: &str = "\u{f067e}";
    const HAIL: &str = "\u{f0592}";
    const UNKNOWN: &str = "\u{f0f2f}";
    let (label, glyph) = match code {
        0 => ("Clear", if day { SUN } else { MOON }),
        1 => ("Mainly clear", if day { SUN } else { MOON }),
        2 => ("Partly cloudy", if day { PARTLY } else { PARTLY_NIGHT }),
        3 => ("Overcast", CLOUD),
        45 => ("Fog", FOG),
        48 => ("Rime fog", FOG),
        51 => ("Light drizzle", RAIN),
        53 => ("Drizzle", RAIN),
        55 => ("Heavy drizzle", RAIN),
        56 => ("Light freezing drizzle", SLEET),
        57 => ("Freezing drizzle", SLEET),
        61 => ("Light rain", RAIN),
        63 => ("Rain", RAIN),
        65 => ("Heavy rain", POURING),
        66 => ("Light freezing rain", SLEET),
        67 => ("Freezing rain", SLEET),
        71 => ("Light snow", SNOW),
        73 => ("Snow", SNOW),
        75 => ("Heavy snow", HEAVY_SNOW),
        77 => ("Snow grains", SNOW),
        80 => ("Light showers", if day { SHOWERS } else { RAIN }),
        81 => ("Showers", RAIN),
        82 => ("Violent showers", POURING),
        85 => ("Light snow showers", if day { SNOW_SHOWERS } else { SNOW }),
        86 => ("Snow showers", HEAVY_SNOW),
        95 => ("Thunderstorm", STORM),
        96 => ("Thunderstorm with hail", HAIL),
        99 => ("Thunderstorm with heavy hail", HAIL),
        _ => ("Unknown conditions", UNKNOWN),
    };
    Condition {
        label,
        glyph,
        tone: if day && code <= 1 { "sun" } else { "plain" },
    }
}

/// Local time at the place: its named zone when Open-Meteo sent one chrono
/// knows, so a DST change inside the week lands on the right hour, else the
/// fixed offset it reported for the moment of the fetch.
pub(super) struct Clock {
    zone: Option<Tz>,
    offset: FixedOffset,
}

impl Clock {
    pub(super) fn of(forecast: &Forecast) -> Self {
        Clock {
            zone: forecast.timezone.parse::<Tz>().ok(),
            offset: FixedOffset::east_opt(forecast.utc_offset)
                .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC")),
        }
    }

    pub(super) fn local(&self, epoch: i64) -> Option<DateTime<FixedOffset>> {
        let instant = Utc.timestamp_opt(epoch, 0).single()?;
        Some(match self.zone {
            Some(zone) => {
                let local = instant.with_timezone(&zone);
                local.with_timezone(&local.offset().fix())
            }
            None => instant.with_timezone(&self.offset),
        })
    }

    pub(super) fn hm(&self, epoch: i64) -> String {
        self.local(epoch)
            .map(|v| v.format("%H:%M").to_string())
            .unwrap_or_default()
    }

    pub(super) fn date(&self, epoch: i64) -> Option<NaiveDate> {
        self.local(epoch).map(|v| v.date_naive())
    }
}

/// The day row holding `now`: the last one starting at or before it, as long
/// as it is still the same local date at the place.
fn today_index(forecast: &Forecast, clock: &Clock, now: i64) -> Option<usize> {
    let date = clock.date(now)?;
    forecast
        .days
        .iter()
        .position(|day| clock.date(day.time) == Some(date))
}

fn rain(probability: Option<f64>) -> String {
    match probability {
        Some(value) if value >= RAIN_SHOWN => format!("{}%", value.round() as i64),
        _ => String::new(),
    }
}

/// What "now" is: the current reading while it is recent, else the hourly
/// forecast for this hour, which a cached forecast still holds while offline.
pub(super) fn reading(forecast: &Forecast, now: i64) -> Option<(f64, u8, bool, Option<&Current>)> {
    if let Some(current) = forecast
        .current
        .as_ref()
        .filter(|c| (now - c.time).abs() <= CURRENT_FOR)
    {
        return Some((
            current.temperature,
            current.code,
            current.day,
            Some(current),
        ));
    }
    forecast
        .hours
        .iter()
        .find(|hour| hour.time <= now && now < hour.time + 3600)
        .map(|hour| (hour.temperature, hour.code, hour.day, None))
}

pub(super) fn current(forecast: &Forecast, now: i64, units: Units) -> Value {
    let Some((temperature, code, day, reading)) = self::reading(forecast, now) else {
        return Value::Null;
    };
    let clock = Clock::of(forecast);
    let condition = condition(code, day);
    let today = today_index(forecast, &clock, now).map(|index| &forecast.days[index]);
    let mut facts = Vec::new();
    let mut fact = |glyph: &str, text: String, name: String| {
        facts.push(json!({"glyph": glyph, "text": text, "name": name}));
    };
    if let Some(reading) = reading {
        if let Some(apparent) = reading.apparent {
            let value = units.temperature(apparent);
            fact(
                "\u{f050f}",
                format!("Feels {value}"),
                format!("Feels like {value}"),
            );
        }
        if let Some(kmh) = reading.wind_speed {
            // Calm air blows from nowhere, so it carries no direction.
            let text = match reading.wind_direction {
                Some(direction) if units.speed_value(kmh) > 0 => {
                    format!("{} {}", units.speed(kmh), compass(direction))
                }
                _ => units.speed(kmh),
            };
            fact("\u{f059d}", text.clone(), format!("Wind {text}"));
        }
        if let Some(humidity) = reading.humidity {
            let value = format!("{}%", humidity.round() as i64);
            fact("\u{f058e}", value.clone(), format!("Humidity {value}"));
        }
    }
    if let Some(today) = today {
        if let Some(rise) = today.sunrise {
            let at = clock.hm(rise);
            fact("\u{f059c}", at.clone(), format!("Sunrise {at}"));
        }
        if let Some(set) = today.sunset {
            let at = clock.hm(set);
            fact("\u{f059b}", at.clone(), format!("Sunset {at}"));
        }
    }
    json!({
        "glyph": condition.glyph,
        "tone": condition.tone,
        "condition": condition.label,
        "temperature": units.temperature(temperature),
        "high": today.map(|d| units.temperature(d.high)).unwrap_or_default(),
        "low": today.map(|d| units.temperature(d.low)).unwrap_or_default(),
        "facts": facts,
    })
}

pub(super) fn hours(forecast: &Forecast, now: i64, units: Units) -> Value {
    let clock = Clock::of(forecast);
    let live = forecast
        .current
        .as_ref()
        .filter(|c| (now - c.time).abs() <= CURRENT_FOR);
    Value::Array(
        forecast
            .hours
            .iter()
            .filter(|hour| hour.time + 3600 > now)
            .take(HOURS_SHOWN)
            .map(|hour| {
                let present = hour.time <= now;
                // The strip's first column agrees with the headline above it.
                let (temperature, code, day) = match live {
                    Some(current) if present => (current.temperature, current.code, current.day),
                    _ => (hour.temperature, hour.code, hour.day),
                };
                let condition = condition(code, day);
                let label = if present {
                    "Now".to_owned()
                } else {
                    clock.hm(hour.time)
                };
                json!({
                    "label": label,
                    "glyph": condition.glyph,
                    "tone": condition.tone,
                    "condition": condition.label,
                    "temperature": units.temperature(temperature),
                    "rain": rain(hour.precipitation),
                })
            })
            .collect(),
    )
}

/// The week from today, each row placed on one temperature scale shared by
/// the whole week, so the bars can be compared down the list.
pub(super) fn days(forecast: &Forecast, now: i64, units: Units) -> Value {
    let clock = Clock::of(forecast);
    let Some(start) = today_index(forecast, &clock, now) else {
        return json!([]);
    };
    let shown: Vec<&Day> = forecast.days[start..].iter().take(DAYS_SHOWN).collect();
    let low = shown.iter().map(|d| d.low).fold(f64::INFINITY, f64::min);
    let high = shown
        .iter()
        .map(|d| d.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = high - low;
    let ratio = |value: f64| ((value - low) / span * 1000.0).round() / 1000.0;
    Value::Array(
        shown
            .iter()
            .enumerate()
            .map(|(index, day)| {
                // The middle of the day decides its weekday, whatever the hour of midnight.
                let local = clock.local(day.time + 12 * 3600);
                let condition = condition(day.code, true);
                let (mut from, mut to) = if span > 0.0 {
                    (ratio(day.low), ratio(day.high))
                } else {
                    (0.0, 1.0)
                };
                if to - from < MIN_SPAN {
                    to = (from + MIN_SPAN).min(1.0);
                    from = to - MIN_SPAN;
                }
                json!({
                    "label": if index == 0 { "Today".to_owned() } else {
                        local.map(|v| v.format("%a").to_string()).unwrap_or_default()
                    },
                    "name": local.map(|v| v.format("%A %-d %B").to_string()).unwrap_or_default(),
                    "glyph": condition.glyph,
                    "tone": condition.tone,
                    "condition": condition.label,
                    "high": units.temperature(day.high),
                    "low": units.temperature(day.low),
                    "rain": rain(day.precipitation),
                    "from": from,
                    "to": to,
                })
            })
            .collect(),
    )
}

/// When the forecast on screen was fetched, in this machine's own time.
pub(super) fn fetched(at: i64, now: i64) -> String {
    let (Some(at_local), Some(now_local)) = (
        Local.timestamp_opt(at, 0).single(),
        Local.timestamp_opt(now, 0).single(),
    ) else {
        return String::new();
    };
    if at_local.date_naive() == now_local.date_naive() {
        at_local.format("%H:%M").to_string()
    } else {
        at_local.format("%-d %b %H:%M").to_string()
    }
}
