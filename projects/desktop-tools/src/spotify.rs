use seele_runtime::process::{capture, Limits};
use serde_json::Value;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
pub fn label(value: &Value) -> String {
    if value["is_playing"] != true {
        return String::new();
    }
    let clean = |value: &Value| {
        value
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|c| *c != '"' && !c.is_control() && seele_runtime::redact::visible(*c))
            .take(2048)
            .collect::<String>()
    };
    let song = clean(&value["item"]["name"]);
    let artists = value["item"]["artists"]
        .as_array()
        .map(|artists| {
            artists
                .iter()
                .take(64)
                .map(|artist| clean(&artist["name"]))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("{song} · {artists}")
}
pub fn status(cancel: &AtomicUsize) -> String {
    capture(
        Command::new("spotify_player").args(["get", "key", "playback"]),
        b"",
        Limits {
            timeout: Duration::from_secs(5),
            output: 256 * 1024,
        },
        cancel,
    )
    .ok()
    .filter(|output| output.status.success())
    .and_then(|output| serde_json::from_slice(&output.stdout).ok())
    .map(|value| label(&value))
    .unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn empty_when_paused_and_plain_metadata_when_playing() {
        assert_eq!(label(&json!({"is_playing":false})), "");
        assert_eq!(
            label(
                &json!({"is_playing":true,"item":{"name":"A \"song\"","artists":[{"name":"First"},{"name":"Second"}]}})
            ),
            "A song · First, Second"
        );
    }
}
