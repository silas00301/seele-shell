//! The Audio panel's microphone test.
//!
//! Two shapes of the same question — what does this microphone sound like
//! right now: a bounded five-second sample played back once it is complete,
//! and a live monitor that never accumulates. Both are measured from the
//! captured samples themselves, so the meter and the clipping indicator
//! report the signal the microphone delivered rather than how loudly it is
//! being played back.
//!
//! The worker is resident for as long as the Audio panel is open and owns
//! nothing else: the capture stream, the playback stream and the retained
//! sample all end with the session that started them. The sample lives in
//! memory and is never written anywhere.
//!
//! Line-delimited JSON in both directions. Requests arrive on stdin as
//! `{"command":"sample","input":"<source>","output":"<sink>"}`; three kinds of
//! event leave on stdout, told apart by the key they lead with:
//!
//! * `{"mode":…}` — the session's own state, published on every change.
//! * `{"level":…}` — one meter frame, at twenty frames a second.
//! * `{"users":…}` — who else is holding the microphone, and whether that
//!   could be established at all.
use crate::command::output;
use crate::Result;
use seele_runtime::cancel::Cancellation;
use seele_runtime::process::{capture_file, discard, stream_stdout, Limits};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;
/// Signed 16-bit mono. One frame is two bytes, which every byte offset below
/// depends on.
const FRAME_BYTES: usize = 2;
const SAMPLE_MILLIS: u64 = 5_000;
const SAMPLE_BYTES: usize = RATE as usize * FRAME_BYTES * SAMPLE_MILLIS as usize / 1000;
/// -0.07 dBFS. A converter leaves a clipped signal at the top of the scale
/// rather than exactly at it, and the meter is drawn on a compressed scale,
/// so clipping is decided here on the linear sample values instead.
const CLIP_LEVEL: i16 = 32_512;
/// A single clipped syllable has to stay visible long enough to be read.
const CLIP_HOLD: Duration = Duration::from_millis(1_500);
const FRAME_INTERVAL: Duration = Duration::from_millis(50);
const USERS_INTERVAL: Duration = Duration::from_millis(2_000);
/// Low enough to hear yourself rather than an echo of yourself.
const LATENCY_MILLIS: u32 = 20;
/// About eighty-five milliseconds of audio. The live monitor drops rather
/// than queues, so an output that stalls cannot turn into latency the user
/// then hears for the rest of the session.
const PIPE_BYTES: libc::c_int = 8 * 1024;
/// Both test streams carry these, which is also how the test excludes itself
/// from its own microphone-use report.
const CLIENT_NAME: &str = "Seele microphone test";
const STREAM_NAME: &str = "Microphone test";
/// A resident worker has no duration limit of its own; the panel closing is
/// what ends it.
const NO_DEADLINE: Duration = Duration::from_secs(365 * 86_400);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Recording,
    Playing,
    Live,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Playing => "playing",
            Self::Live => "live",
        }
    }
}

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    output: String,
}

impl Request {
    fn bare(command: &str) -> Self {
        Self {
            command: command.to_owned(),
            input: String::new(),
            output: String::new(),
        }
    }
}

/// Cancellation for one session. `full` retires a capture that reached its own
/// length; `local` retires one the user or a device change ended.
#[derive(Default)]
struct Stop {
    local: AtomicBool,
    full: AtomicBool,
    global: Arc<AtomicUsize>,
}

impl Stop {
    fn new(global: &Arc<AtomicUsize>) -> Arc<Self> {
        Arc::new(Self {
            global: global.clone(),
            ..Default::default()
        })
    }

    fn end(&self) {
        self.local.store(true, Ordering::Relaxed);
    }

    fn ended(&self) -> bool {
        self.local.load(Ordering::Relaxed) || self.global.load(Ordering::Relaxed) != 0
    }
}

impl Cancellation for Stop {
    fn is_cancelled(&self) -> bool {
        self.ended() || self.full.load(Ordering::Relaxed)
    }
}

