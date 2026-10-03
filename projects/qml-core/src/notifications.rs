//! Notification presentation policy. QObject lifetimes remain in the Qt host.
use crate::value::{array, number, string, text, truthy, utf16_len};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    sync::LazyLock,
};

static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)&(?:nbsp|#160);").unwrap());
static URLS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)https?://\S+").unwrap());
static DATES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\b)[0-9]{4}[-/][0-9]{1,2}[-/][0-9]{1,2}(?-u:\b)").unwrap());
static CONTEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(?:(?:verification|security|authentication|confirmation|login|access|reset|sign[ -]?in|one[ -]?time|two[ -]?factor|your)[ -]+(?:code|pin)|otp|2fa|verifizierungscode|bestätigungscode|sicherheitscode|anmeldecode)(?-u:\b)|(?-u:\b)(?:use|enter)(?-u:\b)([^\r\n\u{2028}\u{2029}]{0,80})(?-u:\b)(?:sign[ -]?in|verify|authenticate)(?-u:\b)").unwrap()
});
static CANDIDATES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u:\b)(?:[0-9]{4,8}|[A-Z0-9]{6,8}|[0-9]{3}[ -][0-9]{3})(?-u:\b)").unwrap()
});
static MARKUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>|<").unwrap());
static FORMAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:</?(?:b|i|u)\s*>|<br\s*/?\s*>|</a\s*>)$").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)^<a\s+href=["'](https?://[^"'<>]+|mailto:[^"'<>]+)["']\s*>$"#).unwrap()
});

