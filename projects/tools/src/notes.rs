//! Quick capture into an Obsidian vault. Notes are ordinary Markdown files in
//! one configured directory; that directory is the only source of truth. Text
//! never travels in argv, and nothing Seele needs for itself is written into
//! the vault.
use crate::command::{epoch, home};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SAMPLE_RATE: u32 = 16_000;
const MAX_AUDIO: u32 = SAMPLE_RATE * 2 * 60 * 60;
const MAX_TEXT: usize = 2 * 1024 * 1024;
const MAX_NAME: usize = 60;
// A vault holds far more than the capture directory, so an embed that has to
// be hunted for stops at a bounded sweep rather than walking the whole tree.
const MAX_SEARCH_DIRS: usize = 512;
static STOP: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// The user's own choice, written by the directory picker. Kept beside the
/// application's other private state rather than in the vault.
#[derive(Default, Deserialize, Serialize)]
struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attachments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sidebar: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    collapsed: Option<bool>,
}

/// The optional declarative defaults the parent flake may install. Read-only.
#[derive(Default, Deserialize)]
struct Declared {
    #[serde(default)]
    vault: Option<String>,
    #[serde(default)]
    directory: Option<String>,
    #[serde(default)]
    attachments: Option<String>,
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

fn state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
}

fn private_dir(path: &Path) -> Result {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(())
}

fn private_file(path: &Path, create_new: bool) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(!create_new)
        .create_new(create_new)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

fn lock(path: &Path) -> Result<File> {
    let file = private_file(path, false)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Another Seele Notes session is already running".into());
    }
    Ok(file)
}

/// Every note's identity is its vault-relative path, so a link written today
/// still resolves after the vault is copied to another machine.
struct Vault {
    root: PathBuf,
    directory: String,
    attachments: String,
    state: PathBuf,
}

impl Vault {
    fn notes_dir(&self) -> PathBuf {
        self.root.join(&self.directory)
    }

    fn attachments_dir(&self) -> PathBuf {
        self.notes_dir().join(&self.attachments)
    }

    fn trash_dir(&self) -> PathBuf {
        self.root.join(".trash")
    }

    /// Resolve a vault-relative path, refusing anything that climbs out of the
    /// vault or arrives as an absolute path.
    fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let candidate = Path::new(relative);
        if relative.is_empty() || candidate.is_absolute() {
            return Err("A note path must be relative to the vault".into());
        }
        for component in candidate.components() {
            match component {
                Component::Normal(part) => {
                    if part.as_encoded_bytes().contains(&b'\0') {
                        return Err("Invalid note path".into());
                    }
                }
                Component::CurDir => {}
                _ => return Err("A note path cannot leave the vault".into()),
            }
        }
        Ok(self.root.join(candidate))
    }

    /// The vault-relative form of an absolute path inside it.
    fn relative(&self, path: &Path) -> Option<String> {
        path.strip_prefix(&self.root)
            .ok()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
    }
}

// ---------------------------------------------------------------------------
// Markdown notes
// ---------------------------------------------------------------------------

/// A content digest rather than a timestamp: mtime granularity differs between
/// filesystems and sync tools rewrite it, but the bytes either changed or they
/// did not.
fn digest(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}{:x}", bytes.len())
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("md"))
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// Skip a leading YAML block so a note's frontmatter is never mistaken for its
/// first line of prose.
fn body_after_frontmatter(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---") else {
        return text;
    };
    let rest = rest.strip_prefix('\n').or_else(|| rest.strip_prefix("\r\n"));
    let Some(rest) = rest else { return text };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            return &rest[offset + line.len()..];
        }
        offset += line.len();
    }
    text
}

/// Strip the syntax a heading or a link carries so a list row shows the words
/// rather than the punctuation around them.
fn plain(line: &str) -> String {
    let mut out = String::new();
    let bytes: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        let character = bytes[index];
        match character {
            '!' if bytes.get(index + 1) == Some(&'[') => index += 1,
            '[' => {
                if bytes.get(index + 1) == Some(&'[') {
                    index += 2;
                    let mut inner = String::new();
                    while index < bytes.len() && bytes[index] != ']' {
                        inner.push(bytes[index]);
                        index += 1;
                    }
                    while index < bytes.len() && bytes[index] == ']' {
                        index += 1;
                    }
                    // A wikilink shows its alias when it has one.
                    let shown = inner.split('|').next_back().unwrap_or_default();
                    out.push_str(shown.trim());
                } else {
                    index += 1;
                }
            }
            ']' => {
                index += 1;
                // Drop the target of a Markdown link, keeping its text.
                if bytes.get(index) == Some(&'(') {
                    let mut depth = 0;
                    while index < bytes.len() {
                        if bytes[index] == '(' {
                            depth += 1;
                        } else if bytes[index] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                index += 1;
                                break;
                            }
                        }
                        index += 1;
                    }
                }
            }
            '*' | '_' | '`' | '~' => index += 1,
            _ => {
                out.push(character);
                index += 1;
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A row shows what a line says, not the block syntax that opens it: a task
/// reads as its words, and a bullet is a bullet in the editor rather than in
/// the title of a list row.
fn strip_marker(line: &str) -> &str {
    let mut rest = line.trim_start();
    let bullet = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "));
    if let Some(after) = bullet {
        rest = after.trim_start();
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && rest.len() > digits + 1 {
            let (marker, after) = rest.split_at(digits + 1);
            if marker.ends_with(['.', ')']) && after.starts_with(' ') {
                rest = after.trim_start();
            }
        }
    }
    for checkbox in ["[ ] ", "[x] ", "[X] "] {
        if let Some(after) = rest.strip_prefix(checkbox) {
            return after.trim_start();
        }
    }
    rest
}

fn heading_text(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|value| *value == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') && !rest.is_empty() {
        return None;
    }
    let text = plain(rest.trim_matches('#').trim());
    (!text.is_empty()).then_some(text)
}

/// The parts of a note a list row needs, taken from the text rather than from
/// a second index that could disagree with it.
struct Summary {
    title: String,
    excerpt: String,
    audio: usize,
}

