//! Meeting planning on an absolute-time axis.
//!
//! The axis is one local calendar day of this computer, from its first minute
//! to the first minute of the next, so a daylight-saving day is 23 or 25 hours
//! long instead of a guessed 24. Every participant is read at the same
//! instants: its wall time is the instant plus the UTC offset in force, which
//! libc reports once per transition rather than once per minute. Only the
//! explicit "same time on another day" request turns a wall time back into an
//! instant, and it walks the target day's own grid, so a skipped wall time
//! resolves to the first one after it and a repeated one to its first
//! occurrence.
use super::*;
use serde::Deserialize;
use url::Url;

const MINUTE: i64 = 60;
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
/// The grid a selection moves on, counted from the local day's first minute.
const SLOT: i64 = 15 * MINUTE;
const DURATIONS: [i64; 6] = [15, 30, 45, 60, 90, 120];
/// Busy intervals cover a day and a half either side of a request at most.
const MAX_BUSY: usize = 96;
const MAX_DAY_SHIFT: i64 = 366;
/// How far ahead Next fit looks for a start that suits every participant.
const HORIZON_DAYS: i64 = 14;
/// Participants are measured over this window from the axis's first minute:
/// the selected day, the Next fit horizon after it and a meeting's length.
const WINDOW: i64 = (HORIZON_DAYS + 2) * DAY;
const SUGGESTIONS: usize = 3;
// 1970-01-02 and 2100-12-30 keep every offset and window inside the range
// the strict date parser accepts.
const EARLIEST: i64 = DAY;
const LATEST: i64 = 4_133_808_000;
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Request {
    /// The selected start instant. Omitted means the next quarter hour.
    start: Option<i64>,
    /// A local date to move the selection to, keeping its local wall time.
    date: String,
    /// Local days to move the selection by, keeping its local wall time.
    days: i64,
    duration: i64,
    /// The planner's own busy time as `[start, end)` instants.
    busy: Vec<[i64; 2]>,
}

/// How a minute suits a participant: inside Monday–Friday 09:00–17:00, at the
/// shoulders of that day (07:00–09:00 and 17:00–20:00), or off.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Fit {
    Work,
    Edge,
    Off,
}

impl Fit {
    fn at(wall: i64) -> Self {
        let hour = wall.rem_euclid(DAY) / HOUR;
        if !(1..=5).contains(&weekday(wall)) {
            Fit::Off
        } else if (9..17).contains(&hour) {
            Fit::Work
        } else if (7..9).contains(&hour) || (17..20).contains(&hour) {
            Fit::Edge
        } else {
            Fit::Off
        }
    }

    /// What a minute costs a participant when ranking starts: nothing in
    /// working hours, more the further a shoulder hour is from them, more
    /// again when off, and most in the small hours.
    fn cost(wall: i64) -> u32 {
        if !(1..=5).contains(&weekday(wall)) {
            return 5;
        }
        match wall.rem_euclid(DAY) / HOUR {
            9..=16 => 0,
            8 | 17 => 1,
            7 | 18 => 2,
            19 => 3,
            6 | 20 | 21 => 4,
            _ => 6,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Fit::Work => "work",
            Fit::Edge => "edge",
            Fit::Off => "off",
        }
    }
}

/// Sunday is 0, as in `tm_wday`. The epoch began on a Thursday.
fn weekday(wall: i64) -> usize {
    (wall.div_euclid(DAY) + 4).rem_euclid(7) as usize
}

/// A wall-clock reading taken apart without consulting any timezone: the
/// offset has already been added.
struct Wall {
    year: i64,
    month: usize,
    day: i64,
    hour: i64,
    minute: i64,
    weekday: usize,
}

impl Wall {
    fn new(wall: i64) -> Self {
        // Howard Hinnant's civil_from_days, exact across the supported range.
        let days = wall.div_euclid(DAY) + 719_468;
        let era = days.div_euclid(146_097);
        let day_of_era = days - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let seconds = wall.rem_euclid(DAY);
        Wall {
            year: year_of_era + era * 400 + i64::from(month <= 2),
            month: month as usize,
            day: day_of_year - (153 * shifted_month + 2) / 5 + 1,
            hour: seconds / HOUR,
            minute: seconds % HOUR / MINUTE,
            weekday: weekday(wall),
        }
    }

    fn time(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// `Tue 29 Sep`
    fn date(&self) -> String {
        format!(
            "{} {} {}",
            WEEKDAYS[self.weekday],
            self.day,
            MONTHS[self.month - 1]
        )
    }

    fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// `UTC+2`, `UTC+5:45`, `UTC-3:30`, `UTC+0`.
fn offset_label(offset: i64) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.abs() / MINUTE;
    if minutes % 60 == 0 {
        format!("UTC{sign}{}", minutes / 60)
    } else {
        format!("UTC{sign}{}:{:02}", minutes / 60, minutes % 60)
    }
}

/// tzdata names a zone without a customary abbreviation by its offset
/// (`+0545`). The offset label already says that, so it is not repeated.
fn abbreviation(value: &str) -> Option<&str> {
    (!value.is_empty() && !value.starts_with(['+', '-'])).then_some(value)
}

fn duration_label(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, rest) => format!("{rest} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, rest) => format!("{hours} h {rest} min"),
    }
}

