//! One configured meeting on this machine: open a local note as it starts and
//! leave that file in place when it ends.
//!
//! The guest list is read once for that occurrence and written only into the
//! note. It is not stored in the calendar cache. Nothing here sends mail or
//! chat, and the note stays under the local state directory rather than a vault.
use super::*;
use std::path::Path;
use std::process::{Command, Stdio};

const LEAD: i64 = 120;
const LOOKBACK: i64 = 6 * 60 * 60;
const LEDGER_VERSION: u32 = 1;
const MAX_LEDGER: usize = 256 * 1024;
const MAX_ATTENDEES: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Config {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) opener: String,
}

impl Config {
    pub(super) fn enabled(&self) -> bool {
        !self.id.is_empty() || !self.title.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Attendee {
    pub(super) name: String,
    pub(super) email: String,
}

/// Attendees for the note being written. `Unread` is a failed lookup, which
/// still leaves the section in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Guests {
    Listed(Vec<Attendee>),
    Unread,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    path: String,
    at: i64,
}

impl Ledger {
    pub(super) fn has(&self, key: &str) -> bool {
        self.opened.contains_key(key) || self.parked.contains_key(key)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Ledger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    opened: BTreeMap<String, Record>,
    #[serde(default)]
    parked: BTreeMap<String, Record>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Step {
    pub(super) key: String,
    pub(super) calendar_id: String,
    pub(super) event_id: String,
    pub(super) title: String,
    pub(super) start: i64,
    pub(super) end: i64,
    pub(super) file_name: String,
    pub(super) launch: bool,
}

#[derive(Clone, Debug)]
struct Occurrence {
    key: String,
    calendar_id: String,
    event_id: String,
    title: String,
    start: i64,
    end: i64,
}

pub(super) fn directory() -> PathBuf {
    crate::common::xdg("XDG_STATE_HOME", ".local/state").join("seele-meetings")
}

pub(super) fn load_config() -> Config {
    let path = crate::common::xdg("XDG_CONFIG_HOME", ".config")
        .join("seele-shell/meeting-scratchpad.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Config::default();
    };
    if bytes.len() > 8192 {
        return Config::default();
    }
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return Config::default();
    };
    Config {
        id: crate::common::clean(&value["id"], "", 512),
        title: crate::common::clean(&value["title"], "", 240),
        opener: opener_path(&value["opener"]),
    }
}

fn opener_path(value: &Value) -> String {
    let Some(text) = value.as_str() else {
        return String::new();
    };
    if text.starts_with('/')
        && text.len() <= 4096
        && !text.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        text.to_owned()
    } else {
        String::new()
    }
}

pub(super) fn load_ledger(dir: &Path) -> Ledger {
    let Ok(bytes) = seele_runtime::fs::read_private(&dir.join("ledger.json"), MAX_LEDGER) else {
        return Ledger::default();
    };
    let Ok(mut ledger) = serde_json::from_slice::<Ledger>(&bytes) else {
        return Ledger::default();
    };
    if ledger.version != LEDGER_VERSION {
        return Ledger::default();
    }
    retain(&mut ledger, Utc::now().timestamp());
    ledger
}

/// What to do for the configured meeting at `now`.
///
/// Open while the occurrence is in the two minutes before it starts or still
/// running, once. Park after it ends, within six hours, without opening a
/// window for a meeting that is already over. An occurrence that is opening
/// is not also parked on the same pass.
pub(super) fn plan(
    state: &State,
    ledger: &Ledger,
    config: &Config,
    now: i64,
) -> (Option<Step>, Option<Step>) {
    let found = occurrences(state, config);
    let open = found
        .iter()
        .filter(|item| {
            now >= item.start.saturating_sub(LEAD)
                && now < item.end
                && !ledger.opened.contains_key(&item.key)
        })
        .min_by(|a, b| a.start.cmp(&b.start).then_with(|| a.key.cmp(&b.key)))
        .cloned();
    let park = found
        .iter()
        .filter(|item| {
            item.end <= now
                && now.saturating_sub(item.end) <= LOOKBACK
                && !ledger.parked.contains_key(&item.key)
                && open.as_ref().is_none_or(|opening| opening.key != item.key)
        })
        .max_by(|a, b| a.end.cmp(&b.end).then_with(|| a.key.cmp(&b.key)))
        .cloned();
    (
        park.map(|item| step(&item, false)),
        open.map(|item| step(&item, true)),
    )
}

pub(super) fn project_attendees(value: &Value) -> Vec<Attendee> {
    value["attendees"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(attendee)
                .take(MAX_ATTENDEES)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn note(title: &str, start: i64, end: i64, guests: &Guests) -> String {
    let day = local_date(start)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "undated".into());
    let people = match guests {
        Guests::Unread => "- Attendees could not be read.\n".to_owned(),
        Guests::Listed(items) if items.is_empty() => "- \n".to_owned(),
        Guests::Listed(items) => {
            items
                .iter()
                .map(attendee_line)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
    };
    format!(
        "# {title}\n\n{day} {}–{}\n\n## Attendees\n\n{people}\n## Who\n\n\n## What\n\n\n## When\n\n",
        hm(start),
        hm(end)
    )
}

/// Write the note when this occurrence has no file yet, then record the open
/// or the park. An existing note is left byte-for-byte alone.
pub(super) fn commit(
    dir: &Path,
    mut ledger: Ledger,
    step: &Step,
    guests: &Guests,
    now: i64,
    opener: &str,
) -> Result<Ledger, &'static str> {
    if step.launch && ledger.opened.contains_key(&step.key) {
        return Ok(ledger);
    }
    if !step.launch && ledger.parked.contains_key(&step.key) {
        return Ok(ledger);
    }
    let path = if let Some(existing) = ledger
        .opened
        .get(&step.key)
        .or_else(|| ledger.parked.get(&step.key))
    {
        PathBuf::from(&existing.path)
    } else {
        let path = dir.join(&step.file_name);
        if step.file_name.contains('/') || !step.file_name.ends_with(".md") {
            return Err("Meeting note name is invalid.");
        }
        let body = note(&step.title, step.start, step.end, guests);
        match seele_runtime::fs::atomic_write_new(&path, body.as_bytes()) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("Meeting note could not be saved."),
        }
        path
    };
    if step.launch {
        launch(opener, &path)?;
        ledger.opened.insert(
            step.key.clone(),
            Record {
                path: path.display().to_string(),
                at: now,
            },
        );
    } else {
        ledger.parked.insert(
            step.key.clone(),
            Record {
                path: path.display().to_string(),
                at: now,
            },
        );
    }
    ledger.version = LEDGER_VERSION;
    retain(&mut ledger, now);
    save_ledger(dir, &ledger)?;
    Ok(ledger)
}

fn launch(opener: &str, path: &Path) -> Result<(), &'static str> {
    if opener.is_empty() {
        return Ok(());
    }
    let mut command = Command::new(opener);
    command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // The editor has to outlive this worker. A new session keeps a shell
    // restart from taking the note with it, and the wait below reaps it.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Meeting scratchpad could not be opened.")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn save_ledger(dir: &Path, ledger: &Ledger) -> Result<(), &'static str> {
    let bytes =
        serde_json::to_vec(ledger).map_err(|_| "Meeting scratchpad state could not be encoded.")?;
    if bytes.len() > MAX_LEDGER {
        return Err("Meeting scratchpad state could not be saved.");
    }
    seele_runtime::fs::atomic_write(&dir.join("ledger.json"), &bytes)
        .map_err(|_| "Meeting scratchpad state could not be saved.")
}

