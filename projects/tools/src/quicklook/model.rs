//! What a highlighted file is, and what may be drawn of one.
//!
//! Quick Look is a private reader for the person who highlighted the file, so
//! nothing here redacts content: hiding a credential from the one account that
//! already owns the bytes would only make the preview lie about the file. What
//! is removed is the class of characters that can forge a line of interface —
//! C0/C1 controls apart from tab and newline, and the invisible and
//! direction-changing code points — because a name and a line of text are
//! drawn beside labels the shell wrote itself.
use std::path::Path;

/// One preview is bounded in every dimension a file can grow in. A refusal is
/// always visible as a state in the panel rather than as a longer wait.
pub const MAX_PATHS: usize = 64;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_TEXT_BYTES: usize = 128 * 1024;
pub const MAX_TEXT_LINES: usize = 4000;
pub const MAX_LINE_CHARS: usize = 2000;
pub const MAX_ENTRIES: usize = 256;
pub const MAX_PAGES: u32 = 4096;
/// Enough for every container header this classifies, and small enough that a
/// device that lied about being a regular file cannot deliver a preview.
pub const SNIFF_BYTES: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Directory,
    Image,
    Animation,
    Pdf,
    Markdown,
    Text,
    Audio,
    Video,
    Binary,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::Image => "image",
            Self::Animation => "animation",
            Self::Pdf => "pdf",
            Self::Markdown => "markdown",
            Self::Text => "text",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Binary => "binary",
        }
    }
    /// Whether the panel reads the file itself. Everything else is either
    /// rendered by a bounded helper or described rather than shown.
    pub fn textual(self) -> bool {
        matches!(self, Self::Markdown | Self::Text)
    }
}

const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "bmp", "webp", "svg", "svgz", "tif", "tiff", "ico", "pbm", "pgm",
    "ppm", "pnm", "xbm", "xpm", "avif", "jxl", "heic", "heif",
];
const ANIMATIONS: &[&str] = &["gif", "mng", "apng"];
const AUDIO: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "oga", "opus", "m4a", "m4b", "aac", "wma", "aiff", "aif", "mka",
    "ape", "wv",
];
const VIDEO: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "mov", "avi", "wmv", "flv", "mpg", "mpeg", "ogv", "3gp", "ts",
    "m2ts", "mts",
];
const MARKDOWN: &[&str] = &["md", "markdown", "mdown", "mkd", "mdx"];
/// Extensions worth trusting ahead of the content sniff, so an empty or
/// unusual source file is still offered as text rather than as bytes.
const TEXTS: &[&str] = &[
    "txt",
    "text",
    "log",
    "rst",
    "adoc",
    "asciidoc",
    "org",
    "tex",
    "csv",
    "tsv",
    "json",
    "jsonl",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "properties",
    "env",
    "nix",
    "rs",
    "go",
    "c",
    "h",
    "cc",
    "cpp",
    "cxx",
    "hpp",
    "hh",
    "java",
    "kt",
    "kts",
    "swift",
    "m",
    "mm",
    "py",
    "pyi",
    "rb",
    "pl",
    "pm",
    "php",
    "lua",
    "vim",
    "el",
    "lisp",
    "scm",
    "clj",
    "cljs",
    "hs",
    "ml",
    "mli",
    "ex",
    "exs",
    "erl",
    "zig",
    "dart",
    "scala",
    "sql",
    "sh",
    "bash",
    "zsh",
    "fish",
    "nu",
    "ps1",
    "bat",
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "tsx",
    "vue",
    "svelte",
    "css",
    "scss",
    "sass",
    "less",
    "html",
    "htm",
    "xml",
    "xhtml",
    "qml",
    "gradle",
    "cmake",
    "make",
    "mk",
    "d",
    "diff",
    "patch",
    "gitignore",
    "editorconfig",
    "service",
    "socket",
    "timer",
    "desktop",
    "lock",
];

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Container magic, checked before any extension. A `.txt` that is really a
/// PDF is previewed as the PDF it is, and a renamed archive never reaches the
/// text reader.
fn magic(head: &[u8]) -> Option<Kind> {
    let starts = |prefix: &[u8]| head.starts_with(prefix);
    if starts(b"PK\x03\x04")
        || starts(b"PK\x05\x06")
        || starts(b"PK\x07\x08")
        || starts(b"\x1f\x8b")
        || starts(b"BZh")
        || starts(b"\xfd7zXZ\x00")
        || starts(b"7z\xbc\xaf'\x1c")
        || starts(b"Rar!\x1a\x07")
        || starts(b"\x7fELF")
        || starts(b"MZ")
        || starts(b"!<arch>\n")
        || starts(b"\x28\xb5\x2f\xfd")
    {
        return Some(Kind::Binary);
    }
    if starts(b"%PDF-") {
        return Some(Kind::Pdf);
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some(Kind::Animation);
    }
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some(if head.windows(4).any(|window| window == b"acTL") {
            Kind::Animation
        } else {
            Kind::Image
        });
    }
    if starts(b"\xff\xd8\xff") || starts(b"BM") {
        return Some(Kind::Image);
    }
    if starts(b"\x00\x00\x01\x00") || starts(b"II*\x00") || starts(b"MM\x00*") {
        return Some(Kind::Image);
    }
    if starts(b"RIFF") && head.len() >= 12 {
        return match &head[8..12] {
            b"WEBP" => Some(if head.windows(4).any(|window| window == b"ANIM") {
                Kind::Animation
            } else {
                Kind::Image
            }),
            b"WAVE" => Some(Kind::Audio),
            b"AVI " => Some(Kind::Video),
            _ => None,
        };
    }
    if starts(b"fLaC") || starts(b"ID3") || starts(b"\xff\xfb") || starts(b"\xff\xf3") {
        return Some(Kind::Audio);
    }
    if starts(b"OggS") {
        // Ogg carries both. The codec identifier sits in the first page, so a
        // video codec decides; anything else in an Ogg stream is audio.
        let theora = head.windows(6).any(|window| window == b"theora");
        return Some(if theora { Kind::Video } else { Kind::Audio });
    }
    if starts(b"\x1a\x45\xdf\xa3") {
        return Some(Kind::Video);
    }
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        // Audio and video share the box; only the brand separates them.
        return Some(match &head[8..12] {
            b"M4A " | b"M4B " | b"M4P " => Kind::Audio,
            _ => Kind::Video,
        });
    }
    None
}

