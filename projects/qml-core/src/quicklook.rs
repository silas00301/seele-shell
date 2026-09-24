//! What Quick Look says about a highlighted file, and where its keys move.
//!
//! The worker answers what a path *is*; everything a reader is told about it
//! is decided here, once, so the header, the counter and the footer cannot
//! disagree about the same file. Sizes are binary units, matching the rest of
//! the shell rather than the platform this interaction is borrowed from.
use crate::value::{array, encode_uri_component, fixed, number, string, text};
use serde_json::{Value, json};

const UNITS: &[&str] = &["bytes", "KiB", "MiB", "GiB", "TiB", "PiB"];

/// A glyph per kind, from the same Material set the rest of the shell draws
/// from. An animation keeps the still image's glyph: the label already says
/// which it is, and a second pictogram for the same subject reads as noise.
fn glyph(kind: &str) -> &'static str {
    match kind {
        "directory" => "\u{f024b}",
        "image" | "animation" => "\u{f02e9}",
        "pdf" => "\u{f0226}",
        "markdown" => "\u{f0354}",
        "text" => "\u{f0219}",
        "audio" => "\u{f0386}",
        "video" => "\u{f0381}",
        "unavailable" => "\u{f0026}",
        _ => "\u{f0214}",
    }
}

fn kind_name(kind: &str) -> &'static str {
    match kind {
        "directory" => "Folder",
        "image" => "Image",
        "animation" => "Animation",
        "pdf" => "PDF document",
        "markdown" => "Markdown",
        "text" => "Text",
        "audio" => "Audio",
        "video" => "Video",
        "unavailable" => "Unavailable",
        _ => "File",
    }
}

/// Whether the panel can draw the file rather than only describe it.
fn drawable(kind: &str) -> bool {
    matches!(
        kind,
        "directory" | "image" | "animation" | "pdf" | "markdown" | "text" | "audio" | "video"
    )
}

pub fn size(bytes: f64) -> String {
    if !bytes.is_finite() || bytes < 0.0 {
        return String::new();
    }
    let bytes = bytes.floor();
    if bytes < 1.0 {
        return "Empty".into();
    }
    if bytes < 1024.0 {
        let unit = if bytes == 1.0 { "byte" } else { "bytes" };
        return format!("{} {unit}", bytes as u64);
    }
    let mut value = bytes;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{} {}", fixed(value, 1), UNITS[unit])
}

fn count(value: f64, singular: &str, plural: &str) -> String {
    let value = value.max(0.0).floor() as u64;
    format!("{value} {}", if value == 1 { singular } else { plural })
}

fn lines(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    text.lines().count() as u64
}

fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, suffix))
            if !stem.is_empty()
                && !suffix.is_empty()
                && suffix.len() <= 8
                && suffix.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            suffix.to_uppercase()
        }
        _ => String::new(),
    }
}

/// The header of one preview: its glyph, its name, and the one line of facts
/// underneath. An item the worker could not describe still gets all three, so
/// a missing file reads as a state rather than as an empty panel.
pub fn summary(item: &Value) -> Value {
    let kind = string(item.get("kind"));
    let name = text(item.get("name"));
    let error = text(item.get("error"));
    let mut parts = vec![kind_name(&kind).to_owned()];
    match kind.as_str() {
        "directory" => {
            let total = number(item.get("total"));
            let limited = item
                .get("limited")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            parts.push(if limited {
                format!("{}+ items", total.max(0.0).floor() as u64)
            } else {
                count(total, "item", "items")
            });
            if limited {
                parts.push(format!(
                    "showing {}",
                    count(array(item.get("entries")).len() as f64, "entry", "entries")
                ));
            }
        }
        "pdf" => {
            let pages = number(item.get("pages"));
            if pages > 0.0 {
                parts.push(count(pages, "page", "pages"));
            }
        }
        "markdown" | "text" => {
            let body = string(item.get("text"));
            parts.push(count(lines(&body) as f64, "line", "lines"));
        }
        _ => {}
    }
    let suffix = extension(&name);
    if !suffix.is_empty() && kind != "directory" && kind != "unavailable" {
        parts.insert(1, suffix);
    }
    if kind != "directory" && kind != "unavailable" {
        let bytes = size(number(item.get("size")));
        if !bytes.is_empty() {
            parts.push(bytes);
        }
    }
    json!({
        "glyph": glyph(&kind),
        "kind": kind,
        "title": if name.is_empty() { "Unnamed".to_owned() } else { name },
        "detail": parts.join(" · "),
        "note": error,
        "drawable": drawable(&kind) && error.is_empty(),
    })
}