/// Peak and clipping over one meter frame. The bar is drawn on a square-root
/// scale so ordinary speech is visible along it, and clipping is decided
/// separately on the linear values, so a compressed meter cannot hide it.
#[derive(Default)]
struct Meter {
    odd: Option<u8>,
    peak: i16,
    run: u32,
    clipped: Option<Instant>,
}

impl Meter {
    fn feed(&mut self, bytes: &[u8], now: Instant) {
        for byte in bytes {
            let Some(low) = self.odd.take() else {
                self.odd = Some(*byte);
                continue;
            };
            let magnitude = i16::from_le_bytes([low, *byte]).saturating_abs();
            self.peak = self.peak.max(magnitude);
            if magnitude < CLIP_LEVEL {
                self.run = 0;
                continue;
            }
            self.run += 1;
            // One sample at the top of the scale is a transient; a run of them
            // is the converter having nothing left to give.
            if self.run >= 2 {
                self.clipped = Some(now);
            }
        }
    }

    fn take(&mut self, now: Instant) -> (f32, f32, bool) {
        let peak = (f32::from(std::mem::take(&mut self.peak)) / f32::from(i16::MAX)).clamp(0.0, 1.0);
        (peak.sqrt(), peak, self.clipping(now))
    }

    fn clipping(&self, now: Instant) -> bool {
        self.clipped
            .is_some_and(|at| now.saturating_duration_since(at) < CLIP_HOLD)
    }
}

fn emit(value: &Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")?;
    out.flush()
}

fn millis(bytes: usize) -> u64 {
    bytes as u64 * 1000 / (u64::from(RATE) * FRAME_BYTES as u64)
}

fn list(kind: &str) -> Option<Vec<Value>> {
    serde_json::from_str(&output("pactl", ["--format=json", "list", kind])?).ok()
}

fn named<'a>(entries: &'a [Value], name: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|entry| entry["name"].as_str() == Some(name))
}

/// A monitor source carries an output's own playback. Recording one and
/// playing it back into that same output is precisely the feedback loop this
/// test exists to let the user avoid, so it is never a test input, and a
/// stream reading one is not microphone use either.
fn is_monitor(source: &Value) -> bool {
    if source["name"]
        .as_str()
        .is_some_and(|name| name.ends_with(".monitor"))
    {
        return true;
    }
    match &source["monitor_of_sink"] {
        Value::String(value) => !value.is_empty() && value != "n/a",
        Value::Number(value) => value.as_u64() != Some(u64::from(u32::MAX)),
        _ => false,
    }
}

/// Device names reach `pactl` and `pacat` as their own argv entries, so word
/// splitting is not a concern; a leading dash would still be read as an
/// option, and an unknown name is rejected against the server's own listing
/// either way.
fn plausible(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-:+".contains(&byte))
}

struct Devices {
    source: String,
    sink: String,
    muted: bool,
}

/// The test output is resolved on its own, because replaying a sample needs
/// an output and no longer needs the microphone it came from.
fn resolve_sink(output: &str) -> Result<String> {
    if !plausible(output) {
        return Err("That output cannot be used for the test".into());
    }
    let sinks = list("sinks").ok_or("The audio server is unavailable")?;
    named(&sinks, output).ok_or("The selected test output is no longer available")?;
    Ok(output.to_owned())
}

/// Identity is resolved when a test starts, not when the panel was opened.
fn resolve(input: &str, output: &str) -> Result<Devices> {
    if !plausible(input) {
        return Err("That microphone cannot be tested".into());
    }
    let sources = list("sources").ok_or("The audio server is unavailable")?;
    let source = named(&sources, input).ok_or("The selected microphone is no longer available")?;
    if is_monitor(source) {
        return Err("That input is an output monitor and would feed back".into());
    }
    let muted = source["mute"].as_bool().unwrap_or(false);
    Ok(Devices {
        source: input.to_owned(),
        sink: resolve_sink(output)?,
        muted,
    })
}

