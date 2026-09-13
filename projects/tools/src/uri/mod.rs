mod codes;
mod image;
mod links;
mod ocr;

use image::Image;
use links::Link;
use ocr::Ocr;
use serde::Deserialize;
use serde_json::{json, Value};
use std::error::Error;
use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Result<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const STRIP: usize = 512;
const OVERLAP: usize = 64;
const MAX_CAPTURE: usize = 128 * 1024 * 1024;
const MAX_CAPTURE_TOTAL: usize = 512 * 1024 * 1024;

/// Charge allocation capacity, not just received bytes, across every session.
/// Cancellation releases the charge when the last queued OCR job drops pixels.
struct Memory {
    total: Arc<AtomicUsize>,
    reserved: usize,
}
impl Memory {
    fn reserve(&mut self, bytes: &mut Vec<u8>, needed: usize) -> io::Result<()> {
        if needed > MAX_CAPTURE {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if needed <= bytes.capacity() {
            return Ok(());
        }
        let next = needed
            .max(bytes.capacity().saturating_mul(2))
            .min(MAX_CAPTURE);
        let extra = next - self.reserved;
        self.total
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |value| {
                value
                    .checked_add(extra)
                    .filter(|value| *value <= MAX_CAPTURE_TOTAL)
            })
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        self.reserved = next;
        bytes
            .try_reserve_exact(next - bytes.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        Ok(())
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        self.total.fetch_sub(self.reserved, Ordering::AcqRel);
    }
}

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    outputs: Vec<String>,
}

struct Capture {
    output: String,
    path: PathBuf,
    image: Image,
    _memory: Memory,
}

/// A private, per-invocation directory. Captures never enter the screenshot
/// library or a cache. Cancellation, EOF and ordinary errors all remove it.
struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Result<Self> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let template = CString::new(base.join("seele-uri-XXXXXX").as_os_str().as_bytes())?;
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
    let bytes = seele_runtime::wire::json_frame(&value, 4 * 1024 * 1024)?;
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

fn capture(
    output: String,
    path: PathBuf,
    cancel: &AtomicBool,
    total: Arc<AtomicUsize>,
) -> Result<Capture> {
    if output.is_empty()
        || output.len() > 256
        || output.starts_with('-')
        || output.chars().any(char::is_control)
    {
        return Err("invalid output name".into());
    }
    let mut bytes = Vec::new();
    let mut memory = Memory { total, reserved: 0 };
    let result = seele_runtime::process::stream_stdout(
        Command::new("grim").args(["-t", "ppm", "-o", &output, "-"]),
        b"",
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(3),
            output: 64 * 1024,
        },
        cancel,
        |chunk| {
            let needed = bytes_len_plus(chunk.len(), bytes.len())?;
            memory.reserve(&mut bytes, needed)?;
            bytes.extend_from_slice(chunk);
            Ok(())
        },
    )?;
    if !result.status.success() {
        return Err("screen capture failed".into());
    }
    let image = Image::parse(bytes)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(&image.bytes)?;
    Ok(Capture {
        output,
        image,
        path,
        _memory: memory,
    })
}
fn bytes_len_plus(extra: usize, current: usize) -> io::Result<usize> {
    current
        .checked_add(extra)
        .ok_or_else(|| io::ErrorKind::OutOfMemory.into())
}

