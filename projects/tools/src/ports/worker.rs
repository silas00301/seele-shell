//! The resident, unprivileged half of the port inspector.
//!
//! One process owns discovery, the current query, the selected owner of a
//! shared socket and every action's policy. It answers on stdout in complete
//! JSON lines and writes nothing to disk: closing the panel ends the scan and
//! forgets the identities an authorized lookup resolved.
//!
//! Discovery itself only reads `/proc`. Nothing here asks for authentication
//! until the user confirms a stop or an owner lookup, so opening the panel
//! cannot raise a prompt, and nothing here ever contacts a listening service.
use super::model::{self, Identities, Query, Review, Row};
use super::procfs::{self, Roots};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

const RUN0: &str = "/run/current-system/sw/bin/run0";
const SYSTEMCTL: &str = "/run/current-system/sw/bin/systemctl";
const HELPER: &str = "seele-stop-listener";
const MAX_LINE: u64 = 64 * 1024;
/// A refresh while the panel is open. Long enough that a scan is a rounding
/// error against idle, short enough that a listener that just started appears
/// before the user reaches for Refresh.
const INTERVAL: Duration = Duration::from_secs(2);
const SETTLE: Duration = Duration::from_millis(250);
const GRACE: u32 = 12;
const FORCE_GRACE: u32 = 8;
const MAX_SELECTIONS: usize = 256;
const MAX_ESCALATIONS: usize = 64;

/// Everything the worker is allowed to do besides reading `/proc`. Tests
/// substitute a recorder, so the decision to stop, to escalate or to ask for
/// authentication is exercised without a system manager or a real process.
pub trait Host {
    /// The caller's own system manager, user or system instance.
    fn systemctl(&mut self, user: bool, arguments: &[&str]) -> bool;
    /// A read-only property query. Returns raw `key=value` lines.
    fn show(&mut self, user: bool, unit: &str) -> String;
    /// Authenticate and run the packaged helper with a typed target.
    fn elevate(&mut self, arguments: &[String]) -> Result<Value, &'static str>;
    /// Signal one of our own processes.
    fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool;
    fn settle(&mut self, duration: Duration);
}

fn property(text: &str, key: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// One panel's worth of state. Nothing in it survives the process.
pub struct Worker<'a> {
    roots: Roots,
    host: &'a mut dyn Host,
    self_uid: u32,
    query: Query,
    selection: HashMap<String, u32>,
    identities: Identities,
    rows: Vec<Row>,
    /// Tokens whose graceful stop was attempted and did not free the port.
    /// Force is offered only for these, so an escalation is always the user's
    /// second decision rather than an automatic follow-up.
    escalations: HashSet<String>,
    action: Value,
    open: bool,
    generation: u64,
}

/// One stop reply. A stop is successful only when it named no error and the
/// listener is actually gone, so a reappearing port can never be rendered as a
/// removal.
fn reply(error: &str, remaining: bool, token: &str, force: bool) -> Value {
    json!({
        "type": "stop",
        "token": token,
        "mode": if force { "force" } else { "graceful" },
        "ok": error.is_empty() && !remaining,
        "error": error,
        "remaining": remaining,
    })
}

impl<'a> Worker<'a> {
    pub fn new(roots: Roots, host: &'a mut dyn Host, self_uid: u32) -> Self {
        Self {
            roots,
            host,
            self_uid,
            query: model::parse_query(""),
            selection: HashMap::new(),
            identities: Identities::new(),
            rows: Vec::new(),
            escalations: HashSet::new(),
            action: Value::Null,
            open: false,
            generation: 0,
        }
    }

    pub fn open(&self) -> bool {
        self.open
    }

    pub fn rescan(&mut self) {
        self.rows = model::scan(&self.roots, &self.identities, self.self_uid);
        let live: HashSet<String> = self.rows.iter().map(Row::id).collect();
        // A vanished listener takes its selection with it, so a reused inode
        // can never inherit the owner someone chose for a different socket.
        self.selection.retain(|id, _| live.contains(id));
    }

    fn selected(&self, id: &str) -> u32 {
        self.selection.get(id).copied().unwrap_or(0)
    }