fn verification_code(entry: &Value) -> String {
    let text = format!("{} {}", text(entry.get("summary")), text(entry.get("body")));
    let text = TAGS.replace_all(&text, " ");
    let text = SPACE.replace_all(&text, " ");
    let text = URLS.replace_all(&text, " ");
    let text = DATES.replace_all(&text, " ");
    if !CONTEXT
        .captures_iter(&text)
        .any(|m| m.get(1).is_none_or(|v| utf16_len(v.as_str()) <= 80))
    {
        return String::new();
    }
    let mut code: Option<String> = None;
    for candidate in CANDIDATES.find_iter(&text) {
        let candidate: String = candidate
            .as_str()
            .chars()
            .filter(|c| *c != ' ' && *c != '-')
            .collect();
        if !candidate.bytes().any(|c| c.is_ascii_digit()) {
            continue;
        }
        match &code {
            Some(current) if current != &candidate => return String::new(),
            None => code = Some(candidate),
            _ => {}
        }
    }
    code.unwrap_or_default()
}
fn group_key(entry: &Value) -> String {
    let desktop = text(entry.get("desktop_entry"));
    let desktop = desktop.trim();
    let desktop = desktop
        .strip_suffix(".desktop")
        .unwrap_or(desktop)
        .to_lowercase();
    if !desktop.is_empty() {
        return format!("desktop:{desktop}");
    }
    let app = text(entry.get("app_name")).trim().to_lowercase();
    if !app.is_empty() {
        format!("app:{app}")
    } else {
        format!("id:{}", string(entry.get("id")))
    }
}
// Only stable application identities can be quieted. Anonymous senders retain
// their per-notification grouping but must never silence a reused numeric ID.
const MAX_QUIET_APPS: usize = 256;
const MAX_APP_KEY_BYTES: usize = 512;
fn valid_app_key(key: &str) -> bool {
    key.len() <= MAX_APP_KEY_BYTES
        && ["desktop:", "app:"].iter().any(|prefix| {
            key.strip_prefix(prefix)
                .is_some_and(|name| !name.trim().is_empty())
        })
        && !key.chars().any(char::is_control)
}
fn app_key(entry: &Value) -> String {
    let key = group_key(entry);
    if valid_app_key(&key) {
        key
    } else {
        String::new()
    }
}
fn stacked_rows(entries: &[Value], expanded: &Value) -> Value {
    let mut groups: Vec<(String, Vec<&Value>)> = Vec::new();
    let mut indices = HashMap::new();
    for entry in entries {
        let key = group_key(entry);
        let index = *indices.entry(key.clone()).or_insert_with(|| {
            let index = groups.len();
            groups.push((key, Vec::new()));
            index
        });
        groups[index].1.push(entry);
    }
    json!(groups.into_iter().map(|(key,items)| {let count=items.len();let expanded=count>1&&truthy(expanded.get(&key));json!({"key":key,"group":key,"items":items,"count":count,"expanded":expanded,"depth":if expanded {0} else {(count-1).min(2)}})}).collect::<Vec<_>>())
}
fn local_image(value: Option<&Value>) -> String {
    let source = text(value);
    if !source.starts_with("//")
        && (source.starts_with("image://")
            || source.starts_with("file:///")
            || source.starts_with('/'))
    {
        source
    } else {
        String::new()
    }
}
fn body_markup(value: Option<&Value>) -> String {
    MARKUP
        .replace_all(&text(value), |matched: &regex::Captures<'_>| {
            let tag = &matched[0];
            if tag == "<" {
                "&lt;".into()
            } else if FORMAT.is_match(tag) {
                tag.into()
            } else if let Some(link) = LINK.captures(tag) {
                format!("<a href=\"{}\">", link[1].replace('"', "&quot;"))
            } else {
                String::new()
            }
        })
        .into_owned()
}
fn from_native(n: &Value, now: Option<&Value>) -> Value {
    let hints = &n["hints"];
    let progress = number(hints.get("value"));
    let tag = hints
        .get("x-dunst-stack-tag")
        .filter(|v| truthy(Some(v)))
        .or_else(|| hints.get("x-canonical-private-synchronous"));
    json!({"id":n["id"],"app_name":n["appName"],"app_icon":n["appIcon"],"desktop_entry":n["desktopEntry"],"summary":n["summary"],"body":n["body"],"action_icons":truthy(n.get("hasActionIcons")),"image":local_image(n.get("image")),"urgency":number(n.get("urgency")),"resident":truthy(n.get("resident")),"transient":truthy(n.get("transient")),"timeout":number(n.get("expireTimeout")),"time":now,"progress":if progress.is_finite() {progress.clamp(0.0,100.0)} else {-1.0},"tag":text(tag),"pinned":false})
}
// A quiet period is one length being spent, so the panel is handed what it
// should draw -- which of the three states silence is in, what is left of the
// period and how much of it that is -- instead of three raw numbers it would
// have to do countdown arithmetic on at the call site. The wall-clock end
// stays with Qt, which owns dates and the user's locale.
fn quiet_minutes(remaining: f64) -> u64 {
    if remaining.is_finite() {
        (remaining / 60.0).ceil().max(0.0) as u64
    } else {
        0
    }
}
// What is left of a period, short enough to ride beside a glyph on the control
// that opens the menu.
fn quiet_span(remaining: f64) -> String {
    let minutes = quiet_minutes(remaining);
    match minutes {
        0 => String::new(),
        1..=59 => format!("{minutes}m"),
        _ if minutes.is_multiple_of(60) => format!("{}h", minutes / 60),
        _ => format!("{}h {}m", minutes / 60, minutes % 60),
    }
}
fn quiet_label(remaining: f64) -> String {
    if quiet_minutes(remaining) == 0 {
        "Ending".to_owned()
    } else {
        format!("{} left", quiet_span(remaining))
    }
}
fn quiet_period(dnd: bool, until: f64, minutes: f64, now: f64) -> Value {
    let span = minutes * 60.0;
    let timed = dnd && until > 0.0 && span > 0.0 && now.is_finite();
    if !timed {
        let (state, label) = if dnd {
            ("held", "Until switched off")
        } else {
            ("off", "")
        };
        return json!({"state":state,"minutes":0.0,"remaining":0.0,"label":label,"compact":""});
    }
    // A clock that jumped keeps the countdown inside the length that was asked
    // for rather than reporting a longer period than the one that is running.
    let remaining = (until - now).clamp(0.0, span);
    json!({"state":"timed","minutes":minutes,"remaining":remaining,"label":quiet_label(remaining),"compact":quiet_span(remaining)})
}
fn permanent(entry: &Value) -> bool {
    truthy(entry.get("pinned"))
        || truthy(entry.get("reminder"))
        || entry.get("timeout").and_then(Value::as_f64) == Some(0.0)
        || entry.get("urgency").and_then(Value::as_f64) == Some(2.0)
}
fn popup_duration(entry: &Value) -> f64 {
    if permanent(entry) {
        -1.0
    } else {
        let timeout = number(entry.get("timeout"));
        if timeout > 0.0 {
            timeout / 1000.0
        } else {
            30.0
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct State {
    current: Vec<Record>,
    history: Vec<Value>,
    dnd: bool,
    #[serde(default)]
    quiet_apps: BTreeSet<String>,
    dnd_until: f64,
    dnd_minutes: f64,
    /// Session-only. One configured pull request is in focus; not written to disk.
    #[serde(default)]
    focus: bool,
    paused: bool,
    last_tick: f64,
    restored: serde_json::Map<String, Value>,
    #[cfg(test)]
    #[serde(skip)]
    drop_marker: DropMarker,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    entry: Value,
    remaining: f64,
    clock: f64,
    popup: bool,
    #[serde(default)]
    deferred: bool,
    skip_history: bool,
}
fn mentions(entry: &Value) -> bool {
    let summary = text(entry.get("summary"));
    let body = text(entry.get("body"));
    let combined = format!("{summary}\n{body}");
    if combined.to_ascii_lowercase().contains("mentioned you") {
        return true;
    }
    let chars: Vec<char> = combined.chars().collect();
    for (index, character) in chars.iter().enumerate() {
        if *character != '@' {
            continue;
        }
        let boundary = index == 0
            || !(chars[index - 1].is_ascii_alphanumeric()
                || matches!(chars[index - 1], '.' | '_' | '-' | '+'));
        if boundary
            && chars
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphanumeric())
        {
            return true;
        }
    }
    false
}
fn hold_for_focus(state: &State, entry: &Value, generation: bool) -> bool {
    state.focus && !generation && !mentions(entry)
}
// Bounds account for JSON escaping before cloning or serializing retained text.
const MAX_ENTRIES: usize = 4096;
const MAX_ENTRY_BYTES: usize = 256 * 1024;
const MAX_RETAINED_BYTES: usize = 4 * 1024 * 1024;
// A reminder is for later today or the next few days, not a calendar. The
// notification it belongs to lives in memory, so a longer promise could not be
// kept across the restart that would eventually come first anyway.
const MAX_REMINDER_DELAY: f64 = 7.0 * 86400.0;
// Reminder state rides on the entry, so the panel reads it where it reads
// everything else about the notification and a replacement can carry it over.
const REMINDER_KEYS: [&str; 2] = ["remind_at", "reminder"];
fn reminder_at(entry: &Value) -> Option<f64> {
    entry
        .get("remind_at")
        .and_then(Value::as_f64)
        .filter(|at| at.is_finite() && *at > 0.0)
}
fn forget_reminder(entry: &mut Value) {
    if let Some(entry) = entry.as_object_mut() {
        for key in REMINDER_KEYS {
            entry.remove(key);
        }
    }
}
fn weight(value: &Value) -> usize {
    32usize.saturating_add(match value {
        Value::String(text) => text.len().saturating_mul(6),
        Value::Array(values) => values.iter().map(weight).fold(0, usize::saturating_add),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| key.len().saturating_mul(6).saturating_add(weight(value)))
            .fold(0, usize::saturating_add),
        _ => 0,
    })
}
impl State {
    fn make_room(&mut self, entry: &Value, effects: &mut Vec<Value>) {
        let id = string(entry.get("id"));
        let mut count = self
            .current
            .iter()
            .filter(|r| string(r.entry.get("id")) != id)
            .count();
        let mut bytes = self
            .current
            .iter()
            .filter(|r| string(r.entry.get("id")) != id)
            .map(|r| weight(&r.entry))
            .sum::<usize>()
            + self.history.iter().map(weight).sum::<usize>()
            + weight(entry);
        while bytes > MAX_RETAINED_BYTES {
            let Some(old) = self.history.pop() else {
                break;
            };
            bytes = bytes.saturating_sub(weight(&old));
        }
        while bytes > MAX_RETAINED_BYTES || count >= MAX_ENTRIES {
            let Some(index) = self
                .current
                .iter()
                .rposition(|r| string(r.entry.get("id")) != id)
            else {
                break;
            };
            let old = self.current.remove(index);
            bytes = bytes.saturating_sub(weight(&old.entry));
            count -= 1;
            effects.push(json!({"operation":"dismiss","id":old.entry["id"]}));
        }
    }
    fn find(&self, id: Option<&Value>) -> Option<usize> {
        let id = string(id);
        self.current
            .iter()
            .position(|record| string(record.entry.get("id")) == id)
    }
    fn view(&self) -> Value {
        let items: Vec<_> = self
            .current
            .iter()
            .filter(|r| !truthy(r.entry.get("transient")))
            .map(|r| &r.entry)
            .collect();
        json!({"count":items.len(),"items":items,"popups":self.current.iter().filter(|r|r.popup).map(|r|&r.entry).collect::<Vec<_>>(),"history":self.history,"dndUntil":self.dnd_until,"dndMinutes":self.dnd_minutes,"quietApps":self.quiet_apps})
    }
    fn save(&self) -> Value {
        let metadata: serde_json::Map<_, _> = self.current.iter().map(|record| (string(record.entry.get("id")), json!({"time":record.entry["time"],"pinned":record.entry["pinned"],"popup":record.popup&&permanent(&record.entry),"remindAt":reminder_at(&record.entry),"reminder":truthy(record.entry.get("reminder"))}))).collect();
        json!({"history":self.history,"dnd":self.dnd,"dndUntil":self.dnd_until,"dndMinutes":self.dnd_minutes,"metadata":metadata,"quietApps":self.quiet_apps})
    }
    fn advance(&mut self, timestamp: f64, effects: &mut Vec<Value>) {
        let mut changed = false;
        self.last_tick = timestamp;
        if self.dnd_until > 0.0 && timestamp >= self.dnd_until {
            self.dnd = false;
            self.dnd_until = 0.0;
            self.dnd_minutes = 0.0;
            changed = true;
        }
        // A reminder that comes due while the shell is quiet waits for the
        // quiet to end rather than being spent on a toast nobody is shown.
        // Silencing one application does not hold it: the reminder was asked
        // for by the user, not sent by the app.
        let mut due = Vec::new();
        let mut index = 0;
        while index < self.current.len() {
            if !self.dnd
                && reminder_at(&self.current[index].entry).is_some_and(|at| at <= timestamp)
            {
                due.push(self.current.remove(index));
            } else {
                index += 1;
            }
        }
        let returned = due.len();
        for mut record in due.into_iter().rev() {
            forget_reminder(&mut record.entry);
            record.entry["reminder"] = json!(true);
            record.popup = true;
            record.remaining = popup_duration(&record.entry);
            record.clock = timestamp;
            self.current.insert(0, record);
            changed = true;
        }
        for record in &self.current[..returned] {
            effects.push(json!({"operation":"arrived","entry":record.entry,"fresh":false}));
        }
        for record in &mut self.current {
            let elapsed = (timestamp - record.clock).max(0.0);
            record.clock = timestamp;
            // A held notification waits out focus. Counting it down here would
            // expire a transient one before it is ever shown.
            if record.deferred {
                continue;
            }
            if self.paused
                || record.remaining < 0.0
                || !record.popup && !truthy(record.entry.get("transient"))
            {
                continue;
            }
            record.remaining -= elapsed;
            if record.remaining > 0.0 {
                continue;
            }
            record.popup = false;
            changed = true;
            if truthy(record.entry.get("transient")) {
                effects.push(json!({"operation":"expire","id":record.entry["id"]}));
            }
        }
        let before = self.history.len();
        self.history
            .retain(|item| number(item.get("time")) > timestamp - 86400.0);
        if changed || before != self.history.len() {
            effects.push(json!({"operation":"publish"}));
        }
    }
    fn retire(&mut self, index: usize, effects: &mut Vec<Value>) {
        let record = &mut self.current[index];
        record.popup = false;
        // Hiding the toast is the answer to the reminder, so it stops holding
        // the notification permanent and the panel stops calling it one.
        if let Some(entry) = record.entry.as_object_mut() {
            entry.remove("reminder");
        }
        if truthy(record.entry.get("transient")) {
            effects.push(json!({"operation":"dismiss","id":record.entry["id"]}));
        }
        effects.push(json!({"operation":"publish"}));
    }
}
#[cfg(test)]
#[derive(Default)]
struct DropMarker(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);
#[cfg(test)]
impl Drop for DropMarker {
    fn drop(&mut self) {
        if let Some(marker) = &self.0 {
            marker.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
fn timestamp(value: Option<&Value>) -> Result<f64, String> {
    let value = number(value);
    if value.is_finite() {
        Ok(value)
    } else {
        Err("invalid notification timestamp".into())
    }
}
fn transition(state: &mut State, event: &str, args: &[Value]) -> Result<Value, String> {
    let mut effects = Vec::new();
    let mut result = Value::Null;
    match event {
        "restore" => {
            let saved = args.first().unwrap_or(&Value::Null);
            if truthy(Some(saved)) {
                state.quiet_apps = array(saved.get("quietApps"))
                    .iter()
                    .take(MAX_QUIET_APPS)
                    .filter_map(Value::as_str)
                    .filter(|key| valid_app_key(key))
                    .map(str::to_owned)
                    .collect();
                state.history.clear();
                let mut bytes = state
                    .current
                    .iter()
                    .map(|r| weight(&r.entry))
                    .sum::<usize>();
                for entry in array(saved.get("history")).iter().take(100) {
                    let size = weight(entry);
                    if !entry.is_object()
                        || size > MAX_ENTRY_BYTES
                        || bytes.saturating_add(size) > MAX_RETAINED_BYTES
                    {
                        continue;
                    }
                    state.history.push(entry.clone());
                    bytes += size;
                }
                state.dnd = truthy(saved.get("dnd"));
                state.dnd_until = saved
                    .get("dndUntil")
                    .and_then(Value::as_f64)
                    .filter(|until| state.dnd && *until > 0.0)
                    .unwrap_or(0.0);
                let minutes = number(saved.get("dndMinutes"));
                state.dnd_minutes = if state.dnd_until > 0.0 && minutes.is_finite() {
                    minutes
                } else {
                    0.0
                };
                if state.dnd_until > 0.0 && state.dnd_until <= state.last_tick {
                    state.dnd = false;
                    state.dnd_until = 0.0;
                    state.dnd_minutes = 0.0;
                }
                state.restored.clear();
                if let Some(metadata) = saved.get("metadata").and_then(Value::as_object) {
                    for (id, entry) in metadata.iter().take(MAX_ENTRIES) {
                        if id.parse::<u32>().is_err() {
                            continue;
                        }
                        let Some(time) = entry
                            .get("time")
                            .and_then(Value::as_f64)
                            .filter(|time| time.is_finite())
                        else {
                            continue;
                        };
                        let remind_at = entry
                            .get("remindAt")
                            .and_then(Value::as_f64)
                            .filter(|at| at.is_finite() && *at > 0.0);
                        state.restored.insert(id.clone(),json!({"time":time,"pinned":truthy(entry.get("pinned")),"popup":truthy(entry.get("popup")),"remindAt":remind_at,"reminder":truthy(entry.get("reminder"))}));
                    }
                }
                effects.push(json!({"operation":"publish"}));
            }
        }
        "receive" => {
            let candidate = args
                .first()
                .filter(|v| v.is_object())
                .ok_or("invalid notification entry")?;
            if weight(candidate) > MAX_ENTRY_BYTES {
                return Ok(
                    json!({"effects":[{"operation":"dismiss","id":candidate["id"]}],"result":false,"dnd":state.dnd,"dndUntil":state.dnd_until,"dndMinutes":state.dnd_minutes}),
                );
            }
            let mut entry = candidate.clone();
            let time = timestamp(args.get(1))?;
            let generation = truthy(args.get(2));
            let suppress_popup = truthy(args.get(3));
            state.make_room(&entry, &mut effects);
            if let Some(index) = state.find(entry.get("id")) {
                let mut record = state.current.remove(index);
                entry["pinned"] = record.entry["pinned"].clone();
                // A sender updating its notification in place keeps the
                // reminder the user set on it.
                for key in REMINDER_KEYS {
                    if let Some(value) = record.entry.get(key) {
                        entry[key] = value.clone();
                    }
                }
                let fresh = entry["summary"] != record.entry["summary"]
                    || entry["body"] != record.entry["body"];
                let duration = entry["timeout"] != record.entry["timeout"]
                    || entry["urgency"] != record.entry["urgency"];
                entry["time"] = if fresh {
                    json!(time)
                } else {
                    record.entry["time"].clone()
                };
                record.entry = entry;
                if fresh {
                    let held = hold_for_focus(state, &record.entry, false);
                    record.deferred = held;
                    record.popup = !held
                        && !state.dnd
                        && !suppress_popup
                        && !state.quiet_apps.contains(&group_key(&record.entry));
                    record.remaining = popup_duration(&record.entry);
                    record.clock = time;
                    if record.popup {
                        effects.push(
                            json!({"operation":"arrived","entry":record.entry,"fresh":false}),
                        );
                    }
                } else if duration {
                    record.remaining = popup_duration(&record.entry);
                    record.clock = time;
                }
                state.current.insert(if fresh { 0 } else { index }, record);
            } else {
                if truthy(entry.get("tag")) {
                    let key = group_key(&entry);
                    for other in &mut state.current {
                        if other.entry["tag"] == entry["tag"] && group_key(&other.entry) == key {
                            other.skip_history = true;
                            effects.push(json!({"operation":"dismiss","id":other.entry["id"]}));
                        }
                    }
                }
                let restored = if generation {
                    state.restored.remove(&string(entry.get("id")))
                } else {
                    None
                };
                if let Some(saved) = &restored {
                    entry["time"] = saved["time"].clone();
                    entry["pinned"] = json!(truthy(saved.get("pinned")));
                    if let Some(at) = saved.get("remindAt").and_then(Value::as_f64) {
                        entry["remind_at"] = json!(at);
                    }
                    if truthy(saved.get("reminder")) {
                        entry["reminder"] = json!(true);
                    }
                }
                let held = hold_for_focus(state, &entry, generation);
                let popup = !held
                    && !state.dnd
                    && !suppress_popup
                    && if generation {
                        // Reload restores an existing permanent toast, not a new
                        // arrival. Silencing its app must not discard that toast.
                        restored
                            .as_ref()
                            .is_some_and(|saved| truthy(saved.get("popup")))
                    } else {
                        !state.quiet_apps.contains(&group_key(&entry))
                    };
                if popup {
                    effects.push(json!({"operation":"arrived","entry":entry,"fresh":true}));
                }
                state.current.insert(0, Record {
                    remaining: popup_duration(&entry),
                    entry,
                    clock: time,
                    popup,
                    deferred: held,
                    skip_history: false,
                });
            }
            effects.push(json!({"operation":"publish"}));
        }
        "closed" => {
            if let Some(index) = state.find(args.first()) {
                let mut record = state.current.remove(index);
                if !record.skip_history
                    && !truthy(record.entry.get("transient"))
                    && args.get(1).and_then(Value::as_i64) == Some(2)
                {
                    record.entry["actions"] = json!({});
                    record.entry["image"] = json!("");
                    record.entry["pinned"] = json!(false);
                    // A reminder belongs to a notification still waiting. Once
                    // it is dismissed or withdrawn there is nothing to return.
                    forget_reminder(&mut record.entry);
                    state.history.insert(0, record.entry);
                    state.history.truncate(100);
                }
                effects.push(json!({"operation":"publish"}));
            }
        }
        "advance" => state.advance(timestamp(args.first())?, &mut effects),
        "pause" => {
            state.advance(timestamp(args.get(1))?, &mut effects);
            state.paused = truthy(args.first());
        }
        "retire" | "dismiss" | "pin" => {
            result = json!(false);
            if let Some(index) = state.find(args.first()) {
                result = json!(true);
                match event {
                    "retire" => state.retire(index, &mut effects),
                    "dismiss" => effects
                        .push(json!({"operation":"dismiss","id":state.current[index].entry["id"]})),
                    _ => {
                        let record = &mut state.current[index];
                        let pinned = !truthy(record.entry.get("pinned"));
                        record.entry["pinned"] = json!(pinned);
                        record.remaining = popup_duration(&record.entry);
                        if pinned && !state.dnd {
                            record.popup = true;
                            effects.push(
                                json!({"operation":"arrived","entry":record.entry,"fresh":false}),
                            );
                        }
                        effects.push(json!({"operation":"publish"}));
                    }
                }
            }
        }
        // Sets or cancels the one reminder a notification can carry. `due` is an
        // absolute time, so Qt keeps local dates ("tomorrow morning") and the
        // policy only checks that the time is ahead and within reach. A null
        // or zero time cancels.
        "remind" => {
            result = json!(false);
            let now = timestamp(args.get(2))?;
            let due = args.get(1).and_then(Value::as_f64).unwrap_or(0.0);
            if let Some(index) = state.find(args.first()) {
                let record = &mut state.current[index];
                let cancel = due == 0.0;
                let valid = due.is_finite() && due > now && due - now <= MAX_REMINDER_DELAY;
                if !truthy(record.entry.get("transient")) && (cancel || valid) {
                    if let Some(entry) = record.entry.as_object_mut() {
                        entry.remove("remind_at");
                    }
                    if !cancel {
                        record.entry["remind_at"] = json!(due);
                    }
                    result = json!(true);
                    effects.push(json!({"operation":"publish"}));
                }
            }
        }
        "setAppQuiet" => {
            let key = text(args.first());
            let quiet = truthy(args.get(1));
            // Admission comes from an actual inbox/history group, not arbitrary
            // caller strings. Removal remains possible after that group is gone.
            let known = state.current.iter().any(|r| app_key(&r.entry) == key)
                || state.history.iter().any(|entry| app_key(entry) == key);
            let accepted = valid_app_key(&key)
                && (!quiet || known)
                && (!quiet
                    || state.quiet_apps.contains(&key)
                    || state.quiet_apps.len() < MAX_QUIET_APPS);
            result = json!(accepted);
            if accepted {
                if quiet {
                    state.quiet_apps.insert(key);
                } else {
                    state.quiet_apps.remove(&key);
                }
                // Existing toasts and sender lifetimes stay untouched. Turning
                // silence off never replays messages received while quiet.
                effects.push(json!({"operation":"publish"}));
            }
        }
        "resumeApps" => {
            state.quiet_apps.clear();
            effects.push(json!({"operation":"publish"}));
        }
        "setPrFocus" => {
            let enabled = truthy(args.first());
            let time = timestamp(args.get(1))?;
            result = json!(true);
            if state.focus != enabled {
                state.focus = enabled;
                if !enabled {
                    let quiet_apps = state.quiet_apps.clone();
                    let dnd = state.dnd;
                    for record in &mut state.current {
                        if !record.deferred {
                            continue;
                        }
                        record.deferred = false;
                        if dnd || quiet_apps.contains(&group_key(&record.entry)) {
                            continue;
                        }
                        record.popup = true;
                        record.remaining = popup_duration(&record.entry);
                        record.clock = time;
                        effects.push(
                            json!({"operation":"arrived","entry":record.entry,"fresh":false}),
                        );
                    }
                }
                effects.push(json!({"operation":"publish"}));
            }
        }
        "setDnd" => {
            state.dnd_until = 0.0;
            state.dnd_minutes = 0.0;
            state.dnd = truthy(args.first());
            if state.dnd {
                for record in &mut state.current {
                    record.popup = false;
                }
            }
            effects.push(json!({"operation":"publish"}));
        }
        "snooze" => {
            let minutes = number(args.first());
            let time = number(args.get(1));
            result = json!(
                minutes.is_finite()
                    && minutes.fract() == 0.0
                    && (1.0..=1440.0).contains(&minutes)
                    && time.is_finite()
                    && time >= 0.0
            );
            if result == true {
                state.dnd = true;
                state.dnd_until = time + minutes * 60.0;
                state.dnd_minutes = minutes;
                for record in &mut state.current {
                    record.popup = false;
                }
                effects.push(json!({"operation":"publish"}));
            }
        }
        "clear" => {
            if truthy(args.first()) {
                state.history.clear();
                effects.push(json!({"operation":"publish"}));
            } else {
                for record in &state.current {
                    effects.push(json!({"operation":"dismiss","id":record.entry["id"]}));
                }
            }
        }
        "group" => {
            let key = string(args.first());
            for index in 0..state.current.len() {
                if group_key(&state.current[index].entry) != key {
                    continue;
                }
                if truthy(args.get(1)) {
                    state.retire(index, &mut effects);
                } else {
                    effects
                        .push(json!({"operation":"dismiss","id":state.current[index].entry["id"]}));
                }
            }
        }
        _ => return Err("unknown notification transition".into()),
    }
    Ok(
        json!({"effects":effects,"result":result,"dnd":state.dnd,"dndUntil":state.dnd_until,"dndMinutes":state.dnd_minutes}),
    )
}

pub(crate) fn new_state(now: f64) -> Option<Box<State>> {
    now.is_finite().then(|| {
        Box::new(State {
            last_tick: now,
            ..State::default()
        })
    })
}
pub(crate) fn state_call(
    state: &mut State,
    operation: &str,
    args: &[Value],
) -> Result<Value, String> {
    match operation {
        "view" => Ok(state.view()),
        "save" => Ok(state.save()),
        _ => transition(state, operation, args),
    }
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let first = args.first().unwrap_or(&Value::Null);
    Ok(match function {
        "actions" => json!(
            array(args.get(1))
                .iter()
                .filter_map(|key| {
                    let key = key.as_str()?;
                    if key == "default" {
                        return None;
                    }
                    let label = first.get("actions")?.as_object()?.get(key)?.as_str()?;
                    Some(json!({"key":key,"label":if label.is_empty() {key} else {label}}))
                })
                .collect::<Vec<_>>()
        ),
        "verificationCode" => json!(verification_code(first)),
        "groupKey" => json!(group_key(first)),
        "appQuiet" => {
            let key = app_key(first);
            let quiet = array(args.get(1)).iter().any(|v| v.as_str() == Some(&key));
            json!({"key":key,"quiet":quiet,"available":!key.is_empty() && (quiet || array(args.get(1)).len() < MAX_QUIET_APPS)})
        }
        "stackedRows" => stacked_rows(array(args.first()), args.get(1).unwrap_or(&Value::Null)),
        "localImage" => json!(local_image(args.first())),
        "imageRoles" => {
            let profile = local_image(first.get("image"));
            let app = text(first.get("app_icon")).trim().to_owned();
            json!({"profile":profile,"icon":if profile.is_empty() {app.as_str()} else {""},"badge":if profile.is_empty() {""} else {app.as_str()}})
        }
        "bodyMarkup" => json!(body_markup(args.first())),
        "fromNative" => from_native(first, args.get(1)),
        "permanent" => json!(permanent(first)),
        "popupDuration" => json!(popup_duration(first)),
        "quietPeriod" => quiet_period(
            truthy(args.first()),
            number(args.get(1)),
            number(args.get(2)),
            number(args.get(3)),
        ),
        _ => return Err("unknown notifications function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transient_focus_quiet_keeps_calendar_reminders_in_panel() {
        let mut state = new_state(1000.0).unwrap();
        let reminder = json!({"id":1,"app_name":"Seele Calendar","summary":"Meeting","timeout":-1,"urgency":1});
        state_call(&mut state, "receive", &[
            reminder,
            json!(1000),
            json!(false),
            json!(true),
        ])
        .unwrap();
        assert_eq!(state.current.len(), 1);
        assert!(!state.current[0].popup);
        assert!(state.quiet_apps.is_empty());
        let next =
            json!({"id":2,"app_name":"Seele Calendar","summary":"Next","timeout":-1,"urgency":1});
        state_call(&mut state, "receive", &[
            next,
            json!(1001),
            json!(false),
            json!(false),
        ])
        .unwrap();
        assert!(state.current[0].popup);
        assert!(!state.current[1].popup);
    }
    fn chat(id: u32, summary: &str) -> Value {
        json!({"id":id,"app_name":"Chat","summary":summary,"body":"","timeout":-1,"urgency":1,"transient":false,"actions":{},"pinned":false,"time":1000})
    }
    fn arrivals(response: &Value) -> Vec<Value> {
        response["effects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["operation"] == "arrived")
            .map(|e| e["entry"]["id"].clone())
            .collect()
    }
    #[test]
    fn reminders_return_a_waiting_notification_once() {
        let mut state = new_state(1000.0).unwrap();
        for id in [1, 2] {
            state_call(
                &mut state,
                "receive",
                &[chat(id, "Hello"), json!(1000), json!(false)],
            )
            .unwrap();
        }
        state_call(&mut state, "advance", &[json!(1031)]).unwrap();
        assert!(state.current.iter().all(|r| !r.popup));
        let remind = |state: &mut State, id: u32, due: Value, now: f64| {
            state_call(state, "remind", &[json!(id), due, json!(now)]).unwrap()["result"].clone()
        };
        // Only a time ahead, within a week, on a notification still waiting.
        assert_eq!(remind(&mut state, 1, json!(1031), 1031.0), false);
        assert_eq!(
            remind(
                &mut state,
                1,
                json!(1031.0 + MAX_REMINDER_DELAY + 1.0),
                1031.0
            ),
            false
        );
        assert_eq!(remind(&mut state, 9, json!(2000), 1031.0), false);
        assert_eq!(remind(&mut state, 1, json!(2000), 1031.0), true);
        assert_eq!(state.view()["items"][1]["remind_at"], json!(2000.0));
        // An in-place update from the sender keeps the reminder.
        state_call(
            &mut state,
            "receive",
            &[chat(1, "Hello again"), json!(1500), json!(false)],
        )
        .unwrap();
        assert_eq!(state.current[0].entry["remind_at"], json!(2000.0));
        assert!(
            arrivals(&state_call(&mut state, "advance", &[json!(1999)]).unwrap()).is_empty(),
            "nothing returns before its time"
        );
        // Due: the notification moves to the top and toasts until it is hidden.
        let response = state_call(&mut state, "advance", &[json!(2000)]).unwrap();
        assert_eq!(arrivals(&response), vec![json!(1)]);
        assert_eq!(state.current[0].entry["id"], 1);
        assert!(state.current[0].popup && state.current[0].entry["reminder"] == true);
        assert!(state.current[0].entry.get("remind_at").is_none());
        state_call(&mut state, "advance", &[json!(9000)]).unwrap();
        assert!(state.current[0].popup, "a reminder toast does not time out");
        assert!(arrivals(&state_call(&mut state, "advance", &[json!(9001)]).unwrap()).is_empty());
        state_call(&mut state, "retire", &[json!(1)]).unwrap();
        assert!(!state.current[0].popup && state.current[0].entry.get("reminder").is_none());
        // Cancelling clears it; zero is the cancel value.
        assert_eq!(remind(&mut state, 2, json!(9500), 9001.0), true);
        assert_eq!(remind(&mut state, 2, json!(0), 9001.0), true);
        assert!(arrivals(&state_call(&mut state, "advance", &[json!(9600)]).unwrap()).is_empty());
    }
    #[test]
    fn reminders_wait_out_quiet_and_end_with_their_notification() {
        let mut state = new_state(1000.0).unwrap();
        for id in [1, 2] {
            state_call(
                &mut state,
                "receive",
                &[chat(id, "Hello"), json!(1000), json!(false)],
            )
            .unwrap();
            state_call(&mut state, "remind", &[json!(id), json!(1100), json!(1000)]).unwrap();
        }
        state_call(&mut state, "setAppQuiet", &[json!("app:chat"), json!(true)]).unwrap();
        state_call(&mut state, "snooze", &[json!(5), json!(1000)]).unwrap();
        assert!(arrivals(&state_call(&mut state, "advance", &[json!(1200)]).unwrap()).is_empty());
        assert_eq!(
            state.current[1].entry["remind_at"],
            json!(1100.0),
            "a due reminder is held while quiet"
        );
        // The quiet period ends at 1300; application silence does not hold it.
        let response = state_call(&mut state, "advance", &[json!(1300)]).unwrap();
        assert_eq!(arrivals(&response), vec![json!(2), json!(1)]);
        assert_eq!(
            state.current[0].entry["id"], 2,
            "due reminders keep their order at the top"
        );
        // Dismissing or withdrawing the notification ends its reminder.
        state_call(
            &mut state,
            "receive",
            &[chat(3, "Later"), json!(1300), json!(false)],
        )
        .unwrap();
        state_call(&mut state, "remind", &[json!(3), json!(1400), json!(1300)]).unwrap();
        state_call(&mut state, "closed", &[json!(3), json!(2)]).unwrap();
        assert!(
            state.history[0].get("remind_at").is_none()
                && state.history[0].get("reminder").is_none()
        );
        assert!(arrivals(&state_call(&mut state, "advance", &[json!(1500)]).unwrap()).is_empty());
    }
    #[test]
    fn reminders_survive_a_shell_reload() {
        let mut state = new_state(1000.0).unwrap();
        state_call(
            &mut state,
            "receive",
            &[chat(1, "Hello"), json!(1000), json!(false)],
        )
        .unwrap();
        state_call(&mut state, "remind", &[json!(1), json!(1100), json!(1000)]).unwrap();
        let saved = state.save();
        let mut reloaded = new_state(1050.0).unwrap();
        state_call(&mut reloaded, "restore", &[saved]).unwrap();
        state_call(
            &mut reloaded,
            "receive",
            &[chat(1, "Hello"), json!(1050), json!(true)],
        )
        .unwrap();
        assert_eq!(reloaded.current[0].entry["remind_at"], json!(1100.0));
        assert_eq!(
            arrivals(&state_call(&mut reloaded, "advance", &[json!(1100)]).unwrap()),
            vec![json!(1)]
        );
    }
    #[test]
    fn pull_request_focus_defers_unrelated_notifications_and_keeps_mentions() {
        let mut state = new_state(1000.0).unwrap();
        let ordinary = json!({"id":1,"app_name":"Mail","summary":"Build finished","body":"user@example.com passed","timeout":5000,"urgency":1});
        state_call(&mut state, "receive", &[
            ordinary.clone(),
            json!(1000),
            json!(false),
        ])
        .unwrap();
        assert!(
            state.current[0].popup,
            "focus off still shows an ordinary toast"
        );
        state_call(&mut state, "setPrFocus", &[json!(true), json!(1001)]).unwrap();
        assert!(
            state.current[0].popup,
            "entering focus leaves a toast already on screen"
        );
        let unrelated = json!({"id":2,"app_name":"Chat","summary":"Standup moved","body":"See the calendar","timeout":5000,"urgency":1});
        state_call(&mut state, "receive", &[
            unrelated,
            json!(1002),
            json!(false),
        ])
        .unwrap();
        let mention = json!({"id":3,"app_name":"GitHub","summary":"Review","body":"@silas please look","timeout":5000,"urgency":1});
        state_call(&mut state, "receive", &[mention, json!(1003), json!(false)]).unwrap();
        let phrase = json!({"id":4,"app_name":"GitHub","summary":"ada mentioned you in seele","body":"on the pull request","timeout":5000,"urgency":1});
        state_call(&mut state, "receive", &[phrase, json!(1004), json!(false)]).unwrap();
        let held = state
            .current
            .iter()
            .find(|record| record.entry["id"].as_u64() == Some(2))
            .unwrap();
        assert!(held.deferred);
        assert!(!held.popup);
        assert!(
            state.view()["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"].as_u64() == Some(2))
        );
        assert!(
            state
                .current
                .iter()
                .find(|record| record.entry["id"].as_u64() == Some(3))
                .unwrap()
                .popup
        );
        assert!(
            !state
                .current
                .iter()
                .find(|record| record.entry["id"].as_u64() == Some(3))
                .unwrap()
                .deferred
        );
        assert!(
            state
                .current
                .iter()
                .find(|record| record.entry["id"].as_u64() == Some(4))
                .unwrap()
                .popup
        );
        let transient = json!({"id":5,"app_name":"Volume","summary":"Muted","body":"","timeout":1000,"urgency":1,"transient":true});
        state_call(&mut state, "receive", &[
            transient,
            json!(1005),
            json!(false),
        ])
        .unwrap();
        state_call(&mut state, "advance", &[json!(1010)]).unwrap();
        assert!(
            state
                .current
                .iter()
                .any(|record| record.entry["id"].as_u64() == Some(5)),
            "a deferred transient is not expired unseen"
        );
        let released = state_call(&mut state, "setPrFocus", &[json!(false), json!(1011)]).unwrap();
        assert!(
            released["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|effect| effect["operation"] == "arrived"
                    && effect["entry"]["id"].as_u64() == Some(2))
        );
        assert!(
            state
                .current
                .iter()
                .find(|record| record.entry["id"].as_u64() == Some(2))
                .unwrap()
                .popup
        );
        assert!(state.current.iter().all(|record| !record.deferred));
        assert!(!state.focus);
        let saved = state_call(&mut state, "save", &[]).unwrap();
        assert!(
            saved.get("focus").is_none(),
            "focus is not stored with notification text"
        );
    }
    #[test]
    fn application_silence_is_bounded_and_resumes_at_capacity() {
        let mut state = new_state(1000.0).unwrap();
        let keys: Vec<_> = (0..MAX_QUIET_APPS + 10)
            .map(|n| format!("app:{n}"))
            .collect();
        state_call(&mut state, "restore", &[json!({"quietApps":keys})]).unwrap();
        assert_eq!(state.quiet_apps.len(), MAX_QUIET_APPS);
        let entry = json!({"id":1,"app_name":"new","summary":"hello","timeout":-1,"urgency":1});
        state_call(&mut state, "receive", &[
            entry.clone(),
            json!(1000),
            json!(false),
        ])
        .unwrap();
        assert_eq!(
            state_call(&mut state, "setAppQuiet", &[json!("app:new"), json!(true)]).unwrap()["result"],
            false
        );
        assert_eq!(
            call("appQuiet", &[entry, json!(state.quiet_apps)]).unwrap()["available"],
            false
        );
        assert_eq!(
            state_call(&mut state, "setAppQuiet", &[json!("app:0"), json!(false)]).unwrap()["result"],
            true
        );
        assert_eq!(
            state_call(&mut state, "setAppQuiet", &[json!("app:new"), json!(true)]).unwrap()["result"],
            true
        );
        state_call(&mut state, "restore", &[json!({"quietApps":["id:1","app:","app:\ninvalid",format!("app:{}","x".repeat(513)),"desktop:valid"]})]).unwrap();
        assert_eq!(
            state.quiet_apps,
            BTreeSet::from(["desktop:valid".to_owned()])
        );
        // Legacy in-memory snapshots without this optional field remain valid.
        state_call(&mut state, "restore", &[json!({"history":[],"dnd":true})]).unwrap();
        assert!(state.quiet_apps.is_empty());
        assert!(state.dnd);
    }
    #[test]
    fn opaque_state_drop_releases_owned_data() {
        let marker = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut state = new_state(0.0).unwrap();
        state.drop_marker = DropMarker(Some(marker.clone()));
        state
            .history
            .push(json!({"body":"only owned by this state"}));
        let pointer = Box::into_raw(state).cast();
        // SAFETY: this is the sole owner of the freshly allocated state.
        unsafe {
            crate::seele_notifications_free(pointer);
        }
        assert!(marker.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(std::sync::Arc::strong_count(&marker), 1);
    }
    #[test]
    fn retained_content_is_bounded_and_flood_evictions_are_explicit() {
        let mut state = new_state(0.0).unwrap();
        let mut evicted = 0;
        for id in 0..80 {
            let entry = json!({"id":id,"summary":"local","body":"x".repeat(32*1024),"timeout":0,"urgency":1,"transient":false,"time":0,"actions":{},"pinned":false});
            let response =
                state_call(&mut state, "receive", &[entry, json!(0), json!(false)]).unwrap();
            evicted += response["effects"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["operation"] == "dismiss")
                .count();
        }
        assert!(evicted > 0);
        assert!(state.current.len() < 80);
        assert!(
            state
                .current
                .iter()
                .map(|r| weight(&r.entry))
                .sum::<usize>()
                <= MAX_RETAINED_BYTES
        );
        assert!(state.history.is_empty());
        let before = state.current.len();
        let response = state_call(&mut state, "receive", &[
            json!({"id":999,"body":"x".repeat(MAX_ENTRY_BYTES)}),
            json!(0),
            json!(false),
        ])
        .unwrap();
        assert_eq!(
            response["effects"][0],
            json!({"operation":"dismiss","id":999})
        );
        assert_eq!(state.current.len(), before);
    }
    #[test]
    #[ignore = "reports local throughput; no timing threshold"]
    fn state_snapshot_cost() {
        for count in [0, 20, 100, 1000] {
            let mut state = State::default();
            for id in 0..count {
                state.current.push(Record { entry: json!({"id":id,"app_name":"Chat","summary":"A representative notification","body":"A local message that stays in memory.","actions":{"reply":"Reply"},"timeout":-1,"urgency":1,"transient":false,"time":1000}), remaining:30.0, clock:1000.0, popup:true, deferred:false, skip_history:false });
            }
            let snapshot = json!(state);
            let input=serde_json::to_vec(&json!({"operation":"notifications.transition","arguments":[snapshot,"advance",[1000.25]]})).unwrap();
            let iterations = 100;
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                let value: Value = serde_json::from_slice(&input).unwrap();
                let mut state: State =
                    serde_json::from_value(value["arguments"][0].clone()).unwrap();
                let response = state_call(&mut state, "advance", &[json!(1000.25)]).unwrap();
                std::hint::black_box(
                    serde_json::to_vec(&json!({"state":state,"response":response})).unwrap(),
                );
            }
            eprintln!(
                "notification snapshot: {count} entries, {} bytes, {:.2} us per complete Rust JSON roundtrip",
                input.len(),
                start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
            );
            let resident: State = serde_json::from_value(snapshot).unwrap();
            let pointer = Box::into_raw(Box::new(resident)).cast();
            let input =
                serde_json::to_vec(&json!({"operation":"advance","arguments":[1000.25]})).unwrap();
            let start = std::time::Instant::now();
            for _ in 0..1000 {
                // SAFETY: benchmark owns this state exclusively for the loop.
                unsafe {
                    let output =
                        crate::seele_notifications_call(pointer, input.as_ptr(), input.len());
                    std::hint::black_box(output.length);
                    crate::seele_qml_free(output);
                }
            }
            eprintln!(
                "notification resident: {count} entries, {} bytes, {:.2} us per complete Rust JSON roundtrip",
                input.len(),
                start.elapsed().as_secs_f64() * 1000.0
            );
            // SAFETY: release the unique benchmark owner after all calls.
            unsafe {
                crate::seele_notifications_free(pointer);
            }
        }
    }
}