fn retain(ledger: &mut Ledger, now: i64) {
    let keep = now - 60 * 60 * 24 * 45;
    ledger.opened.retain(|_, record| record.at > keep);
    ledger.parked.retain(|_, record| record.at > keep);
    trim(&mut ledger.opened);
    trim(&mut ledger.parked);
}

fn trim(records: &mut BTreeMap<String, Record>) {
    while records.len() > 200 {
        let Some(key) = records
            .iter()
            .min_by_key(|(_, record)| record.at)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        records.remove(&key);
    }
}

fn occurrences(state: &State, config: &Config) -> Vec<Occurrence> {
    if !config.enabled() {
        return Vec::new();
    }
    let calendars: HashMap<&str, &Value> = state
        .calendars
        .iter()
        .filter_map(|calendar| Some((calendar["id"].as_str()?, calendar)))
        .collect();
    let mut found = Vec::new();
    for event in &state.events {
        let calendar_id = calendar_of(event);
        if !state.selected.contains(calendar_id) {
            continue;
        }
        let Some(calendar) = calendars.get(calendar_id) else {
            continue;
        };
        if !event_matches(config, event) {
            continue;
        }
        let Some((start, end)) = span(event, calendar) else {
            continue;
        };
        let Some(event_id) = event["id"].as_str() else {
            continue;
        };
        found.push(Occurrence {
            key: format!("{calendar_id}:{event_id}:{start}"),
            calendar_id: calendar_id.to_owned(),
            event_id: event_id.to_owned(),
            title: title(event),
            start,
            end,
        });
    }
    found
}

