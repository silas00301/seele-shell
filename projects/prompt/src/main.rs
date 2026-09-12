//! Resident prompt controller. Presentation remains QML; context access, child
//! lifetime and session cleanup are owned by bounded native workers.
mod context;

use seele_runtime::process::{capture, capture_with_stdout, discard, discard_detaching, Limits};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tempfile::{NamedTempFile, TempDir};

const MAX_PROMPT: usize = 16_384;
const MAX_CONTEXT: usize = 65_536;
const MAX_ANSWER: usize = 262_144;

type Job = Box<dyn FnOnce() + Send>;
struct Pool {
    send: Option<mpsc::SyncSender<Job>>,
    workers: Vec<thread::JoinHandle<()>>,
}
impl Pool {
    fn new() -> Self {
        let (send, receive) = mpsc::sync_channel::<Job>(16);
        let receive = Arc::new(Mutex::new(receive));
        let workers = (0..4)
            .map(|_| {
                let receive = receive.clone();
                thread::spawn(move || loop {
                    let job = receive.lock().unwrap().recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                })
            })
            .collect();
        Self {
            send: Some(send),
            workers,
        }
    }
    fn cleanup(&self, job: impl FnOnce() + Send + 'static) {
        if let Err(error) = self.send.as_ref().unwrap().try_send(Box::new(job)) {
            match error {
                mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job(),
            }
        }
    }
    fn queue(&self, job: impl FnOnce() + Send + 'static) -> bool {
        self.send.as_ref().unwrap().try_send(Box::new(job)).is_ok()
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        self.send.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
struct State {
    id: u64,
    active: bool,
    busy: bool,
    window: Value,
    identity: Option<context::ProcessIdentity>,
    output: String,
    answer_generation: u64,
    directory: String,
    cached: HashMap<String, String>,
    tokens: HashMap<String, u64>,
    screen: Option<NamedTempFile>,
    session: String,
    home: Option<Arc<seele_runtime::codex::Context>>,
    model: String,
    answer: String,
    cancel: Arc<AtomicUsize>,
}
struct Shared {
    state: Mutex<State>,
    codex: seele_runtime::codex::Toolless,
    runtime: TempDir,
    workspace: PathBuf,
    stdout: Mutex<seele_runtime::wire::NonblockingFile>,
    stop: Arc<AtomicUsize>,
}
impl Shared {
    fn valid(&self, id: u64) -> bool {
        let state = self.state.lock().unwrap();
        state.active && state.id == id
    }
    fn emit(&self, event: &str, id: u64, mut data: Value) {
        data["event"] = json!(event);
        data["id"] = json!(id);
        let Ok(mut bytes) = serde_json::to_vec(&data) else {
            return;
        };
        bytes.push(b'\n');
        let mut stdout = self.stdout.lock().unwrap();
        if seele_runtime::wire::write_bytes(
            &mut *stdout,
            &bytes,
            Duration::from_secs(5),
            &self.stop,
        )
        .is_err()
        {
            self.stop.store(1, Ordering::Relaxed);
        }
    }
    fn error(&self, event: &str, id: u64, message: &str) {
        self.emit(event, id, json!({"message":message}));
    }
    fn retire(&self) -> (String, Option<Arc<seele_runtime::codex::Context>>) {
        let mut state = self.state.lock().unwrap();
        state.cancel.store(1, Ordering::Relaxed);
        let session = std::mem::take(&mut state.session);
        let home = state.home.take();
        *state = State::default();
        (session, home)
    }
    fn delete(&self, session: &str, home: Option<&seele_runtime::codex::Context>) {
        let Some(home) = home else {
            return;
        };
        if !context::session(session) {
            return;
        }
        for delay in [0, 150, 500] {
            thread::sleep(Duration::from_millis(delay));
            let mut command = binary("CODEX", "codex");
            command.args(["delete", "--force", session]);
            home.environment(&mut command);
            if discard(&mut command, b"", limits(5, 0), &AtomicUsize::new(0))
                .is_ok_and(|status| status.success())
            {
                return;
            }
        }
        eprintln!("seele-ai-prompt: session cleanup failed");
    }
}
fn binary(name: &str, fallback: &str) -> Command {
    Command::new(
        std::env::var_os(format!("SEELE_SHELL_{name}"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| fallback.into()),
    )
}
fn limits(seconds: u64, output: usize) -> Limits {
    Limits {
        timeout: Duration::from_secs(seconds),
        output,
    }
}
fn text(value: &Value, maximum: usize) -> String {
    value.as_str().unwrap_or("").chars().take(maximum).collect()
}
fn positive(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .filter(|value| *value > 0 && *value <= i64::MAX as u64)
}

fn preview(shared: &Arc<Shared>, message: Value) {
    let (id, kind, token) = (
        message["id"].as_u64().unwrap_or(0),
        text(&message["kind"], 20),
        message["token"].as_u64().unwrap_or(0),
    );
    let (cancel, window, output) = {
        let state = shared.state.lock().unwrap();
        if !state.active || state.id != id || state.tokens.get(&kind) != Some(&token) {
            return;
        }
        (
            state.cancel.clone(),
            state.window.clone(),
            state.output.clone(),
        )
    };
    let mut screenshot = None;
    let mut value = String::new();
    let mut response = json!({"kind":kind,"token":token,"available":true});
    let result: seele_runtime::Result = (|| {
        match kind.as_str() {
            "dir" => {
                value = context::terminal_directory(&window);
                response["preview"] = json!(value);
                response["available"] = json!(!value.is_empty());
            }
            "clip" | "select" => {
                let mut command = binary("WL_PASTE", "wl-paste");
                if kind == "select" {
                    command.arg("--primary");
                }
                let result = capture(
                    command.args(["--no-newline", "--type", "text"]),
                    b"",
                    limits(3, MAX_CONTEXT * 4 + 4096),
                    &cancel,
                )?;
                if !result.status.success() {
                    return Err("No text is available".into());
                }
                let raw = String::from_utf8_lossy(&result.stdout);
                value = raw.chars().take(MAX_CONTEXT).collect();
                response["preview"] = json!(value.chars().take(280).collect::<String>());
                response["text"] = json!(value);
                response["characters"] = json!(value.chars().count());
                response["truncated"] = json!(raw.len() > value.len());
            }
            "screen" => {
                if output.is_empty() {
                    return Err("The active output is unavailable".into());
                }
                let target = tempfile::Builder::new()
                    .prefix("screen-")
                    .suffix(".png")
                    .tempfile_in(shared.runtime.path())?;
                if !discard(
                    binary("GRIM", "grim")
                        .arg("-o")
                        .arg(output)
                        .arg(target.path()),
                    b"",
                    limits(12, 0),
                    &cancel,
                )?
                .success()
                    || target.as_file().metadata()?.len() == 0
                    || target.as_file().metadata()?.len() > 64 * 1024 * 1024
                {
                    return Err("Could not capture the active output".into());
                }
                response["path"] = json!(target.path());
                screenshot = Some(target);
            }
            _ => return Err("Unknown context source".into()),
        }
        Ok(())
    })();
    let mut state = shared.state.lock().unwrap();
    if !state.active
        || state.id != id
        || state.tokens.get(&kind) != Some(&token)
        || !Arc::ptr_eq(&state.cancel, &cancel)
    {
        return;
    }
    if result.is_err() {
        state.cached.remove(&kind);
        drop(state);
        response["message"] = json!(if kind == "screen" {
            "Could not capture the active output"
        } else {
            "No text is available"
        });
        shared.emit("context-error", id, response);
        return;
    }
    if kind == "screen" {
        state.screen = screenshot;
    } else if kind == "dir" {
        state.directory = value;
    } else {
        state.cached.insert(kind, value);
    }
    drop(state);
    shared.emit("preview", id, response);
}

struct Turn {
    id: u64,
    request: u64,
    prompt: String,
    session: String,
    home: Option<Arc<seele_runtime::codex::Context>>,
    model: String,
    screen: Option<NamedTempFile>,
    cancel: Arc<AtomicUsize>,
}
fn turn(shared: &Arc<Shared>, turn: Turn) {
    let Turn {
        id,
        request,
        prompt,
        mut session,
        mut home,
        mut model,
        screen,
        cancel,
    } = turn;
    let mut answer = String::new();
    let mut failure = "Codex did not return an answer. Try again.";
    let result: seele_runtime::Result = (|| {
        if !shared.valid(id) {
            return Err("cancelled".into());
        }
        let output = tempfile::Builder::new()
            .prefix("answer-")
            .tempfile_in(shared.runtime.path())?;
        if model.is_empty() {
            model = seele_runtime::inference::configured_model(
                &seele_runtime::inference::default_socket(),
                &cancel,
            )?;
        }
        if home.is_none() {
            home = Some(Arc::new(shared.codex.context().inspect_err(|_| {
                failure = "Codex authentication is unavailable. Check the AI account controls.";
            })?));
        }
        let mut command = shared.codex.command(home.as_deref().unwrap(),seele_runtime::codex::Options {
            workspace: &shared.workspace,
            resume: !session.is_empty(),
            ephemeral: false,
            instructions: "Give a direct, compact answer suitable for the desktop prompt. Use only the explicit text and image context supplied with this conversation.",
        }, &cancel)?;
        command.arg("--model").arg(&model);
        command.arg("--output-last-message").arg(output.path());
        if let Some(screen) = &screen {
            command.arg("--image").arg(screen.path());
        }
        if !session.is_empty() {
            command.arg(&session);
        }
        command
            .arg("-")
            .current_dir(&shared.workspace)
            .env("NO_COLOR", "1");
        let mut pending = Vec::new();
        let result = capture_with_stdout(
            &mut command,
            prompt.as_bytes(),
            limits(180, MAX_ANSWER * 8),
            &cancel,
            |bytes| {
                pending.extend_from_slice(bytes);
                while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                    if let Ok(event) = serde_json::from_slice::<Value>(&pending[..end]) {
                        if seele_runtime::codex::tool_event(&event) {
                            return Err(io::ErrorKind::PermissionDenied.into());
                        }
                        context::event_data(&event, &mut session, &mut answer);
                    }
                    pending.drain(..=end);
                }
                if pending.len() > MAX_ANSWER * 2 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Ok(())
            },
        );
        // Recover a final unterminated event too, including on cancellation.
        if let Ok(event) = serde_json::from_slice::<Value>(&pending) {
            context::event_data(&event, &mut session, &mut answer);
        }
        let result = result?;
        if !result.status.success() {
            return Err("Codex did not return an answer".into());
        }
        let mut bytes = Vec::new();
        File::open(output.path())?
            .take((MAX_ANSWER * 4 + 1) as u64)
            .read_to_end(&mut bytes)?;
        if !bytes.is_empty() {
            answer = String::from_utf8_lossy(&bytes)
                .chars()
                .take(MAX_ANSWER)
                .collect();
        }
        answer = answer.trim().chars().take(MAX_ANSWER).collect();
        if answer.is_empty() {
            return Err("Codex did not return an answer".into());
        }
        Ok(())
    })();
    drop(screen);
    let mut state = shared.state.lock().unwrap();
    let current = state.active && state.id == id && Arc::ptr_eq(&state.cancel, &cancel);
    if current {
        state.busy = false;
        state.session = session.clone();
        state.home = home.clone();
        state.model = model;
    }
    if current && result.is_ok() {
        state.answer = answer.clone();
    }
    drop(state);
    if !current {
        shared.delete(&session, home.as_deref());
    } else if result.is_err() {
        shared.emit("error", id, json!({"request":request,"message":failure}));
    } else {
        shared.emit(
            "answer",
            id,
            json!({"request":request,"text":answer,"resumable":!session.is_empty()}),
        );
    }
}

struct QueuedAction {
    id: u64,
    insert: bool,
    answer: String,
    window: Value,
    identity: Option<context::ProcessIdentity>,
    generation: u64,
    cancel: Arc<AtomicUsize>,
}
impl QueuedAction {
    fn snapshot(state: &State, id: u64, insert: bool) -> Option<Self> {
        (state.active && state.id == id && !state.answer.is_empty()).then(|| Self {
            id,
            insert,
            answer: state.answer.clone(),
            window: state.window.clone(),
            identity: state.identity.clone(),
            generation: state.answer_generation,
            cancel: state.cancel.clone(),
        })
    }
    fn matches(&self, state: &State) -> bool {
        state.active
            && state.id == self.id
            && Arc::ptr_eq(&state.cancel, &self.cancel)
            && state.answer_generation == self.generation
            && state.answer == self.answer
    }
    fn valid(&self, shared: &Shared) -> bool {
        self.matches(&shared.state.lock().unwrap())
    }
}
fn action(shared: &Arc<Shared>, queued: QueuedAction) {
    if !queued.valid(shared) {
        return;
    }
    let (id, insert, answer, window, cancel) = (
        queued.id,
        queued.insert,
        &queued.answer,
        &queued.window,
        &queued.cancel,
    );
    let result: seele_runtime::Result = (|| {
        if !insert {
            if !discard_detaching(
                binary("WL_COPY", "wl-copy").args(["--type", "text/plain;charset=utf-8"]),
                answer.as_bytes(),
                limits(5, 0),
                cancel,
            )?
            .success()
            {
                return Err("copy failed".into());
            }
            return Ok(());
        }
        let address = window["address"].as_str().unwrap_or("");
        let pid = window["pid"]
            .as_u64()
            .filter(|pid| *pid > 1 && *pid <= i32::MAX as u64)
            .ok_or("invalid window")?;
        if !address.strip_prefix("0x").is_some_and(|part| {
            !part.is_empty()
                && part.len() <= 62
                && part.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) || answer.chars().count() > MAX_CONTEXT
            || answer.contains('\0')
        {
            return Err("invalid insertion".into());
        }
        if queued.identity.is_none() || context::process_identity(pid) != queued.identity {
            return Err("source application changed".into());
        }
        let call = format!("hl.dsp.focus({{ window = \"address:{address}\" }})");
        if !discard(
            binary("HYPRCTL", "hyprctl").args(["dispatch", &call]),
            b"",
            limits(3, 0),
            cancel,
        )?
        .success()
        {
            return Err("focus failed".into());
        }
        let mut matched = false;
        for _ in 0..12 {
            let output = capture(
                binary("HYPRCTL", "hyprctl").args(["activewindow", "-j"]),
                b"",
                limits(2, 64 * 1024),
                cancel,
            )?;
            let active: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
            if output.status.success()
                && active["address"]
                    .as_str()
                    .is_some_and(|a| a.eq_ignore_ascii_case(address))
                && active["pid"].as_u64() == Some(pid)
            {
                matched = true;
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        if !matched
            || cancel.load(Ordering::Relaxed) != 0
            || !queued.valid(shared)
            || context::process_identity(pid) != queued.identity
        {
            return Err("focus changed".into());
        }
        // wtype's stdin mode keeps the answer out of /proc/<pid>/cmdline.
        if !discard(
            binary("WTYPE", "wtype").arg("-"),
            answer.as_bytes(),
            limits(15, 0),
            cancel,
        )?
        .success()
        {
            return Err("insertion failed".into());
        }
        Ok(())
    })();
    if cancel.load(Ordering::Relaxed) != 0 || !queued.valid(shared) {
        return;
    }
    if result.is_ok() {
        shared.emit(if insert { "inserted" } else { "copied" }, id, json!({}));
    } else {
        shared.error(
            "action-error",
            id,
            if insert {
                "Could not restore the original window"
            } else {
                "Could not copy the answer"
            },
        );
    }
}

fn handle(shared: &Arc<Shared>, pool: &Pool, message: Value) {
    let id = positive(&message["id"]).unwrap_or(0);
    let command = message["command"].as_str().unwrap_or("");
    if command == "open" {
        if id == 0 {
            shared.error("error", 0, "Invalid prompt generation");
            return;
        }
        let retired = shared.retire();
        if !retired.0.is_empty() {
            let shared = shared.clone();
            pool.cleanup(move || shared.delete(&retired.0, retired.1.as_deref()));
        }
        let window = &message["window"];
        let classes: Vec<_> = window["classes"]
            .as_array()
            .into_iter()
            .flatten()
            .take(8)
            .map(|value| text(value, 256))
            .collect();
        let output = text(&message["screen"], 129);
        let output = if !output.is_empty()
            && output.len() <= 128
            && output
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
        {
            output
        } else {
            String::new()
        };
        let mut state = shared.state.lock().unwrap();
        state.id = id;
        state.active = true;
        state.output = output.clone();
        state.identity = window["pid"].as_u64().and_then(context::process_identity);
        state.window = json!({"address":text(&window["address"],64), "title":text(&window["title"],1024), "app":text(&window["app"],256), "classes":classes,"pid":window["pid"].as_u64().unwrap_or(0)});
        let window = json!({"title":state.window["title"],"app":state.window["app"]});
        drop(state);
        shared.emit("opened", id, json!({"window":window,"screen":output}));
        return;
    }
    if command == "close" {
        if shared.valid(id) {
            let retired = shared.retire();
            if !retired.0.is_empty() {
                let shared = shared.clone();
                pool.cleanup(move || shared.delete(&retired.0, retired.1.as_deref()));
            }
        }
        return;
    }
    if !shared.valid(id) {
        return;
    }
    match command {
        "forget" => {
            let kind = message["kind"].as_str().unwrap_or("");
            let mut state = shared.state.lock().unwrap();
            state.tokens.remove(kind);
            state.cached.remove(kind);
            if kind == "screen" {
                state.screen.take();
            } else if kind == "dir" {
                state.directory.clear();
            }
        }
        "preview" => {
            let kind = message["kind"].as_str().unwrap_or("");
            let Some(token) =
                positive(&message["token"]).filter(|_| context::KINDS.contains(&kind))
            else {
                shared.emit(
                    "context-error",
                    id,
                    json!({"kind":kind,"token":0,"message":"Invalid context request"}),
                );
                return;
            };
            {
                let mut state = shared.state.lock().unwrap();
                state.tokens.insert(kind.into(), token);
                state.cached.remove(kind);
                if kind == "screen" {
                    state.screen.take();
                } else if kind == "dir" {
                    state.directory.clear();
                }
            }
            let work = shared.clone();
            if !pool.queue(move || preview(&work, message)) {
                shared.error("error", id, "Context workers are busy. Try again.");
            }
        }
        "submit" => {
            let request = positive(&message["request"]).unwrap_or(0);
            let prompt = message["prompt"].as_str().unwrap_or("");
            if request == 0 || prompt.trim().is_empty() || prompt.chars().count() > MAX_PROMPT {
                shared.error("error", id, "Enter a shorter prompt");
                return;
            }
            let mentions = context::mentions(prompt);
            let permissions: HashSet<_> = message["permissions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let mut state = shared.state.lock().unwrap();
            if state.busy {
                drop(state);
                shared.error("error", id, "Codex is already answering");
                return;
            }
            for kind in ["clip", "select"] {
                if mentions.contains(&kind)
                    && (!permissions.contains(kind) || !state.cached.contains_key(kind))
                {
                    drop(state);
                    shared.emit("permission", id, json!({"request":request,"kind":kind}));
                    return;
                }
            }
            let mut contexts = state.cached.clone();
            if mentions.contains(&"window") {
                contexts.insert(
                    "window".into(),
                    format!(
                        "Application: {}\nTitle: {}",
                        state.window["app"]
                            .as_str()
                            .unwrap_or("Unknown application"),
                        state.window["title"].as_str().unwrap_or("Untitled window")
                    ),
                );
            }
            if mentions.contains(&"dir") {
                if state.directory.is_empty() {
                    drop(state);
                    shared.error("error", id, "No focused terminal directory is available");
                    return;
                }
                contexts.insert("dir".into(), state.directory.clone());
            }
            if mentions.contains(&"screen") && state.screen.is_none() {
                drop(state);
                shared.error("error", id, "Capture the current output before sending");
                return;
            }
            let screen = state.screen.take().filter(|_| mentions.contains(&"screen"));
            let turn_request = Turn {
                id,
                request,
                prompt: context::prompt(prompt, &contexts),
                session: state.session.clone(),
                home: state.home.clone(),
                model: state.model.clone(),
                screen,
                cancel: state.cancel.clone(),
            };
            state.busy = true;
            state.answer_generation = state.answer_generation.wrapping_add(1);
            state.answer.clear();
            state.directory.clear();
            state.cached.clear();
            state.tokens.clear();
            drop(state);
            shared.emit("started", id, json!({"request":request}));
            let work = shared.clone();
            if !pool.queue(move || turn(&work, turn_request)) {
                shared.state.lock().unwrap().busy = false;
                shared.error("error", id, "Prompt workers are busy. Try again.");
            }
        }
        "copy" | "insert" => {
            let insert = command == "insert";
            let Some(queued) = QueuedAction::snapshot(&shared.state.lock().unwrap(), id, insert)
            else {
                return;
            };
            let work = shared.clone();
            if !pool.queue(move || action(&work, queued)) {
                shared.error("action-error", id, "Prompt workers are busy. Try again.");
            }
        }
        _ => shared.error("error", id, "Unknown command"),
    }
}

fn main() -> seele_runtime::Result {
    let stop = seele_runtime::process::termination_signal()?;
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let runtime = tempfile::Builder::new()
        .prefix("seele-ai-prompt-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(root)?;
    let workspace = runtime.path().join("workspace");
    seele_runtime::fs::private_directory(&workspace)?;
    let stdout = seele_runtime::wire::nonblocking_stdout()?;
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        codex: seele_runtime::codex::Toolless::new(
            binary("CODEX", "codex").get_program().into(),
            runtime.path().into(),
        ),
        runtime,
        workspace,
        stdout: Mutex::new(stdout),
        stop: stop.clone(),
    });
    let (send, receive) = mpsc::sync_channel(16);
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut frame = Vec::new();
        while let Ok(true) = seele_runtime::wire::read_frame(&mut stdin, &mut frame, 1024 * 1024) {
            let value = serde_json::from_slice::<Value>(&frame).unwrap_or(Value::Null);
            if send.send(value).is_err() {
                break;
            }
        }
    });
    let pool = Pool::new();
    while stop.load(Ordering::Relaxed) == 0 {
        match receive.recv_timeout(Duration::from_millis(50)) {
            Ok(message) if message.is_object() => handle(&shared, &pool, message),
            Ok(_) => shared.error("error", 0, "Invalid worker request"),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(_) => break,
        }
    }
    let session = shared.retire();
    drop(pool); // Joining the bounded workers also recovers partial session IDs.
    shared.delete(&session.0, session.1.as_deref());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_actions_are_bound_to_reviewed_answer_and_panel_generation() {
        let mut state = State {
            id: 1,
            active: true,
            answer: "reviewed answer".into(),
            ..State::default()
        };
        let queued = QueuedAction::snapshot(&state, 1, true).unwrap();
        assert!(queued.matches(&state));
        state.answer = "later answer".into();
        assert!(!queued.matches(&state));
        state.answer = "reviewed answer".into();
        state.answer_generation += 1;
        assert!(!queued.matches(&state));
        state.answer_generation = 0;
        state.cancel = Arc::new(AtomicUsize::new(0));
        assert!(!queued.matches(&state));
    }
}
