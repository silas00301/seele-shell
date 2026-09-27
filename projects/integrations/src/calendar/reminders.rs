//! Reminder times from Google's defaults and overrides, and the words a
//! notification carries.
use super::*;

pub(super) fn start_time(event: &Value, calendar: &Value) -> Option<i64> {
    let zone = event["start"]["timeZone"]
        .as_str()
        .or_else(|| calendar["timeZone"].as_str())
        .and_then(|v| v.parse::<chrono_tz::Tz>().ok());
    if let Some(value) = event["start"]["dateTime"].as_str() {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
            return Some(parsed.timestamp());
        }
        let local = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
        return zone?
            .from_local_datetime(&local)
            .earliest()
            .map(|v| v.timestamp());
    }
    let day = date(event["start"]["date"].as_str()?)?;
    match zone {
        Some(zone) => first_instant(day, &zone),
        None => local_midnight(day),
    }
}

pub(super) fn reminder_minutes(event: &Value, calendar: &Value) -> Vec<i64> {
    let source = if event["reminders"]["useDefault"] == false {
        &event["reminders"]["overrides"]
    } else {
        &calendar["defaultReminders"]
    };
    source
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|v| v["method"] == "popup")
                .filter_map(|v| v["minutes"].as_i64())
                .filter(|v| (0..=40320).contains(v))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Due {
    pub(super) key: String,
    pub(super) occurrence: String,
    pub(super) title: String,
    pub(super) start: i64,
    pub(super) end: i64,
    pub(super) all_day: bool,
}

/// Reminders whose time fell in `(previous, now]` and were not delivered yet.
pub(super) fn due_reminders(state: &State, previous: i64, now: i64) -> Vec<Due> {
    let calendars: HashMap<&str, &Value> = state
        .calendars
        .iter()
        .filter_map(|c| Some((c["id"].as_str()?, c)))
        .collect();
    let mut due = Vec::new();
    for event in &state.events {
        let calendar_id = calendar_of(event);
        if !state.selected.contains(calendar_id) {
            continue;
        }
        let Some(calendar) = calendars.get(calendar_id) else {
            continue;
        };
        let Some(start) = start_time(event, calendar) else {
            continue;
        };
        let Some(id) = event["id"].as_str() else {
            continue;
        };
        for minutes in reminder_minutes(event, calendar) {
            let at = start - minutes * 60;
            let key = format!("{calendar_id}:{id}:{at}");
            if at <= now && at > previous && !state.delivered.contains_key(&key) {
                due.push(Due {
                    key,
                    occurrence: format!("{calendar_id}:{id}:{start}"),
                    title: title(event),
                    start,
                    end: instant(&event["end"]).unwrap_or(start),
                    all_day: event["start"]["date"].is_string(),
                });
            }
        }
    }
    due
}

pub(super) fn starts(due: &Due, now: i64, today: NaiveDate) -> String {
    if due.all_day {
        let day = local_date(due.start).unwrap_or(today);
        return match (day - today).num_days() {
            0 => "Today · all day".into(),
            1 => "Tomorrow · all day".into(),
            _ => format!("{} · all day", day.format("%a %-d %b")),
        };
    }
    let range = if due.end > due.start {
        format!("{}–{}", hm(due.start), hm(due.end))
    } else {
        hm(due.start)
    };
    let delta = due.start - now;
    if delta > 30 {
        format!("Starts in {} · {range}", duration(delta))
    } else if delta >= -60 {
        format!("Starts now · {range}")
    } else {
        format!("Started at {}", hm(due.start))
    }
}

/// One notification per check: a single reminder names its event, several, or
/// any missed across sleep, arrive as one summary.
pub(super) fn reminder_text(
    due: &[Due],
    now: i64,
    today: NaiveDate,
    missed: bool,
) -> (String, String) {
    let mut seen = HashSet::new();
    let mut occurrences: Vec<&Due> = due.iter().filter(|d| seen.insert(&d.occurrence)).collect();
    occurrences.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.title.cmp(&b.title)));
    if let [only] = occurrences.as_slice() {
        let body = starts(only, now, today);
        return (
            only.title.clone(),
            if missed {
                format!("Missed reminder · {body}")
            } else {
                body
            },
        );
    }
    let count = occurrences.len();
    let title = if missed {
        format!("{count} missed calendar reminders")
    } else {
        format!("{count} calendar reminders")
    };
    let mut body = occurrences
        .iter()
        .take(5)
        .map(|d| {
            if d.all_day {
                d.title.clone()
            } else {
                format!("{} {}", hm(d.start), d.title)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if count > 5 {
        body.push_str(&format!(" · +{} more", count - 5));
    }
    (title, body)
}

pub(super) async fn notify(title: String, body: String) {
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("notify-send")
            .args([
                "-a",
                "Seele Calendar",
                "-i",
                "x-office-calendar",
                "--",
                title.as_str(),
                body.as_str(),
            ])
            .kill_on_drop(true)
            .status(),
    )
    .await;
}
