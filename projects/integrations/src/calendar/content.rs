//! Event text and links: descriptions to plain text, and meeting links found
//! in conference data or pasted into an invitation.
use super::*;

pub(super) fn safe_link(value: &str) -> Option<Url> {
    if value.len() > 4096 {
        return None;
    }
    let url = Url::parse(value).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|h| !h.is_empty()))
    .then_some(url)
}

pub(super) fn meeting_service(host: &str) -> Option<&'static str> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let on = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    Some(if on("meet.google.com") {
        "Google Meet"
    } else if on("zoom.us") || on("zoomgov.com") {
        "Zoom"
    } else if on("teams.microsoft.com") || on("teams.live.com") {
        "Microsoft Teams"
    } else if on("webex.com") {
        "Webex"
    } else if on("whereby.com") {
        "Whereby"
    } else if on("meet.jit.si") {
        "Jitsi Meet"
    } else if on("gotomeeting.com") || on("meet.goto.com") {
        "GoTo Meeting"
    } else if on("chime.aws") {
        "Amazon Chime"
    } else {
        return None;
    })
}

/// The first link to a known meeting service in free text, such as a Zoom
/// invitation pasted into the location or description.
pub(super) fn find_meeting(text: &str) -> Option<(String, &'static str)> {
    for (at, _) in text.match_indices("https://") {
        let candidate = text[at..]
            .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']'))
            .next()
            .unwrap_or("")
            .trim_end_matches(['.', ',', ';', ':']);
        if let Some(url) = safe_link(candidate) {
            if let Some(name) = url.host_str().and_then(meeting_service) {
                return Some((url.to_string(), name));
            }
        }
    }
    None
}

pub(super) fn meeting(value: &Value) -> Option<(String, String)> {
    let video = value["conferenceData"]["entryPoints"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["entryPointType"] == "video"))
        .and_then(|item| item["uri"].as_str())
        .and_then(safe_link);
    if let Some(url) = video {
        let mut name = crate::common::clean(
            &value["conferenceData"]["conferenceSolution"]["name"],
            "",
            40,
        );
        if name.is_empty() {
            name = url
                .host_str()
                .and_then(meeting_service)
                .unwrap_or("Video call")
                .to_owned();
        }
        return Some((url.to_string(), name));
    }
    if let Some(url) = value["hangoutLink"].as_str().and_then(safe_link) {
        return Some((url.to_string(), "Google Meet".to_owned()));
    }
    [&value["location"], &value["description"]]
        .into_iter()
        .filter_map(Value::as_str)
        .find_map(|text| find_meeting(&text[..floor_boundary(text, 65_536)]))
        .map(|(url, name)| (url, name.to_owned()))
}

pub(super) fn floor_boundary(text: &str, limit: usize) -> usize {
    let mut at = text.len().min(limit);
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub(super) fn entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)?
        }
    })
}

pub(super) fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200e}' | '\u{200f}' | '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

/// Google stores descriptions written in its editor as HTML. The agenda shows
/// plain text: block tags become line breaks, list items bullets, entities are
/// decoded, and every other tag disappears. A `<` that does not open a tag is text.
pub(super) fn plain_text(value: &str, limit: usize) -> String {
    let mut out = String::with_capacity(value.len().min(limit * 2));
    let mut rest = value;
    while let Some(at) = rest.find(['<', '&']) {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(inner) = rest.strip_prefix('<') {
            let opens = inner
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!');
            if let (true, Some(close)) = (opens, inner.find('>')) {
                let closing = inner.starts_with('/');
                let tag = inner[..close]
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                // Adjacent block tags make one line break, not a run of blank lines.
                let block = matches!(
                    tag.as_str(),
                    "br" | "p"
                        | "div"
                        | "tr"
                        | "ul"
                        | "ol"
                        | "li"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "blockquote"
                        | "pre"
                );
                if block && !out.ends_with('\n') {
                    out.push('\n');
                }
                if tag == "li" && !closing {
                    out.push_str("• ");
                }
                rest = &inner[close + 1..];
                continue;
            }
            out.push('<');
            rest = inner;
        } else {
            // `;` is ASCII, so its byte position is a character boundary.
            if let Some(end) = rest.bytes().take(12).position(|b| b == b';') {
                if let Some(ch) = entity(&rest[1..end]) {
                    out.push(ch);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    let mut lines: Vec<String> = Vec::new();
    let mut gap = false;
    for line in out.split('\n') {
        let line = line
            .chars()
            .filter(|c| !c.is_control() && !invisible(*c))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if line.is_empty() || line == "•" {
            gap = !lines.is_empty();
            continue;
        }
        if gap {
            lines.push(String::new());
            gap = false;
        }
        lines.push(line);
    }
    let text = lines.join("\n");
    match text.char_indices().nth(limit) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text,
    }
}