    fn find(&self, id: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.id() == id)
    }

    pub fn snapshot(&mut self) -> Value {
        self.generation += 1;
        let mut rows = Vec::new();
        let mut limited = false;
        for row in &self.rows {
            limited |= row.owners.is_empty();
            if self.query.matches(row) {
                rows.push(model::row_value(
                    row,
                    &self.query,
                    self.selected(&row.id()),
                    self.self_uid,
                ));
            }
        }
        json!({
            "version": 1,
            "type": "snapshot",
            "generation": self.generation,
            "open": self.open,
            "selfUid": self.self_uid,
            "total": self.rows.len(),
            "limited": limited,
            "query": {
                "text": self.query.text,
                "port": self.query.port,
                "host": self.query.host,
                "scheme": self.query.scheme,
                "explicitScheme": self.query.explicit_scheme,
            },
            "rows": rows,
            "action": self.action.clone(),
        })
    }

    /// What a confirmation has to disclose before this target is stopped.
    ///
    /// Everything here is read at plan time and only at plan time: a refresh
    /// never runs `systemctl show`, so listing ports stays a pure `/proc` read.
    fn plan(&mut self, id: &str, mode: &str) -> Value {
        let mode = if mode == "force" { "force" } else { "graceful" };
        let Some(row) = self.find(id) else {
            return json!({"type": "plan", "id": id, "mode": mode, "ok": false, "error": "gone"});
        };
        let selected = self.selected(id);
        let target = row.target(selected, self.self_uid);
        let token = row.review(selected, self.self_uid).token();
        let binding = row.listener.binding();
        let port = row.listener.port;
        let unit = target.unit.clone();
        let user_scope = target.scope == "user";
        let owners = row.owners.len();
        let proxy = row.owner(selected).map(|owner| owner.proxy).unwrap_or("");
        let siblings: Vec<String> = self
            .rows
            .iter()
            .filter(|other| other.id() != id)
            .filter(|other| match target.kind {
                "service" => other
                    .service()
                    .is_some_and(|(name, scope)| name == unit && scope == target.scope),
                "process" => other.owners.iter().any(|owner| owner.pid == target.pid),
                _ => false,
            })
            .map(|other| other.listener.binding())
            .collect();
        let mut disclosure = Vec::new();
        let mut restart = String::new();
        let mut triggers = String::new();
        if target.kind == "service" {
            let properties = self.host.show(user_scope, &unit);
            restart = property(&properties, "Restart");
            triggers = property(&properties, "TriggeredBy");
            disclosure.push(format!(
                "Stopping {unit} stops the service, not only the process listening on {binding}."
            ));
            if owners > 1 {
                disclosure.push(format!("It currently runs {owners} processes."));
            }
            if !restart.is_empty() && restart != "no" {
                disclosure.push(format!(
                    "Restart={restart}: systemd starts it again on its own, so port {port} may reopen."
                ));
            }
            if !triggers.is_empty() {
                disclosure.push(format!(
                    "{triggers} can activate it again. Stop does not disable anything."
                ));
            }
        } else if target.kind == "process" {
            disclosure.push(format!(
                "Only process {} ({}) is asked to stop.",
                target.pid, target.name
            ));
            if target.privileged {
                disclosure.push(format!(
                    "It runs as {}, so stopping it needs authentication.",
                    if target.user.is_empty() {
                        format!("uid {}", target.uid)
                    } else {
                        target.user.clone()
                    }
                ));
            }
        }
        if !proxy.is_empty() {
            disclosure
                .push("This is a host-side proxy, not the application it forwards to.".to_owned());
        }
        if !siblings.is_empty() {
            disclosure.push(format!("It also holds {}.", siblings.join(", ")));
        }
        json!({
            "type": "plan",
            "id": id,
            "mode": mode,
            "ok": target.kind == "service" || target.kind == "process",
            "error": if target.kind == "service" || target.kind == "process" { "" } else { "unavailable" },
            "token": token,
            "binding": binding,
            "port": port,
            "target": {
                "kind": target.kind,
                "unit": target.unit,
                "scope": target.scope,
                "pid": target.pid,
                "name": target.name,
                "user": target.user,
                "privileged": target.privileged,
                "reason": target.reason,
            },
            "restart": restart,
            "triggeredBy": triggers,
            "also": siblings,
            "disclosure": disclosure,
            "canForce": self.escalations.contains(&token),
        })
    }

    /// Ask, with authentication, who owns a listener `/proc` will not show us.
    /// This reads and returns identity; it never signals anything.
    fn identify(&mut self, id: &str) -> Value {
        let Some(row) = self.find(id) else {
            return json!({"type": "identify", "id": id, "ok": false, "error": "gone"});
        };
        if !row.identifiable(self.self_uid) {
            return json!({"type": "identify", "id": id, "ok": false, "error": "unavailable"});
        }
        let inode = row.listener.inode;
        let arguments = vec![
            "identify".to_owned(),
            inode.to_string(),
            row.listener.binding(),
            row.listener.port.to_string(),
            row.listener.uid.to_string(),
        ];
        match self.host.elevate(&arguments) {
            Ok(value) => {
                let resolved: Vec<(u32, u64)> = value["owners"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|owner| {
                        Some((
                            u32::try_from(owner["pid"].as_u64()?).ok()?,
                            owner["start"].as_u64()?,
                        ))
                    })
                    .collect();
                if resolved.is_empty() {
                    self.identities.remove(&inode);
                    return json!({"type": "identify", "id": id, "ok": false, "error": "unknown"});
                }
                if self.identities.len() < MAX_SELECTIONS {
                    self.identities.insert(inode, resolved);
                }
                self.rescan();
                json!({"type": "identify", "id": id, "ok": true, "error": ""})
            }
            Err(error) => json!({"type": "identify", "id": id, "ok": false, "error": error}),
        }
    }

    /// Resolve the reviewed listener from the kernel again and return the
    /// decision it supports now. The token is the comparison: it carries the
    /// whole decision and nothing else, so a relabelled row still matches and
    /// a redirected one never can. The freshly derived target — not the one
    /// parsed out of the token — is what is then acted on, because privilege
    /// is a property of the current owner rather than of the review.
    fn revalidate(&self, review: &Review) -> Result<(String, model::Target), &'static str> {
        let listener =
            procfs::find_listener(&self.roots, review.inode, &review.binding, review.port)
                .ok_or("gone")?;
        let id = listener.id();
        let row = self.find(&id).ok_or("gone")?;
        let current = row.review(self.selected(&id), self.self_uid);
        Ok((current.token(), current.target))
    }

    /// Has the reviewed socket actually gone away? A stop that leaves the
    /// listener bound is reported as exactly that, never as a success.
    fn remaining(&mut self, review: &Review, attempts: u32) -> bool {
        for _ in 0..attempts {
            if procfs::find_listener(&self.roots, review.inode, &review.binding, review.port)
                .is_none()
            {
                return false;
            }
            self.host.settle(SETTLE);
        }
        procfs::find_listener(&self.roots, review.inode, &review.binding, review.port).is_some()
    }

    fn perform(
        &mut self,
        review: &Review,
        target: &model::Target,
        force: bool,
    ) -> Result<(), &'static str> {
        let ok = match (target.kind, target.privileged) {
            ("service", false) => {
                let unit = target.unit.clone();
                let user = target.scope == "user";
                let arguments: Vec<&str> = if force {
                    vec!["kill", "--kill-whom=all", "--signal=SIGKILL", "--", &unit]
                } else {
                    vec!["stop", "--", &unit]
                };
                self.host.systemctl(user, &arguments)
            }
            ("service", true) => {
                let arguments = vec![
                    "service".to_owned(),
                    if force { "force" } else { "graceful" }.to_owned(),
                    target.unit.clone(),
                    review.inode.to_string(),
                    review.binding.clone(),
                    review.port.to_string(),
                ];
                self.host.elevate(&arguments)?;
                true
            }
            ("process", false) => {
                // The same three checks the privileged helper repeats after
                // authentication, because even an unprivileged signal must not
                // land on a recycled PID.
                if procfs::start_time(&self.roots, target.pid) != Some(target.start)
                    || procfs::process_uid(&self.roots, target.pid) != Some(target.uid)
                    || !procfs::holds_socket(&self.roots, target.pid, review.inode)
                {
                    return Err("changed");
                }
                self.host.signal(
                    target.pid,
                    if force { libc::SIGKILL } else { libc::SIGTERM },
                    &mut || {
                        procfs::start_time(&self.roots, target.pid) == Some(target.start)
                            && procfs::process_uid(&self.roots, target.pid) == Some(target.uid)
                            && procfs::holds_socket(&self.roots, target.pid, review.inode)
                    },
                )
            }
            ("process", true) => {
                let arguments = vec![
                    "process".to_owned(),
                    if force { "force" } else { "graceful" }.to_owned(),
                    target.pid.to_string(),
                    target.start.to_string(),
                    target.uid.to_string(),
                    review.inode.to_string(),
                    review.binding.clone(),
                    review.port.to_string(),
                ];
                self.host.elevate(&arguments)?;
                true
            }
            _ => return Err("unavailable"),
        };
        if ok {
            Ok(())
        } else {
            Err("failed")
        }
    }

    /// Stop exactly what a confirmation disclosed, or refuse and say why.
    fn stop(&mut self, token: &str, force: bool) -> Value {
        let Some(review) = Review::parse(token) else {
            return reply("invalid", false, token, force);
        };
        // Force is never a fallback the panel can reach on its own: only a
        // graceful attempt that left the listener in place unlocks it.
        if force && !self.escalations.contains(token) {
            return reply("not-escalatable", true, token, force);
        }
        self.rescan();
        let (current, target) = match self.revalidate(&review) {
            Ok(resolved) => resolved,
            Err(error) => return reply(error, error == "changed", token, force),
        };
        if current != token {
            return reply("changed", true, token, force);
        }
        if let Err(error) = self.perform(&review, &target, force) {
            // A refused or cancelled authentication leaves the target running
            // and stays available for another deliberate attempt.
            return reply(error, true, token, force);
        }
        let remaining = self.remaining(&review, if force { FORCE_GRACE } else { GRACE });
        if remaining && !force {
            if self.escalations.len() < MAX_ESCALATIONS {
                self.escalations.insert(token.to_owned());
            }
        } else if !remaining {
            self.escalations.remove(token);
        }
        self.rescan();
        let mut value = reply("", remaining, token, force);
        value["canForce"] = json!(self.escalations.contains(token));
        value["target"] = json!(review.target.kind);
        value["unit"] = json!(review.target.unit);
        value
    }

    fn select(&mut self, id: &str, pid: u32) -> Value {
        let known = self
            .find(id)
            .is_some_and(|row| row.owners.iter().any(|owner| owner.pid == pid));
        if known && self.selection.len() < MAX_SELECTIONS {
            self.selection.insert(id.to_owned(), pid);
        } else if pid == 0 {
            self.selection.remove(id);
        }
        json!({"type": "select", "id": id, "ok": known || pid == 0})
    }

    /// Handle one request line. Returns the replies to write, snapshot last.
    pub fn request(&mut self, line: &str) -> Vec<Value> {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let operation = message["op"].as_str().unwrap_or("");
        let id = message["id"].as_str().unwrap_or("");
        self.action = Value::Null;
        let mut replies = Vec::new();
        match operation {
            "panel" => {
                self.open = message["open"].as_bool().unwrap_or(false);
                if self.open {
                    self.rescan();
                } else {
                    // Closing the panel forgets everything it learned,
                    // including identities an authentication paid for.
                    self.rows.clear();
                    self.selection.clear();
                    self.identities.clear();
                    self.escalations.clear();
                }
            }
            "query" => {
                self.query = model::parse_query(message["text"].as_str().unwrap_or(""));
            }
            "refresh" => self.rescan(),
            "select" => {
                let pid = u32::try_from(message["pid"].as_u64().unwrap_or(0)).unwrap_or(0);
                replies.push(self.select(id, pid));
            }
            "plan" => {
                self.rescan();
                let mode = message["mode"].as_str().unwrap_or("graceful").to_owned();
                let value = self.plan(id, &mode);
                replies.push(value);
            }
            "identify" => {
                self.rescan();
                let value = self.identify(id);
                replies.push(value);
            }
            "stop" => {
                let token = message["token"].as_str().unwrap_or("").to_owned();
                let force = message["mode"].as_str() == Some("force");
                let value = self.stop(&token, force);
                self.action = value.clone();
                replies.push(value);
            }
            _ => return Vec::new(),
        }
        let snapshot = self.snapshot();
        replies.push(snapshot);
        replies
    }

    pub fn tick(&mut self) -> Option<Value> {
        if !self.open {
            return None;
        }
        self.rescan();
        Some(self.snapshot())
    }
}

