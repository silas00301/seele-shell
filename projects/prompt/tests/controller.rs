//! Real worker lifecycle against private local executable fixtures. No desktop,
//! clipboard, model account or live application is contacted.
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
struct Worker {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Value>,
    root: tempfile::TempDir,
    configuration: Configuration,
    terminal: Child,
}
struct Configuration {
    stop: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Configuration {
    fn new(root: &Path) -> Self {
        use std::os::unix::net::UnixListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let listener = UnixListener::bind(root.join("seele-codex.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let stopping = stop.clone();
        let counted = calls.clone();
        let worker = thread::spawn(move || {
            while stopping.load(Ordering::Relaxed) == 0 {
                let (stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                let request =
                    seele_runtime::wire::receive(&stream, 4096, Duration::from_secs(2), &stopping)
                        .unwrap();
                assert_eq!(request, json!({"op":"configuration"}));
                counted.fetch_add(1, Ordering::Relaxed);
                seele_runtime::wire::send(&stream,&json!({"ok":true,"epoch":"00000000-0000-4000-8000-000000000001","model":"gpt-5.6-luna"}),4096,Duration::from_secs(2),&stopping).unwrap();
            }
        });
        Self {
            stop,
            calls,
            worker: Some(worker),
        }
    }
}
impl Drop for Configuration {
    fn drop(&mut self) {
        self.stop.store(1, std::sync::atomic::Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn executable(root: &Path, name: &str, script: &str) {
    let path = root.join(name);
    let root_assignment = format!(
        "FIXTURE_ROOT=\"{}\"\nexport FIXTURE_ROOT\n",
        root.parent().unwrap().display()
    );
    let shell = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|path| path.join("sh"))
        .find(|path| path.is_file())
        .unwrap();
    let script = script.replacen(
        "#!/bin/sh\n",
        &format!("#!{}\n{root_assignment}", shell.display()),
        1,
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn await_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}
impl Worker {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        fs::create_dir(root.path().join("home")).unwrap();
        fs::create_dir(root.path().join("codex")).unwrap();
        let auth = root.path().join("codex/auth.json");
        fs::write(&auth, br#"{"OPENAI_API_KEY":"synthetic-test-only"}"#).unwrap();
        fs::set_permissions(auth, fs::Permissions::from_mode(0o600)).unwrap();
        let configuration = Configuration::new(root.path());
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let sleep = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join("sleep"))
            .find(|p| p.is_file())
            .unwrap();
        fs::copy(sleep, bin.join("ghostty")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let terminal = loop {
            match Command::new(bin.join("ghostty"))
                .arg("30")
                .current_dir(root.path())
                .spawn()
            {
                Ok(child) => break child,
                Err(error)
                    if error.raw_os_error() == Some(libc::ETXTBSY) && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("fixture terminal spawn failed: {error}"),
            }
        };
        executable(
            &bin,
            "codex",
            r#"#!/bin/sh
if [ "$1" = features ]; then printf 'shell_tool stable true\nskip_host_skill_discovery experimental false\n'; exit 0; fi
if [ "$1" = delete ]; then printf '%s\n' "$3" >> "$FIXTURE_ROOT/deleted"; exit 0; fi
input=$(cat)
printf '%s' "$input" > "$FIXTURE_ROOT/prompt"
mode=first
while [ "$#" -gt 0 ]; do
  case "$1" in --output-last-message) output=$2; shift;; resume) mode=resume;; esac
  shift
done
case "$input" in *BLOCK*) session=00000000-0000-4000-8000-000000000024;; *) session=00000000-0000-4000-8000-000000000023;; esac
printf '{"type":"thread.started","thread_id":"%s"}\n' "$session"
case "$input" in *BLOCK*) : > "$FIXTURE_ROOT/blocked"; sleep 30;; esac
if [ "$mode" = resume ]; then printf 'Follow-up answer' > "$output"; else printf 'Initial answer' > "$output"; fi
"#,
        );
        executable(
            &bin,
            "wl-paste",
            r#"#!/bin/sh
: > "$FIXTURE_ROOT/clipboard-read"
if [ -e "$FIXTURE_ROOT/fail-clip" ]; then exit 1; fi
printf 'clipboard text\n$(touch /tmp/never)'
"#,
        );
        executable(
            &bin,
            "grim",
            r#"#!/bin/sh
if [ -e "$FIXTURE_ROOT/fail-screen" ]; then exit 1; fi
for output do :; done
printf '\211PNG\r\n\032\nfixture' > "$output"
"#,
        );
        executable(
            &bin,
            "wl-copy",
            r#"#!/bin/sh
cat > "$FIXTURE_ROOT/copied"
"#,
        );
        executable(
            &bin,
            "wtype",
            r#"#!/bin/sh
printf '%s\n' "$@" > "$FIXTURE_ROOT/wtype-args"
cat > "$FIXTURE_ROOT/typed"
"#,
        );
        executable(
            &bin,
            "hyprctl",
            r#"#!/bin/sh
if [ "$1" = activewindow ]; then printf '{"address":"0xabc123","pid":%s}\n' "$SOURCE_PID"; fi
"#,
        );
        let stderr = fs::File::create(root.path().join("stderr")).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_seele-ai-prompt-worker"));
        command
            .env("XDG_RUNTIME_DIR", root.path())
            .env("HOME", root.path().join("home"))
            .env("CODEX_HOME", root.path().join("codex"))
            .env("FIXTURE_ROOT", root.path())
            .env("SOURCE_PID", terminal.id().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr);
        for (key, name) in [
            ("CODEX", "codex"),
            ("WL_PASTE", "wl-paste"),
            ("WL_COPY", "wl-copy"),
            ("GRIM", "grim"),
            ("HYPRCTL", "hyprctl"),
            ("WTYPE", "wtype"),
        ] {
            command.env(format!("SEELE_SHELL_{key}"), bin.join(name));
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = serde_json::from_str(&line) {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            child,
            input,
            events,
            root,
            configuration,
            terminal,
        }
    }
    fn send(&mut self, message: Value) {
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }
    fn event(&self, name: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let value = self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| {
                    panic!(
                        "missing {name}: {}",
                        fs::read_to_string(self.root.path().join("stderr")).unwrap_or_default()
                    )
                });
            if name == "answer" && value["event"] == "error" {
                panic!(
                    "answer failed: {value}; {}",
                    format_args!(
                        "{}\n{}",
                        fs::read_to_string(self.root.path().join("codex-errors"))
                            .unwrap_or_default(),
                        fs::read_to_string(self.root.path().join("codex-events"))
                            .unwrap_or_default()
                    )
                );
            }
            if value["event"] == name {
                return value;
            }
        }
    }
    fn open(&mut self, id: u64) {
        self.send(json!({"command":"open","id":id,"screen":"DP-1","window":{"address":"0xabc123","pid":self.terminal.id(),"app":"Ghostty","classes":["com.mitchellh.ghostty"],"title":"Private title"}}));
        self.event("opened");
    }
    fn stop(&mut self) {
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("worker shutdown timed out");
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            self.stop();
        }
        let _ = self.terminal.kill();
        let _ = self.terminal.wait();
    }
}
#[test]
fn privacy_context_consumption_session_reuse_actions_and_cleanup() {
    let mut worker = Worker::new();
    worker.open(1);
    assert!(!worker.root.path().join("clipboard-read").exists());
    assert_eq!(
        worker
            .configuration
            .calls
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert!(!worker.root.path().join("prompt").exists());
    worker.send(json!({"command":"preview","id":1,"kind":"dir","token":1}));
    assert_eq!(
        worker.event("preview")["preview"],
        worker.root.path().to_str().unwrap()
    );
    worker.send(
        json!({"command":"submit","id":1,"request":1,"prompt":"Explain @clip","permissions":[]}),
    );
    assert_eq!(worker.event("permission")["kind"], "clip");
    assert!(!worker.root.path().join("prompt").exists());
    worker.send(json!({"command":"preview","id":1,"kind":"clip","token":2}));
    assert_eq!(
        worker.event("preview")["text"],
        "clipboard text\n$(touch /tmp/never)"
    );
    worker.send(json!({"command":"preview","id":1,"kind":"screen","token":3}));
    let screen = PathBuf::from(worker.event("preview")["path"].as_str().unwrap());
    assert!(screen.is_file());
    assert_eq!(
        fs::metadata(&screen).unwrap().permissions().mode() & 0o777,
        0o600
    );
    worker.send(json!({"command":"submit","id":1,"request":2,"prompt":"Explain @clip and @window from @dir with @screen","permissions":["clip"]}));
    worker.event("started");
    assert_eq!(worker.event("answer")["text"], "Initial answer");
    assert!(!screen.exists());
    let prompt = fs::read_to_string(worker.root.path().join("prompt")).unwrap();
    assert!(prompt.contains("\"clipboard text\\n$(touch /tmp/never)\""));
    assert!(prompt.contains("Application: Ghostty\\nTitle: Private title"));
    worker
        .send(json!({"command":"submit","id":1,"request":3,"prompt":"Follow up","permissions":[]}));
    worker.event("started");
    assert_eq!(worker.event("answer")["text"], "Follow-up answer");
    assert_eq!(
        worker
            .configuration
            .calls
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    worker.send(json!({"command":"copy","id":1}));
    worker.event("copied");
    assert_eq!(
        fs::read_to_string(worker.root.path().join("copied")).unwrap(),
        "Follow-up answer"
    );
    worker.send(json!({"command":"insert","id":1}));
    worker.event("inserted");
    assert_eq!(
        fs::read_to_string(worker.root.path().join("typed")).unwrap(),
        "Follow-up answer"
    );
    assert_eq!(
        fs::read_to_string(worker.root.path().join("wtype-args")).unwrap(),
        "-\n"
    );
    worker.send(json!({"command":"close","id":1}));
    await_file(&worker.root.path().join("deleted"));
    worker.stop();
    assert!(!fs::read_dir(worker.root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("seele-ai-prompt-")));
}
#[test]
fn failed_refresh_cannot_submit_old_screen_or_clipboard() {
    let mut worker = Worker::new();
    worker.open(1);
    worker.send(json!({"command":"preview","id":1,"kind":"screen","token":1}));
    let screen = PathBuf::from(worker.event("preview")["path"].as_str().unwrap());
    fs::write(worker.root.path().join("fail-screen"), "").unwrap();
    worker.send(json!({"command":"preview","id":1,"kind":"screen","token":2}));
    worker.event("context-error");
    assert!(!screen.exists());
    worker.send(
        json!({"command":"submit","id":1,"request":1,"prompt":"Explain @screen","permissions":[]}),
    );
    assert!(worker.event("error")["message"]
        .as_str()
        .unwrap()
        .contains("Capture"));
    worker.send(json!({"command":"preview","id":1,"kind":"clip","token":3}));
    worker.event("preview");
    fs::write(worker.root.path().join("fail-clip"), "").unwrap();
    worker.send(json!({"command":"preview","id":1,"kind":"clip","token":4}));
    worker.event("context-error");
    worker.send(json!({"command":"submit","id":1,"request":2,"prompt":"Explain @clip","permissions":["clip"]}));
    worker.event("permission");
    assert!(!worker.root.path().join("prompt").exists());
}
#[test]
fn close_and_shutdown_recover_partial_session_identity() {
    for shutdown in [false, true] {
        let mut worker = Worker::new();
        worker.open(1);
        worker.send(json!({"command":"submit","id":1,"request":1,"prompt":"BLOCK until cancelled","permissions":[]}));
        worker.event("started");
        await_file(&worker.root.path().join("blocked"));
        if shutdown {
            worker.stop();
        } else {
            worker.send(json!({"command":"close","id":1}));
        }
        await_file(&worker.root.path().join("deleted"));
        assert!(fs::read_to_string(worker.root.path().join("deleted"))
            .unwrap()
            .contains("00000000-0000-4000-8000-000000000024"));
    }
}

#[test]
fn insertion_rejects_a_closed_or_replaced_source_process() {
    let mut worker = Worker::new();
    worker.open(1);
    worker.send(json!({"command":"submit","id":1,"request":1,"prompt":"Answer","permissions":[]}));
    worker.event("answer");
    worker.terminal.kill().unwrap();
    worker.terminal.wait().unwrap();
    worker.send(json!({"command":"insert","id":1}));
    worker.event("action-error");
    assert!(!worker.root.path().join("typed").exists());
}

/// Optional check against a real installed Codex. All authentication/configuration
/// is private and the only provider is a loopback HTTP fixture.
#[test]
fn real_codex_first_and_resumed_image_turns_have_no_tools() {
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let Some(real) = std::env::var_os("SEELE_TEST_CODEX") else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let quit = Arc::new(AtomicBool::new(false));
    let stop = quit.clone();
    let (send, requests) = mpsc::channel();
    let server = thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut length = 0;
            let mut headers = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                headers += line.len();
                assert!(headers < 65536);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            assert!(length < 2 * 1024 * 1024);
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            let response = if first.starts_with("POST ") {
                send.send(serde_json::from_slice::<Value>(&bytes).unwrap())
                    .unwrap();
                let events = [
                    json!({"type":"response.created","response":{"id":"fixture"}}),
                    json!({"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"42"}]}}),
                    json!({"type":"response.completed","response":{"id":"fixture","status":"completed","output":[],"usage":{"input_tokens":2,"output_tokens":1,"total_tokens":3}}}),
                ];
                events
                    .iter()
                    .map(|event| format!("data: {event}\n\n"))
                    .collect::<String>()
            } else {
                "{\"models\":[]}".into()
            };
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
    });
    struct Stop(Arc<AtomicBool>, Option<thread::JoinHandle<()>>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
            self.1.take().unwrap().join().unwrap();
        }
    }
    let _server = Stop(quit, Some(server));
    let mut worker = Worker::new();
    let marker = worker.root.path().join("mcp-ran");
    fs::write(worker.root.path().join("codex/config.toml"), format!("developer_instructions=\"HOST_CONFIG_MUST_NOT_LEAK\"\n[mcp_servers.fixture]\ncommand=\"sh\"\nargs=[\"-c\",\"touch {}\"]\n", marker.display())).unwrap();
    fs::write(
        worker.root.path().join("codex/AGENTS.md"),
        "HOST_INSTRUCTIONS_MUST_NOT_LEAK",
    )
    .unwrap();
    let quoted_real = real.to_str().unwrap().replace('\'', "'\"'\"'");
    let script = format!(
        r#"#!/bin/sh
if [ "$1" = delete ]; then printf '%s\n' "$3" > "$FIXTURE_ROOT/deleted"; fi
if [ "$1" = exec ]; then
export HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9 NO_PROXY=127.0.0.1,localhost
export http_proxy=$HTTP_PROXY https_proxy=$HTTPS_PROXY all_proxy=$ALL_PROXY no_proxy=$NO_PROXY
remaining=$#
while [ "$remaining" -gt 1 ]; do value=$1; shift; set -- "$@" "$value"; remaining=$((remaining - 1)); done
last=$1; shift
exec '{quoted_real}' "$@" -c 'model_provider="fixture"' -c 'model_providers.fixture.name="fixture"' -c 'model_providers.fixture.base_url="http://127.0.0.1:{port}"' -c 'model_providers.fixture.wire_api="responses"' -c 'model_providers.fixture.requires_openai_auth=false' "$last" 2>> "$FIXTURE_ROOT/codex-errors"
fi
exec '{quoted_real}' "$@"
"#
    );
    executable(&worker.root.path().join("bin"), "codex", &script);
    worker.open(1);
    for request in 1..=2 {
        worker.send(json!({"command":"preview","id":1,"kind":"screen","token":request}));
        let image = PathBuf::from(worker.event("preview")["path"].as_str().unwrap());
        // A complete, valid one-pixel PNG, avoiding external image dependencies.
        fs::write(
            &image,
            [
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0,
                1, 8, 4, 0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100,
                248, 15, 0, 1, 5, 1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
                130,
            ],
        )
        .unwrap();
        worker.send(json!({"command":"submit","id":1,"request":request,"prompt":"Describe @screen","permissions":[]}));
        worker.event("started");
        assert_eq!(worker.event("answer")["text"], "42");
        let sent = requests.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            sent["tools"].as_array().is_none_or(Vec::is_empty),
            "tool exposure: {}",
            sent["tools"]
        );
        assert!(sent.to_string().contains("input_image"));
        assert!(!sent.to_string().contains("HOST_CONFIG_MUST_NOT_LEAK"));
        assert!(!sent.to_string().contains("HOST_INSTRUCTIONS_MUST_NOT_LEAK"));
        assert!(!marker.exists(), "MCP was launched");
        if request == 2 {
            assert!(
                sent.to_string().contains("42"),
                "resumed turn lost prior answer"
            );
        }
        assert!(!image.exists());
    }
    worker.send(json!({"command":"close","id":1}));
    await_file(&worker.root.path().join("deleted"));
    worker.stop();
}
