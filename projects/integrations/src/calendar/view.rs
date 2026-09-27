//! What the shell draws: a day's agenda, a month's dots and the bar indicator,
//! all read from an index of where each cached event falls.
use super::*;

pub(super) const MAX_SPAN_DAYS: i64 = 400;
pub(super) const INDICATOR_LEAD: i64 = 15 * 60;
pub(super) const DOTS: usize = 3;

pub(super) fn local_date(secs: i64) -> Option<NaiveDate> {
    Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|v| v.date_naive())
}

pub(super) fn first_instant(day: NaiveDate, zone: &impl TimeZone) -> Option<i64> {
    // A day whose midnight a DST change skips starts at its first real minute.
    (0..180).find_map(|minute| {
        zone.from_local_datetime(&day.and_hms_opt(0, minute / 60, minute % 60)?)
            .earliest()
            .map(|v| v.timestamp())
    })
}

pub(super) fn local_midnight(day: NaiveDate) -> Option<i64> {
    first_instant(day, &Local)
}

pub(super) fn hm(secs: i64) -> String {
    Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|v| v.format("%H:%M").to_string())
        .unwrap_or_default()
}

pub(super) fn title(event: &Value) -> String {
    let title = crate::common::clean(&event["summary"], "", 120);
    if title.is_empty() {
        "(No title)".into()
    } else {
        title
    }
}

