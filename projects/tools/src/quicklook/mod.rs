//! The resident worker behind Quick Look.
//!
//! One process answers what a highlighted path is and, for the formats that
//! need a renderer, produces a private image of it. It reads files and writes
//! only inside a per-invocation directory below `XDG_RUNTIME_DIR` that is
//! removed on supersession, cancellation, EOF and termination, so a preview
//! never becomes a cache entry or a thumbnail library.
//!
//! Everything that is merely a picture, a sound or a moving picture is left to
//! the panel: Qt already decodes those, and handing it the path avoids a
//! second copy of the file. What is here is the part QML cannot do safely —
//! deciding what a file is, bounding what is read of it, and driving Poppler.
mod model;

use model::Kind;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::fd::FromRawFd;
use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

type Result<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Poppler is given a page at a time and a hard ceiling on the raster it may
/// produce, so one enormous plate cannot become an enormous file.
const PAGE_EDGE: &str = "1800";
const RENDER_TIMEOUT: Duration = Duration::from_secs(10);
const INFO_TIMEOUT: Duration = Duration::from_secs(5);
const HELPER_OUTPUT: usize = 64 * 1024;
const FRAME: usize = 8 * 1024 * 1024;
/// Rendered pages held per invocation. Revisiting a page is instant, and a
/// long document cannot fill the runtime directory by being paged through.
const MAX_CACHED_PAGES: usize = 32;

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    index: usize,
    #[serde(default)]
    page: u32,
}

/// A private, per-process directory. Nothing below it outlives the worker.
struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Result<Self> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let template = CString::new(base.join("seele-quicklook-XXXXXX").as_os_str().as_bytes())?;
        let mut bytes = template.into_bytes_with_nul();
        // SAFETY: the buffer is a NUL-terminated template owned for the call.
        let path = unsafe { libc::mkdtemp(bytes.as_mut_ptr().cast()) };
        if path.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Self(PathBuf::from(
            unsafe { CStr::from_ptr(path) }.to_str()?,
        )))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What one open panel is looking at. A new request replaces it, taking its
/// rendered pages with it.
struct Session {
    id: u64,
    directory: PathBuf,
    items: Vec<Item>,
    pages: HashMap<(usize, u32), PathBuf>,
    order: Vec<(usize, u32)>,
}

impl Session {
    fn release(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
        self.pages.clear();
        self.order.clear();
    }
}

struct Item {
    path: PathBuf,
    kind: Option<Kind>,
}

fn emit(value: &Value) -> Result {
    let bytes = seele_runtime::wire::json_frame(value, FRAME)?;
    let mut out = seele_runtime::wire::nonblocking_stdout()?;
    seele_runtime::wire::write_bytes(
        &mut out,
        &bytes,
        Duration::from_secs(5),
        &AtomicBool::new(false),
    )?;
    Ok(())
}