/// The desktop session's own effects.
pub struct Desktop {
    cancel: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    helper: Option<PathBuf>,
}

/// The packaged helper is found next to this executable rather than through
/// `PATH`, so nothing in the environment can substitute the program that is
/// about to be run as root.
fn helper() -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let executable = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    let path = executable.parent()?.join(HELPER);
    let metadata = std::fs::metadata(&path).ok()?;
    (metadata.is_file() && metadata.mode() & 0o111 != 0 && metadata.mode() & 0o022 == 0)
        .then_some(path)
}

impl Desktop {
    pub fn new() -> Self {
        Self {
            cancel: seele_runtime::process::termination_signal()
                .unwrap_or_else(|_| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
            helper: helper(),
        }
    }

    fn command(program: &str) -> std::process::Command {
        let mut command = std::process::Command::new(program);
        command
            .env_clear()
            .env("PATH", "/run/current-system/sw/bin")
            .env("LANG", "C.UTF-8")
            .current_dir("/");
        for name in [
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "WAYLAND_DISPLAY",
        ] {
            if let Ok(value) = std::env::var(name) {
                command.env(name, value);
            }
        }
        command
    }
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for Desktop {
    fn systemctl(&mut self, user: bool, arguments: &[&str]) -> bool {
        let mut command = Self::command(SYSTEMCTL);
        if user {
            command.arg("--user");
        }
        command.args(arguments);
        seele_runtime::process::discard(
            &mut command,
            b"",
            seele_runtime::process::Limits {
                timeout: Duration::from_secs(120),
                output: 0,
            },
            &*self.cancel,
        )
        .is_ok_and(|status| status.success())
    }

    fn show(&mut self, user: bool, unit: &str) -> String {
        if !procfs::valid_unit(unit) {
            return String::new();
        }
        let mut command = Self::command(SYSTEMCTL);
        if user {
            command.arg("--user");
        }
        command.args([
            "show",
            "--property=Id",
            "--property=Restart",
            "--property=TriggeredBy",
            "--property=ActiveState",
            "--",
            unit,
        ]);
        seele_runtime::process::capture(
            &mut command,
            b"",
            seele_runtime::process::Limits {
                timeout: Duration::from_secs(20),
                output: 64 * 1024,
            },
            &*self.cancel,
        )
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
    }

    fn elevate(&mut self, arguments: &[String]) -> Result<Value, &'static str> {
        let path = self.helper.as_deref().ok_or("unavailable")?;
        let mut command = Self::command(RUN0);
        command.arg(path).args(arguments);
        let output = seele_runtime::process::capture(
            &mut command,
            b"",
            seele_runtime::process::Limits {
                // Authentication is a person at a prompt, not a timeout to
                // race. Cancelling it simply fails the action.
                timeout: Duration::from_secs(150),
                output: 64 * 1024,
            },
            &*self.cancel,
        )
        .map_err(|_| "failed")?;
        // run0 may prepend its own notices, so the helper's reply is the last
        // line that is a complete JSON object.
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let value = text
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .filter(Value::is_object);
        match value {
            Some(value) if value["ok"] == json!(true) => Ok(value),
            Some(value) => Err(match value["error"].as_str().unwrap_or("") {
                "changed" => "changed",
                "gone" => "gone",
                "invalid_target" => "invalid",
                _ => "failed",
            }),
            // No reply at all is an authentication that never completed.
            None => Err("not-authorized"),
        }
    }

    fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool {
        seele_runtime::process::signal_checked(pid, number, validate)
    }

    fn settle(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn requests() -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        loop {
            let mut line = String::new();
            match (&mut reader).take(MAX_LINE).read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    receiver
}

fn emit(value: &Value) -> bool {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{value}").is_ok() && out.flush().is_ok()
}

pub fn run() {
    let incoming = requests();
    let mut desktop = Desktop::new();
    // SAFETY: getuid has no preconditions.
    let self_uid = unsafe { libc::getuid() };
    let mut worker = Worker::new(Roots::default(), &mut desktop, self_uid);
    let first = worker.snapshot();
    if !emit(&first) {
        return;
    }
    loop {
        let wait = if worker.open() {
            INTERVAL
        } else {
            Duration::from_secs(3600)
        };
        match incoming.recv_timeout(wait) {
            Ok(line) => {
                for value in worker.request(&line) {
                    if !emit(&value) {
                        return;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(value) = worker.tick() {
                    if !emit(&value) {
                        return;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root with no process table at all. These tests are about the policy
    /// in front of an action, so they read no host state whatsoever; the
    /// synthetic `/proc` lives in the crate's integration test.
    fn roots() -> Roots {
        Roots {
            proc: PathBuf::from("/nonexistent/seele-ports/proc"),
            passwd: PathBuf::from("/nonexistent/seele-ports/passwd"),
        }
    }

    #[derive(Default)]
    pub struct Recorder {
        pub units: Vec<String>,
        pub signals: Vec<(u32, i32)>,
        pub elevations: Vec<Vec<String>>,
        pub properties: String,
        pub authorized: bool,
        pub succeeds: bool,
    }

    impl Host for Recorder {
        fn systemctl(&mut self, user: bool, arguments: &[&str]) -> bool {
            self.units.push(format!(
                "{}{}",
                if user { "--user " } else { "" },
                arguments.join(" ")
            ));
            self.succeeds
        }
        fn show(&mut self, _user: bool, _unit: &str) -> String {
            self.properties.clone()
        }
        fn elevate(&mut self, arguments: &[String]) -> Result<Value, &'static str> {
            self.elevations.push(arguments.to_vec());
            if self.authorized {
                Ok(json!({"ok": true, "owners": []}))
            } else {
                Err("not-authorized")
            }
        }
        fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool {
            if !validate() {
                return false;
            }
            self.signals.push((pid, number));
            self.succeeds
        }
        fn settle(&mut self, _duration: Duration) {}
    }

    #[test]
    fn an_unknown_request_is_ignored_rather_than_answered() {
        let mut recorder = Recorder::default();
        let mut worker = Worker::new(roots(), &mut recorder, 1000);
        assert!(worker.request("not json").is_empty());
        assert!(worker.request("{\"op\":\"reboot\"}").is_empty());
        assert!(!worker.open(), "a closed panel never starts scanning");
        assert!(worker.tick().is_none());
    }

    #[test]
    fn force_is_refused_until_a_graceful_stop_has_failed() {
        let mut recorder = Recorder::default();
        let mut worker = Worker::new(roots(), &mut recorder, 1000);
        let token = "1|41|127.0.0.1:3000|3000|process|||99999|1|1000";
        let replies =
            worker.request(&json!({"op": "stop", "token": token, "mode": "force"}).to_string());
        assert_eq!(replies[0]["error"], json!("not-escalatable"));
        assert_eq!(replies[0]["ok"], json!(false));
        assert_eq!(replies.len(), 2, "every reply is followed by a snapshot");
        assert_eq!(replies[1]["type"], json!("snapshot"));
    }

    #[test]
    fn a_malformed_or_vanished_target_is_refused_before_anything_runs() {
        let mut recorder = Recorder::default();
        let mut worker = Worker::new(roots(), &mut recorder, 1000);
        for token in [
            "",
            "1|41|127.0.0.1:3000|3000|reboot|||1|1|0",
            "1|41|127.0.0.1:3000|3000|service|a b.service|system|0|0|0",
        ] {
            let replies = worker.request(&json!({"op": "stop", "token": token}).to_string());
            assert_eq!(replies[0]["error"], json!("invalid"), "{token}");
        }
        // A well-formed token for a socket that does not exist is refused by
        // the fresh scan, not by the helper.
        let replies = worker.request(
            &json!({"op": "stop", "token": "1|41|127.0.0.1:3000|3000|process|||99999|1|1000"})
                .to_string(),
        );
        assert_eq!(replies[0]["error"], json!("gone"));
        assert!(recorder.signals.is_empty() && recorder.elevations.is_empty());
        assert!(recorder.units.is_empty());
    }

    #[test]
    fn systemd_properties_are_read_as_key_value_lines() {
        let text =
            "Id=nginx.service\nRestart=always\nTriggeredBy=nginx.socket\nActiveState=active\n";
        assert_eq!(property(text, "Restart"), "always");
        assert_eq!(property(text, "TriggeredBy"), "nginx.socket");
        assert_eq!(property(text, "Missing"), "");
    }
}
