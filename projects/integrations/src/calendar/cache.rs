//! The private cache: its file, the date windows it holds, and how a fetch
//! reconciles into it.
use super::*;

pub(super) const STATE_VERSION: u32 = 2;
pub(super) const MAX_STATE: usize = 8 * 1024 * 1024;
/// Today's window, the agenda's and one the month list browsed to.
pub(super) const MAX_WINDOWS: usize = 3;
pub(super) const WINDOW_BEFORE: i64 = 45;
pub(super) const WINDOW_AFTER: i64 = 46;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Window {
    pub(super) start: String,
    pub(super) end: String,
    pub(super) fetched_at: i64,
    pub(super) used_at: i64,
}

impl Window {
    pub(super) fn bounds(&self) -> Option<(NaiveDate, NaiveDate)> {
        date(&self.start).zip(date(&self.end))
    }
    pub(super) fn covers(&self, day: NaiveDate) -> bool {
        self.bounds().is_some_and(|(a, b)| a <= day && day < b)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct State {
    pub(super) version: u32,
    pub(super) client_id: String,
    /// Presence only; the value stays in Secret Service.
    pub(super) has_client_secret: bool,
    pub(super) signed_in: bool,
    pub(super) account_id: String,
    pub(super) calendars: Vec<Value>,
    /// Google's event colour palette, `colorId` to `#rrggbb`.
    pub(super) palette: BTreeMap<String, String>,
    pub(super) palette_at: i64,
    pub(super) selected: BTreeSet<String>,
    /// Selected calendars whose events are present for every cached window.
    pub(super) fetched: BTreeSet<String>,
    pub(super) events: Vec<Value>,
    pub(super) windows: Vec<Window>,
    pub(super) refreshed_at: i64,
    pub(super) checked_at: i64,
    pub(super) delivered: BTreeMap<String, i64>,
}

pub(super) fn fresh() -> State {
    State {
        version: STATE_VERSION,
        ..State::default()
    }
}

pub(super) fn state_path() -> PathBuf {
    crate::common::xdg("XDG_STATE_HOME", ".local/state").join("seele-calendar/state.json")
}

pub(super) fn read_state() -> Result<Option<State>, &'static str> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(state_path())
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("unreadable"),
    };
    let meta = file.metadata().map_err(|_| "unreadable")?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.len() as usize > MAX_STATE
    {
        return Err("not private");
    }
    let mut bytes = Vec::new();
    file.take((MAX_STATE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable")?;
    if bytes.len() > MAX_STATE {
        return Err("too large");
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "invalid")
}

/// A cache that cannot be read is disposable: the worker starts again rather
/// than refusing to run, and the next save replaces the file with a private one.
pub(super) fn load() -> (State, &'static str) {
    match read_state() {
        Ok(Some(state)) => (upgrade(state), ""),
        Ok(None) => (fresh(), ""),
        Err(_) => (
            fresh(),
            "The calendar cache could not be read and was started again.",
        ),
    }
}

/// Earlier caches stored events in another shape. The account, the choice of
/// calendars and delivered reminder keys survive; events are fetched again.
pub(super) fn upgrade(state: State) -> State {
    if state.version == STATE_VERSION {
        return state;
    }
    State {
        version: STATE_VERSION,
        signed_in: state.signed_in || !state.account_id.is_empty(),
        client_id: state.client_id,
        has_client_secret: state.has_client_secret,
        account_id: state.account_id,
        selected: state.selected,
        checked_at: state.checked_at,
        delivered: state.delivered,
        ..State::default()
    }
}

pub(super) fn save(path: &std::path::Path, state: &State) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(state).map_err(|_| "Calendar state could not be encoded.")?;
    if bytes.len() > MAX_STATE {
        return Err("Calendar cache is full.");
    }
    seele_runtime::fs::atomic_write(path, &bytes).map_err(|_| "Calendar state could not be saved.")
}

pub(super) fn date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

pub(super) fn days(count: i64) -> ChronoDuration {
    ChronoDuration::days(count)
}

pub(super) fn window_around(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    (day - days(WINDOW_BEFORE), day + days(WINDOW_AFTER))
}

pub(super) fn calendar_of(event: &Value) -> &str {
    event["calendar_id"].as_str().unwrap_or("")
}

pub(super) fn event_key(event: &Value) -> String {
    format!(
        "{}:{}",
        calendar_of(event),
        event["id"].as_str().unwrap_or("")
    )
}

pub(super) fn overlaps(event: &Value, start: NaiveDate, end: NaiveDate) -> bool {
    if let (Some(a), Some(b)) = (
        event["start"]["date"].as_str(),
        event["end"]["date"].as_str(),
    ) {
        return a < end.to_string().as_str() && b > start.to_string().as_str();
    }
    let a = instant(&event["start"]);
    let b = instant(&event["end"]).or(a);
    let start = start
        .and_hms_opt(0, 0, 0)
        .map(|v| v.and_utc().timestamp())
        .unwrap_or_default();
    let end = end
        .and_hms_opt(0, 0, 0)
        .map(|v| v.and_utc().timestamp())
        .unwrap_or_default();
    a.is_some_and(|v| v < end) && b.is_some_and(|v| v >= start)
}

pub(super) fn instant(endpoint: &Value) -> Option<i64> {
    DateTime::parse_from_rfc3339(endpoint["dateTime"].as_str()?)
        .ok()
        .map(|v| v.timestamp())
}

pub(super) fn evict(state: &mut State, keep: &[NaiveDate]) -> bool {
    let victim = state
        .windows
        .iter()
        .enumerate()
        .filter(|(_, w)| !keep.iter().any(|day| w.covers(*day)))
        .min_by_key(|(_, w)| w.used_at)
        .map(|(index, _)| index);
    match victim {
        Some(index) => {
            state.windows.remove(index);
            prune(state);
            true
        }
        None => false,
    }
}

pub(super) fn prune(state: &mut State) {
    let windows: Vec<_> = state.windows.iter().filter_map(Window::bounds).collect();
    let selected = &state.selected;
    state.events.retain(|event| {
        selected.contains(calendar_of(event)) && windows.iter().any(|&(a, b)| overlaps(event, a, b))
    });
}

pub(super) fn upsert_window(
    state: &mut State,
    start: NaiveDate,
    end: NaiveDate,
    now: i64,
    keep: &[NaiveDate],
) {
    let (from, to) = (start.to_string(), end.to_string());
    if let Some(window) = state
        .windows
        .iter_mut()
        .find(|w| w.start == from && w.end == to)
    {
        window.fetched_at = now;
        window.used_at = now;
        return;
    }
    // A window mostly inside the new one is superseded by it, so today's
    // window moving by a day replaces yesterday's instead of piling up.
    state.windows.retain(|w| {
        w.bounds().is_some_and(|(a, b)| {
            let inside = (b.min(end) - a.max(start)).num_days().max(0);
            inside * 2 <= (b - a).num_days()
        })
    });
    state.windows.push(Window {
        start: from,
        end: to,
        fetched_at: now,
        used_at: now,
    });
    while state.windows.len() > MAX_WINDOWS && evict(state, keep) {}
}

pub(super) fn merge(state: &mut State, job: &Job, fetched: Fetched, now: i64, keep: &[NaiveDate]) {
    let primary = fetched
        .calendars
        .iter()
        .find(|c| c["primary"] == true)
        .and_then(|c| c["id"].as_str())
        .map(str::to_owned);
    if let Some(primary) = primary {
        if !state.account_id.is_empty() && state.account_id != primary {
            // Nothing chosen, cached or delivered for another account applies.
            state.selected.clear();
            state.fetched.clear();
            state.events.clear();
            state.windows.clear();
            state.delivered.clear();
        }
        state.account_id = primary;
    }
    let known: BTreeSet<String> = fetched
        .calendars
        .iter()
        .filter_map(|c| c["id"].as_str().map(str::to_owned))
        .collect();
    state.calendars = fetched.calendars;
    if let Some(id) = &fetched.adopted {
        if state.selected.is_empty() {
            state.selected.insert(id.clone());
        }
    }
    state.selected.retain(|id| known.contains(id));
    if let Some(palette) = fetched.palette {
        state.palette = palette;
        state.palette_at = now;
    }
    let refreshed: BTreeSet<String> = fetched.fetched.into_iter().collect();
    for (&(start, end), events) in job.windows.iter().zip(fetched.events) {
        // Everything Google returned for this window replaces the cached copy,
        // so changed, moved and cancelled events reconcile here.
        state.events.retain(|event| {
            !(refreshed.contains(calendar_of(event)) && overlaps(event, start, end))
        });
        let selected = &state.selected;
        state.events.extend(
            events
                .into_iter()
                .filter(|event| selected.contains(calendar_of(event))),
        );
        upsert_window(state, start, end, now, keep);
    }
    // Overlapping windows return one event twice; the copy fetched last wins.
    let mut seen = HashSet::new();
    state.events.reverse();
    state.events.retain(|event| seen.insert(event_key(event)));
    state.events.reverse();
    if job.full || fetched.adopted.is_some() {
        state.fetched = refreshed;
    }
    let selected = &state.selected;
    state.fetched.retain(|id| selected.contains(id));
    prune(state);
    while state.events.len() > MAX_EVENTS && evict(state, keep) {}
    state.refreshed_at = now;
}
