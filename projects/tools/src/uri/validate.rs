//! Validation never serializes a parsed URL: its spelling is the captured text.
use std::{collections::HashSet, sync::OnceLock};
use url::{Host, Url};

const MAX_URI_BYTES: usize = 8192;

pub(crate) fn scheme(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"+.-".contains(&c))
}

fn invisible(c: char) -> bool {
    matches!(c,
        '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}' | '\u{1160}' |
        '\u{17b4}' | '\u{17b5}' | '\u{180e}' | '\u{200b}'..='\u{200f}' |
        '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{3164}' |
        '\u{fe00}'..='\u{fe0f}' | '\u{feff}' | '\u{ffa0}' | '\u{fffd}' |
        '\u{e0000}'..='\u{e0fff}')
}

fn valid_text(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_URI_BYTES
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || invisible(c) || "\"<>`\\".contains(c))
        && s.as_bytes().iter().enumerate().all(|(i, c)| {
            *c != b'%'
                || s.as_bytes()
                    .get(i + 1..i + 3)
                    .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
        })
}

/// ICANN rules, including their IDNA spellings, come from the pinned offline
/// list. Private suffixes cannot turn arbitrary code attributes into links.
fn public_tld(host: &str) -> bool {
    static TLDS: OnceLock<HashSet<String>> = OnceLock::new();
    let tlds = TLDS.get_or_init(|| {
        include_str!(env!("URI_PUBLIC_SUFFIX_LIST"))
            .lines()
            .take_while(|line| !line.contains("END ICANN DOMAINS"))
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .filter_map(|line| line.rsplit('.').next())
            .filter_map(|tld| match Host::parse(tld).ok()? {
                Host::Domain(name) => Some(name),
                _ => None,
            })
            .collect()
    });
    host.rsplit_once('.')
        .is_some_and(|(_, tld)| tlds.contains(tld))
}

fn valid_host(host: &str, public: bool) -> bool {
    // WHATWG accepts percent-encoded hosts and abbreviated/octal IPv4. Those
    // are too ambiguous for a visual picker; require a visibly complete host.
    if host.is_empty() || host.contains('%') {
        return false;
    }
    if host.starts_with('[') {
        return !public
            && host
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .is_some_and(|s| s.parse::<std::net::Ipv6Addr>().is_ok());
    }
    match Host::parse(host) {
        Ok(Host::Ipv4(_)) => !public && host.parse::<std::net::Ipv4Addr>().is_ok(),
        Ok(Host::Ipv6(_)) => false,
        Ok(Host::Domain(ascii)) => {
            let ascii = ascii.strip_suffix('.').unwrap_or(&ascii);
            ascii.len() <= 253
                && ascii.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                })
                && (!public || public_tld(ascii))
        }
        Err(_) => false,
    }
}

fn authority(s: &str, public: bool, credentials: bool) -> bool {
    let host_port = if let Some((user, host)) = s.split_once('@') {
        if !credentials || user.is_empty() || host.contains('@') {
            return false;
        }
        host
    } else {
        s
    };
    let (host, port) = if host_port.starts_with('[') {
        let Some(close) = host_port.find(']') else {
            return false;
        };
        let tail = &host_port[close + 1..];
        if tail.is_empty() {
            (&host_port[..=close], None)
        } else if let Some(port) = tail.strip_prefix(':') {
            (&host_port[..=close], Some(port))
        } else {
            return false;
        }
    } else if let Some((host, port)) = host_port.split_once(':') {
        (host, Some(port))
    } else {
        (host_port, None)
    };
    valid_host(host, public)
        && port.is_none_or(|port| {
            !port.is_empty()
                && port.bytes().all(|c| c.is_ascii_digit())
                && port.parse::<u16>().is_ok()
        })
}

fn mailbox(s: &str, public: bool) -> bool {
    let Some((local, host)) = s.split_once('@') else {
        return false;
    };
    // Unquoted dot-atoms cover visible ordinary mailboxes. Do not interpret a
    // path, URL query, or a second @ as part of a mailbox.
    !local.is_empty()
        && local.len() <= 64
        && s.len() <= 254
        && local.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-=^_{|}~".contains(&c))
        })
        && valid_host(host, public)
}