/// The offset and abbreviation the current `TZ` applies at an instant.
fn probe(epoch: i64) -> (i64, String) {
    unsafe {
        let raw = epoch as libc::time_t;
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&raw, &mut local);
        let mut buffer = [0_i8; 64];
        libc::strftime(buffer.as_mut_ptr(), buffer.len(), c"%Z".as_ptr(), &local);
        (
            local.tm_gmtoff as i64,
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

struct Segment {
    from: i64,
    offset: i64,
    abbreviation: String,
}

/// Every offset the current `TZ` applies between two instants. Rules change
/// at most once an hour, so hourly probes find each change and a binary search
/// places it to the second.
fn segments(from: i64, to: i64) -> Vec<Segment> {
    let (offset, abbreviation) = probe(from);
    let mut result = vec![Segment {
        from,
        offset,
        abbreviation,
    }];
    let mut at = from;
    while at < to {
        let next = (at + HOUR).min(to);
        let current = result.last().unwrap();
        let same = |reading: &(i64, String)| {
            reading.0 == current.offset && reading.1 == current.abbreviation
        };
        if !same(&probe(next)) {
            let (mut low, mut high) = (at, next);
            while high - low > 1 {
                let middle = low + (high - low) / 2;
                if same(&probe(middle)) {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            let (offset, abbreviation) = probe(high);
            result.push(Segment {
                from: high,
                offset,
                abbreviation,
            });
        }
        at = next;
    }
    result
}

/// One row of the plan: this computer or a pinned zone, measured over the
/// whole window once and then read arithmetically.
struct Participant {
    id: String,
    label: String,
    home: bool,
    segments: Vec<Segment>,
    /// Prefix sums, per minute of the window, of minutes outside working
    /// hours, of minutes off altogether, and of what the minutes cost.
    outside: Vec<u32>,
    off: Vec<u32>,
    cost: Vec<u32>,
}

impl Participant {
    /// Measure under whatever `TZ` is in force; the caller selects the zone.
    fn measure(id: &str, label: &str, home: bool, origin: i64) -> Self {
        let segments = segments(origin, origin + WINDOW);
        let minutes = (WINDOW / MINUTE) as usize;
        let mut outside = Vec::with_capacity(minutes + 1);
        let mut off = Vec::with_capacity(minutes + 1);
        let mut cost = Vec::with_capacity(minutes + 1);
        outside.push(0);
        off.push(0);
        cost.push(0);
        let mut index = 0;
        for minute in 0..minutes as i64 {
            let at = origin + minute * MINUTE;
            while index + 1 < segments.len() && segments[index + 1].from <= at {
                index += 1;
            }
            let wall = at + segments[index].offset;
            let fit = Fit::at(wall);
            outside.push(outside.last().unwrap() + u32::from(fit != Fit::Work));
            off.push(off.last().unwrap() + u32::from(fit == Fit::Off));
            cost.push(cost.last().unwrap() + Fit::cost(wall));
        }
        Participant {
            id: id.into(),
            label: label.into(),
            home,
            segments,
            outside,
            off,
            cost,
        }
    }

    fn segment(&self, at: i64) -> &Segment {
        let index = self.segments.partition_point(|segment| segment.from <= at);
        &self.segments[index.saturating_sub(1)]
    }

    fn wall(&self, at: i64) -> i64 {
        at + self.segment(at).offset
    }

    fn span(&self, origin: i64, start: i64, end: i64) -> (usize, usize) {
        (
            ((start - origin) / MINUTE) as usize,
            ((end - origin) / MINUTE) as usize,
        )
    }

    /// Minutes outside working hours and minutes off within `[start, end)`.
    fn minutes(&self, origin: i64, start: i64, end: i64) -> (u32, u32) {
        let (first, last) = self.span(origin, start, end);
        (
            self.outside[last] - self.outside[first],
            self.off[last] - self.off[first],
        )
    }

    fn cost(&self, origin: i64, start: i64, end: i64) -> u32 {
        let (first, last) = self.span(origin, start, end);
        self.cost[last] - self.cost[first]
    }

    fn fit(&self, origin: i64, start: i64, end: i64) -> Fit {
        match self.minutes(origin, start, end) {
            (0, _) => Fit::Work,
            (_, 0) => Fit::Edge,
            _ => Fit::Off,
        }
    }

    /// Why an interval does not suit this participant, read at its worst
    /// minute and, among equals, its first: `weekend` or `night` when it is
    /// off, `early` or `late` at the shoulders of a working day.
    fn note(&self, start: i64, end: i64) -> &'static str {
        let walls = || {
            (start..end)
                .step_by(MINUTE as usize)
                .map(|at| self.wall(at))
        };
        let Some(fit) = walls().map(Fit::at).max().filter(|fit| *fit != Fit::Work) else {
            return "";
        };
        let wall = walls().find(|wall| Fit::at(*wall) == fit).unwrap();
        match fit {
            _ if !(1..=5).contains(&weekday(wall)) => "weekend",
            Fit::Off => "night",
            _ if wall.rem_euclid(DAY) < 12 * HOUR => "early",
            _ => "late",
        }
    }

    /// The participant's own hours across the axis, split wherever its wall
    /// hour or its offset changes, so a repeated hour appears twice and a
    /// skipped one not at all. A cell that begins a new local date names it.
    fn cells(&self, start: i64, end: i64) -> Vec<Value> {
        let mut cells: Vec<Value> = Vec::new();
        let mut key = None;
        let mut date = None;
        for at in (start..end).step_by(MINUTE as usize) {
            let segment = self.segment(at);
            let wall = at + segment.offset;
            let next_key = (wall.div_euclid(HOUR), segment.from);
            if key == Some(next_key) {
                continue;
            }
            if let Some(last) = cells.last_mut() {
                last["to"] = json!((at - start) / MINUTE);
            }
            let reading = Wall::new(wall);
            let today = wall.div_euclid(DAY);
            cells.push(json!({
                "from": (at - start) / MINUTE,
                "to": (end - start) / MINUTE,
                "label": format!("{:02}", reading.hour),
                "day": if date.is_some_and(|previous| previous != today) {
                    WEEKDAYS[reading.weekday]
                } else {
                    ""
                },
                "kind": Fit::at(wall).name(),
            }));
            key = Some(next_key);
            date = Some(today);
        }
        cells
    }
}

/// `TZ` for a catalog zone, read from the packaged database.
fn zone_spec(directory: &std::path::Path, source: &ZoneSource) -> String {
    if source.zone.contains('/') {
        format!(":{}", directory.join(&source.zone).display())
    } else {
        source.zone.clone()
    }
}

/// This computer's own zone in the catalog, when it has a name there.
fn home_zone(zones: &[ZoneSource]) -> Option<&ZoneSource> {
    let named = |value: &str| {
        let value = value.trim_start_matches(':');
        let value = value
            .rsplit_once("zoneinfo/")
            .map_or(value, |(_, name)| name);
        let id = resolve(value, zones)?;
        zones.iter().find(|zone| zone.id == id)
    };
    match env::var("TZ") {
        Ok(value) if !value.is_empty() => named(&value),
        _ => fs::read_link("/etc/localtime")
            .ok()
            .and_then(|target| named(&target.to_string_lossy())),
    }
}

/// Participants measured from one axis origin, for one home zone and pin set.
pub(super) struct Cache {
    origin: i64,
    key: Vec<String>,
    participants: Vec<Participant>,
}

/// The local calendar date containing an instant, as `(year, month, day)`.
fn local_date(epoch: i64) -> (i32, i32, i32) {
    unsafe {
        let raw = epoch as libc::time_t;
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&raw, &mut local);
        (local.tm_year + 1900, local.tm_mon + 1, local.tm_mday)
    }
}

fn civil(epoch: i64) -> (i32, i32, i32) {
    let wall = Wall::new(epoch);
    (wall.year as i32, wall.month as i32, wall.day as i32)
}

fn civil_epoch((year, month, day): (i32, i32, i32)) -> i64 {
    let mut date: libc::tm = unsafe { std::mem::zeroed() };
    date.tm_year = year - 1900;
    date.tm_mon = month - 1;
    date.tm_mday = day;
    unsafe { libc::timegm(&mut date) as i64 }
}

/// The first minute of a local date. A zone that skipped the date altogether
/// (Samoa at the end of 2011) has no first minute to give.
fn day_start(date: (i32, i32, i32)) -> Result<i64> {
    let noon = civil_epoch(date) + 12 * HOUR;
    let mut at = civil_epoch(date) - probe(noon).0;
    for _ in 0..48 {
        if local_date(at) < date {
            break;
        }
        at -= HOUR;
    }
    for _ in 0..4 * 60 {
        if local_date(at) >= date {
            break;
        }
        at += MINUTE;
    }
    if local_date(at) != date {
        return Err("That date does not exist in this computer's timezone".into());
    }
    Ok(at)
}

fn next_date(date: (i32, i32, i32)) -> (i32, i32, i32) {
    civil(civil_epoch(date) + DAY)
}

/// The local day containing an instant: its first minute and the next day's.
fn local_day(epoch: i64) -> Result<(i64, i64)> {
    let date = local_date(epoch);
    Ok((day_start(date)?, day_start(next_date(date))?))
}

/// The first grid slot of a local date at or after a wall time of day.
fn same_time_on(date: (i32, i32, i32), minute_of_day: i64) -> Result<i64> {
    let start = day_start(date)?;
    let end = day_start(next_date(date))?;
    Ok((start..end)
        .step_by(SLOT as usize)
        .find(|at| (at + probe(*at).0).rem_euclid(DAY) / MINUTE >= minute_of_day)
        .unwrap_or(end - SLOT))
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
        return Err("Enter a date as YYYY-MM-DD".into());
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

fn in_range(epoch: i64) -> Result<i64> {
    if (EARLIEST..=LATEST).contains(&epoch) {
        Ok(epoch)
    } else {
        Err("Choose a date between 1970 and 2100".into())
    }
}

/// Resolve a request to a start instant on the grid of its local day.
fn resolve_start(request: &Request, now: i64) -> Result<i64> {
    if !(-MAX_DAY_SHIFT..=MAX_DAY_SHIFT).contains(&request.days) {
        return Err("Move by at most a year at a time".into());
    }
    let start = match request.start {
        Some(start) => {
            let (day, _) = local_day(in_range(start)?)?;
            start - (start - day).rem_euclid(SLOT)
        }
        None => {
            let (day, _) = local_day(in_range(now)?)?;
            now + (day - now).rem_euclid(SLOT)
        }
    };
    if request.date.is_empty() && request.days == 0 {
        return Ok(start);
    }
    let date = if request.date.is_empty() {
        local_date(start)
    } else {
        civil(day_epoch(&request.date)?)
    };
    let date = civil(in_range(civil_epoch(date) + request.days * DAY)?);
    let minute_of_day = (start + probe(start).0).rem_euclid(DAY) / MINUTE;
    same_time_on(date, minute_of_day)
}

fn validate_busy(busy: &[[i64; 2]]) -> Result {
    if busy.len() > MAX_BUSY {
        return Err(format!("Send at most {MAX_BUSY} busy intervals").into());
    }
    if busy
        .iter()
        .any(|[start, end]| start >= end || *start < EARLIEST || *end > LATEST)
    {
        return Err("A busy interval is invalid".into());
    }
    Ok(())
}

fn busy_seconds(busy: &[[i64; 2]], start: i64, end: i64) -> i64 {
    busy.iter()
        .map(|[from, to]| (end.min(*to) - start.max(*from)).max(0))
        .sum()
}

/// Lower is better: every participant's minutes cost what `Fit::cost` says,
/// and a minute already busy costs three, as much as an early hour.
fn score(rows: &[Participant], busy: &[[i64; 2]], origin: i64, start: i64, end: i64) -> i64 {
    let hours: i64 = rows
        .iter()
        .map(|row| i64::from(row.cost(origin, start, end)))
        .sum();
    hours + 3 * busy_seconds(busy, start, end).div_euclid(MINUTE)
}

fn worst(rows: &[Participant], origin: i64, start: i64, end: i64) -> Fit {
    rows.iter()
        .map(|row| row.fit(origin, start, end))
        .max()
        .unwrap_or(Fit::Work)
}

fn phrase(note: &str) -> &str {
    if note == "weekend" {
        "the weekend"
    } else {
        note
    }
}

fn capitalised(text: &str) -> String {
    let mut characters = text.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}

/// `Berlin`, `Berlin and Tokyo`, `Berlin, Tokyo and Lima`.
fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Participants outside working hours, grouped by why: off before the
/// shoulders of a working day, then in row order.
fn notes(rows: &[Participant], start: i64, end: i64) -> Vec<(&'static str, Vec<&str>)> {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut rows: Vec<_> = rows.iter().collect();
    rows.sort_by_key(|row| matches!(row.note(start, end), "early" | "late"));
    for row in rows {
        let note = row.note(start, end);
        if note.is_empty() {
            continue;
        }
        match groups.iter_mut().find(|(group, _)| *group == note) {
            Some((_, names)) => names.push(&row.label),
            None => groups.push((note, vec![&row.label])),
        }
    }
    groups
}

/// `Early in Los Angeles · late in Tokyo`, or one reason for everyone.
fn sentence(rows: &[Participant], start: i64, end: i64) -> String {
    let groups = notes(rows, start, end);
    match groups.as_slice() {
        [] => "Working hours for everyone".into(),
        [(note, names)] if rows.len() > 1 && names.len() == rows.len() => {
            capitalised(&format!("{} for everyone", phrase(note)))
        }
        _ => capitalised(
            &groups
                .iter()
                .map(|(note, names)| format!("{} in {}", phrase(note), listed(names)))
                .collect::<Vec<_>>()
                .join(" · "),
        ),
    }
}

/// A suggestion's caption: who it is hardest on, and how many others it
/// does not suit, as `Night in Tokyo +1`.
fn caption(rows: &[Participant], busy: bool, start: i64, end: i64) -> String {
    let groups = notes(rows, start, end);
    let outside: usize = groups.iter().map(|(_, names)| names.len()).sum();
    let hours = match groups.first() {
        None => "Everyone".to_owned(),
        Some((note, _)) if rows.len() > 1 && outside == rows.len() && groups.len() == 1 => {
            capitalised(&format!("{} for everyone", phrase(note)))
        }
        Some((note, names)) if outside == 1 => {
            capitalised(&format!("{} in {}", phrase(note), names[0]))
        }
        Some((note, names)) => capitalised(&format!(
            "{} in {} +{}",
            phrase(note),
            names[0],
            outside - 1
        )),
    };
    if busy {
        format!("Busy · {hours}")
    } else {
        hours
    }
}

/// One participant's reading of `[start, end)`: the wall range, the dates it
/// touches and the offset in force at either end.
struct Reading {
    start: Wall,
    end: Wall,
    start_zone: String,
    end_zone: String,
    start_offset: i64,
    end_offset: i64,
}

impl Reading {
    fn new(row: &Participant, start: i64, end: i64) -> Self {
        let first = row.segment(start);
        let last = row.segment(end);
        Reading {
            start: Wall::new(start + first.offset),
            end: Wall::new(end + last.offset),
            start_zone: first.abbreviation.clone(),
            end_zone: last.abbreviation.clone(),
            start_offset: first.offset,
            end_offset: last.offset,
        }
    }

    fn range(&self) -> String {
        format!("{}–{}", self.start.time(), self.end.time())
    }

    fn crosses_date(&self) -> bool {
        self.start.iso() != self.end.iso()
    }

    /// `CEST UTC+2`, or both ends when a transition falls inside.
    fn zone(&self) -> String {
        let label = |name: &str, offset: i64| match abbreviation(name) {
            Some(name) => format!("{name} {}", offset_label(offset)),
            None => offset_label(offset),
        };
        let first = label(&self.start_zone, self.start_offset);
        let last = label(&self.end_zone, self.end_offset);
        if first == last {
            first
        } else {
            format!("{first} → {last}")
        }
    }

    fn caption(&self) -> String {
        let dates = if self.crosses_date() {
            format!("{} → {}", self.start.date(), self.end.date())
        } else {
            self.start.date()
        };
        format!("{dates} · {}", self.zone())
    }

    /// One line of the copied summary. The date is written only where it is
    /// not the heading's.
    fn line(&self, label: &str, heading: &str) -> String {
        let range = if self.crosses_date() {
            format!(
                "{} {} – {} {}",
                self.start.date(),
                self.start.time(),
                self.end.date(),
                self.end.time()
            )
        } else if self.start.iso() != heading {
            format!("{} {}", self.start.date(), self.range())
        } else {
            self.range()
        };
        format!("- {label}: {range} {}", self.zone())
    }
}

fn participants<'a>(catalog: &'a mut Catalog, pinned: &[String], origin: i64) -> &'a [Participant] {
    let home = home_zone(&catalog.zones).map(|zone| (zone.id.clone(), zone.label.clone()));
    // A zone the catalog cannot name (a POSIX rule in TZ) is called by its
    // abbreviation, or failing that its offset, so "early in EDT" still reads.
    let (id, label) = home.unwrap_or_else(|| {
        let (offset, name) = probe(origin);
        let label = abbreviation(&name).map_or_else(|| offset_label(offset), str::to_owned);
        ("local".into(), label)
    });
    let mut key = vec![id.clone(), label.clone()];
    key.extend(pinned.iter().cloned());
    let fresh = catalog
        .meeting
        .as_ref()
        .is_some_and(|cache| cache.origin == origin && cache.key == key);
    if !fresh {
        let mut rows = vec![Participant::measure(&id, &label, true, origin)];
        let previous_tz = env::var_os("TZ");
        let directory = zoneinfo();
        for pin in pinned.iter().filter(|pin| **pin != id) {
            if let Some(source) = catalog.zones.iter().find(|source| &source.id == pin) {
                env::set_var("TZ", zone_spec(&directory, source));
                unsafe {
                    tzset();
                }
                rows.push(Participant::measure(
                    &source.id,
                    &source.label,
                    false,
                    origin,
                ));
            }
        }
        restore_timezone(previous_tz);
        catalog.meeting = Some(Cache {
            origin,
            key,
            participants: rows,
        });
    }
    &catalog.meeting.as_ref().unwrap().participants
}

/// Up to three starts on the axis day, best first: by score, then on the hour
/// or half hour, then earliest, each at least the meeting or half an hour
/// from the others.
fn suggest(
    rows: &[Participant],
    busy: &[[i64; 2]],
    origin: i64,
    day_end: i64,
    earliest: i64,
    length: i64,
) -> Vec<i64> {
    // A start at which nobody is working for any of the meeting suggests
    // nothing; a weekend offers none.
    let mut candidates: Vec<(i64, bool, i64)> = (origin..day_end)
        .step_by(SLOT as usize)
        .filter(|slot| *slot >= earliest)
        .filter(|slot| {
            rows.iter()
                .any(|row| row.minutes(origin, *slot, slot + length).1 < (length / MINUTE) as u32)
        })
        .map(|slot| {
            let minute = rows[0].wall(slot).rem_euclid(HOUR) / MINUTE;
            (
                score(rows, busy, origin, slot, slot + length),
                minute % 30 != 0,
                slot,
            )
        })
        .collect();
    candidates.sort_unstable();
    let spacing = length.max(30 * MINUTE);
    let mut chosen: Vec<i64> = Vec::new();
    for (_, _, slot) in candidates {
        if chosen.len() == SUGGESTIONS {
            break;
        }
        if chosen.iter().all(|other| (other - slot).abs() >= spacing) {
            chosen.push(slot);
        }
    }
    chosen
}

/// The next free opening after the selection, within the horizon, at the
/// best fit the horizon offers: working hours for everyone when that ever
/// happens, otherwise nobody off. A team that never shares either has none.
/// An opening the selection already sits in is not the next one, and nothing
/// before `earliest` is offered.
fn next_fit(
    rows: &[Participant],
    busy: &[[i64; 2]],
    origin: i64,
    selected: i64,
    earliest: i64,
    length: i64,
) -> Option<i64> {
    let slots = || {
        (selected + SLOT..=selected + HORIZON_DAYS * DAY)
            .step_by(SLOT as usize)
            .filter(|slot| *slot >= earliest)
    };
    let free = |slot: i64| busy_seconds(busy, slot, slot + length) == 0;
    let fit = |slot: i64| worst(rows, origin, slot, slot + length);
    let target = slots()
        .filter(|slot| free(*slot))
        .map(fit)
        .min()
        .filter(|fit| *fit != Fit::Off)?;
    let open = |slot: i64| free(slot) && fit(slot) <= target;
    let mut inside = selected >= earliest && open(selected);
    for slot in slots() {
        let opening = open(slot);
        if opening && !inside {
            return Some(slot);
        }
        inside = inside && opening;
    }
    None
}

pub(super) fn project(catalog: &mut Catalog, request: Request, now: i64) -> Result<Value> {
    catalog.load(now)?;
    let pinned = pins(&catalog.zones);
    plan(catalog, request, now, &pinned)
}

fn plan(catalog: &mut Catalog, request: Request, now: i64, pinned: &[String]) -> Result<Value> {
    let duration = if request.duration == 0 {
        60
    } else {
        request.duration
    };
    if !DURATIONS.contains(&duration) {
        return Err("Choose a meeting length from 15 minutes to 2 hours".into());
    }
    validate_busy(&request.busy)?;
    let start = resolve_start(&request, now)?;
    let (origin, day_end) = local_day(start)?;
    let length = duration * MINUTE;
    let end = start + length;
    let busy = &request.busy;
    let rows = participants(catalog, pinned, origin);
    let home = &rows[0];
    // Nothing is suggested before the next quarter hour.
    let earliest = origin.max(now + (origin - now).rem_euclid(SLOT));

    let suggestions: Vec<Value> = suggest(rows, busy, origin, day_end, earliest, length)
        .into_iter()
        .map(|slot| {
            let conflict = busy_seconds(busy, slot, slot + length) > 0;
            json!({
                "start": slot,
                "range": Reading::new(home, slot, slot + length).range(),
                "fit": worst(rows, origin, slot, slot + length).name(),
                "busy": conflict,
                "caption": caption(rows, conflict, slot, slot + length),
            })
        })
        .collect();
    let next = next_fit(rows, busy, origin, start, earliest, length).map(|slot| {
        let wall = Wall::new(home.wall(slot));
        json!({
            "start": slot,
            "label": format!("{} {}", wall.date(), wall.time()),
            "fit": worst(rows, origin, slot, slot + length).name(),
        })
    });

    let first = Wall::new(home.wall(origin));
    let heading = first.iso();
    let mut lines = vec![format!(
        "{} {} · {}",
        first.date(),
        first.year,
        duration_label(duration)
    )];
    let rows_json: Vec<Value> = rows
        .iter()
        .map(|row| {
            let reading = Reading::new(row, start, end);
            lines.push(reading.line(&row.label, &heading));
            json!({
                "id": row.id,
                "label": row.label,
                "home": row.home,
                "range": reading.range(),
                "caption": reading.caption(),
                "fit": row.fit(origin, start, end).name(),
                "note": row.note(start, end),
                "cells": row.cells(origin, day_end),
            })
        })
        .collect();
    let (utc_start, utc_end) = (Wall::new(start), Wall::new(end));
    let utc_range = format!("{}–{}", utc_start.time(), utc_end.time());
    lines.push(if utc_start.iso() == heading && utc_end.iso() == heading {
        format!("- UTC: {utc_range}")
    } else if utc_start.iso() == utc_end.iso() {
        format!("- UTC: {} {utc_range}", utc_start.date())
    } else {
        format!(
            "- UTC: {} {} – {} {}",
            utc_start.date(),
            utc_start.time(),
            utc_end.date(),
            utc_end.time()
        )
    });
    let summary = lines.join("\n");
    // Google's own event editor, prefilled. Nothing is created until the
    // person saves it there.
    let mut calendar = Url::parse("https://calendar.google.com/calendar/render")?;
    calendar
        .query_pairs_mut()
        .append_pair("action", "TEMPLATE")
        .append_pair(
            "dates",
            &format!(
                "{}/{}",
                utc(start, "%Y%m%dT%H%M%SZ"),
                utc(end, "%Y%m%dT%H%M%SZ")
            ),
        )
        .append_pair("details", &summary);
    if home.id.contains('/') {
        calendar.query_pairs_mut().append_pair("ctz", &home.id);
    }
    let conflicts: Vec<_> = busy
        .iter()
        .filter(|[from, to]| *from < end && *to > start)
        .collect();
    Ok(json!({
        "start": start,
        "end": end,
        "duration": duration,
        "now": now,
        "day": {
            "date": heading,
            "label": format!("{} {}", first.date(), first.year),
            "start": origin,
            "end": day_end,
            "minutes": (day_end - origin) / MINUTE,
            "today": (origin..day_end).contains(&now),
            "past": day_end <= now,
        },
        "range": Reading::new(home, start, end).range(),
        "zone": Reading::new(home, start, end).zone(),
        "utc": format!("{utc_range} UTC"),
        "fit": worst(rows, origin, start, end).name(),
        "status": sentence(rows, start, end),
        "conflicts": conflicts,
        "rows": rows_json,
        "suggestions": suggestions,
        "next": next,
        "summary": summary,
        "calendarUrl": calendar.as_str(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_dates_reject_normalization_and_unsafe_ranges() {
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

    #[test]
    fn wall_readings_match_libc_across_the_supported_range() {
        let mut at = EARLIEST;
        while at < LATEST {
            let wall = Wall::new(at);
            assert_eq!(wall.iso(), utc(at, "%Y-%m-%d"), "{at}");
            assert_eq!(wall.time(), utc(at, "%H:%M"), "{at}");
            assert_eq!(WEEKDAYS[wall.weekday], utc(at, "%a"), "{at}");
            assert_eq!(MONTHS[wall.month - 1], utc(at, "%b"), "{at}");
            at += 7 * DAY + 3 * HOUR + 17 * MINUTE;
        }
    }

    #[test]
    fn labels_are_compact() {
        assert_eq!(offset_label(2 * HOUR), "UTC+2");
        assert_eq!(offset_label(5 * HOUR + 45 * MINUTE), "UTC+5:45");
        assert_eq!(offset_label(-(3 * HOUR + 30 * MINUTE)), "UTC-3:30");
        assert_eq!(offset_label(0), "UTC+0");
        assert_eq!(abbreviation("+0545"), None);
        assert_eq!(abbreviation("CEST"), Some("CEST"));
        assert_eq!(duration_label(45), "45 min");
        assert_eq!(duration_label(60), "1 h");
        assert_eq!(duration_label(90), "1 h 30 min");
    }

    #[test]
    fn fit_follows_the_weekday_and_its_shoulders() {
        let monday = day_epoch("2026-09-28").unwrap();
        assert_eq!(Fit::at(monday + 9 * HOUR), Fit::Work);
        assert_eq!(Fit::at(monday + 17 * HOUR - 1), Fit::Work);
        assert_eq!(Fit::at(monday + 17 * HOUR), Fit::Edge);
        assert_eq!(Fit::at(monday + 7 * HOUR), Fit::Edge);
        assert_eq!(Fit::at(monday + 20 * HOUR), Fit::Off);
        assert_eq!(Fit::at(monday + 7 * HOUR - 1), Fit::Off);
        assert_eq!(Fit::at(monday - 12 * HOUR), Fit::Off, "Sunday noon");
        let costs: Vec<_> = (0..24)
            .map(|hour| Fit::cost(monday + hour * HOUR))
            .collect();
        assert_eq!(
            costs,
            [6, 6, 6, 6, 6, 6, 4, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 4, 6, 6]
        );
        assert_eq!(Fit::cost(monday - 12 * HOUR), 5);
    }

    /// New York's rules as a POSIX string, which the catalog cannot name,
    /// with Berlin's pinned beside it: the planner's arithmetic without
    /// depending on the machine's timezone files. `real` below adds tzdata.
    struct Fixture {
        catalog: Catalog,
        pinned: Vec<String>,
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Fixture {
        fn new() -> Self {
            let lock = TIMEZONE.lock().unwrap_or_else(|poison| poison.into_inner());
            let previous = env::var_os("TZ");
            env::set_var("TZ", "EST5EDT,M3.2.0,M11.1.0");
            unsafe {
                tzset();
            }
            let catalog = Catalog {
                zones: vec![ZoneSource {
                    id: "berlin".into(),
                    zone: "CET-1CEST,M3.5.0,M10.5.0".into(),
                    label: "Berlin".into(),
                    flag: String::new(),
                    aliases: String::new(),
                    kind: "city".into(),
                }],
                ..Catalog::default()
            };
            Fixture {
                catalog,
                pinned: vec!["berlin".into()],
                previous,
                _lock: lock,
            }
        }

        fn plan(&mut self, request: Value) -> Value {
            self.try_plan(request).unwrap()
        }

        fn try_plan(&mut self, request: Value) -> Result<Value> {
            let request = serde_json::from_value(request)?;
            // A Thursday well before every date below, so nothing is past.
            plan(
                &mut self.catalog,
                request,
                at(2026, 1, 1, 12, 0),
                &self.pinned,
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            restore_timezone(self.previous.take());
        }
    }

    fn at(year: i32, month: i32, day: i32, hour: i64, minute: i64) -> i64 {
        civil_epoch((year, month, day)) + hour * HOUR + minute * MINUTE
    }

    fn labels(row: &Value) -> Vec<&str> {
        row["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell["label"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn the_axis_is_the_local_day_however_long_it_is() {
        let mut fixture = Fixture::new();
        // 01:00 New York time, 2026-11-01: the hour that happens twice.
        let fold = fixture.plan(json!({"start": at(2026, 11, 1, 5, 0)}));
        assert_eq!(fold["day"]["minutes"], 1500);
        assert_eq!(fold["day"]["date"], "2026-11-01");
        let home = labels(&fold["rows"][0]);
        assert_eq!(home.iter().filter(|label| **label == "01").count(), 2);
        assert_eq!(home.len(), 25);
        let spring = fixture.plan(json!({"start": at(2026, 3, 8, 12, 0)}));
        assert_eq!(spring["day"]["minutes"], 1380);
        assert!(!labels(&spring["rows"][0]).contains(&"02"));
        // Berlin is six hours ahead that day, and its own midnight is marked.
        let berlin = &spring["rows"][1]["cells"];
        assert_eq!(berlin[0]["label"], "06");
        assert!(berlin
            .as_array()
            .unwrap()
            .iter()
            .any(|cell| cell["label"] == "00" && cell["day"] == "Mon"));
        assert_eq!(
            spring["rows"][0]["label"], "EST",
            "an unnamed zone is called by its abbreviation"
        );
        assert_eq!(spring["rows"][0]["home"], true);
    }

    #[test]
    fn day_moves_keep_the_wall_time_and_resolve_gaps_and_folds() {
        let mut fixture = Fixture::new();
        // 10:00 EST on the Saturday, then the Sunday: 23 hours later.
        let next = fixture.plan(json!({"start": at(2026, 3, 7, 15, 0), "days": 1}));
        assert_eq!(next["start"], at(2026, 3, 8, 14, 0));
        assert_eq!(next["range"], "10:00–11:00");
        // 02:30 does not exist on the Sunday; the first time after it does.
        let gap = fixture.plan(json!({"start": at(2026, 3, 7, 7, 30), "days": 1}));
        assert_eq!(gap["start"], at(2026, 3, 8, 7, 0));
        assert_eq!(gap["range"], "03:00–04:00");
        // 01:30 happens twice on 2026-11-01; the first is chosen.
        let fold = fixture.plan(json!({"start": at(2026, 10, 31, 5, 30), "days": 1}));
        assert_eq!(fold["start"], at(2026, 11, 1, 5, 30));
        assert_eq!(fold["zone"], "EDT UTC-4 → EST UTC-5");
        let typed = fixture.plan(json!({"start": at(2026, 9, 29, 14, 0), "date": "2026-12-24"}));
        assert_eq!(typed["day"]["date"], "2026-12-24");
        assert_eq!(typed["range"], "10:00–11:00");
    }

    #[test]
    fn a_step_past_midnight_lands_on_the_next_days_grid() {
        let mut fixture = Fixture::new();
        let last = fixture.plan(json!({"start": at(2026, 9, 29, 3, 45)}));
        assert_eq!(last["range"], "23:45–00:45");
        assert_eq!(last["day"]["date"], "2026-09-28");
        let next = fixture.plan(json!({"start": last["start"].as_i64().unwrap() + SLOT}));
        assert_eq!(next["day"]["date"], "2026-09-29");
        assert_eq!(next["start"], next["day"]["start"]);
        // Off-grid starts snap back to the quarter hour they fall in.
        let snapped = fixture.plan(json!({"start": at(2026, 9, 29, 13, 7)}));
        assert_eq!(snapped["start"], at(2026, 9, 29, 13, 0));
    }

    #[test]
    fn suggestions_rank_shared_hours_and_avoid_busy_time() {
        let mut fixture = Fixture::new();
        // New York 09:00–11:00 is Berlin 15:00–17:00 on this Tuesday.
        let plan = fixture.plan(json!({"start": at(2026, 9, 29, 13, 0)}));
        let starts: Vec<_> = plan["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|suggestion| suggestion["range"].as_str().unwrap())
            .collect();
        assert_eq!(starts, ["09:00–10:00", "10:00–11:00", "08:00–09:00"]);
        assert_eq!(plan["suggestions"][0]["caption"], "Everyone");
        assert_eq!(plan["suggestions"][0]["fit"], "work");
        assert_eq!(plan["suggestions"][2]["caption"], "Early in EDT");
        assert_eq!(plan["status"], "Working hours for everyone");
        // Already inside the opening: the next one is tomorrow's.
        assert_eq!(plan["next"]["start"], at(2026, 9, 30, 13, 0));
        assert_eq!(plan["next"]["label"], "Wed 30 Sep 09:00");
        assert_eq!(plan["next"]["fit"], "work");

        let busy = [at(2026, 9, 29, 13, 0), at(2026, 9, 29, 14, 0)];
        let plan = fixture.plan(json!({"start": busy[0], "busy": [busy]}));
        assert_eq!(plan["suggestions"][0]["range"], "10:00–11:00");
        assert_eq!(plan["suggestions"][0]["busy"], false);
        assert_eq!(plan["conflicts"], json!([busy]));
        assert_eq!(
            plan["next"]["start"], busy[1],
            "the next opening is after the busy hour"
        );
    }

    #[test]
    fn a_weekend_suggests_nothing_and_points_at_monday() {
        let mut fixture = Fixture::new();
        let plan = fixture.plan(json!({"start": at(2026, 10, 3, 16, 0)}));
        assert_eq!(plan["suggestions"], json!([]));
        assert_eq!(plan["status"], "The weekend for everyone");
        assert_eq!(plan["fit"], "off");
        assert_eq!(plan["next"]["label"], "Mon 5 Oct 09:00");
        let late = fixture.plan(json!({"start": at(2026, 9, 29, 23, 0)}));
        assert_eq!(
            late["status"], "Night in Berlin · late in EDT",
            "the hardest on someone comes first"
        );
    }

    #[test]
    fn the_summary_is_prose_with_dates_only_where_they_differ() {
        let mut fixture = Fixture::new();
        let plan = fixture.plan(json!({"start": at(2026, 9, 29, 13, 0)}));
        assert_eq!(
            plan["summary"],
            "Tue 29 Sep 2026 · 1 h\n- EDT: 09:00–10:00 EDT UTC-4\n- Berlin: 15:00–16:00 CEST UTC+2\n- UTC: 13:00–14:00"
        );
        let url = plan["calendarUrl"].as_str().unwrap();
        assert!(url.starts_with("https://calendar.google.com/calendar/render?action=TEMPLATE&dates=20260929T130000Z%2F20260929T140000Z&details="));
        assert!(
            !url.contains("ctz="),
            "an unnamed zone is not offered to Google"
        );
        let crossing = fixture.plan(json!({"start": at(2026, 9, 29, 21, 30), "duration": 90}));
        assert_eq!(
            crossing["rows"][1]["caption"],
            "Tue 29 Sep → Wed 30 Sep · CEST UTC+2"
        );
        assert!(crossing["summary"]
            .as_str()
            .unwrap()
            .contains("- Berlin: Tue 29 Sep 23:30 – Wed 30 Sep 01:00 CEST UTC+2"));
    }

    #[test]
    fn invalid_requests_are_refused() {
        let mut fixture = Fixture::new();
        for request in [
            json!({"duration": 20}),
            json!({"start": 0}),
            json!({"start": at(2026, 9, 29, 13, 0), "days": 400}),
            json!({"start": at(2026, 9, 29, 13, 0), "date": "2026-02-30"}),
            json!({"start": at(2026, 9, 29, 13, 0), "busy": [[at(2026, 9, 29, 14, 0), at(2026, 9, 29, 13, 0)]]}),
            json!({"start": at(2026, 9, 29, 13, 0), "minute": 5}),
        ] {
            assert!(fixture.try_plan(request.clone()).is_err(), "{request}");
        }
    }

    #[test]
    fn busy_time_is_bounded_and_measured() {
        assert!(validate_busy(&[[DAY, DAY + HOUR]]).is_ok());
        assert!(validate_busy(&[[DAY + HOUR, DAY]]).is_err());
        assert!(validate_busy(&[[0, HOUR]]).is_err());
        assert!(validate_busy(&vec![[DAY, DAY + HOUR]; MAX_BUSY + 1]).is_err());
        assert_eq!(busy_seconds(&[[0, 100], [50, 200]], 60, 120), 100);
    }
}
