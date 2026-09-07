use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub line_start: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Link {
    pub uri: String,
    pub output: String,
    pub x0: f64,
    pub y0: f64,
    pub w: f64,
    pub h: f64,
    pub number: usize,
}

fn scheme(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"+.-".contains(&c))
}

fn domain(s: &str) -> bool {
    let host = s.split(['/', '?', '#', ':']).next().unwrap_or_default();
    let labels: Vec<_> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
        && labels
            .last()
            .is_some_and(|tld| tld.len() >= 2 && tld.chars().all(char::is_alphabetic))
}

/// Convert visible links to absolute URIs. Never repair OCR substitutions or
/// truncated text by guessing a destination. URI punctuation is kept intact;
/// only surrounding prose punctuation and unmatched closing brackets go away.
pub fn normalize(text: &str) -> Option<String> {
    let mut s = text.trim_matches(|c: char| c.is_whitespace() || "\"'`<>“”‘’".contains(c));
    s = s.trim_start_matches(['(', '[', '{']);
    if s.contains('…') || s.ends_with("...") {
        return None;
    }
    loop {
        let before = s;
        s = s.trim_end_matches(['.', ',', ';', '!']);
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            if s.ends_with(close) && s.matches(close).count() > s.matches(open).count() {
                s = &s[..s.len() - close.len_utf8()];
            }
        }
        if before == s {
            break;
        }
    }
    if s.is_empty()
        || s.len() > 8192
        || s.chars()
            .any(|c| c.is_whitespace() || c.is_control() || "\"<>`\\".contains(c))
    {
        return None;
    }
    if let Some((prefix, rest)) = s.split_once("://") {
        if scheme(prefix) && !rest.is_empty() {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            if !authority.is_empty() || prefix.eq_ignore_ascii_case("file") && rest.starts_with('/')
            {
                return Some(s.into());
            }
        }
        return None;
    }
    if let Some((prefix, rest)) = s.split_once(':') {
        if ["mailto", "tel", "sms", "magnet", "geo", "news", "urn"]
            .contains(&prefix.to_ascii_lowercase().as_str())
            && !rest.is_empty()
        {
            return Some(s.into());
        }
    }
    if let Some((local, host)) = s.split_once('@') {
        if !local.is_empty()
            && !local.contains(['/', ':', '@'])
            && !host.contains('@')
            && domain(host)
        {
            return Some(format!("mailto:{s}"));
        }
        return None;
    }
    if domain(s) {
        Some(format!("https://{s}"))
    } else {
        None
    }
}

fn can_join(a: &Word, b: &Word) -> bool {
    if b.line_start || b.left < a.left {
        return false;
    }
    let height = (a.bottom - a.top).max(b.bottom - b.top).max(1);
    let gap = b.left - a.right;
    let overlap = a.bottom.min(b.bottom) - a.top.max(b.top);
    let combined = format!("{}{}", a.text, b.text);
    let plausible =
        normalize(&combined).is_some() || combined.strip_suffix("://").is_some_and(scheme);
    plausible
        && overlap > 0
        && gap <= height * 3 / 4
        && (gap <= height / 6
            || a.text
                .ends_with([':', '/', '.', '@', '=', '?', '#', '&', '%', '-'])
            || b.text.starts_with(['/', '.', '@', '=', '?', '#', '&', '%']))
}

pub fn extract(
    words: Vec<Word>,
    output: &str,
    width: usize,
    height: usize,
    strip_y: usize,
    core_start: usize,
    core_end: usize,
) -> Vec<Link> {
    let mut runs: Vec<Word> = Vec::new();
    for word in words {
        if let Some(last) = runs.last_mut() {
            if can_join(last, &word) {
                last.text.push_str(&word.text);
                last.right = word.right;
                last.top = last.top.min(word.top);
                last.bottom = last.bottom.max(word.bottom);
                continue;
            }
        }
        runs.push(word);
    }
    runs.into_iter()
        .filter_map(|word| {
            let top = word.top.max(0) as usize + strip_y;
            let bottom = word.bottom.max(0) as usize + strip_y;
            let center = (top + bottom) / 2;
            if center < core_start || center >= core_end {
                return None;
            }
            let uri = normalize(&word.text)?;
            Some(Link {
                uri,
                output: output.into(),
                x0: f64::from(word.left.max(0)) / width as f64,
                y0: top as f64 / height as f64,
                w: f64::from((word.right - word.left).max(1)) / width as f64,
                h: (bottom - top).max(1) as f64 / height as f64,
                number: 0,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_uri_syntax_and_strips_prose() {
        for (input, expected) in [
            (
                "(https://en.wikipedia.org/wiki/Rust_(programming_language)).",
                "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            ),
            (
                "https://example.org/a%20b?q=a&b=2#part",
                "https://example.org/a%20b?q=a&b=2#part",
            ),
            ("http://[::1]:8080/test", "http://[::1]:8080/test"),
            ("example.org/path", "https://example.org/path"),
            ("user+tag@example.org", "mailto:user+tag@example.org"),
            ("file:///tmp/document.pdf", "file:///tmp/document.pdf"),
            ("vscode://file/a.rs", "vscode://file/a.rs"),
            ("magnet:?xt=urn:btih:abc", "magnet:?xt=urn:btih:abc"),
            ("mailto:one@example.org", "mailto:one@example.org"),
            ("tel:+491234567", "tel:+491234567"),
        ] {
            assert_eq!(normalize(input).as_deref(), Some(expected), "{input}");
        }
        for input in [
            "text",
            "status:ready",
            "-example.org",
            "https://",
            "https://example.org/…",
            "https://example.org/...",
            "a\n.example.org",
            "--help",
            "https://example.org/`id`",
            "https:///missing-host",
            "999.99",
            "a..org",
        ] {
            assert_eq!(normalize(input), None, "{input}");
        }
    }

    fn word(text: &str, x: i32, width: i32) -> Word {
        Word {
            text: text.into(),
            left: x,
            right: x + width,
            top: 10,
            bottom: 30,
            line_start: false,
        }
    }

    #[test]
    fn joins_ocr_split_punctuation_without_eating_adjacent_prose() {
        let links = extract(
            vec![
                word("Visit", 0, 40),
                word("https:", 50, 60),
                word("//example", 115, 80),
                word(".org/path", 200, 80),
                word("now", 290, 30),
            ],
            "DP-1",
            1000,
            1000,
            480,
            500,
            1000,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.org/path");
        assert_eq!(links[0].x0, 0.05);
        assert_eq!(links[0].y0, 0.49);
        assert_eq!(links[0].w, 0.23);
    }

    #[test]
    fn prose_label_does_not_swallow_a_link() {
        let links = extract(
            vec![word("Link:", 0, 50), word("https://example.org", 60, 180)],
            "DP-1",
            1000,
            1000,
            0,
            0,
            1000,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.org");
    }

    #[test]
    fn overlap_is_owned_by_exactly_one_strip() {
        assert!(extract(
            vec![word("example.org", 10, 100)],
            "DP-1",
            1000,
            1000,
            480,
            0,
            500
        )
        .is_empty());
        assert_eq!(
            extract(
                vec![word("example.org", 10, 100)],
                "DP-1",
                1000,
                1000,
                480,
                500,
                1000
            )
            .len(),
            1
        );
    }
}