fn summarize(name: &str, text: &str) -> Summary {
    let mut title = String::new();
    let mut excerpt = String::new();
    let mut fenced = false;
    for line in body_after_frontmatter(text).lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || trimmed.is_empty() {
            continue;
        }
        if title.is_empty() {
            if let Some(heading) = heading_text(trimmed) {
                title = heading;
                continue;
            }
        }
        let words = plain(strip_marker(trimmed));
        if words.is_empty() {
            continue;
        }
        if title.is_empty() {
            title = words.chars().take(120).collect();
            continue;
        }
        excerpt = words.chars().take(200).collect();
        break;
    }
    Summary {
        title: if title.is_empty() {
            name.to_owned()
        } else {
            title
        },
        excerpt,
        audio: embeds(text).len(),
    }
}

/// Every audio embed a note carries, in the order it carries them. Obsidian
/// writes `![[file.wav]]`; a Markdown embed is accepted too, because Obsidian
/// produces one when its link format is set that way.
fn embeds(text: &str) -> Vec<String> {
    let characters: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index + 2 < characters.len() {
        if characters[index] != '!' {
            index += 1;
            continue;
        }
        if characters[index + 1] == '[' && characters[index + 2] == '[' {
            let mut inner = String::new();
            let mut cursor = index + 3;
            while cursor + 1 < characters.len()
                && !(characters[cursor] == ']' && characters[cursor + 1] == ']')
            {
                inner.push(characters[cursor]);
                cursor += 1;
            }
            let target = inner.split(['|', '#']).next().unwrap_or_default().trim();
            if is_audio(target) {
                found.push(target.to_owned());
            }
            index = cursor + 2;
            continue;
        }
        if characters[index + 1] == '[' {
            let mut cursor = index + 2;
            while cursor < characters.len() && characters[cursor] != ']' {
                cursor += 1;
            }
            if characters.get(cursor + 1) == Some(&'(') {
                let mut inner = String::new();
                cursor += 2;
                while cursor < characters.len() && characters[cursor] != ')' {
                    inner.push(characters[cursor]);
                    cursor += 1;
                }
                let target = inner.split_whitespace().next().unwrap_or_default();
                let target = percent_decode(target.trim_matches(['<', '>']));
                if is_audio(&target) {
                    found.push(target);
                }
            }
            index = cursor + 1;
            continue;
        }
        index += 1;
    }
    found
}

