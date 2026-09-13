//! Pure MPRIS normalization, selection and control policy. Live QObject identity
//! and actual property writes stay in the Qt adapter; selections return indexes.
use crate::value::{SPACE, array, is_space, number, string, trim, truthy};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

fn clean(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        _ => trim(&string(value)).to_owned(),
    }
}
fn list_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| clean(Some(value)))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Some(Value::Object(object)) if object.get("length").is_some_and(Value::is_number) => {
            let length = number(object.get("length"));
            if !length.is_finite() || length <= 0.0 || length > 4096.0 {
                return String::new();
            }
            (0..length.ceil() as usize)
                .map(|index| clean(object.get(&index.to_string())))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        }
        _ => clean(value),
    }
}
fn metadata<'a>(player: &'a Value, key: &str) -> Option<&'a Value> {
    player.get("metadata")?.get(key)
}
fn first(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}
fn title(player: &Value) -> String {
    first([
        clean(player.get("trackTitle")),
        clean(metadata(player, "xesam:title")),
    ])
}
fn artist(player: &Value) -> String {
    first([
        clean(player.get("trackArtist")),
        list_text(metadata(player, "xesam:artist")),
        clean(player.get("trackAlbumArtist")),
        list_text(metadata(player, "xesam:albumArtist")),
    ])
}
fn album(player: &Value) -> String {
    first([
        clean(player.get("trackAlbum")),
        clean(metadata(player, "xesam:album")),
    ])
}
fn length(player: &Value) -> f64 {
    let direct = number(player.get("length"));
    if direct.is_finite() && direct > 0.0 {
        return direct;
    }
    let raw = number(metadata(player, "mpris:length"));
    if raw.is_finite() && raw > 0.0 {
        raw / 1_000_000.0
    } else {
        0.0
    }
}
fn spotify(player: &Value) -> bool {
    ["identity", "desktopEntry", "dbusName"]
        .iter()
        .any(|key| clean(player.get(*key)).to_lowercase().contains("spotify"))
}
fn title_key(title: &str) -> String {
    static SEPARATOR: OnceLock<Regex> = OnceLock::new();
    let separator = SEPARATOR.get_or_init(|| {
        Regex::new(&format!("{SPACE}+[•·—–|]{SPACE}+")).expect("fixed media separator")
    });
    let title = separator
        .find(title)
        .map_or(title, |found| &title[..found.start()]);
    trim(title)
        .to_lowercase()
        .split(is_space)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
struct Player<'a> {
    value: &'a Value,
    title: String,
    artist: String,
    album: String,
    key: String,
    length: f64,
    spotify: bool,
    playing: bool,
}
impl<'a> Player<'a> {
    fn new(value: &'a Value) -> Self {
        let title = title(value);
        let key = title_key(&title);
        Self {
            title,
            key,
            artist: artist(value),
            album: album(value),
            length: length(value),
            spotify: spotify(value),
            playing: truthy(value.get("isPlaying")),
            value,
        }
    }
    fn has_track(&self) -> bool {
        !self.title.is_empty() || !self.artist.is_empty() || !self.album.is_empty()
    }
    fn same(&self, other: &Self) -> bool {
        if !truthy(Some(self.value)) || !truthy(Some(other.value)) {
            return false;
        }
        if !self.artist.is_empty()
            && !other.artist.is_empty()
            && self.artist.to_lowercase() != other.artist.to_lowercase()
        {
            return false;
        }
        if self.length > 0.0 && other.length > 0.0 {
            return (self.length - other.length).abs() <= 1.0;
        }
        !self.key.is_empty() && self.key == other.key
    }
}
fn playing_spotify(players: &[Player<'_>], indexes: &[usize]) -> Option<usize> {
    indexes
        .iter()
        .copied()
        .find(|&index| players[index].playing && players[index].spotify)
}
fn device(players: &[Player<'_>], indexes: &[usize]) -> Option<usize> {
    let spotify = playing_spotify(players, indexes);
    indexes.iter().copied().find(|&index| {
        let player = &players[index];
        player.playing
            && !player.spotify
            && !spotify.is_some_and(|spotify| player.same(&players[spotify]))
    })
}
fn active(players: &[Player<'_>], indexes: &[usize]) -> Option<usize> {
    playing_spotify(players, indexes)
        .or_else(|| device(players, indexes))
        .or_else(|| {
            indexes
                .iter()
                .copied()
                .find(|&index| players[index].has_track())
        })
}
fn available(players: &[Player<'_>]) -> Vec<usize> {
    let mut spotify = None;
    for (index, player) in players.iter().enumerate() {
        if player.spotify && player.has_track() {
            spotify = Some(index);
            if player.playing {
                break;
            }
        }
    }
    players
        .iter()
        .enumerate()
        .filter(|(index, player)| {
            player.has_track()
                && !spotify.is_some_and(|spotify| {
                    *index != spotify && !player.spotify && player.same(&players[spotify])
                })
        })
        .map(|(index, _)| index)
        .collect()
}
fn rates(player: &Value) -> Vec<f64> {
    let current = number(player.get("rate"));
    let minimum = number(player.get("minRate"));
    let maximum = number(player.get("maxRate"));
    if !truthy(player.get("canControl"))
        || !current.is_finite()
        || current <= 0.0
        || !minimum.is_finite()
        || !maximum.is_finite()
        || minimum <= 0.0
        || maximum < minimum
    {
        return Vec::new();
    }
    [0.75, 1.0, 1.25, 1.5, 2.0]
        .into_iter()
        .filter(|value| *value >= minimum && *value <= maximum)
        .collect()
}
fn next_rate(player: &Value) -> Option<f64> {
    let choices = rates(player);
    let current = number(player.get("rate"));
    choices
        .iter()
        .copied()
        .find(|rate| *rate > current + 0.000001)
        .or_else(|| {
            choices
                .first()
                .copied()
                .filter(|rate| (*rate - current).abs() > 0.000001)
        })
}
fn volume(player: &Value) -> Option<f64> {
    if !truthy(player.get("volumeSupported")) {
        return None;
    }
    player
        .get("volume")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}
fn writable(player: &Value) -> bool {
    truthy(player.get("canControl")) && volume(player).is_some()
}
fn can(player: &Value, key: &str) -> bool {
    truthy(player.get("canControl")) && truthy(player.get(key))
}
fn next_loop(current: Option<&Value>, states: &Value) -> Value {
    if current == states.get("None") {
        states.get("Playlist")
    } else if current == states.get("Playlist") {
        states.get("Track")
    } else {
        states.get("None")
    }
    .cloned()
    .unwrap_or(Value::Null)
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let player = args.first().unwrap_or(&null);
    let states = args.get(1).unwrap_or(&null);
    Ok(match function {
        "clean" => json!(clean(args.first())),
        "listText" => json!(list_text(args.first())),
        "metadata" => metadata(player, &string(args.get(1)))
            .cloned()
            .unwrap_or(Value::Null),
        "isSpotify" => json!(spotify(player)),
        "title" => json!(title(player)),
        "artist" => json!(artist(player)),
        "album" => json!(album(player)),
        "subtitle" => json!(first([artist(player), album(player)])),
        "label" => {
            let title = title(player);
            let subtitle = first([artist(player), album(player)]);
            json!(if !title.is_empty() && !subtitle.is_empty() {
                format!("{title} · {subtitle}")
            } else {
                format!("{title}{subtitle}")
            })
        }
        "playerName" => {
            if !truthy(Some(player)) {
                return Ok(json!(""));
            }
            let name = first([
                clean(player.get("identity")),
                clean(player.get("desktopEntry")),
            ]);
            if !name.is_empty() {
                return Ok(json!(name));
            }
            let name = clean(player.get("dbusName"));
            let name = name
                .strip_prefix("org.mpris.MediaPlayer2.")
                .unwrap_or(&name)
                .split('.')
                .next()
                .unwrap_or("");
            let mut segment = String::new();
            let mut separating = false;
            for ch in name.chars() {
                if matches!(ch, '-' | '_') {
                    if !separating {
                        segment.push(' ');
                    }
                    separating = true;
                } else {
                    segment.push(ch);
                    separating = false;
                }
            }
            let segment = trim(&segment);
            let result = if let Some(ch) = segment.chars().next() {
                format!(
                    "{}{}",
                    if ch.len_utf16() == 1 {
                        ch.to_uppercase().collect::<String>()
                    } else {
                        ch.to_string()
                    },
                    &segment[ch.len_utf8()..]
                )
            } else {
                "Media player".into()
            };
            json!(result)
        }
        "lengthSeconds" => json!(length(player)),
        "liveStream" => json!(length(player) >= 31_536_000.0),
        "timelineAvailable" => json!(
            length(player) > 0.0
                && (length(player) >= 31_536_000.0
                    || (truthy(player.get("canSeek"))
                        && truthy(player.get("positionSupported"))
                        && truthy(player.get("lengthSupported"))))
        ),
        "seekTarget" => {
            let duration = length(player);
            let position = number(player.get("position"));
            if !truthy(player.get("canControl"))
                || !truthy(player.get("canSeek"))
                || !truthy(player.get("positionSupported"))
                || !truthy(player.get("lengthSupported"))
                || !position.is_finite()
                || duration <= 0.0
                || duration >= 31_536_000.0
            {
                Value::Null
            } else {
                let step = if truthy(args.get(2)) { 30.0 } else { 5.0 };
                let target = match args.get(1).and_then(Value::as_str) {
                    Some("back") => Some(position - step),
                    Some("forward") => Some(position + step),
                    Some("start") => Some(0.0),
                    Some("end") => Some(duration),
                    _ => None,
                };
                json!(target.map(|target| target.clamp(0.0, duration)))
            }
        }
        "titleKey" => json!(title_key(&title(player))),
        "sameTrack" => json!(Player::new(player).same(&Player::new(states))),
        "spotifyPlayer" | "devicePlayer" | "activePlayer" | "availablePlayers"
        | "selectedPlayer" => {
            let values = array(Some(player));
            if values.len() > 4096 {
                return Err("media player list exceeds its limit".into());
            }
            let players: Vec<_> = values.iter().map(Player::new).collect();
            let indexes: Vec<_> = (0..players.len()).collect();
            match function {
                "availablePlayers" => json!(available(&players)),
                "spotifyPlayer" => json!(playing_spotify(&players, &indexes)),
                "devicePlayer" => json!(device(&players, &indexes)),
                "activePlayer" => json!(active(&players, &indexes)),
                _ => {
                    let available = available(&players);
                    let selected = states
                        .as_u64()
                        .and_then(|value| usize::try_from(value).ok())
                        .filter(|index| available.contains(index));
                    json!(selected.or_else(|| active(&players, &available)))
                }
            }
        }
        "canShuffle" => json!(can(player, "shuffleSupported")),
        "canRepeat" => json!(can(player, "loopSupported")),
        "nextShuffle" => {
            if can(player, "shuffleSupported") {
                json!(!truthy(player.get("shuffle")))
            } else {
                Value::Null
            }
        }
        "nextLoopState" => next_loop(args.first(), states),
        "nextRepeat" => {
            if can(player, "loopSupported") {
                next_loop(player.get("loopState"), states)
            } else {
                Value::Null
            }
        }
        "repeatLabel" => json!(if !truthy(player.get("loopSupported")) {
            "Repeat unavailable"
        } else if player.get("loopState") == states.get("Track") {
            "Repeat one track"
        } else if player.get("loopState") == states.get("Playlist") {
            "Repeat playlist"
        } else {
            "Repeat off"
        }),
        "rates" => json!(rates(player)),
        "nextRate" => json!(next_rate(player)),
        "rateLabel" => {
            let rate = number(player.get("rate"));
            json!(if rate.is_finite() && rate > 0.0 {
                format!("{}×", string(Some(&json!(rate))))
            } else {
                "Unavailable".into()
            })
        }
        "volumeSupported" => json!(volume(player).is_some()),
        "volumeWritable" => json!(writable(player)),
        "volumePercent" => json!(volume(player).map(|volume| (volume.max(0.0) * 100.0).round())),
        "nextVolume" => {
            let next = volume(player)
                .filter(|_| writable(player))
                .zip(
                    args.get(1)
                        .and_then(Value::as_f64)
                        .filter(|delta| delta.is_finite()),
                )
                .and_then(|(volume, delta)| {
                    let value = (volume + delta).clamp(0.0, 1.0);
                    (value != volume).then_some(value)
                });
            json!(next)
        }
        _ => return Err(format!("Unknown media function: {function}")),
    })
}