/// A name for another application holding the microphone, kept to what the
/// server was actually told rather than guessed at.
fn application(props: &Value) -> String {
    let raw = [
        "application.name",
        "application.process.binary",
        "media.name",
    ]
    .into_iter()
    .find_map(|key| props[key].as_str().filter(|value| !value.trim().is_empty()))
    .unwrap_or_default();
    let name: String = raw
        .chars()
        .filter(|character| !character.is_control() && seele_runtime::redact::visible(*character))
        .take(64)
        .collect();
    let name = name.trim().to_owned();
    if name.is_empty() {
        "An application".into()
    } else {
        name
    }
}

/// Every application recording from a real microphone right now, excluding
/// this test's own streams, streams that hold a source without reading it,
/// and streams reading an output's monitor.
fn users(sources: &[Value], streams: &[Value]) -> Vec<String> {
    let monitors: HashSet<u64> = sources
        .iter()
        .filter(|source| is_monitor(source))
        .filter_map(|source| source["index"].as_u64())
        .collect();
    let mut names = Vec::new();
    for stream in streams {
        if stream["corked"].as_bool() == Some(true)
            || stream["source"]
                .as_u64()
                .is_some_and(|index| monitors.contains(&index))
        {
            continue;
        }
        let props = &stream["properties"];
        // The test's own capture and playback are not another application's
        // use of the microphone. Both test streams are named here.
        if props["application.name"].as_str() == Some(CLIENT_NAME)
            || props["media.name"].as_str() == Some(STREAM_NAME)
        {
            continue;
        }
        let name = application(props);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Who else is recording, and whether that could be established at all. A
/// server that cannot be asked reports the limitation rather than an empty
/// list, because "nobody is using it" and "nobody could be asked" are
/// different answers to the question the warning exists for.
fn microphone_users() -> (Vec<String>, bool) {
    let (Some(sources), Some(streams)) = (list("sources"), list("source-outputs")) else {
        return (Vec::new(), false);
    };
    (users(&sources, &streams), true)
}

/// The report is republished while a test runs, because a warning does not
/// make the microphone exclusive and another application may take it up
/// afterwards.
fn watch_users(stop: Arc<Stop>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut last: Option<(Vec<String>, bool)> = None;
        while !stop.ended() {
            let current = microphone_users();
            if last.as_ref() != Some(&current)
                && emit(&json!({"users":current.0,"detection":current.1})).is_err()
            {
                return;
            }
            last = Some(current);
            let deadline = Instant::now() + USERS_INTERVAL;
            while !stop.ended() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
        }
    })
}

fn stream_arguments(device: &str) -> Vec<String> {
    vec![
        "--raw".into(),
        "--format=s16le".into(),
        format!("--rate={RATE}"),
        "--channels=1".into(),
        format!("--device={device}"),
        format!("--client-name={CLIENT_NAME}"),
        format!("--stream-name={STREAM_NAME}"),
        format!("--latency-msec={LATENCY_MILLIS}"),
    ]
}

/// The test's own playback names its device explicitly, so it reaches one
/// output without touching the default or any other application's route.
fn playback_arguments(device: &str) -> Vec<String> {
    let mut arguments = vec!["--playback".into()];
    arguments.extend(stream_arguments(device));
    arguments
}

