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
use std::io::{self, BufRead, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Result<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const STRIP: usize = 512;
const OVERLAP: usize = 64;

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

fn emit(value: Value) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = serde_json::to_writer(&mut out, &value);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

fn capture(output: String, path: PathBuf, cancel: &AtomicBool) -> Result<Capture> {
    if output.is_empty() || output.starts_with('-') || output.contains(['\0', '\n']) {
        return Err("invalid output name".into());
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    let mut child = Command::new("grim")
        .args(["-t", "ppm", "-o", &output, "-"])
        .stdout(file)
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err("screen capture failed".into());
            }
            break;
        }
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("screen capture cancelled or timed out".into());
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(Capture {
        output,
        image: Image::parse(fs::read(&path)?)?,
        path,
    })
}

struct Job {
    capture: Arc<Capture>,
    core_start: usize,
    core_end: usize,
    cancel: Arc<AtomicBool>,
    reply: mpsc::Sender<std::result::Result<Vec<Link>, String>>,
}

struct Pool {
    jobs: Option<mpsc::Sender<Job>>,
    threads: Vec<JoinHandle<()>>,
}

impl Pool {
    fn new() -> Self {
        let (send, receive) = mpsc::channel::<Job>();
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
                        let start = job.core_start.saturating_sub(OVERLAP);
                        let end = (job.core_end + OVERLAP).min(image.height);
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
                                        job.core_start,
                                        job.core_end,
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
    jobs: &mpsc::Sender<Job>,
    started: Instant,
) -> Result {
    emit(
        json!({ "id": request.id, "event": "frames", "captureMs": started.elapsed().as_millis(),
        "frames": captures.iter().map(|c| json!({ "output": c.output, "path": c.path,
            "width": c.image.width, "height": c.image.height })).collect::<Vec<_>>() }),
    );
    let (reply, results) = mpsc::channel();
    let mut remaining = 0;
    // Interleave outputs so every monitor gets its first hints promptly.
    let max_height = captures.iter().map(|c| c.image.height).max().unwrap_or(0);
    for start in (0..max_height).step_by(STRIP) {
        for capture in &captures {
            if start >= capture.image.height {
                continue;
            }
            jobs.send(Job {
                capture: capture.clone(),
                core_start: start,
                core_end: (start + STRIP).min(capture.image.height),
                cancel: cancel.clone(),
                reply: reply.clone(),
            })?;
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
                            emit(json!({ "id": request.id, "event": "links", "links": links }));
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
        );
    }
    Ok(())
}

struct Session {
    cancel: Arc<AtomicBool>,
    done: JoinHandle<()>,
}

fn start(request: Request, jobs: mpsc::Sender<Job>) -> Session {
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
                        scope.spawn(move || capture(output, path, flag).map(Arc::new))
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
                emit(json!({ "id": request.id, "event": "error", "message": error.to_string() }));
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
    let (send, receive) = mpsc::channel();
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
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if line.len() > 65536 {
                continue;
            }
            if let Ok(request) = serde_json::from_str::<Request>(&line) {
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
        let capture = Arc::new(Capture {
            output: "fixture".into(),
            image: Image::parse(fs::read(&path)?)?,
            path,
        });
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
        if request.command == "capture" {
            request.outputs.sort();
            request.outputs.dedup();
            if request.outputs.is_empty() || request.outputs.len() > 16 {
                emit(
                    json!({ "id": request.id, "event": "error", "message": "No capturable outputs" }),
                );
                continue;
            }
            active = Some(start(request, pool.jobs.as_ref().unwrap().clone()));
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
