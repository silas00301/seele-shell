//! The private state file: the place the user chose, if any, and the last
//! good forecast with the place it was fetched for.
use super::*;

pub(super) const STATE_VERSION: u32 = 1;
/// A cached forecast is about 25 KiB; the bound leaves room and no more.
pub(super) const MAX_STATE: usize = 512 * 1024;

/// A place chosen through the search. It replaces the timezone city until
/// "Use timezone city" clears it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Chosen {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) detail: String,
    pub(super) latitude: f64,
    pub(super) longitude: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Cached {
    /// The coordinates the forecast was requested for, as sent.
    pub(super) key: String,
    pub(super) fetched_at: i64,
    pub(super) forecast: Forecast,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct State {
    pub(super) version: u32,
    pub(super) chosen: Option<Chosen>,
    pub(super) cached: Option<Cached>,
}

pub(super) fn fresh() -> State {
    State {
        version: STATE_VERSION,
        ..State::default()
    }
}

pub(super) fn state_path() -> PathBuf {
    crate::common::xdg("XDG_STATE_HOME", ".local/state").join("seele-weather/state.json")
}

/// The file is private state: a symlink, a foreign owner, a second link or a
/// mode another user can read is refused, as is anything over the bound.
pub(super) fn read_state(path: &Path) -> Result<Option<State>, &'static str> {
    let bytes = match seele_runtime::fs::read_private(path, MAX_STATE) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("not private"),
    };
    serde_json::from_slice::<State>(&bytes)
        .map(Some)
        .map_err(|_| "invalid")
}

/// A cache that cannot be read is disposable: the worker starts again rather
/// than refusing to run, and the next save replaces it with a private file.
/// A file from another version keeps nothing, since its shape is unknown.
pub(super) fn load(path: &Path) -> (State, &'static str) {
    match read_state(path) {
        Ok(Some(state)) if state.version == STATE_VERSION => (state, ""),
        Ok(_) => (fresh(), ""),
        Err(_) => (
            fresh(),
            "The weather cache could not be read and was started again.",
        ),
    }
}

pub(super) fn save(path: &Path, state: &State) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(state).map_err(|_| "Weather state could not be encoded.")?;
    if bytes.len() > MAX_STATE {
        return Err("Weather state is too large.");
    }
    seele_runtime::fs::atomic_write(path, &bytes).map_err(|_| "Weather state could not be saved.")
}
