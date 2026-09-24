use serde::Serialize;

pub use super::validate::{destination, normalize};

const MAX_BYTES: usize = 8192;
const MAX_CONTINUATIONS: usize = 8;
const MAX_FRAGMENTS: usize = 64;

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
pub struct Region {
    pub x0: f64,
    pub y0: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Link {
    pub uri: String,
    pub text: String,
    pub code: bool,
    pub output: String,
    pub x0: f64,
    pub y0: f64,
    pub w: f64,
    pub h: f64,
    pub regions: Vec<Region>,
    pub number: usize,
}

#[derive(Debug)]
struct Row {
    words: Vec<Word>,
    top: i32,
    bottom: i32,
}

#[derive(Clone, Debug)]
struct Run {
    word: Word,
    // A preceding prose label may push the first URI to the right. Remember
    // that column's text edge, not the entire output's leftmost text edge.
    column_left: i32,
    first: bool,
    last: bool,
    too_long: bool,
    fragments: usize,
}

fn explicit(s: &str) -> bool {
    s.contains("://")
        || [
            "mailto:", "tel:", "sms:", "magnet:", "geo:", "news:", "urn:",
        ]
        .iter()
        .any(|prefix| {
            s.get(..prefix.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        })
}

fn split_scheme(a: &str, b: &str) -> bool {
    // Only known spellings establish a split scheme. Arbitrary alphabetic
    // prose followed by a custom URI must remain two independent tokens.
    [
        "http://", "https://", "ftp://", "ftps://", "file://", "mailto:",
    ]
    .iter()
    .any(|scheme| {
        a.len() < scheme.len()
            && scheme.starts_with(&a.to_ascii_lowercase())
            && b.get(..scheme.len() - a.len())
                .is_some_and(|rest| rest.eq_ignore_ascii_case(&scheme[a.len()..]))
    })
}

fn incomplete_escape(a: &str, b: &str) -> bool {
    let Some((_, tail)) = a.rsplit_once('%') else {
        return false;
    };
    tail.len() < 2
        && tail.bytes().all(|c| c.is_ascii_hexdigit())
        && b.as_bytes()
            .get(..2 - tail.len())
            .is_some_and(|bytes| bytes.iter().all(u8::is_ascii_hexdigit))
}

fn authority_fragment(a: &str, b: &str) -> bool {
    let Some((_, authority)) = a.split_once("://") else {
        return false;
    };
    // A split public hostname must not turn two independent complete links
    // into one. Single-label authorities remain useful on their own, but can
    // also be the beginning of a visibly wrapped public hostname.
    !authority.is_empty()
        && !authority.contains(['/', '?', '#', '@', ':', '[', ']'])
        && (!authority.trim_end_matches('.').contains('.') || authority.ends_with('-'))
        && b.split(['/', '?', '#']).next().is_some_and(|host| {
            !host.is_empty()
                && (host.contains('.') || authority.ends_with('.'))
                && host
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-'))
        })
}

fn closed(s: &str) -> bool {
    s.ends_with(['"', '\'', '`', '>', '”', '’', ',', ';', '!'])
        || [('(', ')'), ('[', ']'), ('{', '}')]
            .iter()
            .any(|(open, close)| {
                s.ends_with(*close) && s.matches(*close).count() > s.matches(*open).count()
            })
}

fn syntax_boundary(a: &str, b: &str) -> bool {
    a.ends_with([':', '/', '.', '@', '=', '?', '#', '&', '%', '-'])
        || b.starts_with([':', '/', '.', '@', '=', '?', '#', '&', '%'])
        || incomplete_escape(a, b)
        || split_scheme(a, b)
}

fn query_tail(a: &Word, b: &Word, gap: i32) -> bool {
    // Sparse OCR can split inside a query value ("view=ful", "L&page=2").
    // A second query parameter supplies evidence beyond an arbitrary nearby
    // word. Require less than a character's spacing on both sides as well;
    // ordinary prose and space-separated assignments must remain separate.
    let query = a
        .text
        .split_once('?')
        .is_some_and(|(_, query)| query.contains('='))
        || a.text.starts_with('&') && a.text.contains('=');
    if !query || explicit(&b.text) {
        return false;
    }
    let Some((value, parameter)) = b.text.split_once('&') else {
        return false;
    };
    let Some((key, _)) = parameter.split_once('=') else {
        return false;
    };
    if value.is_empty()
        || key.is_empty()
        || !value
            .chars()
            .all(|c| c.is_alphanumeric() || "-._~%".contains(c))
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || "-._~%".contains(c))
    {
        return false;
    }
    [a, b].iter().all(|word| {
        i64::from(gap.max(0)) * 4 * word.text.chars().count() as i64
            <= i64::from(word.right - word.left) * 3
    })
}

fn same_line(a: &Word, b: &Word) -> bool {
    let height = (a.bottom - a.top).max(b.bottom - b.top).max(1);
    let gap = b.left - a.right;
    let query = query_tail(a, b, gap);
    if gap < -height / 4
        || gap > height * 3 / 4
        || a.bottom.min(b.bottom) <= a.top.max(b.top)
        || closed(&a.text)
        || !(syntax_boundary(&a.text, &b.text) || query)
    {
        return false;
    }
    let partial_scheme = split_scheme(&a.text, &b.text);
    let expects_authority = a
        .text
        .strip_suffix("://")
        .is_some_and(super::validate::scheme)
        || ["mailto:", "tel:", "sms:", "news:"]
            .iter()
            .any(|prefix| a.text.eq_ignore_ascii_case(prefix));
    if normalize(&b.text).is_some()
        && !partial_scheme
        && !expects_authority
        && !authority_fragment(&a.text, &b.text)
    {
        return false;
    }
    if explicit(&b.text) && !partial_scheme {
        return false;
    }
    if a.text.ends_with('.')
        && b.text.starts_with(char::is_alphabetic)
        && ((!explicit(&a.text) && gap > height / 6)
            || (explicit(&a.text) && !authority_fragment(&a.text, &b.text)))
    {
        return false;
    }
    // Sparse OCR can mark an inline suffix as a new text line. Geometry
    // remains authoritative; use that flag only for unusually loose spacing.
    if b.line_start && gap > height / 2 && !partial_scheme {
        return false;
    }
    let combined = format!("{}{}", a.text, b.text);
    // Intermediate runs can end halfway through an escape or an authority.
    // Only the completed candidate is published after strict validation.
    query || partial_scheme || explicit(&combined) || normalize(&combined).is_some()
}

fn clipped_word(word: &Word, start: usize, end: usize) -> Word {
    // Tesseract exposes word boxes, not character boxes. Embedded links use
    // proportional Unicode-scalar offsets inside that box; never byte offsets.
    let count = word.text.chars().count().max(1) as i64;
    let width = i64::from((word.right - word.left).max(1));
    let left = i64::from(word.left) + width * word.text[..start].chars().count() as i64 / count;
    let right = i64::from(word.left) + width * word.text[..end].chars().count() as i64 / count;
    Word {
        text: word.text[start..end].into(),
        left: left as i32,
        right: right as i32,
        ..word.clone()
    }
}

fn peel_prefix(text: &str) -> usize {
    let mut start = 0;
    if let Some(position) = text.find("](") {
        if !explicit(&text[..position]) {
            start = position + 2;
        }
    }
    let rest = &text[start..];
    if let Some(position) = rest.find("://") {
        // `url=https://…`, `Link:https://…` and Markdown labels are not part
        // of the URI scheme. Do not strip anything within a started URI.
        let mut scheme_start = position;
        while scheme_start > 0 && rest.as_bytes()[scheme_start - 1].is_ascii_alphanumeric()
            || scheme_start > 0 && b"+.-".contains(&rest.as_bytes()[scheme_start - 1])
        {
            scheme_start -= 1;
        }
        if super::validate::scheme(&rest[scheme_start..position])
            && (scheme_start == 0 || rest[..scheme_start].ends_with(['=', ':', '(', '[', '\'']))
        {
            start += scheme_start;
        }
    } else if let Some((label, value)) = rest.split_once('=') {
        if !label.is_empty()
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && (normalize(value).is_some() || explicit(value))
        {
            start += label.len() + 1;
        }
    }
    start += text[start..].len()
        - text[start..]
            .trim_start_matches(['(', '[', '{', '\''])
            .len();
    start
}

fn tokens(word: Word) -> Vec<Word> {
    // One hostile OCR word cannot force repeated scans of an unbounded string.
    if word.text.len() > MAX_BYTES || word.right <= word.left || word.bottom <= word.top {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut delimiter_checks = 0;
    for (index, character) in word.text.char_indices() {
        let strong = character.is_whitespace() || "\"`<>“”".contains(character);
        if strong && !character.is_whitespace() && start < index {
            let before = &word.text[start..index];
            let after = &word.text[index + character.len_utf8()..];
            if (explicit(before) || normalize(before).is_some())
                && after
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_whitespace() && !".,;!)]}><\"`”".contains(c))
            {
                // An internal broken authority/path must not become a shorter
                // clickable URI just because its prefix happens to validate.
                return Vec::new();
            }
        }
        let separator = matches!(character, ',' | ';') && {
            delimiter_checks += 1;
            if delimiter_checks > MAX_FRAGMENTS {
                return Vec::new();
            }
            let next = &word.text[index + character.len_utf8()..];
            let next = &next[peel_prefix(next)..];
            // A comma inside a path/query is valid URI syntax. Split only if
            // the suffix visibly starts a fresh link; at most 64 pieces below.
            explicit(next)
                && next
                    .find("://")
                    .is_none_or(|p| super::validate::scheme(&next[..p]))
                || next.starts_with("www.")
        };
        if strong || separator {
            if start < index {
                ranges.push((start, index + character.len_utf8()));
            }
            start = index + character.len_utf8();
            if ranges.len() >= MAX_FRAGMENTS {
                return Vec::new();
            }
        }
    }
    if start < word.text.len() {
        ranges.push((start, word.text.len()));
    }
    ranges
        .into_iter()
        .filter_map(|(start, end)| {
            let start = start + peel_prefix(&word.text[start..end]);
            (start < end).then(|| clipped_word(&word, start, end))
        })
        .collect()
}

fn rows(mut words: Vec<Word>) -> Vec<Vec<Run>> {
    words.sort_by_key(|w| (w.top, w.left));
    let mut rows: Vec<Row> = Vec::new();
    for word in words {
        if let Some(row) = rows.last_mut() {
            let overlap = row.bottom.min(word.bottom) - row.top.max(word.top);
            let height = (word.bottom - word.top).min(row.bottom - row.top).max(1);
            if overlap * 2 >= height {
                row.top = row.top.min(word.top);
                row.bottom = row.bottom.max(word.bottom);
                row.words.push(word);
                continue;
            }
        }
        rows.push(Row {
            top: word.top,
            bottom: word.bottom,
            words: vec![word],
        });
    }
    rows.into_iter()
        .map(|mut row| {
            row.words.sort_by_key(|w| w.left);
            let mut runs: Vec<Run> = Vec::new();
            let mut column_left = row.words[0].left;
            let mut previous_right = column_left;
            for word in row.words.into_iter().flat_map(tokens) {
                let gap = word.left - previous_right;
                let new_column = gap > (word.bottom - word.top).max(1) * 3;
                if new_column {
                    column_left = word.left;
                }
                previous_right = word.right;
                if let Some(last) = runs.last_mut() {
                    if !new_column && same_line(&last.word, &word) {
                        if last.too_long
                            || last.fragments >= MAX_FRAGMENTS
                            || last.word.text.len() + word.text.len() > MAX_BYTES
                        {
                            last.too_long = true;
                            last.word.text = word.text;
                        } else {
                            last.word.text.push_str(&word.text);
                        }
                        last.fragments += 1;
                        last.word.right = word.right;
                        last.word.top = last.word.top.min(word.top);
                        last.word.bottom = last.word.bottom.max(word.bottom);
                        continue;
                    }
                    if !new_column {
                        last.last = false;
                    }
                }
                runs.push(Run {
                    word,
                    column_left,
                    first: runs.is_empty() || new_column,
                    last: true,
                    too_long: false,
                    fragments: 1,
                });
            }
            runs
        })
        .collect()
}

fn continuation(
    a: &str,
    previous: &Run,
    next: &Run,
    first: &Run,
    continuation_left: Option<i32>,
) -> bool {
    if !previous.last || !next.first || closed(a) {
        return false;
    }
    let ah = (previous.word.bottom - previous.word.top).max(1);
    let bh = (next.word.bottom - next.word.top).max(1);
    let gap = next.word.top - previous.word.bottom;
    if gap < 0 || gap > ah.max(bh) * 9 / 10 || ah.max(bh) * 2 > ah.min(bh) * 3 {
        return false;
    }
    let tolerance = ah.min(bh) / 2;
    let aligned = if let Some(left) = continuation_left {
        (next.word.left - left).abs() <= tolerance
    } else {
        (next.word.left - first.word.left).abs() <= tolerance
            || (next.word.left - first.column_left).abs() <= tolerance
    };
    if !aligned {
        return false;
    }
    let b = next.word.text.as_str();
    let scheme = split_scheme(a, b);
    let authority = authority_fragment(a, b);
    if a.ends_with('.') && !authority {
        return false;
    }
    if explicit(b) && !scheme || b.starts_with("www.") || b.contains('@') {
        return false;
    }
    if normalize(b).is_some() && !authority && !scheme {
        return false;
    }
    if scheme || authority || incomplete_escape(a, b) {
        return true;
    }
    // Aligned prose after a URL is common. A plain alphabetic fragment is
    // ambiguous even after '/': require actual URI syntax in the continuation.
    // Never dehyphenate, guess O/0, or invent missing separators.
    let structured =
        b.contains(['/', '?', '#', '&', '=', '%', '_']) || b.starts_with([':', '.', '-']);
    (explicit(a) || normalize(a).is_some())
        && structured
        && (syntax_boundary(a, b) || a.contains(['/', '?', '#']))
        && !b.starts_with(['(', '[', '{'])
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
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let rows = rows(words);
    let mut consumed: Vec<Vec<bool>> = rows.iter().map(|row| vec![false; row.len()]).collect();
    let mut links = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        for (run_index, first) in row.iter().enumerate() {
            if consumed[row_index][run_index] || first.too_long {
                continue;
            }
            let mut text = first.word.text.clone();
            let mut pieces = vec![(row_index, run_index)];
            let mut previous = first;
            let mut continuation_left = None;
            let mut over_limit = false;
            for (next_index, next_row) in rows
                .iter()
                .enumerate()
                .skip(row_index + 1)
                .take(MAX_CONTINUATIONS + 1)
            {
                if !previous.last || closed(&text) {
                    break;
                }
                // Rows are sorted, and each is visited at most eight times per
                // candidate. Binary search bounds the search to aligned starts,
                // rather than comparing every item in two neighboring rows.
                let tolerance = (previous.word.bottom - previous.word.top).max(1) / 2;
                let left = continuation_left.unwrap_or(first.column_left.min(first.word.left));
                let right = continuation_left.unwrap_or(first.column_left.max(first.word.left));
                let from = next_row.partition_point(|run| run.word.left < left - tolerance);
                let mut candidates = next_row
                    .iter()
                    .enumerate()
                    .skip(from)
                    .take_while(|(_, run)| run.word.left <= right + tolerance)
                    .filter(|(index, run)| {
                        !consumed[next_index][*index]
                            && continuation(&text, previous, run, first, continuation_left)
                    });
                let Some((index, next)) = candidates.next() else {
                    break;
                };
                if candidates.next().is_some() {
                    break;
                }
                pieces.push((next_index, index));
                if next.too_long
                    || pieces.len() > MAX_CONTINUATIONS + 1
                    || text.len() + next.word.text.len() > MAX_BYTES
                {
                    over_limit = true;
                    break;
                }
                text.push_str(&next.word.text);
                previous = next;
                continuation_left = Some(next.word.left);
            }
            let uri = if over_limit { None } else { normalize(&text) };
            let Some(uri) = uri else {
                for (row, index) in pieces {
                    consumed[row][index] = true;
                }
                continue;
            };
            let count = pieces.len();
            let top = first.word.top.max(0) as usize + strip_y;
            let bottom = first.word.bottom.max(0) as usize + strip_y;
            let center = (top + bottom) / 2;
            if center < core_start || center >= core_end {
                continue;
            }
            let mut regions = Vec::with_capacity(count);
            for (row, index) in pieces.into_iter().take(count) {
                consumed[row][index] = true;
                let word = &rows[row][index].word;
                regions.push(Region {
                    x0: f64::from(word.left.max(0)) / width as f64,
                    y0: (word.top.max(0) as usize + strip_y) as f64 / height as f64,
                    w: f64::from((word.right - word.left).max(1)) / width as f64,
                    h: f64::from((word.bottom - word.top).max(1)) / height as f64,
                });
            }
            let first = &regions[0];
            links.push(Link {
                text: uri.clone(),
                code: false,
                uri,
                output: output.into(),
                x0: first.x0,
                y0: first.y0,
                w: first.w,
                h: first.h,
                regions,
                number: 0,
            });
        }
    }
    links
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_destinations_preserve_payload_punctuation() {
        for uri in [
            "https://example.org/end.;!",
            "https://example.org/a_(b)",
            "https://example.org/...",
            "mailto:person@example.org",
            "custom://open/item",
        ] {
            assert_eq!(destination(uri).as_deref(), Some(uri));
        }
        assert_eq!(
            destination("example.org/end."),
            Some("https://example.org/end.".into())
        );
        for text in [
            "SKU-042",
            "hello\nworld",
            "<b>text</b>",
            "WIFI:T:WPA;S:example;P:secret;;",
            "--help",
            " https://example.org ",
        ] {
            assert_eq!(destination(text), None);
        }
    }

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
            ("EXAMPLE.COM", "https://EXAMPLE.COM"),
            ("example.co.uk/path", "https://example.co.uk/path"),
            ("example.world", "https://example.world"),
            ("example.中国", "https://example.中国"),
            ("\"https://example.org/path\";", "https://example.org/path"),
            (
                "(\"https://example.org/path\");",
                "https://example.org/path",
            ),
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
            "determinate.url",
            "flake.nix",
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
    fn ignores_code_attributes_and_sentence_boundaries() {
        for words in [
            vec![word("determinate.url", 0, 150)],
            vec![word("hello.", 0, 60), word("World", 67, 50)],
            vec![word("hello.", 0, 60), word("Com", 67, 30)],
        ] {
            assert!(extract(words, "DP-1", 1000, 1000, 0, 0, 1000).is_empty());
        }
    }

