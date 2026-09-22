//! Clipboard-only URL projection. Parse for validation, splice the original bytes.
use crate::Result;
use serde::Serialize;
use std::{os::fd::AsFd, time::Duration};

const MAX_BYTES: usize = 16 * 1024;
const INVALID: &str =
    "Copy one HTTP or HTTPS link, without spaces or control characters (maximum 16 KiB).";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    original: String,
    cleaned: String,
    removed: Vec<String>,
    status: &'static str,
    message: &'static str,
    markdown: String,
}

fn protected_key(key: &str) -> bool {
    key.starts_with("x-amz-")
        || key.starts_with("x-goog-")
        || key.contains("signature")
        || key.contains("token")
        || key.contains("auth")
        || matches!(
            key,
            "sig"
                | "s"
                | "hmac"
                | "hash"
                | "h"
                | "key"
                | "api_key"
                | "apikey"
                | "policy"
                | "credential"
                | "expires"
                | "expiry"
                | "exp"
                | "awsaccesskeyid"
                | "key-pair-id"
                | "hdnts"
                | "hdnea"
                | "__gda__"
                | "jwt"
                | "code"
        )
}

fn tracking_key(key: &str) -> bool {
    (key.starts_with("utm_")
        && key.len() > 4
        && key.len() <= 64
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        || matches!(
            key,
            "fbclid"
                | "gclid"
                | "dclid"
                | "msclkid"
                | "mc_cid"
                | "mc_eid"
                | "igshid"
                | "_ga"
                | "_gl"
        )
}

fn key(part: &str) -> String {
    // Decode names only to recognize encoded tracking/signature keys. Retained
    // segments are never serialized through the URL parser.
    url::form_urlencoded::parse(part.as_bytes())
        .next()
        .map(|(key, _)| key.to_ascii_lowercase())
        .unwrap_or_default()
}

fn preview(input: &str) -> Result<Preview> {
    if input.is_empty()
        || input.len() > MAX_BYTES
        || input.chars().any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'))
        || input.contains('\\')
        || !(input.get(..7).is_some_and(|s| s.eq_ignore_ascii_case("http://"))
            || input.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")))
    {
        return Err(INVALID.into());
    }
    let authority = input.split_once("://").ok_or(INVALID)?.1;
    if authority.starts_with(['/', '?', '#']) {
        return Err(INVALID.into());
    }
    let parsed = url::Url::parse(input).map_err(|_| INVALID)?;
    if parsed.host_str().is_none() {
        return Err(INVALID.into());
    }
    // URL accepts malformed percent escapes. Reject these instead of guessing.
    for (i, b) in input.bytes().enumerate() {
        if b == b'%'
            && !input
                .as_bytes()
                .get(i + 1..i + 3)
                .is_some_and(|v| v.iter().all(u8::is_ascii_hexdigit))
        {
            return Err(INVALID.into());
        }
    }
    let (before_fragment, fragment) = input
        .split_once('#')
        .map_or((input, ""), |(a, _)| (a, &input[a.len()..]));
    let query = before_fragment.split_once('?');
    let protected = !parsed.username().is_empty()
        || parsed.password().is_some()
        || query.is_some_and(|(_, q)| q.split('&').any(|part| protected_key(&key(part))))
        || fragment
            .trim_start_matches('#')
            .split('&')
            .any(|part| protected_key(&key(part)))
        || parsed.path().split('/').any(|part| {
            part.split('~')
                .any(|value| protected_key(&key(value)) && value.contains('='))
        });
    let mut cleaned = input.to_owned();
    let mut removed = Vec::new();
    let (status, message) = if protected {
        (
            "protected",
            "Kept unchanged: this link contains authentication or signing markers.",
        )
    } else if let Some((base, query)) = query {
        if query.contains(';') {
            (
                "ambiguous",
                "Kept unchanged: this query uses ambiguous semicolon separators.",
            )
        } else {
            let retained: Vec<_> = query
                .split('&')
                .filter(|part| {
                    let name = key(part);
                    if tracking_key(&name) {
                        if !removed.contains(&name) {
                            removed.push(name);
                        }
                        false
                    } else {
                        true
                    }
                })
                .collect();
            if removed.is_empty() {
                ("unchanged", "No recognized tracking parameters found.")
            } else {
                cleaned = format!(
                    "{base}{}{fragment}",
                    if retained.is_empty() {
                        String::new()
                    } else {
                        format!("?{}", retained.join("&"))
                    }
                );
                ("cleaned", "Review the link, then copy it when ready.")
            }
        }
    } else {
        ("unchanged", "No recognized tracking parameters found.")
    };
    // Indented code keeps URL contents inert: no remote images or clickable
    // markdown can be created by clipboard content, including backticks.
    let markdown = format!("{message}\n\n## Original\n\n    {input}\n\n## {}\n\n    {cleaned}\n\n## Removed parameters\n\n    {}", if status == "cleaned" { "Cleaned" } else { "Unchanged" }, if removed.is_empty() { "None".to_owned() } else { removed.join(", ") });
    Ok(Preview {
        original: input.into(),
        cleaned,
        removed,
        status,
        message,
        markdown,
    })
}

pub fn run(arguments: &[String]) -> Result {
    if arguments.len() != 1 {
        return Err("Clean Link accepts clipboard text on stdin only.".into());
    }
    let stop = crate::command::shutdown_signal();
    let mut stdin = seele_runtime::wire::NonblockingFile::new(
        std::io::stdin().as_fd().try_clone_to_owned()?.into(),
    )?;
    let bytes =
        seele_runtime::wire::read_bytes(&mut stdin, MAX_BYTES, Duration::from_secs(2), &stop)
            .map_err(|_| "Clipboard input unavailable or too large.")?;
    let input = std::str::from_utf8(&bytes).map_err(|_| INVALID)?;
    println!("{}", serde_json::to_string(&preview(input)?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_retained_bytes_and_removes_only_known_keys() {
        let p = preview(
            "HTTPS://Example.COM:443/a%2fb?keep=a+b&%75tm_source=x&keep=%2f&fbclid=y&ref=z#A%2fb",
        )
        .unwrap();
        assert_eq!(
            p.cleaned,
            "HTTPS://Example.COM:443/a%2fb?keep=a+b&keep=%2f&ref=z#A%2fb"
        );
        assert_eq!(p.removed, ["utm_source", "fbclid"]);
        assert_eq!(
            preview("https://a.test/?utm_source=x&utm_source=y#z")
                .unwrap()
                .cleaned,
            "https://a.test/#z"
        );
        assert_eq!(
            preview("https://a.test/?utm_source=x&&x=1&")
                .unwrap()
                .cleaned,
            "https://a.test/?&x=1&"
        );
        for value in [
            "https://a.test/?",
            "https://a.test/?x=%2f&x=+",
            "https://a.test/#utm_source=x",
            "https://a.test/?utm_=x&ref=x",
        ] {
            assert_eq!(preview(value).unwrap().cleaned, value);
        }
    }
    #[test]
    fn preserves_protected_and_ambiguous_links() {
        for suffix in [
            "sig=x",
            "%73ignature=x",
            "X-Amz-Credential=x",
            "X-Goog-Algorithm=x",
            "access_token=x",
            "auth=x",
            "Policy=x",
            "expires=1",
            "hdnts=x",
            "code=x",
        ] {
            let input = format!("https://a.test/?utm_source=x&{suffix}");
            let p = preview(&input).unwrap();
            assert_eq!(p.cleaned, input);
            assert_eq!(p.status, "protected");
            assert!(p.removed.is_empty());
        }
        for input in [
            "https://user:pass@a.test/?utm_source=x",
            "https://a.test/?utm_source=x#access_token=x",
            "https://a.test/token=abc~exp=1/a?utm_source=x",
            "https://a.test/?utm_source=x;sig=y",
        ] {
            assert_eq!(preview(input).unwrap().cleaned, input);
        }
    }
    #[test]
    fn markdown_content_is_inert_even_with_image_and_fence_syntax() {
        for input in [
            "https://a.test/![x](https://remote.test/image)?utm_source=x",
            "https://a.test/```![x](https://remote.test/image)?utm_source=x",
        ] {
            let p = preview(input).unwrap();
            for line in p.markdown.lines().filter(|line| line.contains("https://")) {
                assert!(line.starts_with("    "));
            }
            assert_eq!(
                p.markdown
                    .lines()
                    .filter(|line| line.contains("https://"))
                    .count(),
                2
            );
        }
    }
    #[test]
    fn rejects_ambiguous_and_unsafe_clipboard_text_without_echo() {
        for input in [
            "",
            " https://a.test",
            "https://a.test\n",
            "https://a.test https://b.test",
            "file:///tmp/a",
            "https:a.test",
            "https:///a.test",
            "https://a.test/\\x",
            "https://a.test/%xx",
            "https://a.test/\u{202e}",
            "https://a.test/\0",
        ] {
            assert!(preview(input).is_err(), "{input:?}");
        }
        assert!(preview(&format!("https://a.test/{}", "x".repeat(MAX_BYTES))).is_err());
    }
}