pub(super) fn duration(secs: i64) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    if minutes % 1440 == 0 {
        let count = minutes / 1440;
        return format!("{count} day{}", if count == 1 { "" } else { "s" });
    }
    match minutes % 60 {
        0 => format!("{} h", minutes / 60),
        rest => format!("{} h {rest} min", minutes / 60),
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Span {
    pub(super) event: usize,
    pub(super) calendar: usize,
    pub(super) start: i64,
    pub(super) end: i64,
    pub(super) all_day: bool,
    pub(super) first: NaiveDate,
    pub(super) last: NaiveDate,
}

/// Where each cached event falls, rebuilt only when the cache changes, so a
/// day's agenda, its dots and the indicator are lookups rather than scans.
#[derive(Default)]
pub(super) struct Index {
    pub(super) spans: Vec<Span>,
    pub(super) days: BTreeMap<NaiveDate, Vec<usize>>,
}

pub(super) fn span(event: &Value, event_index: usize, calendar: usize) -> Option<Span> {
    let (start, end, all_day, first, last) = match (
        event["start"]["date"].as_str().and_then(date),
        event["end"]["date"].as_str().and_then(date),
    ) {
        (Some(a), b) => {
            let last = b.filter(|b| *b > a).and_then(|b| b.pred_opt()).unwrap_or(a);
            (
                local_midnight(a)?,
                local_midnight(last.succ_opt()?)?,
                true,
                a,
                last,
            )
        }
        _ => {
            let a = instant(&event["start"])?;
            let b = instant(&event["end"]).unwrap_or(a).max(a);
            let first = local_date(a)?;
            let last = if b > a { local_date(b - 1)? } else { first };
            (a, b, false, first, last)
        }
    };
    Some(Span {
        event: event_index,
        calendar,
        start,
        end,
        all_day,
        first,
        last: last.min(first + days(MAX_SPAN_DAYS)),
    })
}

pub(super) fn index(state: &State) -> Index {
    let calendars: HashMap<&str, usize> = state
        .calendars
        .iter()
        .enumerate()
        .filter_map(|(at, c)| {
            let id = c["id"].as_str()?;
            state.selected.contains(id).then_some((id, at))
        })
        .collect();
    let mut index = Index::default();
    for (at, event) in state.events.iter().enumerate() {
        let Some(&calendar) = calendars.get(calendar_of(event)) else {
            continue;
        };
        let Some(span) = span(event, at, calendar) else {
            continue;
        };
        let position = index.spans.len();
        let mut day = span.first;
        while day <= span.last {
            index.days.entry(day).or_default().push(position);
            match day.succ_opt() {
                Some(next) => day = next,
                None => break,
            }
        }
        index.spans.push(span);
    }
    index
}

pub(super) fn event_color(state: &State, event: &Value, calendar: &Value) -> String {
    event["color_id"]
        .as_str()
        .and_then(|id| state.palette.get(id))
        .cloned()
        .unwrap_or_else(|| calendar["color"].as_str().unwrap_or("").to_owned())
}

/// Only opaque timed events block the planner. Titles stay in the shell and
/// never enter the clock worker's request.
pub(super) fn busy(state: &State, index: &Index) -> Value {
    let mut blocks: Vec<Value> = index
        .spans
        .iter()
        .filter_map(|span| {
            let event = &state.events[span.event];
            if span.all_day
                || span.end <= span.start
                || event["transparency"] == "transparent"
                || event["event_type"] == "workingLocation"
            {
                return None;
            }
            let calendar = &state.calendars[span.calendar];
            Some(json!({
                "start": span.start,
                "end": span.end,
                "title": title(event),
                "color": event_color(state, event, calendar),
            }))
        })
        .collect();
    blocks.sort_by(|a, b| a["start"].as_i64().cmp(&b["start"].as_i64()));
    Value::Array(blocks)
}

pub(super) fn day_range(first: NaiveDate, last: NaiveDate, weekday: bool) -> String {
    let lead = if weekday { "%a %-d" } else { "%-d" };
    if first.format("%Y-%m").to_string() == last.format("%Y-%m").to_string() {
        format!(
            "{}–{}",
            first.format(lead),
            last.format(&format!("{lead} %b"))
        )
    } else {
        format!(
            "{} – {}",
            first.format(&format!("{lead} %b")),
            last.format(&format!("{lead} %b"))
        )
    }
}

/// What a row says about its time on the day it is listed under. A multi-day
/// event reads from, until or all day depending on which part of it that day holds.
pub(super) fn time_label(span: &Span, day_start: i64, day_end: i64) -> String {
    if span.all_day {
        return if span.first == span.last {
            "All day".into()
        } else {
            format!("All day · {}", day_range(span.first, span.last, false))
        };
    }
    match (span.start < day_start, span.end > day_end) {
        (true, true) => "All day".into(),
        (true, false) => format!("Until {}", hm(span.end)),
        (false, true) => format!("From {}", hm(span.start)),
        _ if span.end > span.start => format!("{}–{}", hm(span.start), hm(span.end)),
        _ => hm(span.start),
    }
}

/// The unfolded row's full time, with the date when an event crosses midnight.
pub(super) fn when_label(span: &Span) -> String {
    if span.all_day {
        let length = (span.last - span.first).num_days() + 1;
        return if length == 1 {
            "All day".into()
        } else {
            format!("{} · {length} days", day_range(span.first, span.last, true))
        };
    }
    let length = duration(span.end - span.start);
    if span.first == span.last {
        return if span.end > span.start {
            format!("{}–{} · {length}", hm(span.start), hm(span.end))
        } else {
            hm(span.start)
        };
    }
    let at = |secs: i64| {
        Local
            .timestamp_opt(secs, 0)
            .single()
            .map(|v| v.format("%a %-d %b %H:%M").to_string())
            .unwrap_or_default()
    };
    format!("{} – {} · {length}", at(span.start), at(span.end))
}

pub(super) fn rsvp_label(value: &str) -> &'static str {
    match value {
        "accepted" => "Accepted",
        "tentative" => "Tentative",
        "needsAction" => "Not answered",
        _ => "",
    }
}

pub(super) fn role_label(calendar: &Value) -> &'static str {
    if calendar["primary"] == true {
        return "Primary";
    }
    match calendar["role"].as_str().unwrap_or("") {
        "owner" => "Yours",
        "writer" => "Shared with you",
        "reader" => "View only",
        "freeBusyReader" => "Busy times only",
        _ => "",
    }
}

pub(super) fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

