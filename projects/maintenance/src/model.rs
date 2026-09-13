//! Typed lifecycle and the only persistence projection. Diagnostic and model
//! payloads have no serialization path into the persisted records.
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::LazyLock,
};

pub type Result<T> = std::result::Result<T, &'static str>;
pub const WEEK: f64 = 7.0 * 86400.0;
pub const CAPACITY: usize = 4096;
pub const SOURCES: [&str; 6] = [
    "systemd",
    "backups",
    "disk",
    "flake",
    "certificates",
    "inputs",
];
static KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.:@/-]{1,200}$").unwrap());
pub fn valid_key(key: &str) -> bool {
    KEY.is_match(key)
}
pub fn clean(value: &str, limit: usize) -> String {
    seele_runtime::redact::secrets(value, true)
        .chars()
        .filter(|c| (!c.is_control() || *c == '\n') && seele_runtime::redact::visible(*c))
        .take(limit)
        .collect()
}

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Keep existing persisted fingerprints across the runtime migration: canonical
/// metadata JSON uses sorted keys, ASCII escapes and the historic separators.
/// This prevents a restart from re-notifying every unchanged condition.
pub fn fingerprint(finding: &Finding) -> String {
    fn encode(value: &Value, output: &mut String) {
        match value {
            Value::Array(rows) => {
                output.push('[');
                for (index, row) in rows.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    encode(row, output);
                }
                output.push(']');
            }
            Value::Object(values) => {
                output.push('{');
                for (index, (key, value)) in values.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    encode(&json!(key), output);
                    output.push_str(": ");
                    encode(value, output);
                }
                output.push('}');
            }
            Value::String(text) => {
                for character in serde_json::to_string(text).unwrap().chars() {
                    if character.is_ascii() {
                        output.push(character);
                    } else {
                        let mut units = [0u16; 2];
                        for unit in character.encode_utf16(&mut units) {
                            output.push_str(&format!("\\u{unit:04x}"));
                        }
                    }
                }
            }
            _ => output.push_str(&value.to_string()),
        }
    }
    let value = json!([{ "title":finding.title,"explanation":finding.explanation,"details":finding.details,"urgency":finding.urgency,"lifecycle":finding.lifecycle,"actions":finding.actions },finding.diagnostic]);
    let mut bytes = String::new();
    encode(&value, &mut bytes);
    format!("{:x}", Sha256::digest(bytes.as_bytes()))
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Now,
    Soon,
    Eventually,
    Informational,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    #[default]
    Ongoing,
    Notice,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub details: String,
    pub urgency: Urgency,
    #[serde(default)]
    pub lifecycle: Lifecycle,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub diagnostic: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub action: String,
    pub at: f64,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub source: String,
    pub key: String,
    pub title: String,
    pub explanation: String,
    pub details: String,
    pub urgency: Urgency,
    pub lifecycle: Lifecycle,
    pub actions: Vec<String>,
    pub first_seen: f64,
    pub updated: f64,
    pub revision: u64,
    pub recurrence: u64,
    pub snoozed_until: f64,
    pub resolved: f64,
    #[serde(default)]
    pub outcomes: Vec<Outcome>,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(skip)]
    pub busy: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Action {
    pub label: &'static str,
    pub disruptive: bool,
}
pub type Registrations = BTreeMap<String, BTreeMap<String, Action>>;
pub fn registrations(config: &Value) -> Registrations {
    SOURCES
        .iter()
        .filter(|source| config[**source]["enabled"] != false)
        .map(|source| {
            let mut actions = BTreeMap::from([(
                "recheck".into(),
                Action {
                    label: "Recheck",
                    disruptive: false,
                },
            )]);
            if matches!(*source, "systemd" | "backups") {
                actions.insert(
                    "open-logs".into(),
                    Action {
                        label: "Open logs",
                        disruptive: false,
                    },
                );
            }
            if *source == "backups" {
                actions.insert(
                    "retry".into(),
                    Action {
                        label: "Retry backup",
                        disruptive: true,
                    },
                );
            }
            ((*source).into(), actions)
        })
        .collect()
}
#[derive(Clone)]
pub struct Inbox {
    pub registrations: Registrations,
    pub path: Option<PathBuf>,
    pub items: BTreeMap<String, Row>,
    pub diagnostics: BTreeMap<String, String>,
    pub analysis: BTreeMap<String, Value>,
}
impl Inbox {
    pub fn new(registrations: Registrations, path: Option<PathBuf>, now: f64) -> Result<Self> {
        let mut inbox = Self {
            registrations,
            path,
            items: BTreeMap::new(),
            diagnostics: BTreeMap::new(),
            analysis: BTreeMap::new(),
        };
        if let Some(path) = &inbox.path {
            match crate::read_private(path, 16 * 1024 * 1024) {
                Ok(bytes) => {
                    // Corrupt rows are dropped individually. Unknown fields never
                    // survive the typed projection, and persisted IDs are rebuilt.
                    if let Ok(Value::Array(rows)) = serde_json::from_slice(&bytes) {
                        if rows.len() <= CAPACITY {
                            for value in rows {
                                if let Ok(mut row) = serde_json::from_value::<Row>(value) {
                                    let Some(registry) = inbox.registrations.get(&row.source)
                                    else {
                                        continue;
                                    };
                                    if !valid_key(&row.key)
                                        || [
                                            row.first_seen,
                                            row.updated,
                                            row.snoozed_until,
                                            row.resolved,
                                        ]
                                        .iter()
                                        .any(|n| !n.is_finite() || *n < 0.0)
                                    {
                                        continue;
                                    }
                                    row.id = format!("{}:{}", row.source, row.key);
                                    row.title = clean(&row.title, 160);
                                    row.explanation = clean(&row.explanation, 500);
                                    row.details = clean(&row.details, 2000);
                                    row.actions.retain(|a| registry.contains_key(a));
                                    row.actions.dedup();
                                    row.outcomes.retain(|o| {
                                        (registry.contains_key(&o.action) || o.action == "analyze")
                                            && o.at.is_finite()
                                            && o.at >= 0.0
                                            && o.at >= now - WEEK
                                    });
                                    if row.outcomes.len() > 20 {
                                        row.outcomes.drain(..row.outcomes.len() - 20);
                                    }
                                    for o in &mut row.outcomes {
                                        o.result = clean(&o.result, 160);
                                    }
                                    if row.fingerprint.len() != 64
                                        || !row.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
                                    {
                                        row.fingerprint.clear();
                                    }
                                    inbox.items.insert(row.id.clone(), row);
                                }
                            }
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err("state_unavailable"),
            }
        }
        inbox.prune(now);
        Ok(inbox)
    }
    pub fn prune(&mut self, now: f64) {
        self.items.retain(|_, row| {
            self.registrations.contains_key(&row.source)
                && (row.resolved == 0.0 || row.resolved >= now - WEEK)
        });
        for row in self.items.values_mut() {
            row.outcomes.retain(|o| o.at >= now - WEEK);
        }
        self.diagnostics.retain(|id, _| self.items.contains_key(id));
        self.analysis.retain(|id, _| self.items.contains_key(id));
    }
    pub fn persist(&mut self, now: f64) -> Result<()> {
        self.prune(now);
        if let Some(path) = &self.path {
            let bytes = serde_json::to_vec(&self.items.values().collect::<Vec<_>>())
                .map_err(|_| "state_unavailable")?;
            if bytes.len() > 16 * 1024 * 1024 {
                return Err("state_capacity");
            }
            seele_runtime::fs::atomic_write(path, &bytes).map_err(|_| "state_unavailable")?;
        }
        Ok(())
    }
    /// Does not perform I/O: the service stages the entire transaction, persists
    /// it once, and only then replaces the live state and sends notifications.
    pub fn publish(
        &mut self,
        source: &str,
        mut finding: Finding,
        now: f64,
    ) -> Result<(Row, bool, bool)> {
        let registry = self
            .registrations
            .get(source)
            .ok_or("unregistered_source")?;
        if !valid_key(&finding.key)
            || finding.actions.len() > registry.len()
            || finding.actions.iter().any(|a| !registry.contains_key(a))
        {
            return Err("invalid_finding");
        }
        let mut unique = HashSet::new();
        if finding.actions.iter().any(|a| !unique.insert(a)) {
            return Err("invalid_finding");
        }
        if finding.diagnostic.as_ref().is_some_and(|d| d.len() > 16384) {
            return Err("diagnostic_too_large");
        }
        finding.title = clean(&finding.title, 160);
        finding.explanation = clean(&finding.explanation, 500);
        finding.details = clean(&finding.details, 2000);
        finding.diagnostic = finding.diagnostic.map(|d| clean(&d, 16384));
        if finding.title.is_empty() {
            return Err("invalid_finding");
        }
        let id = format!("{source}:{}", finding.key);
        let previous = self.items.get(&id);
        if previous.is_none() && self.items.len() >= CAPACITY {
            return Err("capacity");
        }
        let signature = fingerprint(&finding);
        let changed =
            previous.is_none_or(|row| row.fingerprint != signature || row.resolved != 0.0);
        let mut row = previous.cloned().unwrap_or_else(|| Row {
            id: id.clone(),
            source: source.into(),
            key: finding.key.clone(),
            title: String::new(),
            explanation: String::new(),
            details: String::new(),
            urgency: finding.urgency,
            lifecycle: finding.lifecycle,
            actions: vec![],
            first_seen: now,
            updated: now,
            revision: 0,
            recurrence: 0,
            snoozed_until: 0.0,
            resolved: 0.0,
            outcomes: vec![],
            fingerprint: String::new(),
            busy: String::new(),
        });
        if row.resolved != 0.0 {
            row.recurrence = row.recurrence.checked_add(1).ok_or("revision_limit")?;
        }
        if finding.urgency < row.urgency {
            row.snoozed_until = 0.0;
        }
        row.title = finding.title;
        row.explanation = finding.explanation;
        row.details = finding.details;
        row.urgency = finding.urgency;
        row.lifecycle = finding.lifecycle;
        row.actions = finding.actions;
        row.fingerprint = signature;
        row.resolved = 0.0;
        if changed {
            row.updated = now;
            row.revision = row.revision.checked_add(1).ok_or("revision_limit")?;
        }
        let notify = changed && row.urgency <= Urgency::Soon && row.snoozed_until <= now;
        if let Some(diagnostic) = finding.diagnostic {
            self.diagnostics.insert(id.clone(), diagnostic);
        } else {
            self.diagnostics.remove(&id);
        }
        self.items.insert(id, row.clone());
        Ok((row, notify, changed))
    }
    pub fn resolve(&mut self, source: &str, key: &str, now: f64) -> Result<bool> {
        if !self.registrations.contains_key(source) {
            return Err("unregistered_source");
        }
        if !valid_key(key) {
            return Err("invalid_finding");
        }
        let id = format!("{source}:{key}");
        if let Some(row) = self.items.get_mut(&id).filter(|r| r.resolved == 0.0) {
            row.resolved = now;
            row.busy.clear();
            self.diagnostics.remove(&id);
            self.analysis.remove(&id);
            return Ok(true);
        }
        Ok(false)
    }
    pub fn current(&self, id: &str, revision: u64) -> Result<&Row> {
        self.items
            .get(id)
            .filter(|r| r.resolved == 0.0 && r.revision == revision)
            .ok_or("stale_finding")
    }
    pub fn operation(
        &mut self,
        id: &str,
        revision: u64,
        op: &str,
        seconds: Option<u64>,
        now: f64,
    ) -> Result<()> {
        let row = self.current(id, revision)?.clone();
        match op {
            "done" => {
                if row.lifecycle != Lifecycle::Notice {
                    return Err("publisher_owned_condition");
                }
                self.resolve(&row.source, &row.key, now)?;
            }
            "snooze" => {
                let seconds = seconds
                    .filter(|s| (60..=30 * 86400).contains(s))
                    .ok_or("invalid_duration")?;
                self.items.get_mut(id).unwrap().snoozed_until = now + seconds as f64;
            }
            "unsnooze" => self.items.get_mut(id).unwrap().snoozed_until = 0.0,
            _ => return Err("invalid_operation"),
        }
        Ok(())
    }
    pub fn outcome(&mut self, id: &str, revision: u64, action: &str, result: &str, now: f64) {
        if let Some(row) = self.items.get_mut(id) {
            row.busy.clear();
            row.outcomes.push(Outcome {
                action: action.into(),
                revision: Some(revision),
                at: now,
                result: result.into(),
            });
            if row.outcomes.len() > 20 {
                row.outcomes.remove(0);
            }
        }
    }
    pub fn snapshot(&mut self, now: f64) -> Value {
        self.prune(now);
        let mut active = vec![];
        let mut snoozed = vec![];
        let mut history = vec![];
        for row in self.items.values() {
            let mut value = serde_json::to_value(row).unwrap();
            value.as_object_mut().unwrap().remove("fingerprint");
            value["busy"] = json!(row.busy);
            value["canAnalyze"] =
                json!(self.diagnostics.contains_key(&row.id) && row.resolved == 0.0);
            value["analysis"] = self.analysis.get(&row.id).cloned().unwrap_or(Value::Null);
            value["analysisStale"] = json!(self
                .analysis
                .get(&row.id)
                .is_some_and(|a| a["revision"].as_u64() != Some(row.revision)));
            value["actions"] = json!(row
                .actions
                .iter()
                .filter_map(|id| self.registrations[&row.source]
                    .get(id)
                    .map(|a| json!({"id": id, "label": a.label, "disruptive": a.disruptive})))
                .collect::<Vec<_>>());
            if row.resolved != 0.0 {
                history.push((row, value));
            } else if row.snoozed_until > now {
                snoozed.push((row, value));
            } else {
                active.push((row, value));
            }
        }
        active.sort_by(|a, b| {
            a.0.urgency
                .cmp(&b.0.urgency)
                .then(b.0.updated.total_cmp(&a.0.updated))
        });
        snoozed.sort_by(|a, b| a.0.snoozed_until.total_cmp(&b.0.snoozed_until));
        history.sort_by(|a, b| b.0.resolved.total_cmp(&a.0.resolved));
        let attention: Vec<_> = active
            .iter()
            .filter(|(r, _)| r.urgency != Urgency::Informational)
            .collect();
        json!({"count": attention.len(), "urgency": attention.first().map(|(r,_)| json!(r.urgency)).unwrap_or(json!("")),
            "active": active.into_iter().map(|(_,v)|v).collect::<Vec<_>>(), "snoozed": snoozed.into_iter().map(|(_,v)|v).collect::<Vec<_>>(), "history": history.into_iter().map(|(_,v)|v).collect::<Vec<_>>()})
    }
}