fn is_audio(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    [".wav", ".ogg", ".mp3", ".m4a", ".flac", ".webm", ".opus"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A filename Obsidian, this app and the filesystem all accept.
fn sanitize(value: &str) -> String {
    let mut out = String::new();
    for character in value.chars() {
        match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']' => {
                out.push(' ')
            }
            character if (character as u32) < 0x20 => out.push(' '),
            character => out.push(character),
        }
        if out.chars().count() >= MAX_NAME * 2 {
            break;
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(['.', ' ']);
    trimmed.chars().take(MAX_NAME).collect::<String>()
}

fn stamp() -> String {
    // A local wall-clock name, so a capture is filed under the day it was made.
    let seconds = epoch();
    let time = unsafe {
        let mut out: libc::tm = std::mem::zeroed();
        let raw = seconds as libc::time_t;
        libc::localtime_r(&raw, &mut out);
        out
    };
    format!(
        "{:04}-{:02}-{:02} {:02}{:02}{:02}",
        time.tm_year + 1900,
        time.tm_mon + 1,
        time.tm_mday,
        time.tm_hour,
        time.tm_min,
        time.tm_sec
    )
}

/// The first meaningful line names the file, and only when the file is first
/// written. Later body edits leave the name alone.
fn name_for(text: &str) -> String {
    let mut candidate = String::new();
    for line in body_after_frontmatter(text).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("```") || trimmed.starts_with("---") {
            continue;
        }
        // A note whose first line is only a recording is an audio-only note.
        // Naming it after the audio file would read as a duplicate, so it
        // falls through to the timestamp instead.
        if trimmed.starts_with('!') && !embeds(trimmed).is_empty() {
            continue;
        }
        candidate =
            sanitize(&heading_text(trimmed).unwrap_or_else(|| plain(strip_marker(trimmed))));
        if !candidate.is_empty() {
            break;
        }
    }
    if candidate.is_empty() {
        stamp()
    } else {
        candidate
    }
}

fn unique(directory: &Path, name: &str, extension: &str) -> PathBuf {
    let mut attempt = directory.join(format!("{name}.{extension}"));
    let mut counter = 2;
    while attempt.exists() {
        attempt = directory.join(format!("{name} {counter}.{extension}"));
        counter += 1;
        if counter > 9999 {
            attempt = directory.join(format!("{name} {}.{extension}", stamp()));
            break;
        }
    }
    attempt
}

/// Replace a file's bytes without ever leaving a partial note on disk.
fn write_atomic(path: &Path, text: &str) -> Result {
    let directory = path.parent().ok_or("A note needs a directory")?;
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".seele-{}.tmp", stamp_nanos()));
    let result = (|| -> Result {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn stamp_nanos() -> String {
    format!(
        "{:032x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn modified(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|value| value.modified())
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The vault-backed library
// ---------------------------------------------------------------------------

struct Library {
    vault: Vault,
}

impl Library {
    fn drafts_dir(&self) -> PathBuf {
        self.vault.state.join("drafts")
    }

    fn draft_path(&self, relative: &str) -> PathBuf {
        self.drafts_dir()
            .join(format!("{}.json", digest(relative.as_bytes())))
    }

    /// A draft is kept only while the disk copy cannot accept it. It is
    /// recovery, not a second library: a successful save removes it.
    fn keep_draft(&self, relative: &str, text: &str, reason: &str) {
        let _ = private_dir(&self.drafts_dir());
        let record = json!({"path":relative,"text":text,"reason":reason,"saved":epoch()});
        let path = self.draft_path(relative);
        if let Ok(mut file) = private_file(&path, false) {
            let _ = file.set_len(0);
            let _ = file.write_all(record.to_string().as_bytes());
            let _ = file.sync_all();
        }
    }

    fn drop_draft(&self, relative: &str) {
        let _ = fs::remove_file(self.draft_path(relative));
    }

    fn drafts(&self) -> Vec<Value> {
        let Ok(entries) = fs::read_dir(self.drafts_dir()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                out.push(value);
            }
        }
        out.sort_by_key(|value| value["saved"].as_i64().unwrap_or(0));
        out
    }

    fn index(&self, name: &str) -> Map<String, Value> {
        fs::read_to_string(self.vault.state.join(name))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }

    fn write_index(&self, name: &str, value: &Map<String, Value>) -> Result {
        private_dir(&self.vault.state)?;
        let path = self.vault.state.join(name);
        let temporary = self
            .vault
            .state
            .join(format!(".{name}.{}.tmp", stamp_nanos()));
        let mut file = private_file(&temporary, true)?;
        file.write_all(Value::Object(value.clone()).to_string().as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    }

    fn ensure(&self) -> Result {
        if !self.vault.root.is_dir() {
            return Err("That vault directory does not exist".into());
        }
        fs::create_dir_all(self.vault.notes_dir())?;
        Ok(())
    }

    fn writable(&self) -> bool {
        let directory = self.vault.notes_dir();
        directory.is_dir()
            && unsafe {
                let Ok(raw) = std::ffi::CString::new(directory.as_os_str().as_encoded_bytes())
                else {
                    return false;
                };
                libc::access(raw.as_ptr(), libc::W_OK) == 0
            }
    }

    /// One note as a list row. The text on disk is the only input.
    fn entry(&self, path: &Path) -> Result<Value> {
        let text = fs::read_to_string(path)?;
        let relative = self
            .vault
            .relative(path)
            .ok_or("That note is outside the vault")?;
        let name = stem(path);
        let summary = summarize(&name, &text);
        Ok(json!({
            "path": relative,
            "name": name,
            "title": summary.title,
            "excerpt": summary.excerpt,
            "audio": summary.audio,
            "updated": modified(path),
            "hash": digest(text.as_bytes()),
        }))
    }

    /// The capture directory only. A vault-wide index is deliberately not
    /// built: this app is the quick way into one folder, not a second Obsidian.
    fn list(&self) -> Result<Value> {
        let directory = self.vault.notes_dir();
        let mut notes = Vec::new();
        let mut unreadable = 0;
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !is_markdown(&path) || !entry.file_type().is_ok_and(|value| value.is_file()) {
                    continue;
                }
                match self.entry(&path) {
                    Ok(note) => notes.push(note),
                    Err(_) => unreadable += 1,
                }
            }
        }
        notes.sort_by(|left, right| {
            right["updated"]
                .as_i64()
                .cmp(&left["updated"].as_i64())
                .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
        });
        Ok(json!({
            "notes": notes,
            "trash": self.trashed()?,
            "unreadable": unreadable,
            "writable": self.writable(),
        }))
    }

    fn read(&self, relative: &str) -> Result<Value> {
        let path = self.vault.resolve(relative)?;
        let text = fs::read_to_string(&path)?;
        Ok(json!({
            "note": self.entry(&path)?,
            "text": text,
            "hash": digest(text.as_bytes()),
            "audio": self.audio(relative, &text),
        }))
    }

    /// Where each embed actually is, so the player never has to guess and a
    /// missing file can be reported as its own state.
    fn audio(&self, relative: &str, text: &str) -> Vec<Value> {
        let mut out = Vec::new();
        for (position, target) in embeds(text).into_iter().enumerate() {
            let located = self.locate(relative, &target);
            out.push(json!({
                "index": position,
                "target": target,
                "name": Path::new(&target)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&target),
                "path": located.as_ref().map(|value| value.to_string_lossy().into_owned()),
                "duration": located.as_ref().map(wav_duration).unwrap_or(0),
            }));
        }
        out
    }

    /// Obsidian resolves a bare filename anywhere in the vault, so an embed
    /// keeps working when the capture directory is moved. Follow the same
    /// order it does, then stop: the attachment directory, beside the note,
    /// the vault-relative reading, and a bounded sweep.
    fn locate(&self, relative: &str, target: &str) -> Option<PathBuf> {
        let name = Path::new(target).file_name()?;
        let beside = Path::new(relative).parent().unwrap_or(Path::new(""));
        let candidates = [
            self.vault.attachments_dir().join(name),
            self.vault.root.join(beside).join(target),
            self.vault.notes_dir().join(target),
            self.vault.root.join(target),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        let mut queue = vec![self.vault.root.clone()];
        let mut visited = 0;
        while let Some(directory) = queue.pop() {
            visited += 1;
            if visited > MAX_SEARCH_DIRS {
                return None;
            }
            let entries = fs::read_dir(&directory).ok()?;
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    if !entry.file_name().to_string_lossy().starts_with('.') {
                        queue.push(path);
                    }
                } else if entry.file_name() == name {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Persist a note. `relative` is absent for a capture that has not earned
    /// a filename yet, and `baseline` is the digest the draft was loaded from.
    fn save(&self, relative: Option<&str>, text: &str, baseline: &str) -> Result<Value> {
        if text.len() > MAX_TEXT {
            return Err("This note exceeds the 2 MiB text limit".into());
        }
        if !self.vault.notes_dir().is_dir() {
            return Err("The configured notes directory is missing".into());
        }
        let Some(relative) = relative else {
            let path = unique(&self.vault.notes_dir(), &name_for(text), "md");
            write_atomic(&path, text)?;
            let note = self.entry(&path)?;
            return Ok(json!({"note":note,"hash":digest(text.as_bytes()),"created":true}));
        };
        let path = self.vault.resolve(relative)?;
        if !path.exists() {
            // A note the user moved or deleted elsewhere is not resurrected by
            // an autosave that was already in flight.
            self.keep_draft(relative, text, "gone");
            return Ok(json!({"gone":true,"path":relative}));
        }
        let current = fs::read_to_string(&path)?;
        let disk = digest(current.as_bytes());
        if disk != baseline {
            self.keep_draft(relative, text, "conflict");
            return Ok(json!({
                "conflict": {"path": relative, "text": current, "hash": disk},
            }));
        }
        if current == text {
            // Opening and closing a note must not rewrite it.
            self.drop_draft(relative);
            return Ok(json!({"note":self.entry(&path)?,"hash":disk,"unchanged":true}));
        }
        if let Err(error) = write_atomic(&path, text) {
            // The request was good and the note is real; the filesystem
            // refused it. That text has somewhere to be recovered from.
            self.keep_draft(relative, text, "failed");
            return Err(error);
        }
        self.drop_draft(relative);
        Ok(json!({"note":self.entry(&path)?,"hash":digest(text.as_bytes())}))
    }

    /// The three ways out of a conflict. Each of them keeps both versions.
    fn resolve_conflict(&self, relative: &str, mode: &str, text: &str) -> Result<Value> {
        let path = self.vault.resolve(relative)?;
        let directory = path.parent().ok_or("A note needs a directory")?.to_path_buf();
        let name = stem(&path);
        match mode {
            "copy" => {
                let copy = unique(&directory, &format!("{name} (Seele {})", stamp()), "md");
                write_atomic(&copy, text)?;
                self.drop_draft(relative);
                Ok(json!({"note":self.entry(&copy)?,"hash":digest(text.as_bytes()),"switched":true}))
            }
            "mine" => {
                let current = fs::read_to_string(&path).unwrap_or_default();
                if !current.is_empty() {
                    let kept = unique(&directory, &format!("{name} (external {})", stamp()), "md");
                    write_atomic(&kept, &current)?;
                }
                write_atomic(&path, text)?;
                self.drop_draft(relative);
                Ok(json!({"note":self.entry(&path)?,"hash":digest(text.as_bytes())}))
            }
            "theirs" => {
                let current = fs::read_to_string(&path)?;
                // The draft stays in recovery until the note is saved again,
                // so "use theirs" is still reversible after a misclick.
                self.keep_draft(relative, text, "replaced");
                Ok(json!({
                    "note": self.entry(&path)?,
                    "text": current,
                    "hash": digest(current.as_bytes()),
                    "audio": self.audio(relative, &current),
                    "reload": true,
                }))
            }
            _ => Err("Unknown conflict resolution".into()),
        }
    }

    /// A trashed note can be read but not written. Restoring it is the only
    /// way back to editing, so the preview is text and nothing else.
    fn preview(&self, id: &str) -> Result<Value> {
        if id.contains('/') || id.contains("..") {
            return Err("Invalid trash identifier".into());
        }
        let path = self.vault.trash_dir().join(id);
        let text = fs::read_to_string(&path)?;
        let name = stem(&path);
        let summary = summarize(&name, &text);
        Ok(json!({
            "preview": true,
            "id": id,
            "name": name,
            "title": summary.title,
            "text": text,
        }))
    }

    fn trashed(&self) -> Result<Vec<Value>> {
        let index = self.index("trash.json");
        let mut out = Vec::new();
        for (id, record) in index {
            let path = self.vault.trash_dir().join(&id);
            if !path.is_file() {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap_or_default();
            let name = stem(&path);
            let summary = summarize(&name, &text);
            out.push(json!({
                "id": id,
                "name": name,
                "title": summary.title,
                "excerpt": summary.excerpt,
                "audio": summary.audio,
                "origin": record["origin"].clone(),
                "updated": record["trashed"].as_i64().unwrap_or_else(|| modified(&path)),
            }));
        }
        out.sort_by_key(|value| std::cmp::Reverse(value["updated"].as_i64().unwrap_or(0)));
        Ok(out)
    }

    /// Trash moves the Markdown file into the vault's own local trash and
    /// never touches an attachment another note may still embed.
    fn trash(&self, relative: &str) -> Result<Value> {
        let path = self.vault.resolve(relative)?;
        if !path.is_file() {
            return Err("That note is no longer there".into());
        }
        let directory = self.vault.trash_dir();
        fs::create_dir_all(&directory)?;
        let destination = unique(&directory, &stem(&path), "md");
        fs::rename(&path, &destination)?;
        let id = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("Invalid trash name")?
            .to_owned();
        let mut index = self.index("trash.json");
        index.insert(id.clone(), json!({"origin":relative,"trashed":epoch()}));
        self.write_index("trash.json", &index)?;
        self.drop_draft(relative);
        Ok(json!({"trashed":id}))
    }

    fn restore(&self, id: &str) -> Result<Value> {
        if id.contains('/') || id.contains("..") {
            return Err("Invalid trash identifier".into());
        }
        let source = self.vault.trash_dir().join(id);
        if !source.is_file() {
            return Err("That note is no longer in the trash".into());
        }
        let mut index = self.index("trash.json");
        let origin = index
            .get(id)
            .and_then(|record| record["origin"].as_str())
            .map(str::to_owned);
        let target = match origin.as_deref() {
            Some(origin) => self.vault.resolve(origin)?,
            None => self.vault.notes_dir().join(id),
        };
        let directory = target.parent().ok_or("A note needs a directory")?;
        fs::create_dir_all(directory)?;
        // Restoring never overwrites whatever took the original name.
        let destination = if target.exists() {
            unique(directory, &stem(&target), "md")
        } else {
            target
        };
        fs::rename(&source, &destination)?;
        index.remove(id);
        self.write_index("trash.json", &index)?;
        Ok(json!({"restored": self.vault.relative(&destination)}))
    }
}

fn wav_duration(path: &PathBuf) -> u64 {
    fs::metadata(path)
        .map(|value| value.len().saturating_sub(44) * 1000 / (u64::from(SAMPLE_RATE) * 2))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// One-time migration from the private JSON library
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LegacyNote {
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    updated: i64,
    #[serde(default)]
    trashed: bool,
}

fn legacy_dir() -> PathBuf {
    data_home().join("seele-shell/notes")
}

/// What is left to bring in. A note already migrated is not still waiting,
/// even though its original is deliberately left where it is.
fn legacy_pending(state: &Path) -> usize {
    let done = fs::read_to_string(state.join("migrated.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    legacy_entries()
        .into_iter()
        .filter(|(id, directory)| {
            // An entry that cannot be read is never going to migrate, so it is
            // reported in the preview rather than keeping the offer on screen
            // for the rest of the machine's life.
            !done.contains_key(id)
                && fs::read_to_string(directory.join("note.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<LegacyNote>(&text).ok())
                    .is_some()
        })
        .count()
}

fn legacy_entries() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(legacy_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|value| value.is_dir()) {
            continue;
        }
        if path.join("note.json").is_file() {
            out.push((entry.file_name().to_string_lossy().into_owned(), path));
        }
    }
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

impl Library {
    /// Preview and run share one walk, so the counts the user approves are the
    /// counts the run acts on.
    fn migrate(&self, run: bool) -> Result<Value> {
        let done = self.index("migrated.json");
        let mut items = Vec::new();
        let (mut pending, mut memos, mut malformed, mut migrated) = (0, 0, 0, 0);
        let mut failures = Vec::new();
        for (id, directory) in legacy_entries() {
            let parsed = fs::read_to_string(directory.join("note.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<LegacyNote>(&text).ok());
            let Some(note) = parsed else {
                malformed += 1;
                items.push(json!({"id":id,"state":"malformed"}));
                continue;
            };
            let mut audio: Vec<PathBuf> = fs::read_dir(&directory)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("wav"))
                .collect();
            audio.sort();
            memos += audio.len();
            let name = if note.title.trim().is_empty() {
                name_for(&note.body)
            } else {
                sanitize(note.title.trim())
            };
            if let Some(record) = done.get(&id) {
                items.push(json!({
                    "id": id,
                    "name": name,
                    "target": record.clone(),
                    "trashed": note.trashed,
                    "memos": audio.len(),
                    "state": "done",
                }));
                continue;
            }
            if !run {
                pending += 1;
                items.push(json!({
                    "id": id,
                    "name": name,
                    "target": format!("{}/{name}.md", self.vault.directory),
                    "trashed": note.trashed,
                    "memos": audio.len(),
                    "state": "pending",
                }));
                continue;
            }
            match self.migrate_one(&id, &name, &note, &audio) {
                Ok(target) => {
                    migrated += 1;
                    items.push(json!({
                        "id": id,
                        "name": name,
                        "target": target,
                        "trashed": note.trashed,
                        "memos": audio.len(),
                        "state": "done",
                    }));
                }
                Err(error) => {
                    // Still pending: the walk that runs next has work to do.
                    pending += 1;
                    failures.push(json!({"id":id,"name":name,"error":error.to_string()}));
                    items.push(json!({"id":id,"name":name,"state":"failed"}));
                }
            }
        }
        Ok(json!({
            "migration": {
                "present": !legacy_entries().is_empty(),
                "source": legacy_dir().to_string_lossy(),
                "destination": self.vault.directory,
                "pending": pending,
                "memos": memos,
                "malformed": malformed,
                "migrated": migrated,
                "failures": failures,
                "items": items,
                "ran": run,
            }
        }))
    }

    /// The originals are left exactly as they are; a retry sees the marker and
    /// does nothing rather than writing a second copy.
    fn migrate_one(
        &self,
        id: &str,
        name: &str,
        note: &LegacyNote,
        audio: &[PathBuf],
    ) -> Result<String> {
        fs::create_dir_all(self.vault.notes_dir())?;
        let mut embeds = Vec::new();
        if !audio.is_empty() {
            fs::create_dir_all(self.vault.attachments_dir())?;
        }
        for source in audio {
            let target = unique(
                &self.vault.attachments_dir(),
                &format!("Voice memo {name} {}", &stem(source)[..8.min(stem(source).len())]),
                "wav",
            );
            fs::copy(source, &target)?;
            embeds.push(format!(
                "![[{}]]",
                target
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
            ));
        }
        let mut text = String::new();
        if !note.title.trim().is_empty() {
            text.push_str(&format!("# {}\n\n", note.title.trim()));
        }
        if !note.body.trim().is_empty() {
            text.push_str(note.body.trim_end());
            text.push_str("\n\n");
        }
        for embed in &embeds {
            text.push_str(embed);
            text.push('\n');
        }
        if text.is_empty() {
            text.push('\n');
        }
        let destination = if note.trashed {
            let directory = self.vault.trash_dir();
            fs::create_dir_all(&directory)?;
            let path = unique(&directory, name, "md");
            write_atomic(&path, &text)?;
            let trash_id = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("Invalid trash name")?
                .to_owned();
            let mut index = self.index("trash.json");
            index.insert(
                trash_id.clone(),
                json!({
                    "origin": format!("{}/{name}.md", self.vault.directory),
                    "trashed": if note.updated > 0 { note.updated } else { epoch() },
                }),
            );
            self.write_index("trash.json", &index)?;
            format!(".trash/{trash_id}")
        } else {
            let path = unique(&self.vault.notes_dir(), name, "md");
            write_atomic(&path, &text)?;
            self.vault.relative(&path).unwrap_or_default()
        };
        let mut done = self.index("migrated.json");
        done.insert(id.to_owned(), json!(destination));
        self.write_index("migrated.json", &done)?;
        Ok(destination)
    }
}

// ---------------------------------------------------------------------------
// Choosing the directory
// ---------------------------------------------------------------------------

fn settings_path() -> PathBuf {
    config_home().join("seele-notes/settings.json")
}

fn declared_path() -> PathBuf {
    config_home().join("seele-notes/config.json")
}

fn read_settings() -> Settings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_settings(settings: &Settings) -> Result {
    let directory = settings_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or("No configuration directory")?;
    private_dir(&directory)?;
    let temporary = directory.join(format!(".settings.{}.tmp", stamp_nanos()));
    let mut file = private_file(&temporary, true)?;
    file.write_all(serde_json::to_string_pretty(settings)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, settings_path())?;
    Ok(())
}

fn declared() -> Declared {
    fs::read_to_string(declared_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The user's own choice wins over the flake's default, and the flake's
/// default is only a starting point rather than something the app enforces.
fn vault_from(settings: &Settings) -> Option<Vault> {
    let declared = declared();
    let root = settings
        .vault
        .clone()
        .or(declared.vault)
        .filter(|value| !value.trim().is_empty())?;
    let directory = settings
        .directory
        .clone()
        .or(declared.directory)
        .unwrap_or_else(|| "Inbox".to_owned());
    let attachments = settings
        .attachments
        .clone()
        .or(declared.attachments)
        .unwrap_or_else(|| "Attachments".to_owned());
    Some(Vault {
        root: PathBuf::from(expand(&root)),
        directory: directory.trim_matches('/').to_owned(),
        attachments: attachments.trim_matches('/').to_owned(),
        state: state_home().join("seele-notes"),
    })
}

fn expand(value: &str) -> String {
    match value.strip_prefix("~/") {
        Some(rest) => home().join(rest).to_string_lossy().into_owned(),
        None => value.to_owned(),
    }
}

/// Directories the picker can walk, marking the ones that are Obsidian vaults.
fn browse(path: &str) -> Result<Value> {
    let root = if path.trim().is_empty() {
        home()
    } else {
        PathBuf::from(expand(path))
    };
    let root = root.canonicalize().unwrap_or(root);
    let mut entries = Vec::new();
    if let Ok(listing) = fs::read_dir(&root) {
        for entry in listing.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !entry.file_type().is_ok_and(|value| value.is_dir()) {
                continue;
            }
            entries.push(json!({
                "name": name,
                "path": entry.path().to_string_lossy(),
                "vault": entry.path().join(".obsidian").is_dir(),
            }));
        }
    }
    entries.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .cmp(&right["name"].as_str().unwrap_or_default().to_lowercase())
    });
    Ok(json!({
        "browse": {
            "path": root.to_string_lossy(),
            "parent": root.parent().map(|value| value.to_string_lossy().into_owned()),
            "vault": root.join(".obsidian").is_dir(),
            "readable": root.is_dir(),
            "entries": entries,
        }
    }))
}

/// Vaults already on the machine, so the common case is one click rather than
/// a walk. Bounded, and never followed outside the home directory.
fn discover_vaults() -> Vec<Value> {
    let mut found = Vec::new();
    let mut queue = vec![(home(), 0_usize)];
    let mut visited = 0;
    while let Some((directory, depth)) = queue.pop() {
        visited += 1;
        if visited > MAX_SEARCH_DIRS || found.len() >= 16 {
            break;
        }
        if directory.join(".obsidian").is_dir() {
            found.push(json!({
                "name": directory.file_name().map(|v| v.to_string_lossy().into_owned()),
                "path": directory.to_string_lossy(),
            }));
            continue;
        }
        if depth >= 4 {
            continue;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|value| value.is_dir()) {
                continue;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            queue.push((entry.path(), depth + 1));
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Watching the vault
// ---------------------------------------------------------------------------

/// Obsidian and the vault's sync tool write the same files this app does, so
/// the directory is watched rather than polled and a refresh is coalesced.
struct Watcher {
    fd: i32,
}

impl Watcher {
    fn new() -> Option<Self> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        (fd >= 0).then_some(Self { fd })
    }

    fn watch(&self, path: &Path) {
        let Ok(raw) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
            return;
        };
        let mask = libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_CLOSE_WRITE;
        unsafe { libc::inotify_add_watch(self.fd, raw.as_ptr(), mask) };
    }

    fn drain(&self) {
        let mut buffer = [0_u8; 4096];
        loop {
            let count = unsafe {
                libc::read(
                    self.fd,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if count <= 0 {
                return;
            }
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

// ---------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------

fn emit(value: &Value) -> Result {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")?;
    out.flush()?;
    Ok(())
}

struct Session {
    library: Option<Library>,
    settings: Settings,
}

impl Session {
    fn load() -> Self {
        let settings = read_settings();
        let library = vault_from(&settings).map(|vault| Library { vault });
        Self { library, settings }
    }

    fn require(&self) -> Result<&Library> {
        self.library
            .as_ref()
            .ok_or_else(|| "Choose a vault directory first".into())
    }

    /// Everything the window needs to draw its own state, including the states
    /// that are not about a note: unconfigured, unwritable, pending recovery.
    fn state(&self) -> Value {
        let legacy = legacy_pending(&state_home().join("seele-notes"));
        match &self.library {
            None => json!({
                "config": {
                    "configured": false,
                    "vault": "",
                    "directory": "",
                    "attachments": "",
                    "path": "",
                    "exists": false,
                    "writable": false,
                    "legacy": legacy,
                },
                "ui": {
                    "sidebar": self.settings.sidebar.unwrap_or(0),
                    "collapsed": self.settings.collapsed.unwrap_or(false),
                },
                "drafts": [],
            }),
            Some(library) => {
                let notes = library.vault.notes_dir();
                json!({
                    "config": {
                        "configured": true,
                        "vault": library.vault.root.to_string_lossy(),
                        "directory": library.vault.directory,
                        "attachments": library.vault.attachments,
                        "path": notes.to_string_lossy(),
                        "exists": notes.is_dir(),
                        "isVault": library.vault.root.join(".obsidian").is_dir(),
                        "writable": library.writable(),
                        "legacy": legacy,
                    },
                    "ui": {
                        "sidebar": self.settings.sidebar.unwrap_or(0),
                        "collapsed": self.settings.collapsed.unwrap_or(false),
                    },
                    "drafts": library.drafts(),
                })
            }
        }
    }

    fn request(&mut self, message: &Value) -> Result<Value> {
        let action = message["action"].as_str().unwrap_or("");
        match action {
            "config" => Ok(self.state()),
            "vaults" => Ok(json!({ "vaults": discover_vaults() })),
            "browse" => browse(message["path"].as_str().unwrap_or_default()),
            "configure" => {
                let vault = message["vault"].as_str().ok_or("A vault directory is required")?;
                let directory = message["directory"].as_str().unwrap_or("Inbox").trim();
                if directory.is_empty() {
                    return Err("A notes directory is required".into());
                }
                if Path::new(directory).is_absolute() || directory.contains("..") {
                    return Err("The notes directory must be inside the vault".into());
                }
                let mut settings = read_settings();
                settings.vault = Some(vault.to_owned());
                settings.directory = Some(directory.trim_matches('/').to_owned());
                if let Some(attachments) = message["attachments"].as_str() {
                    settings.attachments = Some(attachments.trim_matches('/').to_owned());
                }
                let candidate = vault_from(&settings).ok_or("A vault directory is required")?;
                let library = Library { vault: candidate };
                library.ensure()?;
                if !library.writable() {
                    return Err("That directory cannot be written to".into());
                }
                write_settings(&settings)?;
                self.settings = settings;
                self.library = Some(library);
                let mut value = self.state();
                merge(&mut value, self.require()?.list()?);
                Ok(value)
            }
            "ui" => {
                if let Some(width) = message["sidebar"].as_i64() {
                    self.settings.sidebar = Some(width);
                }
                if let Some(collapsed) = message["collapsed"].as_bool() {
                    self.settings.collapsed = Some(collapsed);
                }
                write_settings(&self.settings)?;
                Ok(json!({ "ui": self.state()["ui"].clone() }))
            }
            "list" => self.require()?.list(),
            "read" => {
                let library = self.require()?;
                match message["id"].as_str() {
                    Some(id) => library.preview(id),
                    None => library.read(message["path"].as_str().ok_or("A note path is required")?),
                }
            }
            // Where a note's recordings are, without handing back the text.
            // Reloading the document to learn about a new embed would take the
            // caret with it.
            "audio" => {
                let library = self.require()?;
                let path = message["path"].as_str().ok_or("A note path is required")?;
                let text = fs::read_to_string(library.vault.resolve(path)?)?;
                Ok(json!({ "audio": library.audio(path, &text) }))
            }
            "save" => {
                let library = self.require()?;
                let mut value = library.save(
                    message["path"].as_str().filter(|value| !value.is_empty()),
                    message["text"].as_str().ok_or("Note text is required")?,
                    message["baseline"].as_str().unwrap_or_default(),
                )?;
                merge(&mut value, library.list()?);
                Ok(value)
            }
            "resolve" => {
                let library = self.require()?;
                let mut value = library.resolve_conflict(
                    message["path"].as_str().ok_or("A note path is required")?,
                    message["mode"].as_str().unwrap_or_default(),
                    message["text"].as_str().unwrap_or_default(),
                )?;
                merge(&mut value, library.list()?);
                Ok(value)
            }
            "draft" => {
                let library = self.require()?;
                let path = message["path"].as_str().ok_or("A note path is required")?;
                library.keep_draft(path, message["text"].as_str().unwrap_or_default(), "pending");
                Ok(json!({ "drafts": library.drafts() }))
            }
            "discard" => {
                let library = self.require()?;
                library.drop_draft(message["path"].as_str().ok_or("A note path is required")?);
                Ok(json!({ "drafts": library.drafts() }))
            }
            "trash" | "restore" => {
                let library = self.require()?;
                let mut value = if action == "trash" {
                    library.trash(message["path"].as_str().ok_or("A note path is required")?)?
                } else {
                    library.restore(message["id"].as_str().ok_or("A trash item is required")?)?
                };
                merge(&mut value, library.list()?);
                Ok(value)
            }
            "migrate" => {
                let library = self.require()?;
                let run = message["mode"].as_str() == Some("run");
                let mut value = library.migrate(run)?;
                if run {
                    merge(&mut value, library.list()?);
                }
                Ok(value)
            }
            _ => Err("Unknown Notes action".into()),
        }
    }
}

fn merge(target: &mut Value, source: Value) {
    let (Some(target), Value::Object(source)) = (target.as_object_mut(), source) else {
        return;
    };
    for (key, value) in source {
        target.insert(key, value);
    }
}

/// Read whatever arrived on the descriptor without waiting for a full line,
/// so one poll can serve both the request stream and the vault watcher.
fn read_available(fd: i32, buffer: &mut Vec<u8>) -> io::Result<bool> {
    let mut chunk = [0_u8; 8192];
    let count = unsafe { libc::read(fd, chunk.as_mut_ptr() as *mut libc::c_void, chunk.len()) };
    if count < 0 {
        let error = io::Error::last_os_error();
        return match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => Ok(true),
            _ => Err(error),
        };
    }
    if count == 0 {
        return Ok(false);
    }
    buffer.extend_from_slice(&chunk[..count as usize]);
    Ok(true)
}

fn watch() -> Result {
    let state = state_home().join("seele-notes");
    private_dir(&state)?;
    let _writer = lock(&state.join("writer.lock"))?;
    let mut session = Session::load();
    let watcher = Watcher::new();
    let mut ready = session.state();
    if let Some(library) = session.library.as_ref() {
        merge(&mut ready, library.list().unwrap_or_else(|error| {
            json!({"notes":[],"trash":[],"error":error.to_string()})
        }));
    }
    ready["ready"] = json!(true);
    emit(&ready)?;

    let mut buffer: Vec<u8> = Vec::new();
    let mut refresh: Option<Instant> = None;
    loop {
        if let Some(watcher) = watcher.as_ref() {
            if let Some(library) = session.library.as_ref() {
                watcher.watch(&library.vault.notes_dir());
                watcher.watch(&library.vault.attachments_dir());
                watcher.watch(&library.vault.trash_dir());
            }
        }
        let mut fds = [
            libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: watcher.as_ref().map_or(-1, |value| value.fd),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let timeout = if refresh.is_some() { 60 } else { 1000 };
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error.into());
            }
        }
        if fds[1].revents & libc::POLLIN != 0 {
            if let Some(watcher) = watcher.as_ref() {
                watcher.drain();
            }
            refresh = Some(Instant::now());
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
            if !read_available(libc::STDIN_FILENO, &mut buffer)? {
                return Ok(());
            }
            while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
                let line = buffer.drain(..=position).collect::<Vec<_>>();
                let line = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
                if line.trim().is_empty() {
                    continue;
                }
                let message: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(_) => {
                        emit(&json!({"ok":false,"error":"Invalid request"}))?;
                        continue;
                    }
                };
                let mut response = match session.request(&message) {
                    Ok(mut value) => {
                        value["ok"] = json!(true);
                        value
                    }
                    Err(error) => json!({"ok":false,"error":error.to_string()}),
                };
                response["request"] = message["request"].clone();
                let answered = response.get("notes").is_some();
                emit(&response)?;
                // A reply that already carried the listing is its own answer,
                // so the watcher's echo of our own write is not replayed as an
                // external edit on top of it. A reply that did not carry one
                // leaves the pending refresh alone.
                if answered {
                    refresh = None;
                }
            }
        }
        if refresh.is_some_and(|since| since.elapsed() >= Duration::from_millis(200)) {
            refresh = None;
            if let Some(library) = session.library.as_ref() {
                let mut value = library.list()?;
                value["changed"] = json!(true);
                emit(&value)?;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Recording
// ---------------------------------------------------------------------------

fn wav_header(bytes: u32) -> [u8; 44] {
    let mut header = [0; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(bytes + 36).to_le_bytes());
    header[8..16].copy_from_slice(b"WAVEfmt ");
    header[16..20].copy_from_slice(&16_u32.to_le_bytes());
    header[20..22].copy_from_slice(&1_u16.to_le_bytes());
    header[22..24].copy_from_slice(&1_u16.to_le_bytes());
    header[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    header[28..32].copy_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    header[32..34].copy_from_slice(&2_u16.to_le_bytes());
    header[34..36].copy_from_slice(&16_u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&bytes.to_le_bytes());
    header
}

extern "C" fn stop_recording(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

/// Audio is a vault file in its own right, referenced by an embed rather than
/// owned by a note, so removing an embed never destroys a recording another
/// note may still be pointing at.
fn record() -> Result {
    let settings = read_settings();
    let vault = vault_from(&settings).ok_or("Choose a vault directory first")?;
    let state = vault.state.clone();
    private_dir(&state)?;
    let _recording = lock(&state.join("recording.lock"))
        .map_err(|_| "A recording is already running")?;
    let directory = vault.attachments_dir();
    fs::create_dir_all(&directory)?;
    let partial = directory.join(format!(".seele-recording-{}.part", stamp_nanos()));
    let mut file = private_file(&partial, true)?;
    file.write_all(&wav_header(0))?;
    // PulseAudio's native client follows PipeWire's default microphone. Only
    // this process owns the recorder, and every exit path reaps it.
    let mut child = match Command::new("parecord")
        .args([
            "--raw",
            "--format=s16le",
            "--rate=16000",
            "--channels=1",
            "--client-name=Seele Notes",
            "--stream-name=Voice memo",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&partial);
            return Err(format!("The microphone could not be opened: {error}").into());
        }
    };
    STOP.store(false, Ordering::Relaxed);
    unsafe {
        libc::signal(
            libc::SIGTERM,
            stop_recording as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            stop_recording as *const () as libc::sighandler_t,
        );
    }
    std::thread::spawn(|| {
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
        STOP.store(true, Ordering::Relaxed);
    });
    let result = (|| -> Result<u32> {
        let mut audio = child.stdout.take().ok_or("Recorder stdout unavailable")?;
        let fd = audio.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let (mut bytes, mut peak) = (0_u32, 0_f32);
        let mut last_emit = Instant::now();
        let mut last_audio = Instant::now();
        let mut stopping = None;
        let mut buffer = [0; 4096];
        emit(&json!({"recording":true}))?;
        loop {
            if (STOP.load(Ordering::Relaxed) || bytes >= MAX_AUDIO) && stopping.is_none() {
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGINT);
                }
                stopping = Some(Instant::now());
            }
            match audio.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let count = count.min((MAX_AUDIO - bytes) as usize);
                    file.write_all(&buffer[..count])?;
                    bytes += count as u32;
                    last_audio = Instant::now();
                    for sample in buffer[..count].chunks_exact(2) {
                        peak = peak.max(
                            (i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
                                .abs()
                                .sqrt(),
                        );
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error.into()),
            }
            if last_emit.elapsed() >= Duration::from_millis(50) {
                emit(
                    &json!({"level":peak,"duration":bytes as u64 * 1000 / (SAMPLE_RATE as u64 * 2)}),
                )?;
                last_emit = Instant::now();
                peak = 0.0;
            }
            if stopping.is_some_and(|time| time.elapsed() > Duration::from_secs(2)) {
                let _ = child.kill();
                break;
            }
            if last_audio.elapsed() > Duration::from_secs(10) {
                return Err("The microphone stopped delivering audio".into());
            }
        }
        if stopping.is_none() {
            return Err("The microphone disconnected while recording".into());
        }
        Ok(bytes - bytes % 2)
    })();
    let _ = child.kill();
    let _ = child.wait();
    match result {
        Ok(bytes) if bytes > 0 => {
            file.set_len(44 + u64::from(bytes))?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&wav_header(bytes))?;
            file.sync_all()?;
            drop(file);
            let target = unique(&directory, &format!("Voice memo {}", stamp()), "wav");
            // Audio joins the vault as ordinary content, readable by Obsidian.
            fs::set_permissions(&partial, fs::Permissions::from_mode(0o644))?;
            fs::rename(&partial, &target)?;
            File::open(&directory)?.sync_all()?;
            let name = target
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            emit(&json!({
                "saved": name,
                "path": target.to_string_lossy(),
                "duration": u64::from(bytes) * 1000 / (u64::from(SAMPLE_RATE) * 2),
            }))?;
            Ok(())
        }
        Ok(_) => {
            let _ = fs::remove_file(partial);
            Err("No audio was recorded".into())
        }
        Err(error) => {
            // Whatever was captured before the failure is finalized into the
            // attachment directory rather than thrown away, so a microphone
            // that disconnects mid-memo still leaves a playable file behind.
            let captured = file
                .metadata()
                .map(|value| value.len().saturating_sub(44) as u32)
                .unwrap_or(0);
            let captured = captured - captured % 2;
            if captured > 0
                && file.set_len(44 + u64::from(captured)).is_ok()
                && file.seek(SeekFrom::Start(0)).is_ok()
                && file.write_all(&wav_header(captured)).is_ok()
                && file.sync_all().is_ok()
            {
                drop(file);
                let salvage =
                    unique(&directory, &format!("Voice memo {} (partial)", stamp()), "wav");
                let _ = fs::set_permissions(&partial, fs::Permissions::from_mode(0o644));
                let _ = fs::rename(&partial, &salvage);
                let _ = emit(&json!({
                    "salvaged": salvage.file_name().and_then(|value| value.to_str()),
                    "duration": u64::from(captured) * 1000 / (u64::from(SAMPLE_RATE) * 2),
                }));
            } else {
                let _ = fs::remove_file(&partial);
            }
            Err(error)
        }
    }
}

pub fn run(arguments: &[String]) -> Result {
    match arguments.first().map(String::as_str).unwrap_or("watch") {
        "watch" => watch(),
        "record" => record(),
        _ => Err("Usage: seele-notes-store [watch|record]".into()),
    }
}