fn unavailable(path: &Path, message: &str) -> Value {
    json!({
        "path": path.to_string_lossy(),
        "name": model::sanitize_name(&file_name(path)),
        "kind": "unavailable",
        "error": message,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Read the beginning of a regular file without blocking on a FIFO or device
/// and without refusing a file merely for being large.
fn head(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn modified(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn helper(program: &str, arguments: &[&str], timeout: Duration) -> Result<String> {
    let mut command = Command::new(program);
    command.args(arguments);
    let output = seele_runtime::process::capture(
        &mut command,
        b"",
        seele_runtime::process::Limits {
            timeout,
            output: HELPER_OUTPUT,
        },
        &AtomicBool::new(false),
    )?;
    if !output.status.success() {
        return Err(format!("{program} failed").into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Poppler's own page count. A document it will not open has none, which is
/// how an encrypted or damaged file becomes a stated limit instead of a wait.
fn pages(path: &Path) -> Result<u32> {
    let path = path.to_str().ok_or("unprintable path")?;
    let report = helper("pdfinfo", &[path], INFO_TIMEOUT)?;
    let count: u32 = report
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .ok_or("no page count")?
        .trim()
        .parse()?;
    if count == 0 || count > model::MAX_PAGES {
        return Err("unsupported page count".into());
    }
    Ok(count)
}

fn directory_entries(path: &Path) -> Result<(Vec<Value>, bool)> {
    let mut names: Vec<(bool, String)> = Vec::new();
    let mut limited = false;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if names.len() >= model::MAX_ENTRIES {
            limited = true;
            break;
        }
        let directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        names.push((
            directory,
            model::sanitize_name(&entry.file_name().to_string_lossy()),
        ));
    }
    names.sort_by_key(|(_, name)| name.to_lowercase());
    Ok((
        names
            .into_iter()
            .map(|(directory, name)| json!({ "name": name, "directory": directory }))
            .collect(),
        limited,
    ))
}

/// Everything one highlighted path contributes to the panel. A path that
/// cannot be described says so rather than disappearing from the set: the
/// numbering the user is walking through has to stay the one they saw.
fn inspect(raw: &str) -> (Value, Option<Kind>) {
    let path = Path::new(raw);
    let Ok(path) = path.canonicalize() else {
        return (unavailable(path, "This file is no longer there"), None);
    };
    let Ok(metadata) = fs::metadata(&path) else {
        return (unavailable(&path, "This file cannot be read"), None);
    };
    let name = model::sanitize_name(&file_name(&path));
    let mut item = Map::new();
    item.insert("path".into(), json!(path.to_string_lossy()));
    item.insert("name".into(), json!(name));
    item.insert("size".into(), json!(metadata.len()));
    item.insert("modified".into(), json!(modified(&metadata)));
    item.insert("error".into(), json!(""));
    if metadata.is_dir() {
        item.insert("kind".into(), json!(Kind::Directory.name()));
        match directory_entries(&path) {
            Ok((entries, limited)) => {
                let total = entries.len();
                item.insert("entries".into(), Value::Array(entries));
                item.insert("total".into(), json!(total));
                item.insert("limited".into(), json!(limited));
            }
            Err(_) => {
                item.insert("error".into(), json!("This folder cannot be read"));
            }
        }
        return (Value::Object(item), Some(Kind::Directory));
    }
    if !metadata.is_file() {
        return (
            unavailable(&path, "Only files and folders can be previewed"),
            None,
        );
    }
    let sniff = head(&path, model::SNIFF_BYTES).unwrap_or_default();
    let kind = model::classify(&file_name(&path), &sniff, false);
    item.insert("kind".into(), json!(kind.name()));
    if kind.textual() {
        match head(&path, model::MAX_TEXT_BYTES) {
            Ok(bytes) => {
                let complete = bytes.len() as u64 >= metadata.len();
                let (text, stripped) = model::sanitize(&String::from_utf8_lossy(&bytes));
                item.insert("text".into(), json!(text));
                item.insert("truncated".into(), json!(stripped || !complete));
            }
            Err(_) => {
                item.insert("error".into(), json!("This file cannot be read"));
            }
        }
    }
    if kind == Kind::Pdf {
        match pages(&path) {
            Ok(count) => {
                item.insert("pages".into(), json!(count));
            }
            Err(_) => {
                item.insert("error".into(), json!("This document cannot be opened"));
            }
        }
    }
    (Value::Object(item), Some(kind))
}

/// Render one page into the invocation's own directory. Poppler receives an
/// absolute canonical path, so a leading dash can never become an option.
fn render(session: &Session, index: usize, page: u32) -> Result<PathBuf> {
    let item = session.items.get(index).ok_or("no such preview")?;
    if item.kind != Some(Kind::Pdf) {
        return Err("not a document".into());
    }
    let source = item.path.to_str().ok_or("unprintable path")?;
    let stem = session.directory.join(format!("{index}-{page}"));
    let root = stem.to_str().ok_or("unprintable destination")?;
    let number = page.to_string();
    helper(
        "pdftoppm",
        &[
            "-png",
            "-singlefile",
            "-scale-to",
            PAGE_EDGE,
            "-f",
            &number,
            "-l",
            &number,
            source,
            root,
        ],
        RENDER_TIMEOUT,
    )?;
    let rendered = stem.with_extension("png");
    if !rendered.is_file() {
        return Err("no page produced".into());
    }
    Ok(rendered)
}

fn open(workspace: &Workspace, request: &Request) -> Session {
    let directory = workspace.0.join(request.id.to_string());
    let _ = fs::create_dir_all(&directory);
    let mut values = Vec::new();
    let mut items = Vec::new();
    for raw in request.paths.iter().take(model::MAX_PATHS) {
        if !model::acceptable(raw) {
            let path = PathBuf::from(raw);
            values.push(unavailable(&path, "This file cannot be previewed"));
            items.push(Item { path, kind: None });
            continue;
        }
        let (value, kind) = inspect(raw);
        items.push(Item {
            path: PathBuf::from(
                value
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or(raw)
                    .to_owned(),
            ),
            kind,
        });
        values.push(value);
    }
    let _ = emit(&json!({ "id": request.id, "event": "items", "items": values }));
    Session {
        id: request.id,
        directory,
        items,
        pages: HashMap::new(),
        order: Vec::new(),
    }
}

fn page(session: &mut Session, request: &Request) {
    let key = (request.index, request.page);
    if let Some(existing) = session.pages.get(&key) {
        let _ = emit(&json!({
            "id": session.id, "event": "page", "index": key.0, "page": key.1,
            "path": existing.to_string_lossy(), "error": "",
        }));
        return;
    }
    match render(session, key.0, key.1) {
        Ok(path) => {
            session.pages.insert(key, path.clone());
            session.order.push(key);
            while session.order.len() > MAX_CACHED_PAGES {
                let oldest = session.order.remove(0);
                if let Some(stale) = session.pages.remove(&oldest) {
                    let _ = fs::remove_file(stale);
                }
            }
            let _ = emit(&json!({
                "id": session.id, "event": "page", "index": key.0, "page": key.1,
                "path": path.to_string_lossy(), "error": "",
            }));
        }
        Err(_) => {
            let _ = emit(&json!({
                "id": session.id, "event": "page", "index": key.0, "page": key.1,
                "path": "", "error": "This page cannot be drawn",
            }));
        }
    }
}

/// SIGTERM on a shell reload follows the same path as stdin EOF, so the
/// workspace is removed by the same `Drop` rather than left behind in the
/// runtime directory. Signals are blocked and consumed on a signalfd; no
/// allocation or file operation runs inside an asynchronous handler.
fn requests() -> Result<mpsc::Receiver<Option<Request>>> {
    let (send, receive) = mpsc::sync_channel(8);
    let descriptor = unsafe {
        let mut mask = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&mut mask, signal);
        }
        if libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()) != 0 {
            return Err("cannot configure worker shutdown".into());
        }
        libc::signalfd(-1, &mask, libc::SFD_CLOEXEC)
    };
    if descriptor < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let shutdown = send.clone();
    thread::spawn(move || {
        let mut file = unsafe { std::fs::File::from_raw_fd(descriptor) };
        let mut event = [0; std::mem::size_of::<libc::signalfd_siginfo>()];
        let _ = file.read_exact(&mut event);
        let _ = shutdown.send(None);
    });
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        let mut frame = Vec::new();
        while let Ok(true) = seele_runtime::wire::read_frame(&mut input, &mut frame, 256 * 1024) {
            if let Ok(request) = serde_json::from_slice::<Request>(&frame) {
                if send.send(Some(request)).is_err() {
                    return;
                }
            }
        }
        let _ = send.send(None);
    });
    Ok(receive)
}

pub fn run() -> Result {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|value| value == "--help") {
        println!(
            "seele-quicklook: newline JSON open/page/cancel requests on stdin\n  Preview images are private runtime files removed with the request."
        );
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("unknown arguments".into());
    }
    // Every file a renderer publishes below the workspace is ours alone, even
    // though the workspace itself is already 0700.
    // SAFETY: umask has no preconditions and is called before any thread.
    unsafe { libc::umask(0o077) };
    let workspace = Workspace::new()?;
    let incoming = requests()?;
    let mut session: Option<Session> = None;
    while let Ok(Some(request)) = incoming.recv() {
        match request.command.as_str() {
            "open" => {
                if let Some(mut previous) = session.take() {
                    previous.release();
                }
                session = Some(open(&workspace, &request));
            }
            "page" => {
                if let Some(active) = session.as_mut() {
                    if active.id == request.id {
                        page(active, &request);
                    }
                }
            }
            "cancel"
                if session
                    .as_ref()
                    .is_some_and(|active| active.id == request.id) =>
            {
                if let Some(mut active) = session.take() {
                    active.release();
                }
            }
            _ => {}
        }
    }
    if let Some(mut active) = session.take() {
        active.release();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_missing_or_unreadable_path_keeps_its_place_in_the_set() {
        let directory = fixture();
        let absent = directory.path().join("gone.txt");
        let (value, kind) = inspect(absent.to_str().unwrap());
        assert_eq!(kind, None);
        assert_eq!(value["kind"], "unavailable");
        assert_eq!(value["name"], "gone.txt");
        assert!(value["error"].as_str().unwrap().contains("no longer there"));
    }

    #[test]
    fn text_is_bounded_sanitized_and_reports_that_it_was_cut() {
        let directory = fixture();
        let path = directory.path().join("notes.txt");
        fs::write(&path, "visible\u{202e}\nrest\n").unwrap();
        let (value, kind) = inspect(path.to_str().unwrap());
        assert_eq!(kind, Some(Kind::Text));
        assert_eq!(value["text"], "visible\nrest\n");
        assert_eq!(value["truncated"], true);

        let long = directory.path().join("long.txt");
        fs::write(&long, "a".repeat(model::MAX_TEXT_BYTES * 2)).unwrap();
        let (value, _) = inspect(long.to_str().unwrap());
        assert_eq!(value["truncated"], true);
        assert!(value["text"].as_str().unwrap().len() <= model::MAX_TEXT_BYTES);
    }

    #[test]
    fn a_folder_stops_after_proving_that_its_bounded_listing_is_incomplete() {
        let directory = fixture();
        for index in 0..(model::MAX_ENTRIES + 5) {
            fs::write(directory.path().join(format!("file-{index:04}")), b"x").unwrap();
        }
        let (value, kind) = inspect(directory.path().to_str().unwrap());
        assert_eq!(kind, Some(Kind::Directory));
        assert_eq!(
            value["entries"].as_array().unwrap().len(),
            model::MAX_ENTRIES
        );
        assert_eq!(value["total"], model::MAX_ENTRIES);
        assert_eq!(value["limited"], true);
    }

    #[test]
    fn a_device_or_pipe_is_refused_rather_than_read() {
        let (value, kind) = inspect("/dev/zero");
        assert_eq!(kind, None);
        assert_eq!(value["kind"], "unavailable");
    }

    #[test]
    fn a_workspace_is_private_and_disappears_with_the_worker() {
        let path;
        {
            let workspace = Workspace::new().unwrap();
            path = workspace.0.clone();
            let mode = std::os::unix::fs::PermissionsExt::mode(
                &fs::metadata(&path).unwrap().permissions(),
            );
            assert_eq!(mode & 0o777, 0o700);
        }
        assert!(!path.exists());
    }
}
