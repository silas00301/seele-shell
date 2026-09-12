//! Markdown source formatting, independent of Qt objects and mutable text.
//!
//! Patterns deliberately match the previous QRegularExpression grammar. PCRE2
//! compiles them once, with bounded backtracking. Disjoint claims are indexed
//! by start position; accepting a match costs O(log n), not a scan of all prior
//! matches. Offsets crossing the ABI are always UTF-16 code units.

mod pattern;
use pattern::{Budget, Captures, Pattern};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::OnceLock,
};

pub const INHERIT: u32 = 1;
pub const BOLD: u32 = 2;
pub const ITALIC: u32 = 4;
pub const STRIKE: u32 = 8;
pub const UNDERLINE: u32 = 16;
pub const MONO: u32 = 32;
pub const BACKGROUND: u32 = 64;
pub const TEXT: u32 = 0;
pub const MUTED: u32 = 1;
pub const ACCENT: u32 = 2;
pub const CODE: u32 = 3;
pub const QUOTE: u32 = 4;
pub const DONE: u32 = 5;
pub const KEEP_COLOR: u32 = 6;

/// Inline work is bounded per text block. Oversized blocks remain editable
/// plain source, retaining structural formats and fenced/frontmatter state.
pub const MAX_INLINE_UNITS: usize = 128 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub length: u32,
    pub flags: u32,
    pub color: u32,
    pub heading: u32,
}

#[derive(Debug, Default)]
pub struct Highlight {
    pub spans: Vec<Span>,
    pub state: i32,
}

struct Source {
    text: String,
    offsets: Option<Vec<u32>>,
    valid: bool,
    units: u32,
    ascii: bool,
}

impl Source {
    fn new(units: &[u16]) -> Self {
        let (text, valid) = match String::from_utf16(units) {
            Ok(text) => (text, true),
            Err(_) => (String::from_utf16_lossy(units), false),
        };
        let ascii = text.is_ascii();
        let offsets = (!ascii && units.len() <= MAX_INLINE_UNITS).then(|| {
            let mut offsets = vec![0; text.len() + 1];
            let mut offset = 0;
            for (byte, character) in text.char_indices() {
                offsets[byte..byte + character.len_utf8()].fill(offset);
                offset += character.len_utf16() as u32;
            }
            offsets[text.len()] = offset;
            offsets
        });
        Self {
            text,
            offsets,
            valid,
            units: units.len() as u32,
            ascii,
        }
    }

    fn offset(&self, byte: usize) -> u32 {
        if byte == self.text.len() {
            return self.units;
        }
        if self.ascii {
            return byte as u32;
        }
        self.offsets.as_ref().map_or_else(
            || self.text[..byte].encode_utf16().count() as u32,
            |offsets| offsets[byte],
        )
    }
}

struct Parser {
    source: Source,
    output: Highlight,
    claims: BTreeMap<usize, usize>,
    budget: Budget,
}

impl Parser {
    fn emit(&mut self, start: usize, end: usize, color: u32, flags: u32, heading: u32) {
        if end <= start {
            return;
        }
        let start = self.source.offset(start);
        self.output.spans.push(Span {
            start,
            length: self.source.offset(end) - start,
            color,
            flags,
            heading,
        });
    }

    fn claim(&mut self, start: usize, end: usize) -> bool {
        if self
            .claims
            .range(..end)
            .next_back()
            .is_some_and(|(_, old_end)| *old_end > start)
        {
            return false;
        }
        self.claims.insert(start, end);
        true
    }

