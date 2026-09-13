//! Gregorian civil-date arithmetic is independent of timezone/DST. Qt supplies
//! local Date fields and UTC epoch milliseconds at the thin binding boundary.
use crate::value::{array, finite, number, string, text, truthy};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(crate) fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let y = year - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    era * 146097 + y * 365 + y / 4 - y / 100 + (153 * m + 2) / 5 + day - 1 - 719468
}
pub(crate) fn civil(day: i64) -> (i64, i64, i64) {
    let z = day + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}
fn date(value: &Value) -> Option<(i64, i64, i64)> {
    value
        .get("epoch")?
        .as_f64()
        .filter(|n| n.abs() <= 8.64e15)?;
    let y = number(value.get("year"));
    let m = number(value.get("month"));
    let d = number(value.get("day"));
    if !(-271821.0..=275760.0).contains(&y)
        || !(1.0..=12.0).contains(&m)
        || !(1.0..=31.0).contains(&d)
        || y.fract() != 0.0
        || m.fract() != 0.0
        || d.fract() != 0.0
    {
        return None;
    }
    Some((y as i64, m as i64, d as i64))
}
fn stamp((y, m, d): (i64, i64, i64)) -> String {
    format!("{:0>4}-{m:02}-{d:02}", y.to_string())
}
fn week(day: i64) -> i64 {
    let thursday = day + 3 - (day + 3).rem_euclid(7);
    let year = civil(thursday).0;
    (thursday - days(year, 1, 1)) / 7 + 1
}
fn month(now: &Value, offset: Option<&Value>) -> Option<(i64, i64)> {
    let (y, m, _) = date(now)?;
    let y = if (0..=99).contains(&y) { y + 1900 } else { y };
    let offset = if truthy(offset) { number(offset) } else { 0.0 };
    if !offset.is_finite() || offset.abs() > 1_000_000.0 {
        return None;
    }
    let whole = (m as f64 - 1.0 + offset).trunc() as i64;
    Some((y + whole.div_euclid(12), whole.rem_euclid(12) + 1))
}
fn offset(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || !matches!(bytes[0], b'+' | b'-')
        || !bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let hours = i64::from(bytes[1] - b'0') * 10 + i64::from(bytes[2] - b'0');
    let minutes = i64::from(bytes[3] - b'0') * 10 + i64::from(bytes[4] - b'0');
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some((hours * 60 + minutes) * if bytes[0] == b'-' { -1 } else { 1 })
}
fn format_offset(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() == 5
        && matches!(bytes[0], b'+' | b'-')
        && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        format!("UTC{}:{}", &value[..3], &value[3..])
    } else {
        String::new()
    }
}
fn searchable(value: &str) -> String {
    value.to_lowercase().replace(['_', '/'], " ")
}
fn filtered(zones: &[Value], query: Option<&Value>) -> Vec<usize> {
    let query = searchable(&text(query));
    let words: Vec<_> = query
        .split(crate::value::is_space)
        .filter(|word| !word.is_empty())
        .collect();
    zones
        .iter()
        .enumerate()
        .filter_map(|(index, zone)| {
            let mut haystack = ["id", "zone", "label", "aliases", "flag", "abbreviation"]
                .map(|key| text(zone.get(key)))
                .join(" ");
            haystack.push(' ');
            haystack.push_str(&format_offset(&text(zone.get("offset"))));
            let haystack = searchable(&haystack);
            words
                .iter()
                .all(|word| haystack.contains(word))
                .then_some(index)
        })
        .collect()
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let now = args.first().unwrap_or(&null);
    Ok(match function {
        "calendarDate" => json!(date(now).map(stamp).unwrap_or_default()),
        "calendarCopyDate" => json!(if !truthy(now.get("week")) && truthy(now.get("inMonth")) {
            text(now.get("date"))
        } else {
            String::new()
        }),
        "sameDay" => json!(date(now).is_some() && date(now) == date(args.get(1).unwrap_or(&null))),
        "monthDate" => month(now, args.get(1))
            .map(|(y, m)| json!([y, m]))
            .unwrap_or(Value::Null),
        "isoWeek" => date(now)
            .map(|(y, m, d)| {
                json!(week(days(
                    if (0..=99).contains(&y) { y + 1900 } else { y },
                    m,
                    d
                )))
            })
            .unwrap_or(Value::Null),
        "calendarWeeks" | "calendarCells" => {
            let Some((y, m)) = month(now, args.get(1)) else {
                return Ok(if function == "calendarCells" {
                    json!([])
                } else {
                    Value::Null
                });
            };
            let first = days(y, m, 1);
            let monday = (first + 3).rem_euclid(7);
            // Subsequent Date constructors retain their 0..99 remapping even
            // when an earlier month overflow produced one of those years.
            let constructed = if (0..=99).contains(&y) { y + 1900 } else { y };
            let constructed_first = days(constructed, m, 1);
            let next = if m == 12 {
                days(constructed + 1, 1, 1)
            } else {
                days(constructed, m + 1, 1)
            };
            let weeks = (monday + next - constructed_first + 6) / 7;
            if function == "calendarWeeks" {
                json!(weeks)
            } else {
                let today = date(now);
                let mut cells = Vec::with_capacity(48);
                for row in 0..weeks {
                    let start = constructed_first - monday + row * 7;
                    let cursor = civil(start);
                    let cursor_year = if (0..=99).contains(&cursor.0) {
                        cursor.0 + 1900
                    } else {
                        cursor.0
                    };
                    let constructed_cursor = days(cursor_year, cursor.1, 1) + cursor.2 - 1;
                    cells.push(json!({"week":true,"label":week(constructed_cursor)}));
                    for column in 0..7 {
                        let value = civil(constructed_cursor + column);
                        let inside = value.0 == y && value.1 == m;
                        cells.push(json!({"week":false,"inMonth":inside,"day":if inside{value.2}else{0},"date":if inside{stamp(value)}else{String::new()},"today":inside&&Some(value)==today}));
                    }
                }
                json!(cells)
            }
        }
        "filterZones" => json!(filtered(array(args.first()), args.get(1))),
        "orderZones" => {
            let zones = array(args.first());
            let selected = filtered(zones, args.get(2));
            let mut result = Vec::new();
            let mut seen = HashSet::new();
            for pin in array(args.get(1)) {
                if let Some(index) = selected
                    .iter()
                    .copied()
                    .find(|i| zones[*i].get("id") == Some(pin))
                {
                    let key = string(zones[index].get("id"));
                    if seen.insert(key) {
                        result.push(index);
                    }
                }
            }
            for index in selected {
                if !seen.contains(&string(zones[index].get("id"))) {
                    result.push(index);
                }
            }
            json!(result)
        }
        "formatOffset" => json!(format_offset(&text(args.first()))),
        "offsetTime" | "clockTimestamp" => {
            let epoch = now.get("epoch").and_then(Value::as_f64);
            let raw = if function == "clockTimestamp"
                && args.get(1).is_some_and(|v| v["local"] == true)
            {
                let local = finite(now.get("offset"), 0.0);
                if local.abs() > 1439.0 {
                    return Ok(json!(""));
                }
                let local = local as i64;
                format!(
                    "{}{:02}{:02}",
                    if local < 0 { '-' } else { '+' },
                    local.abs() / 60,
                    local.abs() % 60
                )
            } else {
                text(args.get(1))
            };
            let Some(minutes) = offset(&raw) else {
                return Ok(json!(""));
            };
            let Some(epoch) = epoch.filter(|n| n.abs() <= 8.64e15) else {
                return Ok(json!(""));
            };
            let seconds = (epoch / 1000.0).floor() as i64 + minutes * 60;
            let day = seconds.div_euclid(86400);
            let clock = seconds.rem_euclid(86400);
            let (h, m, s) = (clock / 3600, clock % 3600 / 60, clock % 60);
            if function == "offsetTime" {
                json!(format!(
                    "{h:02}:{m:02}{}",
                    if truthy(args.get(2)) {
                        format!(":{s:02}")
                    } else {
                        String::new()
                    }
                ))
            } else {
                let date = civil(day);
                if !(0..=9999).contains(&date.0) {
                    json!("")
                } else {
                    json!(format!(
                        "{}T{h:02}:{m:02}:{s:02}{}:{}",
                        stamp(date),
                        &raw[..3],
                        &raw[3..]
                    ))
                }
            }
        }
        "moveCalendarDate" => {
            let Some(current) = date(now) else {
                return Ok(Value::Null);
            };
            let selected = if truthy(args.get(1)) {
                text(args.get(1))
            } else {
                stamp(current)
            };
            let parts: Vec<_> = selected.split('-').collect();
            if parts.len() != 3
                || parts[0].len() != 4
                || parts[1].len() != 2
                || parts[2].len() != 2
                || parts
                    .iter()
                    .any(|part| !part.bytes().all(|b| b.is_ascii_digit()))
            {
                return Ok(Value::Null);
            }
            let y = parts[0].parse::<i64>().unwrap_or(0);
            let m = parts[1].parse::<i64>().unwrap_or(0);
            let d = parts[2].parse::<i64>().unwrap_or(0);
            if !(1..=12).contains(&m) || !(1..=31).contains(&d) || civil(days(y, m, d)) != (y, m, d)
            {
                return Ok(Value::Null);
            }
            // The legacy constructor maps years 0..99 to 1900..1999 before
            // setFullYear restores the explicit year; preserve its leap-day check.
            if (0..=99).contains(&y) && civil(days(y + 1900, m, 1) + d - 1).1 != m {
                return Ok(Value::Null);
            }
            let step = number(args.get(2));
            if !step.is_finite() || step.fract() != 0.0 || step.abs() > 1_000_000.0 {
                return Ok(Value::Null);
            }
            let value = civil(days(y, m, d) + step as i64);
            let offset = (value.0 - current.0) * 12 + value.1 - current.1;
            if !(-60..=60).contains(&offset) {
                Value::Null
            } else {
                json!({"date":stamp(value),"monthOffset":offset})
            }
        }
        _ => return Err(format!("Unknown time function: {function}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now(year: i64, month: i64, day: i64) -> Value {
        json!({"epoch":days(year,month,day)*86400000,"year":year,"month":month,"day":day,"offset":0})
    }
    #[test]
    fn civil_math_handles_leap_centuries_and_round_trips() {
        for year in [-400, 0, 1, 99, 100, 1900, 2000, 2026, 2100, 2400] {
            for month in 1..=12 {
                for day in 1..=28 {
                    assert_eq!(civil(days(year, month, day)), (year, month, day));
                }
            }
        }
        for (year, count) in [(1900, 28), (2000, 29), (2100, 28), (2400, 29)] {
            let cells = call("calendarCells", &[now(year, 2, 15), json!(0)]).unwrap();
            assert_eq!(
                cells
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|cell| cell["inMonth"] == true)
                    .count(),
                count
            );
        }
    }
    #[test]
    fn legacy_year_constructor_and_navigation_are_preserved() {
        assert_eq!(
            call("monthDate", &[now(0, 2, 15), json!(0)]).unwrap(),
            json!([1900, 2])
        );
        assert_eq!(
            call("monthDate", &[now(100, 1, 15), json!(-1)]).unwrap(),
            json!([99, 12])
        );
        assert!(
            call("calendarCells", &[now(100, 1, 15), json!(-1)])
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .all(|cell| cell["inMonth"] != true)
        );
        assert_eq!(
            call(
                "moveCalendarDate",
                &[now(2026, 3, 28), json!("2026-03-28"), json!(1)]
            )
            .unwrap(),
            json!({"date":"2026-03-29","monthOffset":0})
        );
        assert_eq!(
            call(
                "moveCalendarDate",
                &[now(2026, 3, 28), json!("2026-02-29"), json!(1)]
            )
            .unwrap(),
            Value::Null
        );
    }
}
