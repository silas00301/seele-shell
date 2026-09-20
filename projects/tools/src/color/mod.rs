//! Freeze every output and answer the colour of one pixel at a time.
//!
//! This is deliberately the thin half of the frozen URI picker. That worker has
//! to hold each capture's pixels because OCR reads all of them; a colour picker
//! never needs more than three bytes once the frame is on disk, so the capture
//! is streamed straight into its private file and every sample afterwards is a
//! seek and a three-byte read. No image ever lives in this process's memory,
//! which is also why there is no allocation budget here to get wrong.
use serde::Deserialize;
use serde_json::{json, Value};
use std::error::Error;
use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::FromRawFd;
use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Result<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Grim writes a minimal `P6 <width> <height> 255` header with no comments.
/// Anything longer than this is not a frame we produced, and failing closed is
/// better than scanning an arbitrary prefix for a header that is not there.
const HEADER: usize = 128;
const MAX_DIMENSION: u64 = 32768;
const MAX_FRAME_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_FRAME_BYTES: u64 = HEADER as u64 + MAX_FRAME_PIXELS * 3;
const MAX_SESSION_BYTES: u64 = 512 * 1024 * 1024;
const SAMPLE_QUEUE: usize = 8;

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    output: String,
    /// Echoed back untouched so the shell can drop a reply for a point the
    /// pointer has already left instead of painting a stale colour.
    #[serde(default)]
    token: u64,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    commit: bool,
}

struct Frame {
    output: String,
    path: PathBuf,
    file: File,
    offset: u64,
    width: u32,
    height: u32,
}

/// A private, per-invocation directory. Captures are runtime files: they never
/// enter the screenshot library or a cache, and cancellation, EOF, errors and
/// SIGTERM all remove the whole directory with them.
struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Result<Self> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let template = CString::new(base.join("seele-color-XXXXXX").as_os_str().as_bytes())?;
        let mut bytes = template.into_bytes_with_nul();
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

fn emit(value: Value) -> Result {
    let bytes = seele_runtime::wire::json_frame(&value, 64 * 1024)?;
    let stdout = io::stdout();
    let _lock = stdout.lock();
    let mut out = seele_runtime::wire::nonblocking_stdout()?;
    seele_runtime::wire::write_bytes(
        &mut out,
        &bytes,
        Duration::from_secs(5),
        &AtomicBool::new(false),
    )?;
    Ok(())
}

fn next_token<'a>(bytes: &'a [u8], at: &mut usize) -> Result<&'a [u8]> {
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
    let start = *at;
    while bytes
        .get(*at)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        *at += 1;
    }
    if start == *at {
        return Err("incomplete PPM header".into());
    }
    Ok(&bytes[start..*at])
}

/// Returns the first pixel's byte offset and the frame's dimensions.
fn header(bytes: &[u8]) -> Result<(u64, u32, u32)> {
    let mut at = 0;
    if next_token(bytes, &mut at)? != b"P6" {
        return Err("capture is not a binary RGB PPM".into());
    }
    let mut fields = [0u64; 3];
    for field in &mut fields {
        *field = std::str::from_utf8(next_token(bytes, &mut at)?)?.parse::<u64>()?;
    }
    if fields[2] != 255 || !bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        return Err("unsupported PPM sample depth".into());
    }
    // Exactly one delimiter closes the header: the first pixel's own red byte
    // can itself be a whitespace value.
    at += 1;
    let [width, height, _] = fields;
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || width.saturating_mul(height) > MAX_FRAME_PIXELS
    {
        return Err("invalid capture dimensions".into());
    }
    Ok((at as u64, width as u32, height as u32))
}

fn capture(
    output: &str,
    path: &Path,
    cancel: &AtomicBool,
    session_bytes: &AtomicU64,
) -> Result<Frame> {
    if output.is_empty()
        || output.len() > 256
        || output.starts_with('-')
        || output.chars().any(char::is_control)
    {
        return Err("invalid output name".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let mut head = Vec::new();
    let mut written = 0u64;
    let mut expected = None;
    let result = seele_runtime::process::stream_stdout(
        Command::new("grim").args(["-t", "ppm", "-o", output, "-"]),
        b"",
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(3),
            output: 64 * 1024,
        },
        cancel,
        |chunk| {
            if head.len() < HEADER {
                let wanted = (HEADER - head.len()).min(chunk.len());
                head.extend_from_slice(&chunk[..wanted]);
                match header(&head) {
                    Ok((offset, width, height)) => {
                        let bytes = offset + u64::from(width) * u64::from(height) * 3;
                        if bytes > MAX_FRAME_BYTES {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "screen capture exceeds the frame budget",
                            ));
                        }
                        expected = Some(bytes);
                    }
                    Err(_) if head.len() == HEADER => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "screen capture has an invalid header",
                        ));
                    }
                    Err(_) => {}
                }
            }
            let next = written.saturating_add(chunk.len() as u64);
            if next > expected.unwrap_or(MAX_FRAME_BYTES) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "screen capture exceeds its declared size",
                ));
            }
            if session_bytes
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                    used.checked_add(chunk.len() as u64)
                        .filter(|total| *total <= MAX_SESSION_BYTES)
                })
                .is_err()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "screen captures exceed the session budget",
                ));
            }
            file.write_all(chunk)?;
            written = next;
            Ok(())
        },
    )?;
    if !result.status.success() {
        return Err("screen capture failed".into());
    }
    let (offset, width, height) = header(&head)?;
    // A frame that is not exactly as long as its header claims cannot be
    // indexed safely, and a short read at the end would silently sample the
    // wrong row rather than fail.
    if offset + u64::from(width) * u64::from(height) * 3 != written {
        return Err("capture pixel data is incomplete".into());
    }
    Ok(Frame {
        output: output.to_owned(),
        path: path.to_owned(),
        file,
        offset,
        width,
        height,
    })
}

