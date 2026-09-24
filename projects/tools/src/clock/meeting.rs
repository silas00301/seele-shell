//! Absolute-time meeting planning. UTC is the input axis, so neither a skipped
//! wall time nor either occurrence of a repeated wall time is ever guessed.
use super::*;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Request {
    date: String,
    minute: i64,
    duration: i64,
    shift: i64,
}

pub(super) struct Cache {
    day: i64,
    duration: i64,
    pins: Vec<String>,
    slots: Vec<Vec<bool>>,
}

// Build one prefix table for a zone/day. Every quarter-hour interval is then
// a constant-time query instead of duration separate localtime conversions.
fn slots(day: i64, duration: i64) -> Vec<bool> {
    let mut prefix = vec![0_u32];
    for minute in 0..1440 + duration {
        prefix.push(prefix.last().unwrap() + u32::from(!working(day + minute * 60)));
    }
    (0..96)
        .map(|slot| {
            let start = slot * 15;
            let end = start + duration as usize;
            prefix[end] == prefix[start] && working(day + end as i64 * 60 - 1)
        })
        .collect()
}

fn utc(epoch: i64, pattern: &str) -> String {
    unsafe {
        let raw = epoch as libc::time_t;
        let mut date: libc::tm = std::mem::zeroed();
        libc::gmtime_r(&raw, &mut date);
        let pattern = CString::new(pattern).unwrap();
        let mut buffer = [0_i8; 128];
        libc::strftime(buffer.as_mut_ptr(), buffer.len(), pattern.as_ptr(), &date);
        CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

fn day_epoch(value: &str) -> Result<i64> {
    if value.len() != 10
        || !value.bytes().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
    {
        return Err("Enter a UTC date as YYYY-MM-DD".into());
    }
    let year: i32 = value[..4].parse()?;
    if !(1970..=2100).contains(&year) {
        return Err("Choose a date between 1970 and 2100".into());
    }
    let mut date: libc::tm = unsafe { std::mem::zeroed() };
    date.tm_year = year - 1900;
    date.tm_mon = value[5..7].parse::<i32>()? - 1;
    date.tm_mday = value[8..].parse()?;
    let epoch = unsafe { libc::timegm(&mut date) as i64 };
    if utc(epoch, "%Y-%m-%d") != value {
        return Err("That calendar date does not exist".into());
    }
    Ok(epoch)
}

fn working(epoch: i64) -> bool {
    unsafe {
        let raw = epoch as libc::time_t;
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&raw, &mut local);
        (1..=5).contains(&local.tm_wday) && (9..17).contains(&local.tm_hour)
    }
}

// Check the entire interval, not just its start. Minute resolution includes
// half-hour DST changes, fractional UTC offsets, and the final boundary.
fn available(epoch: i64, duration: i64) -> bool {
    (0..duration).all(|minute| working(epoch + minute * 60)) && working(epoch + duration * 60 - 1)
}

fn offset_label(offset: &str) -> String {
    if offset.len() == 5 && offset.is_ascii() {
        format!("UTC{}:{}", &offset[..3], &offset[3..])
    } else {
        format!("UTC{offset}")
    }
}

fn row(id: &str, label: &str, epoch: i64, duration: i64, slots: &[bool]) -> Value {
    let [time, date, abbreviation, offset] = zone_time(epoch);
    let [end_time, end_date, end_abbreviation, end_offset] = zone_time(epoch + duration * 60);
    let offset = offset_label(&offset);
    let end_offset = offset_label(&end_offset);
    let end = if end_date != date || end_offset != offset {
        format!("{end_date} {end_time} {end_abbreviation} {end_offset}")
    } else {
        end_time.clone()
    };
    json!({"id":id, "label":label, "time":time, "date":date,
        "abbreviation":abbreviation, "offset":offset, "end":end,
        "working":available(epoch, duration), "slots":slots,
        "summary":format!("{label} [{id}]: {date} {time} {abbreviation} {offset} → {end}")})
}

pub(super) fn project(catalog: &mut Catalog, request: Request, now: i64) -> Result<Value> {
    let duration = if request.duration == 0 {
        60
    } else {
        request.duration
    };
    if ![15, 30, 60, 90, 120, 180].contains(&duration) {
        return Err("Choose a meeting duration from 15 to 180 minutes".into());
    }
    if !(-1..=1).contains(&request.shift) || !(0..1440).contains(&request.minute) {
        return Err("Meeting time is outside the UTC day".into());
    }
    let is_now = request.date.is_empty();
    let date = if is_now {
        utc(now, "%Y-%m-%d")
    } else {
        request.date
    };
    let day = day_epoch(&date)? + request.shift * 86400;
    let date = utc(day, "%Y-%m-%d");
    day_epoch(&date)?;
    let minute = if is_now {
        (now - now.div_euclid(86400) * 86400) / 60
    } else {
        request.minute
    };
    let epoch = day + minute * 60;
    catalog.load(now)?;
    let pinned = pins(&catalog.zones);
    let cached = catalog
        .meeting
        .as_ref()
        .filter(|cache| cache.day == day && cache.duration == duration && cache.pins == pinned);
    let mut all_slots =
        vec![cached.map_or_else(|| slots(day, duration), |cache| cache.slots[0].clone())];
    let mut rows = vec![row(
        "local",
        "This computer",
        epoch,
        duration,
        &all_slots[0],
    )];
    let previous_tz = env::var_os("TZ");
    let directory = zoneinfo();
    for (index, id) in pinned.iter().enumerate() {
        if let Some(source) = catalog.zones.iter().find(|source| &source.id == id) {
            let timezone = if source.zone.contains('/') {
                format!(":{}", directory.join(&source.zone).display())
            } else {
                source.zone.clone()
            };
            env::set_var("TZ", timezone);
            unsafe {
                tzset();
            }
            let zone_slots = cached.map_or_else(
                || slots(day, duration),
                |cache| cache.slots[index + 1].clone(),
            );
            rows.push(row(&source.id, &source.label, epoch, duration, &zone_slots));
            all_slots.push(zone_slots);
        }
    }
    restore_timezone(previous_tz);
    catalog.meeting = Some(Cache {
        day,
        duration,
        pins: pinned,
        slots: all_slots,
    });
    let overlap: Vec<_> = (0..96)
        .map(|slot| rows.iter().all(|row| row["slots"][slot] == true))
        .collect();
    let all_working = rows.iter().all(|row| row["working"] == true);
    let summary = format!("Meeting · {duration} minutes\n{} → {}\n{}\nWorking-hours guide: Mon–Fri 09:00–17:00 local; not calendar availability.",
        utc(epoch, "%Y-%m-%d %H:%M UTC"), utc(epoch + duration * 60, "%Y-%m-%d %H:%M UTC"),
        rows.iter().filter_map(|row| row["summary"].as_str()).collect::<Vec<_>>().join("\n"));
    Ok(
        json!({"date":date,"minute":minute,"duration":duration,"epoch":epoch,
        "time":utc(epoch,"%H:%M"),"end":utc(epoch + duration * 60,"%Y-%m-%d %H:%M UTC"),
        "rows":rows,"overlap":overlap,"allWorking":all_working,"summary":summary}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_utc_dates_reject_normalization_and_unsafe_ranges() {
        for invalid in [
            "2026-02-29",
            "2026-13-01",
            "2026-00-00",
            "26-01-01",
            "2026-01-32",
            "1969-12-31",
            "2101-01-01",
            "٢٠٢٦-01-01",
        ] {
            assert!(day_epoch(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            utc(day_epoch("2028-02-29").unwrap(), "%Y-%m-%d"),
            "2028-02-29"
        );
    }
}