pub(super) fn agenda_items(state: &State, index: &Index, day: NaiveDate) -> Vec<Value> {
    let (Some(day_start), Some(day_end)) =
        (local_midnight(day), day.succ_opt().and_then(local_midnight))
    else {
        return Vec::new();
    };
    let mut spans: Vec<&Span> = index
        .days
        .get(&day)
        .map(|at| at.iter().map(|at| &index.spans[*at]).collect())
        .unwrap_or_default();
    let whole = |s: &Span| s.all_day || (s.start < day_start && s.end > day_end);
    // All-day events lead, then timed events by start, end and title.
    spans.sort_by(|a, b| {
        whole(b)
            .cmp(&whole(a))
            .then(a.start.cmp(&b.start))
            .then(a.end.cmp(&b.end))
            .then_with(|| title(&state.events[a.event]).cmp(&title(&state.events[b.event])))
            .then_with(|| event_key(&state.events[a.event]).cmp(&event_key(&state.events[b.event])))
    });
    spans
        .into_iter()
        .map(|span| {
            let event = &state.events[span.event];
            let calendar = &state.calendars[span.calendar];
            let rsvp = text(&event["rsvp"]);
            json!({
                "key": event_key(event),
                "title": title(event),
                "calendar": text(&calendar["name"]),
                "color": event_color(state, event, calendar),
                "all_day": whole(span),
                "time": time_label(span, day_start, day_end),
                "when": when_label(span),
                "start": span.start * 1000,
                "end": span.end * 1000,
                "rsvp": rsvp,
                "rsvp_label": rsvp_label(rsvp),
                "location": text(&event["location"]),
                "description": text(&event["description"]),
                "join": text(&event["join"]),
                "join_label": text(&event["join_label"]),
                "link": text(&event["link"]),
            })
        })
        .collect()
}

pub(super) fn dots(state: &State, index: &Index) -> Value {
    let mut out = Map::new();
    for (day, at) in &index.days {
        let mut calendars: Vec<usize> = at.iter().map(|at| index.spans[*at].calendar).collect();
        calendars.sort_unstable();
        calendars.dedup();
        let colors: Vec<&str> = calendars
            .iter()
            .take(DOTS)
            .map(|at| text(&state.calendars[*at]["color"]))
            .collect();
        out.insert(
            day.to_string(),
            json!({"colors": colors, "more": calendars.len().saturating_sub(DOTS)}),
        );
    }
    Value::Object(out)
}

/// The bar shows the next event starting within 15 minutes, otherwise the
/// ongoing event ending soonest. An event lasting a day or longer is treated
/// like an all-day one and stays out of the countdown.
pub(super) fn indicator(state: &State, index: &Index, now: i64, today: NaiveDate) -> Value {
    let mut eligible: Vec<&Span> = index
        .spans
        .iter()
        .filter(|s| {
            !s.all_day && s.end - s.start < 86_400 && s.start <= now + INDICATOR_LEAD && s.end > now
        })
        .collect();
    let key = |s: &Span| event_key(&state.events[s.event]);
    eligible.sort_by(|a, b| {
        let (ua, ub) = (a.start > now, b.start > now);
        ub.cmp(&ua)
            .then(if ua {
                a.start.cmp(&b.start)
            } else {
                a.end.cmp(&b.end)
            })
            .then_with(|| key(a).cmp(&key(b)))
    });
    let Some(first) = eligible.first() else {
        return Value::Null;
    };
    let event = &state.events[first.event];
    let calendar = &state.calendars[first.calendar];
    let upcoming = first.start > now;
    let extra = eligible.len() - 1;
    let range = format!("{}–{}", hm(first.start), hm(first.end));
    json!({
        "key": event_key(event),
        "day": (if upcoming { local_date(first.start).unwrap_or(today) } else { today }).to_string(),
        "title": title(event),
        "color": event_color(state, event, calendar),
        "label": (if upcoming { format!("in {}m", (first.start - now + 59) / 60) } else { "now".to_owned() }),
        "extra": extra,
        "detail": (match extra {
            0 => range,
            1 => format!("{range} · 1 more event"),
            n => format!("{range} · {n} more events"),
        }),
    })
}