fn event_matches(config: &Config, event: &Value) -> bool {
    let id = event["id"].as_str().unwrap_or("");
    let id_ok = config.id.is_empty() || id_matches(&config.id, id);
    let title_ok = config.title.is_empty() || title(event) == config.title;
    id_ok && title_ok
}

/// Exact id, or a recurring instance `{id}_{YYYYMMDDTHHMMSSZ}`.
fn id_matches(configured: &str, event_id: &str) -> bool {
    if event_id == configured {
        return true;
    }
    let Some(suffix) = event_id
        .strip_prefix(configured)
        .and_then(|rest| rest.strip_prefix('_'))
    else {
        return false;
    };
    let bytes = suffix.as_bytes();
    bytes.len() == 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z'
}

fn span(event: &Value, calendar: &Value) -> Option<(i64, i64)> {
    if event["start"]["date"].is_string() {
        return None;
    }
    let start = start_time(event, calendar)?;
    let end = instant(&event["end"]).filter(|end| *end >= start)?;
    Some((start, end))
}

fn step(item: &Occurrence, launch: bool) -> Step {
    Step {
        file_name: file_name(item.start, &item.title, &item.event_id),
        key: item.key.clone(),
        calendar_id: item.calendar_id.clone(),
        event_id: item.event_id.clone(),
        title: item.title.clone(),
        start: item.start,
        end: item.end,
        launch,
    }
}

fn file_name(start: i64, title: &str, event_id: &str) -> String {
    let day = local_date(start)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "undated".into());
    let clock = hm(start).replace(':', "");
    let mut hasher = sha2::Sha256::new();
    hasher.update(event_id.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{day}-{clock}-{}-{}.md", slug(title), &digest[..8])
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "meeting".into()
    } else {
        out
    }
}

fn attendee(value: &Value) -> Option<Attendee> {
    if value["responseStatus"] == "declined" {
        return None;
    }
    let name = plain(&crate::common::clean(&value["displayName"], "", 120));
    let email = plain(&crate::common::clean(&value["email"], "", 160));
    if name.is_empty() && email.is_empty() {
        return (value["self"] == true).then_some(Attendee {
            name: "You".into(),
            email: String::new(),
        });
    }
    Some(Attendee { name, email })
}

fn attendee_line(person: &Attendee) -> String {
    match (person.name.as_str(), person.email.as_str()) {
        ("", email) => format!("- {email}"),
        (name, "") => format!("- {name}"),
        (name, email) => format!("- {name} <{email}>"),
    }
}