    fn code(&mut self, from: usize) {
        // A regex's greedy opening run can rescan an unmatched backtick suffix
        // quadratically. Index runs once. At each opening, the original grammar
        // chooses the longest suffix with a later equal-length closing run,
        // then the earliest such closing. Runs always have non-backtick content
        // between them; Qt supplies one text block (no LF) at a time.
        let bytes = self.source.text.as_bytes();
        let mut runs = Vec::new();
        let mut future: BTreeMap<usize, VecDeque<usize>> = BTreeMap::new();
        let mut cursor = from;
        while cursor < bytes.len() {
            if bytes[cursor] != b'`' {
                cursor += 1;
                continue;
            }
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] == b'`' {
                cursor += 1;
            }
            future
                .entry(cursor - start)
                .or_default()
                .push_back(runs.len());
            runs.push((start, cursor));
        }
        let mut index = 0;
        while index < runs.len() {
            let (start, end) = runs[index];
            let length = end - start;
            let queue = future.get_mut(&length).expect("indexed code run");
            queue.pop_front();
            if queue.is_empty() {
                future.remove(&length);
            }
            index += 1;
            let Some((&fence, queue)) = future.range(..=length).next_back() else {
                continue;
            };
            let closing = *queue.front().expect("nonempty future runs");
            let (close_start, close_end) = runs[closing];
            let open_start = end - fence;
            self.emit(open_start, close_end, CODE, MONO | BACKGROUND, 0);
            self.emit(open_start, end, MUTED, 0, 0);
            self.emit(close_start, close_end, MUTED, 0, 0);
            self.claim(open_start, close_end);
            while index <= closing {
                let (start, end) = runs[index];
                let queue = future
                    .get_mut(&(end - start))
                    .expect("indexed consumed run");
                queue.pop_front();
                if queue.is_empty() {
                    future.remove(&(end - start));
                }
                index += 1;
            }
        }
    }

    fn inline(&mut self, from: usize) {
        if !self.source.valid
            || self.source.text.contains('\n')
            || self.source.offset(self.source.text.len()) as usize > MAX_INLINE_UNITS
        {
            return;
        }
        let original = self.output.spans.len();
        self.code(from);
        for (kind, regex) in patterns()[..7].iter().enumerate() {
            let mut captures = regex.capture_locations();
            let mut cursor = from;
            while let Ok(Some(found)) =
                regex.captures_read_at(&mut captures, &self.source.text, cursor, &mut self.budget)
            {
                let (start, end) = (found.start(), found.end());
                // None of these fixed patterns can match an empty string.
                cursor = end;
                if !self.claim(start, end) {
                    continue;
                }
                match kind {
                    0 => {
                        let (left, right) = group(&captures, 1);
                        self.emit(start, end, ACCENT, 0, 0);
                        self.emit(start, start + right - left + 2, MUTED, 0, 0);
                        self.emit(end - 2, end, MUTED, 0, 0);
                    }
                    1 => {
                        let (label_start, label_end) = group(&captures, 2);
                        self.emit(label_start, label_end, ACCENT, 0, 0);
                        self.emit(start, label_start, MUTED, 0, 0);
                        self.emit(label_end, end, MUTED, 0, 0);
                    }
                    2 => self.emit(start, end, ACCENT, UNDERLINE, 0),
                    _ => {
                        let (body_start, body_end) = group(&captures, 2);
                        let flags = INHERIT
                            | match kind {
                                3 => BOLD | ITALIC,
                                4 => BOLD,
                                5 => STRIKE,
                                _ => ITALIC,
                            };
                        self.emit(
                            body_start,
                            body_end,
                            if kind == 5 { MUTED } else { KEEP_COLOR },
                            flags,
                            0,
                        );
                        self.emit(start, body_start, MUTED, 0, 0);
                        self.emit(body_end, end, MUTED, 0, 0);
                    }
                }
            }
        }
        if self.budget.exhausted {
            self.output.spans.truncate(original);
        }
    }

    fn block(&mut self, previous: i32, first: bool) {
        let length = self.source.text.len();
        let trimmed = self.source.text.trim();
        if previous <= 0 && first && trimmed == "---" {
            self.emit(0, length, MUTED, 0, 0);
            self.output.state = 2;
            return;
        }
        if previous == 2 {
            self.output.state = if matches!(trimmed, "---" | "...") {
                0
            } else {
                2
            };
            self.emit(0, length, MUTED, MONO, 0);
            return;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            self.output.state = i32::from(previous != 1);
            self.emit(0, length, MUTED, MONO | BACKGROUND, 0);
            return;
        }
        if previous == 1 {
            self.output.state = 1;
            self.emit(0, length, CODE, MONO | BACKGROUND, 0);
            return;
        }
        let first_char = trimmed.chars().next();
        if trimmed.len() >= 3
            && matches!(first_char, Some('-' | '*' | '_'))
            && trimmed
                .chars()
                .all(|c| Some(c) == first_char || c.is_whitespace())
        {
            self.emit(0, length, MUTED, 0, 0);
            return;
        }
        let cursor = length - self.source.text.trim_start().len();
        if cursor == length {
            return;
        }
        let after = &self.source.text[cursor..];
        let hashes = after.bytes().take_while(|c| *c == b'#').count();
        if (1..=6).contains(&hashes)
            && after[hashes..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            self.emit(cursor, length, TEXT, BOLD, hashes as u32);
            self.emit(cursor, cursor + hashes, MUTED, BOLD, hashes as u32);
            self.inline(cursor + hashes);
            return;
        }
        if after.starts_with('>') {
            self.emit(cursor, length, QUOTE, ITALIC, 0);
            self.emit(cursor, cursor + 1, MUTED, 0, 0);
            self.inline(cursor + 1);
            return;
        }
        // Structural scans are linear and retain Qt's ASCII regex classes.
        let mut after = cursor;
        if self.source.valid {
            let mut captures = patterns()[7].capture_locations();
            if let Ok(Some(found)) = patterns()[7].captures_read_at(
                &mut captures,
                &self.source.text,
                0,
                &mut self.budget,
            ) {
                after = found.end();
                let (start, end) = group(&captures, 2);
                self.emit(start, end, ACCENT, 0, 0);
                if let Ok(Some(found)) = patterns()[8].captures_read_at(
                    &mut captures,
                    &self.source.text,
                    after,
                    &mut self.budget,
                ) {
                    let (start, end) = (found.start(), found.end());
                    let (content, _) = group(&captures, 1);
                    let done = self.source.text.as_bytes()[content] != b' ';
                    self.emit(start, end, if done { DONE } else { MUTED }, BOLD, 0);
                    after = end;
                    if done {
                        self.emit(after, length, MUTED, STRIKE, 0);
                    }
                }
            }
        }
        self.inline(after);
    }
}