/// Convert visible text to a URI. Strip surrounding prose, never OCR spelling
/// errors. Sentence-ending punctuation in OCR is a prose heuristic; decoded
/// payloads use `destination` instead. Reject truncation before removing
/// punctuation, including `...).`.
pub fn normalize(text: &str) -> Option<String> {
    if text.len() > MAX_URI_BYTES || text.chars().any(|c| c.is_control() || invisible(c)) {
        return None;
    }
    let mut s = text.trim();
    s = s.trim_start_matches(|c| "([{\"'`<“‘".contains(c));
    if s.contains('…') {
        return None;
    }
    loop {
        if s.ends_with("...") {
            return None;
        }
        let before = s;
        // Remove one character at a time so trailing punctuation cannot hide
        // a truncated ellipsis from the check above.
        if s.ends_with(|c| ".,;!\"'`<>“”‘’".contains(c)) {
            s = &s[..s.char_indices().next_back()?.0];
        }
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            if s.ends_with(close) && s.matches(close).count() > s.matches(open).count() {
                s = &s[..s.len() - close.len_utf8()];
            }
        }
        if before == s {
            break;
        }
    }
    destination(s)
}

/// Decoded code payloads have no prose to clean. Validate without trimming or
/// reserializing: callers retain the original payload even when it cannot open.
pub fn destination(s: &str) -> Option<String> {
    if !valid_text(s) {
        return None;
    }
    if let Some((prefix, rest)) = s
        .split_once(':')
        .filter(|(prefix, rest)| scheme(prefix) && rest.starts_with("//"))
    {
        let rest = &rest[2..];
        if rest.is_empty() {
            return None;
        }
        let host = rest.split(['/', '?', '#']).next()?;
        let file = prefix.eq_ignore_ascii_case("file");
        if file {
            if host.is_empty() {
                if !rest.starts_with('/') {
                    return None;
                }
            } else if !valid_host(host, false) {
                return None;
            }
        } else if !authority(host, false, true) {
            return None;
        }
        Url::parse(s).ok()?;
        return Some(s.into());
    }
    if let Some((prefix, rest)) = s.split_once(':') {
        let known = ["mailto", "tel", "sms", "magnet", "geo", "news", "urn"]
            .contains(&prefix.to_ascii_lowercase().as_str());
        if known {
            if rest.is_empty() {
                return None;
            }
            if prefix.eq_ignore_ascii_case("mailto") {
                let addresses = rest.split('?').next()?;
                if !addresses.split(',').all(|address| mailbox(address, false)) {
                    return None;
                }
            }
            Url::parse(s).ok()?;
            return Some(s.into());
        }
    }
    let host = s.split(['/', '?', '#']).next()?;
    if authority(host, true, false) {
        let uri = format!("https://{s}");
        Url::parse(&uri).ok()?;
        return Some(uri);
    }
    if mailbox(s, true) {
        return Some(format!("mailto:{s}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_destinations_keep_their_original_spelling() {
        for uri in [
            "HTTPS://EXAMPLE.COM:443/a%2fb?q=1&b=2#fragment",
            "http://localhost:3000/",
            "http://192.168.0.1:65535/a",
            "https://[2001:db8::1]:443/a",
            "http://[::ffff:192.0.2.128]/",
            "https://bücher.de/日本語",
            "https://example.org./",
            "https://user:pass@example.org/path",
            "https://example.org/a_(b)",
            "https://example.org/end.;!",
            "https://example.org/...",
            "file:///tmp/document.pdf",
            "file://localhost/tmp/a",
            "custom://open/item",
            "vscode://file/a.rs",
            "MAILTO:user+tag@example.org",
            "mailto:one@example.org,two@example.org?subject=Hello%20there",
            "mailto:one@example.org?body=https://example.org",
            "tel:+491234567",
            "sms:+491234567",
            "geo:52.5,13.4",
            "magnet:?xt=urn:btih:abc",
            "news:comp.lang.rust",
            "urn:isbn:1234",
        ] {
            assert_eq!(destination(uri).as_deref(), Some(uri), "{uri}");
        }
        for (text, expected) in [
            ("example.org", "https://example.org"),
            ("EXAMPLE.COM:8080/a", "https://EXAMPLE.COM:8080/a"),
            ("bücher.de", "https://bücher.de"),
            ("example.中国", "https://example.中国"),
            ("example.xn--fiqs8s", "https://example.xn--fiqs8s"),
            ("example.org/path@host", "https://example.org/path@host"),
            (
                "example.org/?email=a@b.org",
                "https://example.org/?email=a@b.org",
            ),
            (
                "example.org/?url=https://other.org",
                "https://example.org/?url=https://other.org",
            ),
            (
                "user.name+tag@example.org",
                "mailto:user.name+tag@example.org",
            ),
            ("user@example.中国", "mailto:user@example.中国"),
        ] {
            assert_eq!(destination(text).as_deref(), Some(expected), "{text}");
        }
    }

    #[test]
    fn refuses_malformed_or_ambiguous_hosts_and_mailboxes() {
        for text in [
            "https://",
            "https:///missing-host",
            "http:example.org",
            "https://?q=a",
            "https://user@",
            "https://user@@example.org",
            "https://@example.org",
            "https://bad_host.org/",
            "https://-bad.org/",
            "https://bad-.org/",
            "https://a..org/",
            "https://%65xample.org/",
            "https://example.org:abc",
            "https://example.org:",
            "https://example.org:65536",
            "https://example.org:-1",
            "http://[::1",
            "http://[::1]junk/",
            "http://[gg::1]/",
            "http://::1/",
            "http://127.1/",
            "http://2130706433/",
            "http://0177.0.0.1/",
            "http://256.0.0.1/",
            "http://0x7f.0.0.1/",
            "http://[::1]:/",
            "example.org:99999",
            "localhost",
            "127.0.0.1",
            "flake.nix",
            "determinate.url",
            "user@example.org/path",
            "user@example.org?query",
            "user@example.org#fragment",
            ".user@example.org",
            "user.@example.org",
            "user..name@example.org",
            "one@@example.org",
            "user@-example.org",
            "user:pass@example.org",
            "mailto:not-a-mailbox",
            "mailto:user@example.org/path",
            "mailto:@example.org",
            "foo/bar@example.org",
            "file://user@host/tmp/a",
            "file://host:80/tmp/a",
        ] {
            assert_eq!(destination(text), None, "{text}");
        }
    }

    #[test]
    fn rejects_controls_invisibles_whitespace_and_broken_escapes() {
        for tail in [
            "%",
            "%2",
            "%xx",
            "%2G",
            "a b",
            "a\tb",
            "a\nb",
            "a\u{0}b",
            "a\u{200b}b",
            "a\u{202e}b",
            "a\u{2066}b",
            "a\u{ad}b",
            "a\u{feff}b",
            "a\u{fffd}b",
            "a\\b",
            "a<b",
            "a`b",
            "a\"b",
        ] {
            let text = format!("https://example.org/{tail}");
            assert_eq!(destination(&text), None, "{text:?}");
            assert_eq!(normalize(&text), None, "{text:?}");
        }
        assert_eq!(destination(" https://example.org "), None);
    }

    #[test]
    fn prose_wrappers_do_not_hide_truncation_or_remove_balanced_parentheses() {
        for (text, expected) in [
            ("(https://example.org/a_(b)).", "https://example.org/a_(b)"),
            (
                "(\"https://example.org/path\");",
                "https://example.org/path",
            ),
            ("[http://[::1]:8080/a]", "http://[::1]:8080/a"),
            ("  <https://example.org/path>  ", "https://example.org/path"),
            (
                "https://example.org/a_(b_(c))",
                "https://example.org/a_(b_(c))",
            ),
        ] {
            assert_eq!(normalize(text).as_deref(), Some(expected), "{text}");
        }
        for text in [
            "https://example.org/...",
            "(https://example.org/...).",
            "\"https://example.org/...\";",
            "https://example.org/...!",
            "https://example.org/…)",
        ] {
            assert_eq!(normalize(text), None, "{text}");
        }
        assert_eq!(
            destination("https://example.org/..."),
            Some("https://example.org/...".into())
        );
    }

    #[test]
    fn bounds_uri_hostname_label_and_mailbox_lengths() {
        let prefix = "https://example.org/";
        let maximum = format!("{prefix}{}", "a".repeat(MAX_URI_BYTES - prefix.len()));
        assert_eq!(destination(&maximum), Some(maximum.clone()));
        assert_eq!(destination(&(maximum + "a")), None);
        for length in [63, 64] {
            let text = format!("https://{}.org/", "a".repeat(length));
            assert_eq!(destination(&text).is_some(), length == 63);
        }
        for length in [64, 65] {
            let text = format!("{}@example.org", "a".repeat(length));
            assert_eq!(destination(&text).is_some(), length == 64);
        }
        for last in [61, 62] {
            let text = format!(
                "https://{}.{}.{}.{}",
                "a".repeat(63),
                "b".repeat(63),
                "c".repeat(63),
                "d".repeat(last)
            );
            assert_eq!(destination(&text).is_some(), last == 61);
        }
    }
}