/// Whether a head of bytes reads as text. A NUL is decisive, and so is a
/// dense run of other control bytes: both mean the reader would draw noise.
fn textual(head: &[u8]) -> bool {
    if head.is_empty() {
        return true;
    }
    if head.contains(&0) {
        return false;
    }
    // A truncated head can split a multi-byte character, so judge the prefix
    // that decoded rather than rejecting the whole file for the cut.
    let text = match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(error) if error.valid_up_to() > 0 => {
            // `valid_up_to` is exactly the length that decoded.
            match std::str::from_utf8(&head[..error.valid_up_to()]) {
                Ok(text) => text,
                Err(_) => return false,
            }
        }
        Err(_) => return false,
    };
    let mut control = 0usize;
    let mut total = 0usize;
    for character in text.chars() {
        total += 1;
        if character.is_control() && !matches!(character, '\t' | '\n' | '\r') {
            control += 1;
        }
    }
    total > 0 && control * 10 < total
}

/// The kind a path is previewed as: directory, then container magic, then the
/// extension, then what the bytes themselves read as.
pub fn classify(name: &str, head: &[u8], directory: bool) -> Kind {
    if directory {
        return Kind::Directory;
    }
    if let Some(kind) = magic(head) {
        return kind;
    }
    let extension = extension(name);
    let listed = |list: &[&str]| list.contains(&extension.as_str());
    if listed(MARKDOWN) {
        return if textual(head) {
            Kind::Markdown
        } else {
            Kind::Binary
        };
    }
    if listed(IMAGES) {
        return Kind::Image;
    }
    if listed(ANIMATIONS) {
        return Kind::Animation;
    }
    if listed(AUDIO) {
        return Kind::Audio;
    }
    if listed(VIDEO) {
        return Kind::Video;
    }
    if extension == "pdf" {
        return Kind::Pdf;
    }
    if listed(TEXTS) {
        return if textual(head) {
            Kind::Text
        } else {
            Kind::Binary
        };
    }
    if textual(head) {
        Kind::Text
    } else {
        Kind::Binary
    }
}

/// Strip what could forge interface, then bound the result by lines and by
/// line length. Returns the text and whether anything was left out.
pub fn sanitize(input: &str) -> (String, bool) {
    let mut out = String::new();
    let mut truncated = false;
    for (lines, line) in input.split_inclusive('\n').enumerate() {
        if lines >= MAX_TEXT_LINES {
            truncated = true;
            break;
        }
        let ends = line.ends_with('\n');
        let mut characters = 0usize;
        for character in line.chars() {
            if character == '\n' {
                continue;
            }
            if character == '\t' {
                out.push('\t');
                characters += 1;
                continue;
            }
            if character.is_control() || !seele_runtime::redact::visible(character) {
                truncated = true;
                continue;
            }
            if characters >= MAX_LINE_CHARS {
                truncated = true;
                break;
            }
            out.push(character);
            characters += 1;
        }
        if ends {
            out.push('\n');
        }
    }
    (out, truncated)
}

