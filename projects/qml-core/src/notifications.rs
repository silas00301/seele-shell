//! Notification presentation policy. QObject lifetimes remain in the Qt host.
use crate::value::{array, number, string, text, truthy, utf16_len};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::LazyLock};

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
fn permanent(entry: &Value) -> bool {
    truthy(entry.get("pinned"))
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
    dnd_until: f64,
    dnd_minutes: f64,
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
    skip_history: bool,
}
// Bounds account for JSON escaping before cloning or serializing retained text.
const MAX_ENTRIES: usize = 4096;
const MAX_ENTRY_BYTES: usize = 256 * 1024;
const MAX_RETAINED_BYTES: usize = 4 * 1024 * 1024;
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
        json!({"count":items.len(),"items":items,"popups":self.current.iter().filter(|r|r.popup).map(|r|&r.entry).collect::<Vec<_>>(),"history":self.history,"dndUntil":self.dnd_until,"dndMinutes":self.dnd_minutes})
    }
    fn save(&self) -> Value {
        let metadata: serde_json::Map<_, _> = self.current.iter().map(|record| (string(record.entry.get("id")), json!({"time":record.entry["time"],"pinned":record.entry["pinned"],"popup":record.popup&&permanent(&record.entry)}))).collect();
        json!({"history":self.history,"dnd":self.dnd,"dndUntil":self.dnd_until,"dndMinutes":self.dnd_minutes,"metadata":metadata})
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
        for record in &mut self.current {
            let elapsed = (timestamp - record.clock).max(0.0);
            record.clock = timestamp;
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
                        state.restored.insert(id.clone(),json!({"time":time,"pinned":truthy(entry.get("pinned")),"popup":truthy(entry.get("popup"))}));
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
            state.make_room(&entry, &mut effects);
            if let Some(index) = state.find(entry.get("id")) {
                let mut record = state.current.remove(index);
                entry["pinned"] = record.entry["pinned"].clone();
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
                    record.popup = !state.dnd;
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
                }
                let popup = !state.dnd
                    && (!generation
                        || restored
                            .as_ref()
                            .is_some_and(|saved| truthy(saved.get("popup"))));
                if popup {
                    effects.push(json!({"operation":"arrived","entry":entry,"fresh":true}));
                }
                state.current.insert(
                    0,
                    Record {
                        remaining: popup_duration(&entry),
                        entry,
                        clock: time,
                        popup,
                        skip_history: false,
                    },
                );
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
        _ => return Err("unknown notifications function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let response = state_call(
            &mut state,
            "receive",
            &[
                json!({"id":999,"body":"x".repeat(MAX_ENTRY_BYTES)}),
                json!(0),
                json!(false),
            ],
        )
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
                state.current.push(Record { entry: json!({"id":id,"app_name":"Chat","summary":"A representative notification","body":"A local message that stays in memory.","actions":{"reply":"Reply"},"timeout":-1,"urgency":1,"transient":false,"time":1000}), remaining:30.0, clock:1000.0, popup:true, skip_history:false });
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