fn plain(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, '<' | '>' | '`'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn meeting(id: &str, summary: &str, start: i64, end: i64) -> Value {
        json!({
            "id": id,
            "calendar_id": "primary",
            "summary": summary,
            "start": {"dateTime": Utc.timestamp_opt(start, 0).unwrap().to_rfc3339()},
            "end": {"dateTime": Utc.timestamp_opt(end, 0).unwrap().to_rfc3339()},
        })
    }

    fn configured(events: Vec<Value>) -> State {
        let mut state = fresh();
        state.selected.insert("primary".into());
        state
            .calendars
            .push(json!({"id": "primary", "timeZone": "UTC"}));
        state.events = events;
        state
    }

    fn weekly() -> Config {
        Config {
            id: "weekly".into(),
            title: String::new(),
            opener: String::new(),
        }
    }

    #[test]
    fn opens_the_configured_instance_and_names_the_sections() {
        let start = 1_800_000_000;
        let state = configured(vec![
            meeting("weekly_20260926T100000Z", "Staff sync", start, start + 1800),
            meeting("other", "Staff sync", start, start + 1800),
            meeting("weekly_sync", "Staff sync", start, start + 1800),
        ]);
        let (park, open) = plan(&state, &Ledger::default(), &weekly(), start - 120);
        assert!(park.is_none());
        let open = open.expect("scratchpad");
        assert!(open.launch);
        assert_eq!(open.event_id, "weekly_20260926T100000Z");
        assert_eq!(open.title, "Staff sync");
        let body = note(
            &open.title,
            open.start,
            open.end,
            &Guests::Listed(vec![Attendee {
                name: "Ada Lovelace".into(),
                email: "ada@example.com".into(),
            }]),
        );
        assert!(body.starts_with("# Staff sync\n"), "{body}");
        assert!(
            body.contains("## Attendees\n\n- Ada Lovelace <ada@example.com>\n"),
            "{body}"
        );
        assert!(
            body.contains("## Who\n\n\n## What\n\n\n## When\n"),
            "{body}"
        );
        let unread = note("Staff sync", open.start, open.end, &Guests::Unread);
        assert!(unread.contains("Attendees could not be read."));
    }

    #[test]
    fn title_match_skips_other_meetings_and_all_day_events() {
        let start = 1_800_000_000;
        let mut all_day = meeting("weekly_20260926T100000Z", "Staff sync", start, start + 60);
        all_day["start"] = json!({"date": "2026-10-02"});
        all_day["end"] = json!({"date": "2026-10-03"});
        let state = configured(vec![
            meeting("abc", "Other", start, start + 1800),
            meeting("def", "Staff sync", start, start + 1800),
            all_day,
        ]);
        let config = Config {
            title: "Staff sync".into(),
            ..Config::default()
        };
        let (_, open) = plan(&state, &Ledger::default(), &config, start);
        assert_eq!(open.unwrap().event_id, "def");
    }

    #[test]
    fn end_parks_without_opening_and_a_running_meeting_still_opens() {
        let start = 1_800_000_000;
        let ended = start - 3600;
        let state = configured(vec![
            meeting("weekly_20260926T090000Z", "Staff sync", ended, ended + 1800),
            meeting("weekly_20260926T100000Z", "Staff sync", start, start + 1800),
        ]);
        let (park, open) = plan(&state, &Ledger::default(), &weekly(), start);
        assert_eq!(park.unwrap().event_id, "weekly_20260926T090000Z");
        assert!(!park_is_launch(&state, start));
        assert_eq!(open.unwrap().event_id, "weekly_20260926T100000Z");
        let mut ledger = Ledger::default();
        let open = plan(&state, &ledger, &weekly(), start).1.unwrap();
        ledger.opened.insert(
            open.key.clone(),
            Record {
                path: "kept.md".into(),
                at: start,
            },
        );
        assert!(plan(&state, &ledger, &weekly(), start).1.is_none());
        let late = start + 1800 + LOOKBACK + 1;
        assert!(plan(&state, &Ledger::default(), &weekly(), late)
            .0
            .is_none());
    }

    fn park_is_launch(state: &State, now: i64) -> bool {
        plan(state, &Ledger::default(), &weekly(), now)
            .0
            .unwrap()
            .launch
    }

    #[test]
    fn an_unselected_calendar_and_an_empty_config_do_nothing() {
        let start = 1_800_000_000;
        let mut state = configured(vec![meeting(
            "weekly_20260926T100000Z",
            "Staff sync",
            start,
            start + 60,
        )]);
        state.selected.clear();
        assert!(plan(&state, &Ledger::default(), &weekly(), start)
            .1
            .is_none());
        state.selected.insert("primary".into());
        assert!(plan(&state, &Ledger::default(), &Config::default(), start)
            .1
            .is_none());
    }

    #[test]
    fn declined_attendees_are_omitted() {
        let people = project_attendees(&json!({"attendees": [
            {"displayName": "Ada", "email": "ada@example.com", "responseStatus": "accepted"},
            {"email": "skip@example.com", "responseStatus": "declined"},
            {"self": true, "responseStatus": "tentative"}
        ]}));
        assert_eq!(
            people,
            vec![
                Attendee {
                    name: "Ada".into(),
                    email: "ada@example.com".into()
                },
                Attendee {
                    name: "You".into(),
                    email: String::new()
                },
            ]
        );
    }

    #[test]
    fn open_writes_a_private_note_and_end_keeps_edits() {
        let dir = tempfile::tempdir().unwrap();
        let start = 1_800_000_000;
        let state = configured(vec![meeting(
            "weekly_20260926T100000Z",
            "Staff sync",
            start,
            start + 1800,
        )]);
        let open = plan(&state, &Ledger::default(), &weekly(), start)
            .1
            .unwrap();
        let ledger = commit(
            dir.path(),
            Ledger::default(),
            &open,
            &Guests::Listed(vec![Attendee {
                name: "Ada".into(),
                email: "ada@example.com".into(),
            }]),
            start,
            "/bin/true",
        )
        .unwrap();
        let path = PathBuf::from(&ledger.opened[&open.key].path);
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("# Staff sync"));
        assert!(body.contains("- Ada <ada@example.com>"));
        assert!(body.contains("## Who"));
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        std::fs::write(&path, "edited who\n").unwrap();
        let ended = start + 1800;
        let park = plan(&state, &ledger, &weekly(), ended).0.unwrap();
        assert!(!park.launch);
        let parked = commit(
            dir.path(),
            ledger,
            &park,
            &Guests::Unread,
            ended,
            "/no/such/opener",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited who\n");
        assert_eq!(parked.parked[&park.key].path, path.display().to_string());
        let again = commit(
            dir.path(),
            parked.clone(),
            &open,
            &Guests::Unread,
            ended,
            "/no/such/opener",
        )
        .unwrap();
        assert_eq!(again, parked);
    }

    #[test]
    fn a_failed_open_leaves_the_note_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let start = 1_800_000_000;
        let state = configured(vec![meeting("weekly", "Staff sync", start, start + 60)]);
        let open = plan(&state, &Ledger::default(), &weekly(), start)
            .1
            .unwrap();
        let error = commit(
            dir.path(),
            Ledger::default(),
            &open,
            &Guests::Listed(Vec::new()),
            start,
            "/no/such/opener",
        );
        assert!(error.is_err());
        let path = dir.path().join(&open.file_name);
        assert!(std::fs::read_to_string(&path).unwrap().contains("## What"));
        let ledger = commit(
            dir.path(),
            Ledger::default(),
            &open,
            &Guests::Listed(Vec::new()),
            start,
            "",
        )
        .unwrap();
        assert!(ledger.opened.contains_key(&open.key));
    }
}