/// A name drawn in the panel header. Same rules as body text, on one line.
pub fn sanitize_name(name: &str) -> String {
    let (text, _) = sanitize(&name.replace('\n', " "));
    let trimmed: String = text.chars().take(256).collect();
    if trimmed.trim().is_empty() {
        "Unnamed".into()
    } else {
        trimmed
    }
}

/// A path the shell may ask about: absolute, bounded and free of the
/// characters that would let a name reach past its own argument.
pub fn acceptable(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && path.starts_with('/')
        && !path.contains('\0')
        && !path.chars().any(|character| character.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_outranks_a_wrong_extension_and_brands_separate_audio_from_video() {
        assert_eq!(classify("notes.txt", b"%PDF-1.7\n", false), Kind::Pdf);
        assert_eq!(
            classify("clip.mp4", b"\x00\x00\x00\x18ftypM4A ", false),
            Kind::Audio
        );
        assert_eq!(
            classify("clip.m4a", b"\x00\x00\x00\x18ftypisom", false),
            Kind::Video
        );
        assert_eq!(classify("sheet.png", b"GIF89a", false), Kind::Animation);
        assert_eq!(
            classify("anything", b"RIFF\x00\x00\x00\x00WEBP", false),
            Kind::Image
        );
        assert_eq!(
            classify("anything", b"RIFF\x00\x00\x00\x00WEBPVP8XANIM", false),
            Kind::Animation
        );
        assert_eq!(
            classify("anything", b"\x89PNG\r\n\x1a\nacTL", false),
            Kind::Animation
        );
        assert_eq!(
            classify("anything", b"RIFF\x00\x00\x00\x00WAVE", false),
            Kind::Audio
        );
    }

    #[test]
    fn extensionless_and_empty_files_read_as_text_while_binary_does_not() {
        assert_eq!(
            classify("Makefile", b"all:\n\techo hi\n", false),
            Kind::Text
        );
        assert_eq!(classify("empty", b"", false), Kind::Text);
        assert_eq!(
            classify("blob", b"\x7fELF\x02\x01\x01\x00\x00", false),
            Kind::Binary
        );
        assert_eq!(
            classify("blob", &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06], false),
            Kind::Binary
        );
        // A head cut inside a multi-byte character is still text.
        let cut = "héllo wörld".as_bytes();
        assert_eq!(classify("cut", &cut[..cut.len() - 1], false), Kind::Text);
        assert_eq!(classify("anything", b"", true), Kind::Directory);
    }

    #[test]
    fn a_text_suffix_does_not_override_archive_or_executable_magic() {
        assert_eq!(
            classify("archive.txt", b"PK\x03\x04more", false),
            Kind::Binary
        );
        assert_eq!(classify("archive.md", b"\x1f\x8bmore", false), Kind::Binary);
        assert_eq!(
            classify("program.txt", b"\x7fELF\x02\x01", false),
            Kind::Binary
        );
    }

    #[test]
    fn sanitizing_removes_forgeable_characters_and_reports_every_bound() {
        let (text, truncated) = sanitize("safe\u{202e}evil\u{7}\nsecond\n");
        assert_eq!(text, "safeevil\nsecond\n");
        assert!(truncated);
        let (kept, cut) = sanitize("one\ttwo\nthree\n");
        assert_eq!(kept, "one\ttwo\nthree\n");
        assert!(!cut);
        let long = "x".repeat(MAX_LINE_CHARS + 10);
        let (bounded, cropped) = sanitize(&long);
        assert_eq!(bounded.chars().count(), MAX_LINE_CHARS);
        assert!(cropped);
        let many = "line\n".repeat(MAX_TEXT_LINES + 5);
        let (limited, dropped) = sanitize(&many);
        assert_eq!(limited.lines().count(), MAX_TEXT_LINES);
        assert!(dropped);
    }

    #[test]
    fn names_never_carry_control_or_direction_characters() {
        assert_eq!(sanitize_name("report\u{202e}fdp.exe"), "reportfdp.exe");
        assert_eq!(sanitize_name("  \u{200b} "), "Unnamed");
        assert_eq!(sanitize_name("two\nlines"), "two lines");
    }

    #[test]
    fn only_absolute_bounded_control_free_paths_are_accepted() {
        assert!(acceptable("/home/user/a file.txt"));
        assert!(!acceptable("relative/path"));
        assert!(!acceptable(""));
        assert!(!acceptable("/tmp/new\nline"));
        assert!(!acceptable(&format!("/{}", "x".repeat(MAX_PATH_BYTES))));
    }
}