/// A private pipe carrying live audio straight from the capture child to the
/// playback child, deliberately small so the monitor stays a monitor.
fn pipe() -> io::Result<(File, File)> {
    let mut descriptors = [-1; 2];
    // SAFETY: pipe2 initializes exactly the two descriptors it is given.
    if unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both descriptors are freshly created and owned by this process.
    let (read, write) = unsafe {
        (
            File::from_raw_fd(descriptors[0]),
            File::from_raw_fd(descriptors[1]),
        )
    };
    let fd = write.as_raw_fd();
    // SAFETY: the write end is owned above and stays open across both calls.
    unsafe {
        // A kernel that refuses the smaller buffer leaves the default one,
        // which is correct but less responsive.
        let _ = libc::fcntl(fd, libc::F_SETPIPE_SZ, PIPE_BYTES);
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok((read, write))
}

struct Playback {
    handle: thread::JoinHandle<io::Result<ExitStatus>>,
    stop: Arc<Stop>,
}

impl Playback {
    /// The retained sample, played once.
    fn bytes(device: &str, audio: Vec<u8>, stop: Arc<Stop>) -> Self {
        let arguments = playback_arguments(device);
        let cancel = stop.clone();
        Self {
            handle: thread::spawn(move || {
                discard(
                    Command::new("pacat").args(arguments),
                    &audio,
                    Limits {
                        timeout: NO_DEADLINE,
                        output: 0,
                    },
                    &cancel,
                )
            }),
            stop,
        }
    }

    /// The live monitor, reading the private pipe the capture writes into.
    fn stream(device: &str, input: File, stop: Arc<Stop>) -> Self {
        let arguments = playback_arguments(device);
        let cancel = stop.clone();
        Self {
            handle: thread::spawn(move || {
                capture_file(
                    Command::new("pacat").args(arguments),
                    &input,
                    Limits {
                        timeout: NO_DEADLINE,
                        output: 64 * 1024,
                    },
                    &cancel,
                )
                .map(|output| output.status)
            }),
            stop,
        }
    }

    fn done(&self) -> bool {
        self.handle.is_finished()
    }

    /// Ends the stream and reports whether the output itself stayed healthy,
    /// which is how a sink that disappeared mid-test is told apart from a
    /// deliberate stop. `graceful` waits for playback that is already over.
    fn finish(self, graceful: bool) -> bool {
        if !graceful {
            self.stop.end();
        }
        match self.handle.join() {
            Ok(Ok(status)) => status.success(),
            // Cancellation is the ordinary way a live monitor ends.
            Ok(Err(error)) => error.kind() == io::ErrorKind::Interrupted,
            Err(_) => false,
        }
    }
}

/// One capture stream, metered from its own samples. `consume` receives every
/// captured byte; `watch` runs once per meter frame and ends the session by
/// cancelling `stop`.
fn capture(
    device: &str,
    stop: &Arc<Stop>,
    limit: Option<usize>,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
    mut watch: impl FnMut() -> io::Result<()>,
) -> io::Result<ExitStatus> {
    let mut meter = Meter::default();
    let mut captured = 0_usize;
    let mut emitted = Instant::now();
    stream_stdout(
        Command::new("parecord").args(stream_arguments(device)),
        b"",
        Limits {
            timeout: NO_DEADLINE,
            output: 64 * 1024,
        },
        stop.as_ref(),
        |chunk| {
            let room = limit.map_or(chunk.len(), |limit| limit.saturating_sub(captured));
            let chunk = &chunk[..room.min(chunk.len())];
            let now = Instant::now();
            meter.feed(chunk, now);
            captured += chunk.len();
            consume(chunk)?;
            if limit.is_some_and(|limit| captured >= limit) {
                stop.full.store(true, Ordering::Relaxed);
            }
            if now.saturating_duration_since(emitted) >= FRAME_INTERVAL {
                emitted = now;
                let (level, peak, clipping) = meter.take(now);
                emit(&json!({"level":level,"peak":peak,"clipped":clipping,
                    "remaining":limit.map(|limit| millis(limit.saturating_sub(captured)))}))?;
                watch()?;
            }
            Ok(())
        },
    )
    .map(|output| output.status)
}

/// A capture that ended by itself ended because its device did.
fn capture_failure(result: &io::Result<ExitStatus>) -> String {
    match result {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => String::new(),
        _ => "The microphone stopped delivering audio".into(),
    }
}

fn clipped(audio: &[u8]) -> bool {
    let mut meter = Meter::default();
    let now = Instant::now();
    meter.feed(audio, now);
    meter.clipping(now)
}

/// Everything the panel is shown about the test, published whole on every
/// change so a dropped line cannot leave the surface disagreeing with the
/// worker.
struct Worker {
    shutdown: Arc<AtomicUsize>,
    mode: Mode,
    sample: Vec<u8>,
    clipped: bool,
    input: String,
    output: String,
    muted: bool,
    error: String,
}

impl Worker {
    fn new(shutdown: Arc<AtomicUsize>) -> Self {
        Self {
            shutdown,
            mode: Mode::Idle,
            sample: Vec::new(),
            clipped: false,
            input: String::new(),
            output: String::new(),
            muted: false,
            error: String::new(),
        }
    }

    fn stopping(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed) != 0
    }

    fn publish(&self) -> io::Result<()> {
        emit(&json!({
            "mode": self.mode.name(),
            "sample": !self.sample.is_empty(),
            "clipped": self.clipped,
            "input": self.input,
            "output": self.output,
            "muted": self.muted,
            "error": self.error,
        }))
    }

    fn idle(&mut self, error: &str) -> io::Result<()> {
        self.mode = Mode::Idle;
        self.error = error.to_owned();
        self.publish()
    }

    /// A sample belongs to the microphone it came from, so anything that ends
    /// a session without producing one leaves nothing behind to replay.
    fn discard(&mut self) {
        self.sample = Vec::new();
        self.clipped = false;
    }

    fn start(&mut self, request: &Request) -> std::result::Result<Devices, String> {
        let devices = resolve(&request.input, &request.output).map_err(|error| error.to_string())?;
        self.input = devices.source.clone();
        self.output = devices.sink.clone();
        self.muted = devices.muted;
        self.error.clear();
        Ok(devices)
    }

    /// Capture five seconds, then play them back once. Nothing is played into
    /// the output while the sample is being captured.
    fn record(
        &mut self,
        request: &Request,
        requests: &mpsc::Receiver<Request>,
    ) -> io::Result<Option<Request>> {
        self.discard();
        let devices = match self.start(request) {
            Ok(devices) => devices,
            Err(error) => return self.idle(&error).map(|()| None),
        };
        self.mode = Mode::Recording;
        self.publish()?;
        let stop = Stop::new(&self.shutdown);
        let watchers = watch_users(stop.clone());
        let mut audio = Vec::with_capacity(SAMPLE_BYTES);
        let mut next = None;
        let result = capture(
            &devices.source,
            &stop,
            Some(SAMPLE_BYTES),
            |chunk| {
                audio.extend_from_slice(chunk);
                Ok(())
            },
            || {
                if let Ok(request) = requests.try_recv() {
                    next = Some(request);
                    stop.end();
                }
                Ok(())
            },
        );
        let complete = stop.full.load(Ordering::Relaxed);
        stop.end();
        let _ = watchers.join();
        if !complete {
            // A cancelled or failed capture leaves no half sample behind.
            let message = if next.is_some() || self.stopping() {
                String::new()
            } else {
                capture_failure(&result)
            };
            return self.idle(&message).map(|()| next);
        }
        self.clipped = clipped(&audio);
        self.sample = audio;
        if next.is_some() || self.stopping() {
            return self.idle("").map(|()| next);
        }
        self.play(&devices.sink, requests)
    }

    fn replay(
        &mut self,
        request: &Request,
        requests: &mpsc::Receiver<Request>,
    ) -> io::Result<Option<Request>> {
        if self.sample.is_empty() {
            return self.idle("There is no recording to replay").map(|()| None);
        }
        let sink = match resolve_sink(&request.output) {
            Ok(sink) => sink,
            Err(error) => return self.idle(&error.to_string()).map(|()| None),
        };
        self.output = sink.clone();
        self.error.clear();
        self.play(&sink, requests)
    }

    fn play(
        &mut self,
        sink: &str,
        requests: &mpsc::Receiver<Request>,
    ) -> io::Result<Option<Request>> {
        self.mode = Mode::Playing;
        self.publish()?;
        // Nothing is being captured, so the meter rests rather than holding
        // the last frame of the recording.
        emit(&json!({"level":0.0,"peak":0.0,"clipped":false,"remaining":null}))?;
        let playback = Playback::bytes(sink, self.sample.clone(), Stop::new(&self.shutdown));
        let mut next = None;
        while !playback.done() {
            if let Ok(request) = requests.try_recv() {
                next = Some(request);
                break;
            }
            if self.stopping() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let interrupted = next.is_some() || self.stopping();
        let played = playback.finish(!interrupted);
        let message = if played || interrupted {
            ""
        } else {
            "The test output stopped playing"
        };
        self.idle(message).map(|()| next)
    }

    /// Capture and playback run together through a bounded pipe, so live
    /// monitoring costs one buffer rather than a growing recording.
    fn live(
        &mut self,
        request: &Request,
        requests: &mpsc::Receiver<Request>,
    ) -> io::Result<Option<Request>> {
        // A live monitor is not a recording, and starting one retires the
        // sample the panel was still offering to replay.
        self.discard();
        let devices = match self.start(request) {
            Ok(devices) => devices,
            Err(error) => return self.idle(&error).map(|()| None),
        };
        let Ok((read, mut write)) = pipe() else {
            return self
                .idle("The microphone test could not open its audio path")
                .map(|()| None);
        };
        let stop = Stop::new(&self.shutdown);
        let playback = Playback::stream(&devices.sink, read, stop.clone());
        let watchers = watch_users(stop.clone());
        self.mode = Mode::Live;
        self.publish()?;
        let mut next = None;
        let mut lost = false;
        let result = capture(
            &devices.source,
            &stop,
            None,
            |chunk| match write.write(chunk) {
                Ok(_) => Ok(()),
                // The monitor drops audio it cannot hand over immediately.
                // Queueing it would be heard as latency for the rest of the
                // session rather than as the dropout it actually is.
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::BrokenPipe
                    ) =>
                {
                    Ok(())
                }
                Err(error) => Err(error),
            },
            || {
                if let Ok(request) = requests.try_recv() {
                    next = Some(request);
                    stop.end();
                } else if playback.done() {
                    lost = true;
                    stop.end();
                }
                Ok(())
            },
        );
        stop.end();
        // Closing the write end lets the playback child drain whatever it
        // already holds before it is reaped.
        drop(write);
        let _ = watchers.join();
        let played = playback.finish(false);
        let message = if next.is_some() || self.stopping() {
            String::new()
        } else if lost || !played {
            "The test output stopped playing".into()
        } else {
            capture_failure(&result)
        };
        self.idle(&message).map(|()| next)
    }
}