fn group(captures: &Captures, index: usize) -> (usize, usize) {
    captures.get(index).expect("fixed Markdown pattern capture")
}

fn patterns() -> &'static [Pattern; 9] {
    static PATTERNS: OnceLock<[Pattern; 9]> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(!?)\[\[([^\]]*)\]\]",
            r"(!?)\[([^\]]*)\]\(([^)]*)\)",
            r"<[a-zA-Z][a-zA-Z0-9+.-]*:[^>\s]+>|\bhttps?://[^\s<>\)\]]+",
            r"(\*\*\*|___)(?=\S)(.+?)(?<=\S)\1",
            r"(\*\*|__)(?=\S)(.+?)(?<=\S)\1",
            r"(~~)(?=\S)(.+?)(?<=\S)\1",
            r"(?<![\w*])(\*|_)(?=\S)(.+?)(?<=\S)\1(?![\w*])",
            r"^(\s*)([-*+]|\d+[.)])\s",
            r"\G\[([ xX])\]\s",
        ]
        .map(Pattern::new)
    })
}

pub fn highlight(text: &[u16], previous: i32, first: bool) -> Highlight {
    let mut parser = Parser {
        source: Source::new(text),
        output: Highlight::default(),
        claims: BTreeMap::new(),
        budget: Budget::new(text.len()),
    };
    parser.block(previous, first);
    parser.output
}

#[repr(C)]
pub struct HighlightResult {
    pub spans: *mut Span,
    pub length: usize,
    pub state: i32,
}

/// # Safety
/// `text` must point to `length` readable UTF-16 units for this call. The caller
/// must release the returned allocation exactly once with `seele_markdown_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seele_markdown_highlight(
    text: *const u16,
    length: usize,
    previous: i32,
    first: bool,
) -> HighlightResult {
    let empty = || HighlightResult {
        spans: std::ptr::null_mut(),
        length: 0,
        state: previous.max(0),
    };
    if text.is_null() || length > i32::MAX as usize {
        return empty();
    }
    // SAFETY: the Qt caller supplies its live QString storage for this call.
    let text = unsafe { std::slice::from_raw_parts(text, length) };
    std::panic::catch_unwind(|| {
        let result = highlight(text, previous, first);
        let spans = result.spans.into_boxed_slice();
        let length = spans.len();
        HighlightResult {
            spans: Box::into_raw(spans).cast(),
            length,
            state: result.state,
        }
    })
    .unwrap_or_else(|_| empty())
}

/// # Safety
/// `result` must be an unmodified, not yet freed result of the highlight call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seele_markdown_free(result: HighlightResult) {
    if !result.spans.is_null() {
        // SAFETY: reconstruct exactly the boxed slice created above.
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                result.spans,
                result.length,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str, previous: i32, first: bool) -> Highlight {
        highlight(&text.encode_utf16().collect::<Vec<_>>(), previous, first)
    }

    #[test]
    fn unicode_offsets_and_precedence() {
        let result = parse("😀 `**x**` [[🦀]] ***yes***", 0, false);
        assert_eq!(
            result.spans[0],
            Span {
                start: 3,
                length: 7,
                flags: MONO | BACKGROUND,
                color: CODE,
                heading: 0
            }
        );
        assert_eq!(result.spans[3].start, 11);
        assert_eq!(result.spans[3].length, 6);
        assert_eq!(result.spans[6].flags, INHERIT | BOLD | ITALIC);
    }

    #[test]
    fn structural_state_and_task_format_are_stable() {
        assert_eq!(parse(" --- ", -1, true).state, 2);
        assert_eq!(parse("...", 2, false).state, 0);
        assert_eq!(parse("```rust", 0, false).state, 1);
        assert_eq!(parse("~~~", 1, false).state, 0);
        let task = parse("- [x] done **bold**", 0, false);
        assert_eq!((task.spans[1].color, task.spans[2].flags), (DONE, STRIKE));
        assert_eq!(task.spans[3].flags, INHERIT | BOLD);
        assert_eq!(parse("# **bold**", 0, false).spans[0].heading, 1);
    }

    #[test]
    fn malformed_and_oversized_blocks_remain_source() {
        let result = highlight(&[0xd800, b'*' as u16, b'x' as u16, b'*' as u16], 0, false);
        assert!(result.spans.is_empty());
        assert!(
            parse(&"`x` ".repeat(MAX_INLINE_UNITS / 4 + 1), 0, false)
                .spans
                .is_empty()
        );
        assert_eq!(
            parse(&format!("```{}", "x".repeat(MAX_INLINE_UNITS)), 0, false).state,
            1
        );
    }
}
