//! Assemble OCR in output coordinates before assigning hints. Strip jobs may
//! finish in any order; a wrapped URI is published only after its whole output
//! is known. Failed strips remain barriers, never invisible gaps to join over.
use super::links::{self, Link, Word};

const MAX_WORDS: usize = 32_768;
const MAX_TEXT: usize = 2 * 1024 * 1024;

pub struct Page {
    output: String,
    width: usize,
    height: usize,
    pending: usize,
    strips: Vec<Option<Vec<Word>>>,
    words: usize,
    bytes: usize,
    overflow: bool,
}

impl Page {
    pub fn new(output: String, width: usize, height: usize) -> Self {
        let count = height.div_ceil(super::STRIP);
        Self {
            output,
            width,
            height,
            pending: count,
            strips: (0..count).map(|_| None).collect(),
            words: 0,
            bytes: 0,
            overflow: false,
        }
    }

    pub fn finish(
        &mut self,
        core_start: usize,
        core_end: usize,
        result: Result<Vec<Word>, String>,
    ) -> (Vec<Link>, bool) {
        self.pending -= 1;
        let mut failed = result.is_err();
        if let Ok(words) = result {
            if !self.overflow {
                let words = owned(words, core_start, core_end);
                self.words += words.len();
                self.bytes += words.iter().map(|w| w.text.len()).sum::<usize>();
                if self.words > MAX_WORDS || self.bytes > MAX_TEXT {
                    self.overflow = true;
                    self.strips.clear();
                    failed = true;
                } else {
                    self.strips[core_start / super::STRIP] = Some(words);
                }
            }
        }
        if self.pending != 0 || self.overflow {
            return (Vec::new(), failed);
        }
        let mut links = Vec::new();
        let mut run = Vec::new();
        // A failed strip ends a run even if geometry on its far side happens
        // to look adjacent (for example a tiny final strip).
        for strip in self.strips.drain(..).chain(std::iter::once(None)) {
            match strip {
                Some(words) => run.extend(words),
                None if !run.is_empty() => {
                    deduplicate(&mut run);
                    links.extend(links::extract(
                        std::mem::take(&mut run),
                        &self.output,
                        self.width,
                        self.height,
                        0,
                        0,
                        self.height,
                    ));
                }
                None => (),
            }
        }
        (links, failed)
    }
}

// Overlapping OCR strips can disagree by a pixel about which side owns a
// word's center. Collapse only the same spelling at the same geometry; repeated
// destinations elsewhere on screen remain separate selectable occurrences.
fn deduplicate(words: &mut Vec<Word>) {
    use std::collections::HashMap;
    words.sort_by(|a, b| a.text.cmp(&b.text));
    let mut previous = String::new();
    let mut cells: HashMap<[i32; 4], Vec<[i32; 4]>> = HashMap::new();
    words.retain(|word| {
        if word.text != previous {
            cells.clear();
            previous.clone_from(&word.text);
        }
        let rect = [word.left, word.top, word.right, word.bottom];
        let key = rect.map(|v| v.div_euclid(4));
        // Four-dimensional geometry cells keep the search bounded even when
        // many identical words share a row or overlapping OCR rectangles.
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dr in -1..=1 {
                    for db in -1..=1 {
                        if cells
                            .get(&[key[0] + dx, key[1] + dy, key[2] + dr, key[3] + db])
                            .is_some_and(|items| {
                                items.iter().any(|other| {
                                    rect.iter().zip(other).all(|(a, b)| (a - b).abs() <= 2)
                                })
                            })
                        {
                            return false;
                        }
                    }
                }
            }
        }
        cells.entry(key).or_default().push(rect);
        true
    });
}

fn owned(words: Vec<Word>, core_start: usize, core_end: usize) -> Vec<Word> {
    let offset = core_start.saturating_sub(super::OVERLAP) as i32;
    words
        .into_iter()
        .filter_map(|mut word| {
            word.top += offset;
            word.bottom += offset;
            // Ownership is per word rather than per assembled link: every line
            // crossing a strip boundary is retained once, in global coordinates.
            let center = (i64::from(word.top) + i64::from(word.bottom)) / 2;
            (center >= core_start as i64 && center < core_end as i64).then_some(word)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn word(text: &str, top: i32) -> Word {
        Word {
            text: text.into(),
            left: 20,
            right: 400,
            top,
            bottom: top + 20,
            line_start: true,
        }
    }
    #[test]
    fn seam_duplicates_merge_but_repeated_visible_links_remain() {
        let mut words = vec![
            word("example.org", 502),
            word("example.org", 503),
            word("example.org", 550),
        ];
        deduplicate(&mut words);
        assert_eq!(words.len(), 2);
        let mut shifted = word("example.org", 503);
        shifted.left += 1;
        shifted.right += 1;
        let mut words = vec![word("example.org", 502), word("example.org", 800), shifted];
        deduplicate(&mut words);
        assert_eq!(words.len(), 2);
    }
    #[test]
    fn seam_ownership_uses_global_word_centers() {
        assert!(owned(vec![word("example.org", 502)], 0, 512).is_empty());
        let words = owned(vec![word("example.org", 54)], 512, 1024);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].top, 502);
    }
    #[test]
    fn out_of_order_strips_publish_only_complete_output() {
        let mut page = Page::new("DP-1".into(), 1000, 1024);
        assert!(page
            .finish(512, 1024, Ok(vec![word("next?x=1", 74)]))
            .0
            .is_empty());
        let (links, failed) = page.finish(0, 512, Ok(vec![word("https://example.org/path/", 498)]));
        assert!(!failed);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.org/path/next?x=1");
    }
    #[test]
    fn failed_strip_does_not_discard_other_results() {
        let mut page = Page::new("DP-1".into(), 1000, 1024);
        assert!(page.finish(0, 512, Err("timeout".into())).1);
        let (links, failed) = page.finish(512, 1024, Ok(vec![word("https://example.org", 100)]));
        assert!(!failed);
        assert_eq!(links.len(), 1);
    }
    #[test]
    fn excessive_text_fails_once_and_releases_collected_words() {
        let mut page = Page::new("DP-1".into(), 1000, 1024);
        assert!(
            page.finish(0, 512, Ok(vec![word(&"x".repeat(MAX_TEXT + 1), 10)]))
                .1
        );
        assert!(page.strips.is_empty());
        let (links, failed) = page.finish(512, 1024, Ok(vec![word("example.org", 100)]));
        assert!(!failed);
        assert!(links.is_empty());
    }
}