fn read_requests(sender: mpsc::Sender<Request>) {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request = serde_json::from_str::<Request>(&line).unwrap_or_else(|_| Request::bare(""));
        if sender.send(request).is_err() {
            return;
        }
    }
    // The panel closing shuts stdin, which is the ordinary end of a session.
    let _ = sender.send(Request::bare("quit"));
}

pub fn run(arguments: &[String]) -> Result {
    if arguments.iter().any(|value| value == "--help") {
        println!("usage: seele-mic-test");
        println!("Reads JSON requests on stdin and writes JSON events on stdout.");
        return Ok(());
    }
    let shutdown = crate::command::shutdown_signal();
    let (sender, requests) = mpsc::channel();
    thread::spawn(move || read_requests(sender));
    let mut worker = Worker::new(shutdown.clone());
    worker.publish()?;
    let mut pending = None;
    loop {
        let request = match pending.take() {
            Some(request) => request,
            None => match requests.recv_timeout(Duration::from_millis(100)) {
                Ok(request) => request,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if shutdown.load(Ordering::Relaxed) != 0 {
                        break;
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
        };
        match request.command.as_str() {
            "quit" => break,
            "probe" => {
                let (names, detection) = microphone_users();
                emit(&json!({"users":names,"detection":detection}))?;
            }
            "stop" => worker.idle("")?,
            "discard" => {
                worker.discard();
                worker.idle("")?;
            }
            "sample" => pending = worker.record(&request, &requests)?,
            "live" => pending = worker.live(&request, &requests)?,
            "replay" => pending = worker.replay(&request, &requests)?,
            _ => worker.idle("Unknown microphone test command")?,
        }
        if shutdown.load(Ordering::Relaxed) != 0 {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(values: &[i16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn levels_come_from_the_samples_and_survive_a_split_frame() {
        let mut meter = Meter::default();
        let now = Instant::now();
        for byte in samples(&[0, 8192, -16384]) {
            meter.feed(&[byte], now);
        }
        let (level, peak, clipping) = meter.take(now);
        assert!((peak - 0.5).abs() < 0.001, "peak was {peak}");
        assert!((level - peak.sqrt()).abs() < 0.001, "level was {level}");
        assert!(!clipping);
        // The window is emptied by reading it, so a quiet frame reads quiet.
        assert_eq!(meter.take(now).1, 0.0);
    }

    #[test]
    fn clipping_needs_a_run_and_then_stays_visible_for_its_hold() {
        let now = Instant::now();
        let mut single = Meter::default();
        single.feed(&samples(&[i16::MAX, 0, i16::MIN, 0]), now);
        assert!(!single.clipping(now), "isolated peaks are not clipping");
        let mut sustained = Meter::default();
        sustained.feed(&samples(&[i16::MAX, i16::MAX]), now);
        assert!(sustained.clipping(now));
        // The meter is compressed and the indicator is not: a run at the top
        // of the scale reports clipping whatever the bar happens to draw.
        assert!(sustained.take(now).2);
        assert!(sustained.clipping(now + CLIP_HOLD - Duration::from_millis(1)));
        assert!(!sustained.clipping(now + CLIP_HOLD));
        assert!(clipped(&samples(&[i16::MIN, i16::MIN])));
        assert!(!clipped(&samples(&[32_000, 32_000])));
    }

    #[test]
    fn five_seconds_is_measured_in_samples_rather_than_in_wall_time() {
        assert_eq!(SAMPLE_BYTES, 480_000);
        assert_eq!(millis(SAMPLE_BYTES), SAMPLE_MILLIS);
        assert_eq!(millis(SAMPLE_BYTES / 2), SAMPLE_MILLIS / 2);
        assert_eq!(millis(0), 0);
    }

    #[test]
    fn monitor_sources_are_recognized_through_every_encoding() {
        assert!(is_monitor(&json!({"name":"alsa_output.pci-1.monitor"})));
        assert!(is_monitor(
            &json!({"name":"mic","monitor_of_sink":"speakers"})
        ));
        assert!(is_monitor(&json!({"name":"mic","monitor_of_sink":7})));
        assert!(!is_monitor(&json!({"name":"mic"})));
        assert!(!is_monitor(&json!({"name":"mic","monitor_of_sink":"n/a"})));
        assert!(!is_monitor(
            &json!({"name":"mic","monitor_of_sink":4_294_967_295_u32})
        ));
    }

    #[test]
    fn device_names_reaching_the_audio_server_are_bounded() {
        assert!(plausible(
            "alsa_input.pci-0000_00_1f.3-platform.analog-stereo"
        ));
        assert!(plausible("bluez_output.AA_BB_CC_DD_EE_FF.1"));
        for name in ["", "-d", "a b", "a;b", "a$b", &"a".repeat(129)] {
            assert!(!plausible(name), "{name} should be rejected");
        }
    }

    #[test]
    fn application_names_are_bounded_and_never_empty() {
        assert_eq!(
            application(&json!({"application.name":"Firefox","media.name":"Record"})),
            "Firefox"
        );
        assert_eq!(
            application(&json!({"application.process.binary":"zoom"})),
            "zoom"
        );
        assert_eq!(
            application(&json!({"application.name":"  "})),
            "An application"
        );
        assert_eq!(application(&json!({})), "An application");
        assert_eq!(
            application(&json!({"application.name":"a\u{200b}b\nc"})),
            "abc"
        );
        assert_eq!(
            application(&json!({"application.name":"x".repeat(200)})).len(),
            64
        );
    }

    #[test]
    fn the_test_excludes_its_own_streams_corked_streams_and_monitors() {
        let sources = [
            json!({"index":1,"name":"mic"}),
            json!({"index":2,"name":"speakers.monitor","monitor_of_sink":"speakers"}),
        ];
        let streams = [
            json!({"source":1,"corked":false,"properties":{"application.name":"Firefox"}}),
            json!({"source":1,"corked":true,"properties":{"application.name":"Paused"}}),
            json!({"source":2,"corked":false,"properties":{"application.name":"Loopback"}}),
            json!({"source":1,"corked":false,"properties":{"application.name":CLIENT_NAME}}),
            json!({"source":1,"corked":false,"properties":{"media.name":STREAM_NAME}}),
            json!({"source":1,"corked":false,"properties":{"application.name":"Firefox"}}),
        ];
        assert_eq!(users(&sources, &streams), ["Firefox"]);
        assert!(users(&sources, &[]).is_empty());
    }

    #[test]
    fn playback_names_one_device_and_never_a_default() {
        let arguments = playback_arguments("speakers");
        assert_eq!(arguments[0], "--playback");
        assert!(arguments.contains(&"--device=speakers".to_owned()));
        assert!(arguments.contains(&format!("--client-name={CLIENT_NAME}")));
        assert!(arguments.contains(&format!("--latency-msec={LATENCY_MILLIS}")));
        assert!(
            !arguments.iter().any(|argument| argument.contains("DEFAULT")
                || argument.contains("default")),
            "the test stream must never name a default output"
        );
        assert_eq!(stream_arguments("mic")[0], "--raw");
        assert!(stream_arguments("mic").contains(&"--device=mic".to_owned()));
    }

    #[test]
    fn a_session_ends_for_the_shutdown_signal_as_well_as_its_own_stop() {
        let global = Arc::new(AtomicUsize::new(0));
        let stop = Stop::new(&global);
        assert!(!stop.is_cancelled());
        stop.full.store(true, Ordering::Relaxed);
        assert!(stop.is_cancelled(), "a complete capture retires itself");
        assert!(!stop.ended(), "completion is not cancellation");
        global.store(15, Ordering::Relaxed);
        assert!(stop.ended());
        let local = Stop::new(&Arc::new(AtomicUsize::new(0)));
        local.end();
        assert!(local.ended() && local.is_cancelled());
    }

    #[test]
    fn a_stalled_output_costs_one_buffer_rather_than_a_blocked_capture() {
        let (read, mut write) = pipe().unwrap();
        let mut written = 0;
        // A reader that never reads costs one bounded buffer and then refuses
        // further audio, instead of blocking the capture that feeds it.
        for _ in 0..256 {
            match write.write(&[0u8; 4096]) {
                Ok(count) => written += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("{error}"),
            }
        }
        assert!(written > 0, "the pipe accepted nothing");
        assert!(
            written <= 1024 * 1024,
            "the pipe buffered {written} bytes of live audio"
        );
        drop(read);
    }

    #[test]
    fn an_unusable_device_name_never_reaches_the_audio_server() {
        assert!(resolve_sink("-d").is_err());
        assert!(resolve("mic; rm", "speakers").is_err());
    }
}