/// A `file://` URL Qt parses back to exactly this path. Each segment is
/// encoded on its own, so a `#`, `?` or `%` in a name changes the file it
/// names rather than the shape of the URL.
pub fn url(path: &str) -> String {
    if !path.starts_with('/') {
        return String::new();
    }
    let mut out = String::from("file://");
    for segment in path.split('/').skip(1) {
        out.push('/');
        out.push_str(&encode_uri_component(segment));
    }
    out
}

/// Elapsed and total time for a sound or a moving picture. Hours appear only
/// once there are any, so a two-minute recording is not padded out to look
/// like a feature film.
pub fn duration(milliseconds: f64) -> String {
    if !milliseconds.is_finite() || milliseconds < 0.0 {
        return "--:--".into();
    }
    let total = (milliseconds / 1000.0).floor() as u64;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Moving through the set wraps. The counter beside it says which item this
/// is, so a wrap is legible, while an end that simply refuses the key is not.
pub fn step(total: f64, index: f64, delta: f64) -> i64 {
    let total = total.max(0.0).floor() as i64;
    if total <= 0 {
        return 0;
    }
    let index = index.max(0.0).floor() as i64;
    let delta = delta.floor() as i64;
    (index + delta).rem_euclid(total)
}

/// Pages do not wrap: a document has a real first and last page, and landing
/// on page one after the last would read as a failed key rather than a move.
pub fn page(pages: f64, page: f64, delta: f64) -> i64 {
    let pages = pages.max(0.0).floor() as i64;
    if pages <= 0 {
        return 0;
    }
    let page = page.max(1.0).floor() as i64;
    (page + delta.floor() as i64).clamp(1, pages)
}

/// The footer. It names only the keys this preview actually answers, so a
/// single text file is not told about page or item navigation it does not have.
pub fn hint(item: &Value, total: f64) -> String {
    let kind = string(item.get("kind"));
    let mut parts = vec!["Space or Esc to close".to_owned()];
    if total.floor() > 1.0 {
        parts.push("← → between files".into());
    }
    if kind == "pdf" && number(item.get("pages")) > 1.0 && text(item.get("error")).is_empty() {
        parts.push("↑ ↓ between pages".into());
    }
    if matches!(kind.as_str(), "audio" | "video") && text(item.get("error")).is_empty() {
        parts.push("P plays".into());
    }
    if kind != "unavailable" {
        parts.push("Enter to open".into());
    }
    parts.push("Ctrl + C copies the path".into());
    parts.join(" · ")
}

pub fn call(function: &str, arguments: &[Value]) -> Result<Value, String> {
    let argument = |at: usize| arguments.get(at).cloned().unwrap_or(Value::Null);
    match function {
        "summary" => Ok(summary(&argument(0))),
        "url" => Ok(json!(url(&string(arguments.first())))),
        "duration" => Ok(json!(duration(number(arguments.first())))),
        "step" => Ok(json!(step(
            number(arguments.first()),
            number(arguments.get(1)),
            number(arguments.get(2))
        ))),
        "page" => Ok(json!(page(
            number(arguments.first()),
            number(arguments.get(1)),
            number(arguments.get(2))
        ))),
        "hint" => Ok(json!(hint(&argument(0), number(arguments.get(1))))),
        _ => Err("unknown native function".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_binary_units_and_name_the_two_degenerate_cases() {
        assert_eq!(size(0.0), "Empty");
        assert_eq!(size(1.0), "1 byte");
        assert_eq!(size(999.0), "999 bytes");
        assert_eq!(size(1024.0), "1.0 KiB");
        assert_eq!(size(1536.0), "1.5 KiB");
        assert_eq!(size(1024.0 * 1024.0 * 3.25), "3.3 MiB");
        assert_eq!(size(-1.0), "");
        assert_eq!(size(f64::NAN), "");
    }

    #[test]
    fn a_summary_states_the_kind_the_format_and_the_facts_that_kind_has() {
        let document = json!({
            "kind": "pdf", "name": "Rechnung.pdf", "size": 4_300_000, "pages": 12, "error": ""
        });
        let header = summary(&document);
        assert_eq!(header["detail"], "PDF document · PDF · 12 pages · 4.1 MiB");
        assert_eq!(header["drawable"], true);
        assert_eq!(header["glyph"], "\u{f0226}");

        let folder = json!({
            "kind": "directory", "name": "Pictures", "total": 256, "limited": true,
            "entries": vec![json!({"name": "a"}); 256], "error": ""
        });
        assert_eq!(
            summary(&folder)["detail"],
            "Folder · 256+ items · showing 256 entries"
        );

        let note = json!({ "kind": "markdown", "name": "plan.md", "size": 120, "text": "a\nb\n", "error": "" });
        assert_eq!(
            summary(&note)["detail"],
            "Markdown · MD · 2 lines · 120 bytes"
        );

        let one = json!({ "kind": "text", "name": "LICENSE", "size": 1, "text": "x", "error": "" });
        assert_eq!(summary(&one)["detail"], "Text · 1 line · 1 byte");
    }

    #[test]
    fn an_undescribable_item_still_has_a_header_and_is_never_drawable() {
        let missing = json!({
            "kind": "unavailable", "name": "gone.txt", "error": "This file is no longer there"
        });
        let header = summary(&missing);
        assert_eq!(header["title"], "gone.txt");
        assert_eq!(header["detail"], "Unavailable");
        assert_eq!(header["note"], "This file is no longer there");
        assert_eq!(header["drawable"], false);
        // A kind the panel can draw still refuses once the worker reported a
        // failure against it.
        let broken = json!({ "kind": "pdf", "name": "sealed.pdf", "size": 10, "error": "This document cannot be opened" });
        assert_eq!(summary(&broken)["drawable"], false);
    }

    #[test]
    fn a_file_url_survives_every_character_a_name_may_contain() {
        assert_eq!(url("/home/user/a b.png"), "file:///home/user/a%20b.png");
        assert_eq!(
            url("/tmp/100%25 #1?.pdf"),
            "file:///tmp/100%2525%20%231%3F.pdf"
        );
        assert_eq!(url("/"), "file:///");
        assert_eq!(url("relative.png"), "");
        assert_eq!(url(""), "");
    }

    #[test]
    fn durations_grow_a_place_only_when_there_is_one_to_show() {
        assert_eq!(duration(0.0), "0:00");
        assert_eq!(duration(83_000.0), "1:23");
        assert_eq!(duration(3_723_000.0), "1:02:03");
        assert_eq!(duration(-1.0), "--:--");
        assert_eq!(duration(f64::NAN), "--:--");
    }

    #[test]
    fn items_wrap_and_pages_stop() {
        assert_eq!(step(3.0, 2.0, 1.0), 0);
        assert_eq!(step(3.0, 0.0, -1.0), 2);
        assert_eq!(step(1.0, 0.0, 1.0), 0);
        assert_eq!(step(0.0, 0.0, 1.0), 0);
        assert_eq!(page(12.0, 12.0, 1.0), 12);
        assert_eq!(page(12.0, 1.0, -1.0), 1);
        assert_eq!(page(12.0, 1.0, 5.0), 6);
        assert_eq!(page(0.0, 1.0, 1.0), 0);
    }

    #[test]
    fn the_footer_names_only_the_keys_this_preview_answers() {
        let single = json!({ "kind": "text", "name": "a.txt", "error": "" });
        let footer = hint(&single, 1.0);
        assert!(!footer.contains("between files"));
        assert!(!footer.contains("between pages"));
        assert!(footer.contains("Enter to open"));

        let document = json!({ "kind": "pdf", "name": "a.pdf", "pages": 9, "error": "" });
        let footer = hint(&document, 4.0);
        assert!(footer.contains("← → between files"));
        assert!(footer.contains("↑ ↓ between pages"));

        let sealed = json!({ "kind": "pdf", "name": "a.pdf", "pages": 9, "error": "no" });
        assert!(!hint(&sealed, 1.0).contains("between pages"));

        let tune = json!({ "kind": "audio", "name": "a.flac", "error": "" });
        assert!(hint(&tune, 1.0).contains("P plays"));
        let text = json!({ "kind": "text", "name": "a.txt", "error": "" });
        assert!(!hint(&text, 1.0).contains("P plays"));

        let missing = json!({ "kind": "unavailable", "name": "a", "error": "gone" });
        assert!(!hint(&missing, 1.0).contains("Enter to open"));
        assert!(hint(&missing, 1.0).contains("Ctrl + C copies the path"));
    }
}