fn fixture(path: PathBuf) -> Result<Capture> {
    let mut file = File::open(&path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_CAPTURE as u64 {
        return Err("fixture exceeds capture limits".into());
    }
    let mut memory = Memory {
        total: Arc::new(AtomicUsize::new(0)),
        reserved: 0,
    };
    let mut bytes = Vec::new();
    memory.reserve(&mut bytes, metadata.len() as usize)?;
    bytes.resize(metadata.len() as usize, 0);
    file.read_exact(&mut bytes)?;
    if file.read(&mut [0u8; 1])? != 0 {
        return Err("fixture changed while reading".into());
    }
    Ok(Capture {
        output: "fixture".into(),
        image: Image::parse(bytes)?,
        path,
        _memory: memory,
    })
}

struct Job {
    capture: Arc<Capture>,
    strip: Option<(usize, usize)>,
    cancel: Arc<AtomicBool>,
    reply: mpsc::Sender<std::result::Result<Vec<Link>, String>>,
}

fn queue(jobs: &mpsc::SyncSender<Job>, mut job: Job) -> Result {
    loop {
        if job.cancel.load(Ordering::Relaxed) {
            return Err("recognition cancelled".into());
        }
        match jobs.try_send(job) {
            Ok(()) => return Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err("recognition workers unavailable".into())
            }
            Err(mpsc::TrySendError::Full(pending)) => {
                job = pending;
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

struct Pool {
    jobs: Option<mpsc::SyncSender<Job>>,
    threads: Vec<JoinHandle<()>>,
}

impl Pool {
    fn new() -> Self {
        let (send, receive) = mpsc::sync_channel::<Job>(128);
        let receive = Arc::new(Mutex::new(receive));
        // Each Tesseract instance is single-threaded. Bound total parallelism
        // instead of nesting OpenMP teams inside one worker per output.
        let count = thread::available_parallelism()
            .map_or(2, |n| n.get())
            .clamp(1, 6);
        let threads = (0..count)
            .map(|_| {
                let receive = receive.clone();
                thread::spawn(move || {
                    let mut engine = Ocr::new().map_err(|e| e.to_string());
                    loop {
                        let job = receive.lock().unwrap().recv();
                        let Ok(job) = job else {
                            break;
                        };
                        if job.cancel.load(Ordering::Relaxed) {
                            continue;
                        }
                        let image = &job.capture.image;
                        let Some((core_start, core_end)) = job.strip else {
                            let result = codes::scan(image, &job.capture.output, &job.cancel)
                                .map_err(|e| e.to_string());
                            let _ = job.reply.send(result);
                            continue;
                        };
                        let start = core_start.saturating_sub(OVERLAP);
                        let end = (core_end + OVERLAP).min(image.height);
                        let result = match &mut engine {
                            Ok(engine) => engine
                                .words(image, start, end - start, &job.cancel)
                                .map(|words| {
                                    links::extract(
                                        words,
                                        &job.capture.output,
                                        image.width,
                                        image.height,
                                        start,
                                        core_start,
                                        core_end,
                                    )
                                })
                                .map_err(|e| e.to_string()),
                            Err(error) => Err(error.clone()),
                        };
                        let _ = job.reply.send(result);
                    }
                })
            })
            .collect();
        Self {
            jobs: Some(send),
            threads,
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.jobs.take();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn scan(
    request: &Request,
    captures: Vec<Arc<Capture>>,
    cancel: &Arc<AtomicBool>,
    jobs: &mpsc::SyncSender<Job>,
    started: Instant,
) -> Result {
    emit(
        json!({ "id": request.id, "event": "frames", "captureMs": started.elapsed().as_millis(),
        "frames": captures.iter().map(|c| json!({ "output": c.output, "path": c.path,
            "width": c.image.width, "height": c.image.height })).collect::<Vec<_>>() }),
    )?;
    let (reply, results) = mpsc::channel();
    let mut remaining = 0;
    for capture in &captures {
        queue(
            jobs,
            Job {
                capture: capture.clone(),
                strip: None,
                cancel: cancel.clone(),
                reply: reply.clone(),
            },
        )?;
        remaining += 1;
    }
    // Interleave outputs so every monitor gets its first hints promptly.
    let max_height = captures.iter().map(|c| c.image.height).max().unwrap_or(0);
    for start in (0..max_height).step_by(STRIP) {
        for capture in &captures {
            if start >= capture.image.height {
                continue;
            }
            queue(
                jobs,
                Job {
                    capture: capture.clone(),
                    strip: Some((start, (start + STRIP).min(capture.image.height))),
                    cancel: cancel.clone(),
                    reply: reply.clone(),
                },
            )?;
            remaining += 1;
        }
    }
    drop(reply);
    // Jobs own the pixels now. Only the image files live for the whole picker.
    drop(captures);
    let mut next_number = 1;
    let mut failures = 0;
    while remaining > 0 && !cancel.load(Ordering::Relaxed) {
        match results.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => {
                remaining -= 1;
                match result {
                    Ok(mut links) => {
                        for link in &mut links {
                            link.number = next_number;
                            next_number += 1;
                        }
                        if !links.is_empty() {
                            emit(json!({ "id": request.id, "event": "links", "links": links }))?;
                        }
                    }
                    Err(_) => failures += 1,
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("OCR workers disconnected".into())
            }
        }
    }
    if !cancel.load(Ordering::Relaxed) {
        emit(
            json!({ "id": request.id, "event": "done", "count": next_number - 1,
            "failedAreas": failures, "elapsedMs": started.elapsed().as_millis() }),
        )?;
    }
    Ok(())
}

struct Session {
    cancel: Arc<AtomicBool>,
    done: JoinHandle<()>,
}

fn start(request: Request, jobs: mpsc::SyncSender<Job>, total: Arc<AtomicUsize>) -> Session {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let done = thread::spawn(move || {
        let started = Instant::now();
        let result = (|| -> Result {
            let workspace = Workspace::new()?;
            let captures = thread::scope(|scope| {
                let threads: Vec<_> = request
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(index, output)| {
                        let output = output.clone();
                        let path = workspace.0.join(format!("{index}.ppm"));
                        let flag = &flag;
                        let total = total.clone();
                        scope.spawn(move || capture(output, path, flag, total).map(Arc::new))
                    })
                    .collect();
                threads
                    .into_iter()
                    .map(|t| {
                        t.join()
                            .map_err(|_| "capture thread failed".into())
                            .and_then(|r| r)
                    })
                    .collect::<Result<Vec<_>>>()
            })?;
            if flag.load(Ordering::Relaxed) {
                return Ok(());
            }
            scan(&request, captures, &flag, &jobs, started)?;
            // Keep only files, never pixels, until the UI has released them.
            while !flag.load(Ordering::Relaxed) {
                thread::park();
            }
            Ok(())
        })();
        if let Err(error) = result {
            if !flag.load(Ordering::Relaxed) {
                // Errors contain no recognized text or image contents.
                let _ = emit(
                    json!({ "id": request.id, "event": "error", "message": error.to_string() }),
                );
            }
        }
    });
    Session { cancel, done }
}

fn stop(session: &Session) {
    session.cancel.store(true, Ordering::Relaxed);
    session.done.thread().unpark();
}

/// SIGTERM on a shell reload follows the same cleanup path as stdin EOF.
/// Block signals before starting the OCR threads and consume them on signalfd;
/// no allocation or file operations run inside an asynchronous signal handler.
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
    if args.first().is_some_and(|a| a == "--help") {
        println!("seele-uri-worker: newline JSON capture/cancel requests on stdin\n  --image <ppm>  Recognize a fixture and emit the same event stream");
        return Ok(());
    }
    // Set before creating threads or entering libtesseract/libgomp.
    std::env::set_var("OMP_THREAD_LIMIT", "1");
    std::env::set_var("OMP_NUM_THREADS", "1");
    let incoming = if args.is_empty() {
        Some(requests()?)
    } else {
        None
    };
    let pool = Pool::new();
    if args.first().is_some_and(|a| a == "--image") {
        let path = Path::new(args.get(1).ok_or("--image needs a PPM file")?).canonicalize()?;
        let capture = Arc::new(fixture(path)?);
        return scan(
            &Request {
                command: "capture".into(),
                id: 1,
                outputs: vec![],
            },
            vec![capture],
            &Arc::new(AtomicBool::new(false)),
            pool.jobs.as_ref().unwrap(),
            Instant::now(),
        );
    }
    if !args.is_empty() {
        return Err("unknown arguments".into());
    }
    let total = Arc::new(AtomicUsize::new(0));
    let mut active: Option<Session> = None;
    let mut retiring: Vec<Session> = Vec::new();
    for request in incoming.unwrap() {
        let Some(mut request) = request else {
            break;
        };
        if request.command != "capture" && request.command != "cancel" {
            continue;
        }
        if let Some(session) = active.take() {
            stop(&session);
            retiring.push(session);
        }
        let mut pending = Vec::new();
        for session in retiring.drain(..) {
            if session.done.is_finished() {
                let _ = session.done.join();
            } else {
                pending.push(session);
            }
        }
        retiring = pending;
        // Supersession cannot create an unbounded stack of capture sessions.
        while retiring.len() > 1 {
            let _ = retiring.remove(0).done.join();
        }
        if request.command == "capture" {
            request.outputs.sort();
            request.outputs.dedup();
            if request.outputs.is_empty() || request.outputs.len() > 16 {
                emit(
                    json!({ "id": request.id, "event": "error", "message": "No capturable outputs" }),
                )?;
                continue;
            }
            active = Some(start(
                request,
                pool.jobs.as_ref().unwrap().clone(),
                total.clone(),
            ));
        }
    }
    if let Some(session) = active {
        stop(&session);
        retiring.push(session);
    }
    for session in retiring {
        let _ = session.done.join();
    }
    Ok(())
}

#[cfg(test)]
mod memory_tests {
    use super::*;
    #[test]
    fn allocation_budget_is_shared_released_and_checked_before_growth() {
        let total = Arc::new(AtomicUsize::new(0));
        let mut bytes = Vec::new();
        {
            let mut memory = Memory {
                total: total.clone(),
                reserved: 0,
            };
            memory.reserve(&mut bytes, 65536).unwrap();
            assert_eq!(total.load(Ordering::Relaxed), 65536);
            assert!(memory.reserve(&mut bytes, MAX_CAPTURE + 1).is_err());
            assert_eq!(bytes.capacity(), 65536);
            total.store(MAX_CAPTURE_TOTAL, Ordering::Relaxed);
            assert!(memory.reserve(&mut bytes, 65537).is_err());
            total.store(memory.reserved, Ordering::Relaxed);
        }
        assert_eq!(total.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn unsafe_output_names_are_rejected_without_capture() {
        for name in ["", "--help", "DP-1\n", "DP-1\0", "DP-1\u{7f}"] {
            assert!(capture(
                name.into(),
                PathBuf::from("/unused"),
                &AtomicBool::new(false),
                Arc::new(AtomicUsize::new(0))
            )
            .is_err());
        }
    }
}