    #[test]
    fn extracts_quoted_assignment_value() {
        let links = extract(
            vec![
                word("determinate.url", 0, 150),
                word("=", 160, 10),
                word(
                    "\"https://flakehub.com/f/DeterminateSystems/determinate/3\";",
                    180,
                    600,
                ),
            ],
            "DP-1",
            1000,
            1000,
            0,
            0,
            1000,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].uri,
            "https://flakehub.com/f/DeterminateSystems/determinate/3"
        );
        assert!(links[0].x0 >= 0.18 && links[0].x0 < 0.20);
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
    fn joins_split_ports_and_rejects_invalid_complete_authorities() {
        let same_line = scan(vec![
            word("https://example.org", 100, 240),
            word(":443/path", 342, 100),
        ]);
        assert_eq!(same_line.len(), 1);
        assert_eq!(same_line[0].uri, "https://example.org:443/path");
        assert!(scan(vec![
            word("https://example.org", 100, 240),
            word(":70000/path", 342, 120),
        ])
        .is_empty());

        let wrapped = scan(vec![
            at("https://example.org", 100, 10, 240),
            at(":443/path", 100, 40, 100),
        ]);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].uri, "https://example.org:443/path");
        assert!(scan(vec![
            at("https://example.org", 100, 10, 240),
            at(":70000", 100, 40, 90),
        ])
        .is_empty());
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

    fn at(text: &str, x: i32, y: i32, width: i32) -> Word {
        Word {
            top: y,
            bottom: y + 20,
            line_start: true,
            ..word(text, x, width)
        }
    }

    fn scan(words: Vec<Word>) -> Vec<Link> {
        extract(words, "DP-1", 1000, 1000, 0, 0, 1000)
    }

    #[test]
    fn reconstructs_wrapped_paths_queries_fragments_and_real_hyphens() {
        for (first, next, expected) in [
            (
                "https://example.org/a/",
                "b/c?q=1#part",
                "https://example.org/a/b/c?q=1#part",
            ),
            (
                "https://example.org/long-",
                "segment/page",
                "https://example.org/long-segment/page",
            ),
            (
                "https://example.org/?q=value",
                "&other=two",
                "https://example.org/?q=value&other=two",
            ),
            (
                "https://example.org/page",
                "#some-section",
                "https://example.org/page#some-section",
            ),
            (
                "example.org/path/",
                "child?query=1",
                "https://example.org/path/child?query=1",
            ),
            (
                "https://example.org/verylong",
                "path/to/file",
                "https://example.org/verylongpath/to/file",
            ),
        ] {
            let links = scan(vec![at(first, 100, 10, 350), at(next, 100, 40, 180)]);
            assert_eq!(links.len(), 1, "{first} / {next}");
            assert_eq!(links[0].uri, expected);
            assert_eq!(links[0].regions.len(), 2);
            assert_eq!(links[0].x0, 0.1);
            assert_eq!(links[0].y0, 0.01);
            assert_eq!(links[0].w, 0.35);
            assert_eq!(links[0].h, 0.02);
            assert_eq!(links[0].regions[1].y0, 0.04);
        }
    }

    #[test]
    fn completes_split_schemes_hosts_and_percent_escapes() {
        for (first, next, expected) in [
            ("htt", "ps://example.org/path", "https://example.org/path"),
            ("https:", "//example.org/path", "https://example.org/path"),
            ("https://exam", "ple.org/path", "https://example.org/path"),
            ("https://example.", "org/path", "https://example.org/path"),
            (
                "https://example.org/a%",
                "20b?q=1",
                "https://example.org/a%20b?q=1",
            ),
            (
                "https://example.org/a%E",
                "2%82%AC",
                "https://example.org/a%E2%82%AC",
            ),
        ] {
            let links = scan(vec![at(first, 100, 10, 300), at(next, 100, 40, 200)]);
            assert_eq!(links.len(), 1, "{first} / {next}");
            assert_eq!(links[0].uri, expected);
        }
    }

    #[test]
    fn reconstructs_several_rows_once_and_orders_by_geometry() {
        let links = scan(vec![
            at("&other=two#section", 50, 70, 200),
            at("https://example.org/", 50, 10, 300),
            at("path/to/page?q=one", 50, 40, 250),
        ]);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].uri,
            "https://example.org/path/to/page?q=one&other=two#section"
        );
        assert_eq!(links[0].regions.len(), 3);
    }

    #[test]
    fn continuation_can_align_with_prose_start_or_the_link_start() {
        for left in [10, 75] {
            let links = scan(vec![
                word("Visit", 10, 50),
                word("https://example.org/a/", 75, 300),
                at("b/c", left, 40, 60),
            ]);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].uri, "https://example.org/a/b/c");
            assert_eq!(links[0].regions[1].x0, left as f64 / 1000.0);
        }
    }

    #[test]
    fn does_not_join_prose_columns_indentation_blank_rows_or_independent_links() {
        for next in [
            at("continued", 100, 40, 100),
            at("unrelated/path", 180, 40, 150),
            at("unrelated/path", 600, 40, 150),
            at("unrelated/path", 100, 70, 150),
            at("unrelated/path", 100, 20, 150),
            at("https://second.org/path", 100, 40, 300),
            at("second.org/path", 100, 40, 250),
        ] {
            let links = scan(vec![at("https://example.org/path/", 100, 10, 350), next]);
            assert_eq!(links[0].uri, "https://example.org/path/");
            assert_eq!(links[0].regions.len(), 1);
        }
        let links = scan(vec![
            at("https://example.org/path/", 100, 10, 350),
            at("Read", 100, 40, 40),
            at("the/docs", 150, 40, 90),
        ]);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.org/path/");
    }

    #[test]
    fn retains_independent_links_and_closing_delimiters() {
        for text in [
            "\"https://example.org/path/\"",
            "(https://example.org/path/)",
            "<https://example.org/path/>",
            "`https://example.org/path/`",
        ] {
            let links = scan(vec![
                at(text, 100, 10, 350),
                at("unrelated/path", 100, 40, 200),
            ]);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].uri, "https://example.org/path/");
        }
        let links = scan(vec![
            at("https://first.org/", 100, 10, 250),
            at("https://second.org/", 100, 40, 250),
        ]);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].uri, "https://first.org/");
        assert_eq!(links[1].uri, "https://second.org/");
    }

    #[test]
    fn extracts_embedded_assignments_markdown_and_neighboring_links() {
        for (text, expected) in [
            (
                "url=\"https://example.org/path\";",
                vec!["https://example.org/path"],
            ),
            (
                "url='https://example.org/path';",
                vec!["https://example.org/path"],
            ),
            (
                "[label](https://example.org/path)",
                vec!["https://example.org/path"],
            ),
            (
                "[label](example.org/path)",
                vec!["https://example.org/path"],
            ),
            (
                "https://first.org,https://second.org",
                vec!["https://first.org", "https://second.org"],
            ),
            (
                "[a](https://first.org),[b](https://second.org)",
                vec!["https://first.org", "https://second.org"],
            ),
        ] {
            let links = scan(vec![word(text, 100, 600)]);
            assert_eq!(
                links
                    .iter()
                    .map(|link| link.uri.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{text}"
            );
            assert!(links
                .iter()
                .all(|link| link.x0 >= 0.1 && link.x0 + link.w <= 0.701));
        }
    }

    #[test]
    fn region_offsets_count_unicode_characters_and_preserve_strip_ownership() {
        let links = extract(
            vec![
                word("説明=\"https://example.org\"", 100, 250),
                at("https://example.net/path/", 100, 40, 300),
                at("child?x=1", 100, 70, 120),
            ],
            "DP-1",
            1000,
            1000,
            480,
            500,
            1000,
        );
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].x0, 0.141); // Four Unicode characters, not eight UTF-8 bytes.
        assert_eq!(links[1].y0, 0.52);
        assert_eq!(links[1].regions[1].y0, 0.55);
    }

    #[test]
    fn bounds_long_words_and_continuation_depth_without_publishing_prefixes() {
        assert!(scan(vec![word(
            &format!("https://example.org/{}", "a".repeat(MAX_BYTES)),
            0,
            500
        )])
        .is_empty());
        let mut words = vec![at("https://example.org/path/", 100, 10, 350)];
        for i in 1..=MAX_CONTINUATIONS {
            words.push(at("child/", 100, 10 + i as i32 * 30, 100));
        }
        let links = scan(words.clone());
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].regions.len(), MAX_CONTINUATIONS + 1);
        words.push(at(
            "last/",
            100,
            10 + (MAX_CONTINUATIONS + 1) as i32 * 30,
            100,
        ));
        assert!(scan(words).is_empty());
        let words = vec![
            at(
                &format!("https://example.org/{}", "a".repeat(5000)),
                100,
                10,
                350,
            ),
            at(&format!("{}/", "b".repeat(5000)), 100, 40, 350),
        ];
        assert!(scan(words).is_empty());
        assert!(scan(vec![word(
            &format!("https://example.org/{}", ",".repeat(1000)),
            0,
            500
        )])
        .is_empty());
    }

    #[test]
    fn invalid_continuations_and_embedded_bytes_do_not_publish_valid_prefixes() {
        for text in [
            "https://user\"@example.org",
            "https://exa<mple.org",
            "https://example.org/`id`",
            "foo@https://example.org",
            "foo%https://example.org",
            "https://example.org/%ZZ",
        ] {
            assert!(scan(vec![word(text, 100, 300)]).is_empty(), "{text}");
        }
        for text in ["%ZZ", "%20bad%ZZ", "..."] {
            assert!(
                scan(vec![
                    at("https://example.org/a", 100, 10, 300),
                    at(text, 100, 40, 100)
                ])
                .is_empty(),
                "{text}"
            );
        }
        let links = scan(vec![
            at("https://example.org/path.", 100, 10, 300),
            at("other/path", 100, 40, 200),
        ]);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.org/path");
    }

    #[test]
    fn rejects_oversized_same_line_runs() {
        assert!(scan(vec![
            word(
                &format!("https://example.org/{}/", "a".repeat(5000)),
                100,
                300
            ),
            word(&format!("{}/", "b".repeat(5000)), 401, 300)
        ])
        .is_empty());
    }

    #[test]
    fn sparse_ocr_line_markers_do_not_hide_invalid_inline_suffixes() {
        for suffix in [".0rg:70000/path", ".org/bad%2X", ".org/truncated..."] {
            let links = scan(vec![
                Word {
                    text: "https:".into(),
                    left: 82,
                    right: 160,
                    top: 160,
                    bottom: 186,
                    line_start: true,
                },
                Word {
                    text: "//example".into(),
                    left: 166,
                    right: 290,
                    top: 160,
                    bottom: 186,
                    line_start: false,
                },
                Word {
                    text: suffix.into(),
                    left: 295,
                    right: 499,
                    top: 160,
                    bottom: 186,
                    line_start: true,
                },
            ]);
            assert!(links.is_empty(), "{suffix}");
        }
        let links = scan(vec![
            Word {
                text: "https:".into(),
                left: 82,
                right: 160,
                top: 160,
                bottom: 186,
                line_start: true,
            },
            Word {
                text: "//example".into(),
                left: 166,
                right: 290,
                top: 160,
                bottom: 186,
                line_start: false,
            },
            Word {
                text: ".".into(),
                left: 295,
                right: 300,
                top: 176,
                bottom: 180,
                line_start: true,
            },
            Word {
                text: ".com/path".into(),
                left: 309,
                right: 429,
                top: 160,
                bottom: 186,
                line_start: false,
            },
        ]);
        assert!(links.is_empty());
    }
    #[test]
    fn retains_query_suffix_split_inside_a_word() {
        let prefix = Word {
            text: "https://example.org/docs/".into(),
            left: 122,
            right: 469,
            top: 120,
            bottom: 146,
            line_start: true,
        };
        let value = Word {
            text: "chapter?view=ful".into(),
            left: 122,
            right: 342,
            top: 158,
            bottom: 184,
            line_start: true,
        };
        let tail = Word {
            text: "L&page=2".into(),
            left: 351,
            right: 456,
            top: 158,
            bottom: 184,
            line_start: false,
        };
        let links = extract(
            vec![prefix.clone(), value.clone(), tail.clone()],
            "DP-1",
            1000,
            400,
            0,
            0,
            400,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].uri,
            "https://example.org/docs/chapter?view=fulL&page=2"
        );
        assert_eq!(links[0].regions.len(), 2);
        assert!((links[0].regions[1].x0 + links[0].regions[1].w - 0.456).abs() < 1e-9);
        // Actual word spacing must not be silently deleted.
        let mut spaced = tail;
        spaced.left += 10;
        spaced.right += 10;
        let links = extract(vec![prefix, value, spaced], "DP-1", 1000, 400, 0, 0, 400);
        assert_eq!(links[0].uri, "https://example.org/docs/chapter?view=ful");
    }

    #[test]
    fn query_fragment_join_does_not_swallow_neighboring_text() {
        for suffix in [
            "hello_world",
            "and/or",
            "example.org",
            "user@example.org",
            "https://example.org",
            "option=true",
            "next?x=1",
        ] {
            for gap in [1, 4, 10, 16] {
                let first = Word {
                    text: "https://example.org/?view=full".into(),
                    left: 100,
                    right: 500,
                    top: 10,
                    bottom: 36,
                    line_start: true,
                };
                let next = Word {
                    text: suffix.into(),
                    left: 500 + gap,
                    right: 700 + gap,
                    top: 10,
                    bottom: 36,
                    line_start: false,
                };
                let links = scan(vec![first, next]);
                assert_eq!(
                    links[0].uri, "https://example.org/?view=full",
                    "{suffix} gap {gap}"
                );
            }
        }
    }
    #[test]
    fn independently_valid_destinations_remain_separate_after_a_slash() {
        for next in ["example.net", "user@example.net"] {
            let links = scan(vec![
                word("https://example.org/", 100, 300),
                word(next, 405, 160),
            ]);
            assert_eq!(links.len(), 2);
            assert_eq!(links[0].uri, "https://example.org/");
        }
        let links = scan(vec![
            word("https://", 100, 100),
            word("example.org", 201, 150),
        ]);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.org");
    }
}