/// Normalized surface coordinates map onto the frame's own pixel grid. The
/// shell stretches the whole capture across its layer surface, so this is the
/// same mapping in reverse and it stays correct under fractional scaling.
fn pixel(value: f64, size: u32) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    ((value * f64::from(size)).floor().max(0.0) as u32).min(size - 1)
}

fn sample(frame: &mut Frame, x: f64, y: f64) -> Result<[u8; 3]> {
    let column = u64::from(pixel(x, frame.width));
    let row = u64::from(pixel(y, frame.height));
    let at = frame.offset + (row * u64::from(frame.width) + column) * 3;
    frame.file.seek(SeekFrom::Start(at))?;
    let mut rgb = [0u8; 3];
    frame.file.read_exact(&mut rgb)?;
    Ok(rgb)
}

struct Session {
    id: u64,
    cancel: Arc<AtomicBool>,
    samples: Option<mpsc::SyncSender<Request>>,
    done: JoinHandle<()>,
}

fn start(request: Request) -> Session {
    let id = request.id;
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let (samples, asks) = mpsc::sync_channel::<Request>(SAMPLE_QUEUE);
    let done = thread::spawn(move || {
        let started = Instant::now();
        let result = (|| -> Result {
            let workspace = Workspace::new()?;
            let session_bytes = Arc::new(AtomicU64::new(0));
            // Capture every output at once. A picker that freezes the desktop
            // one monitor at a time shows frames taken tens of milliseconds
            // apart, so a window that moved between them is frozen twice in
            // two places and the colour under the pointer is not the colour
            // that was on screen when the chord was pressed.
            let mut frames = thread::scope(|scope| {
                let threads: Vec<_> = request
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(index, output)| {
                        let path = workspace.0.join(format!("{index}.ppm"));
                        let flag = &flag;
                        let session_bytes = session_bytes.clone();
                        scope.spawn(move || capture(output, &path, flag, &session_bytes))
                    })
                    .collect();
                threads
                    .into_iter()
                    .map(|thread| {
                        thread
                            .join()
                            .map_err(|_| "capture thread failed".into())
                            .and_then(|result| result)
                    })
                    .collect::<Result<Vec<_>>>()
            })?;
            if flag.load(Ordering::Relaxed) {
                return Ok(());
            }
            emit(
                json!({ "id": id, "event": "frames", "captureMs": started.elapsed().as_millis(),
                "frames": frames.iter().map(|frame| json!({ "output": frame.output,
                    "path": frame.path, "width": frame.width, "height": frame.height }))
                    .collect::<Vec<_>>() }),
            )?;
            // Sampling needs no pool and no thread of its own: it is one seek
            // and three bytes out of a file the kernel already has cached.
            // Serving it here also keeps replies in the order they were asked
            // for, which is what lets the shell trust the last one it sees.
            while let Ok(ask) = asks.recv() {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let Some(frame) = frames.iter_mut().find(|frame| frame.output == ask.output) else {
                    continue;
                };
                let [red, green, blue] = sample(frame, ask.x, ask.y)?;
                emit(json!({ "id": id, "event": "sample", "token": ask.token,
                    "commit": ask.commit, "output": ask.output,
                    "r": red, "g": green, "b": blue }))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if !flag.load(Ordering::Relaxed) {
                // Diagnostics describe the failure, never a captured pixel.
                let _ = emit(json!({ "id": id, "event": "error", "message": error.to_string() }));
            }
        }
    });
    Session {
        id,
        cancel,
        samples: Some(samples),
        done,
    }
}

/// Cancel the capture and close the sample channel. The first stops grim
/// mid-stream, the second wakes a session already waiting for work, and the
/// thread's own `Workspace` removes the files on its way out either way.
fn stop(session: &mut Session) {
    session.cancel.store(true, Ordering::Relaxed);
    session.samples.take();
}

fn reap(retiring: &mut Vec<Session>) {
    let mut pending = Vec::new();
    for session in retiring.drain(..) {
        if session.done.is_finished() {
            let _ = session.done.join();
        } else {
            pending.push(session);
        }
    }
    *retiring = pending;
    // Supersession cannot build an unbounded stack of capture sessions.
    while retiring.len() > 1 {
        let _ = retiring.remove(0).done.join();
    }
}

/// SIGTERM on a shell reload follows the same cleanup path as stdin EOF. The
/// signal is blocked and consumed on a signalfd, so no allocation or file
/// removal ever runs inside an asynchronous signal handler.
fn requests() -> Result<mpsc::Receiver<Option<Request>>> {
    let (send, receive) = mpsc::sync_channel(8);
    let fd = unsafe {
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
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let shutdown = send.clone();
    thread::spawn(move || {
        let mut file = unsafe { File::from_raw_fd(fd) };
        let mut event = [0; std::mem::size_of::<libc::signalfd_siginfo>()];
        let _ = file.read_exact(&mut event);
        let _ = shutdown.send(None);
    });
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        let mut frame = Vec::new();
        while let Ok(true) = seele_runtime::wire::read_frame(&mut input, &mut frame, 65536) {
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|argument| argument == "--help") {
        println!("seele-color-worker: newline JSON capture/sample/cancel requests on stdin");
        return Ok(());
    }
    if !args.is_empty() {
        return Err("unknown arguments".into());
    }
    let incoming = requests()?;
    let mut active: Option<Session> = None;
    let mut retiring: Vec<Session> = Vec::new();
    for message in incoming {
        let Some(mut request) = message else {
            break;
        };
        if request.command == "sample" {
            let mut overflow = false;
            if let Some(session) = active.as_ref().filter(|session| session.id == request.id) {
                if let Some(samples) = session.samples.as_ref() {
                    overflow =
                        matches!(samples.try_send(request), Err(mpsc::TrySendError::Full(_)));
                }
            }
            if overflow {
                emit(
                    json!({ "id": active.as_ref().map_or(0, |session| session.id),
                    "event": "error", "message": "Too many pending colour samples" }),
                )?;
                if let Some(mut session) = active.take() {
                    stop(&mut session);
                    retiring.push(session);
                }
            }
            continue;
        }
        if request.command != "capture" && request.command != "cancel" {
            continue;
        }
        if let Some(mut session) = active.take() {
            stop(&mut session);
            retiring.push(session);
        }
        reap(&mut retiring);
        if request.command == "capture" {
            request.outputs.sort();
            request.outputs.dedup();
            if request.outputs.is_empty() || request.outputs.len() > 16 {
                emit(json!({ "id": request.id, "event": "error",
                    "message": "No capturable outputs" }))?;
                continue;
            }
            active = Some(start(request));
        }
    }
    if let Some(mut session) = active {
        stop(&mut session);
        retiring.push(session);
    }
    for session in retiring {
        let _ = session.done.join();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_reads_grim_frames_and_rejects_anything_else() {
        assert_eq!(header(b"P6\n1920 1080\n255\n").unwrap(), (17, 1920, 1080));
        // A whitespace-valued first pixel must not be eaten as a delimiter.
        assert_eq!(header(b"P6 2 1 255 \t\0\xff\x80 ").unwrap().0, 11);
        for invalid in [
            &b"P3\n1 1\n255\n"[..],
            b"P6\n0 1\n255\n",
            b"P6\n1 0\n255\n",
            b"P6\n99999 1\n255\n",
            b"P6\n10000 10000\n255\n",
            b"P6\n1 1\n65535\n",
            b"P6\n# grim\n1 1\n255\n",
            b"P6\n1 1\n255",
            b"",
        ] {
            assert!(header(invalid).is_err());
        }
    }

    #[test]
    fn normalized_points_stay_inside_the_frame() {
        assert_eq!(pixel(0.0, 1920), 0);
        assert_eq!(pixel(0.5, 1920), 960);
        // One is the surface's far edge, not a pixel that exists.
        assert_eq!(pixel(1.0, 1920), 1919);
        assert_eq!(pixel(-1.0, 1920), 0);
        assert_eq!(pixel(1e300, 1920), 1919);
        assert_eq!(pixel(f64::NAN, 1920), 0);
        assert_eq!(pixel(0.999999, 1), 0);
    }

    #[test]
    fn unsafe_output_names_are_rejected_without_capture() {
        for name in ["", "--help", "DP-1\n", "DP-1\0", "DP-1\u{7f}"] {
            assert!(capture(
                name,
                Path::new("/unused"),
                &AtomicBool::new(false),
                &AtomicU64::new(0)
            )
            .is_err());
        }
    }
}
