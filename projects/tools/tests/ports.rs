//! Port inspector discovery and action policy over a synthetic `/proc`.
//!
//! Nothing here reads the host's process table, contacts a system manager,
//! authenticates, signals a process or opens a socket. Every listener, owner,
//! service and identity below is a file in a temporary directory, and the two
//! effects the worker is allowed to have are recorded instead of performed.
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

#[path = "../src/ports/model.rs"]
mod model;
#[allow(dead_code)]
#[path = "../src/ports/privileged.rs"]
mod privileged;
#[path = "../src/ports/procfs.rs"]
mod procfs;
#[allow(dead_code)]
#[path = "../src/ports/worker.rs"]
mod worker;

use worker::{Host, Worker};

/// Everything the worker asked the system to do, and everything the system was
/// configured to answer. Shared so a test can read it while the worker is
/// still holding the host.
#[derive(Default)]
struct Log {
    units: Vec<String>,
    signals: Vec<(u32, i32)>,
    elevations: Vec<Vec<String>>,
    properties: String,
    authorized: bool,
    succeeds: bool,
    owners: Vec<Value>,
}

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Log>>);

impl Recorder {
    fn log(&self) -> Rc<RefCell<Log>> {
        Rc::clone(&self.0)
    }
}

impl Host for Recorder {
    fn systemctl(&mut self, user: bool, arguments: &[&str]) -> bool {
        let mut log = self.0.borrow_mut();
        log.units.push(format!(
            "{}{}",
            if user { "--user " } else { "" },
            arguments.join(" ")
        ));
        log.succeeds
    }
    fn show(&mut self, _user: bool, _unit: &str) -> String {
        self.0.borrow().properties.clone()
    }
    fn elevate(&mut self, arguments: &[String]) -> Result<Value, &'static str> {
        let mut log = self.0.borrow_mut();
        log.elevations.push(arguments.to_vec());
        if log.authorized {
            Ok(json!({"ok": true, "owners": log.owners.clone()}))
        } else {
            // Exactly what a cancelled or refused prompt produces.
            Err("not-authorized")
        }
    }
    fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool {
        let mut log = self.0.borrow_mut();
        if !validate() {
            return false;
        }
        log.signals.push((pid, number));
        log.succeeds
    }
    fn settle(&mut self, _duration: Duration) {}
}

/// A process table built by hand, in the exact shapes the kernel writes.
struct Fixture {
    root: tempfile::TempDir,
    tcp: Vec<String>,
    tcp6: Vec<String>,
}

fn hex4(address: &str) -> String {
    let octets: Vec<u8> = address
        .split('.')
        .map(|part| part.parse::<u8>().unwrap())
        .collect();
    // /proc prints each 32-bit word in host order; the fixtures assume the
    // little-endian machines this inspector runs on.
    format!(
        "{:08X}",
        u32::from_be_bytes([octets[0], octets[1], octets[2], octets[3]]).swap_bytes()
    )
}

fn hex6(address: &str) -> String {
    let parsed: std::net::Ipv6Addr = address.parse().unwrap();
    let octets = parsed.octets();
    let mut text = String::new();
    for word in 0..4 {
        let value = u32::from_be_bytes([
            octets[word * 4],
            octets[word * 4 + 1],
            octets[word * 4 + 2],
            octets[word * 4 + 3],
        ]);
        text.push_str(&format!("{:08X}", value.swap_bytes()));
    }
    text
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        fs::create_dir_all(root.path().join("proc/net")).unwrap();
        fs::write(
            root.path().join("passwd"),
            "root:x:0:0::/root:/bin/sh\nsilash:x:1000:1000::/home/silash:/bin/fish\n",
        )
        .unwrap();
        // A UDP table that must never appear in a single row.
        fs::write(
            root.path().join("proc/net/udp"),
            "  sl  local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n   0: 0100007F:14E9 00000000:0000 07 00000000:00000000 00:00000000 00000000 1000 0 90001 2 0 0\n",
        )
        .unwrap();
        Self {
            root,
            tcp: Vec::new(),
            tcp6: Vec::new(),
        }
    }

    fn roots(&self) -> procfs::Roots {
        procfs::Roots {
            proc: self.root.path().join("proc"),
            passwd: self.root.path().join("passwd"),
        }
    }

    fn listener(&mut self, address: &str, port: u16, uid: u32, inode: u64, state: &str) {
        let six = address.contains(':');
        let local = if six {
            format!("{}:{port:04X}", hex6(address))
        } else {
            format!("{}:{port:04X}", hex4(address))
        };
        let remote = if six {
            "00000000000000000000000000000000:0000".to_owned()
        } else {
            "00000000:0000".to_owned()
        };
        let line = format!(
            "{:4}: {local} {remote} {state} 00000000:00000000 00:00000000 00000000 {uid:5} 0 {inode} 1 0 100 0 0\n",
            if six { self.tcp6.len() } else { self.tcp.len() }
        );
        if six {
            self.tcp6.push(line);
        } else {
            self.tcp.push(line);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        pid: u32,
        name: &str,
        uid: u32,
        start: u64,
        cgroup: &str,
        inodes: &[u64],
        cwd: Option<&Path>,
    ) {
        let base = self.root.path().join("proc").join(pid.to_string());
        fs::create_dir_all(base.join("fd")).unwrap();
        fs::write(base.join("comm"), format!("{name}\n")).unwrap();
        // Field 22 of stat is the start time; the command name is skipped by
        // counting from the last closing parenthesis.
        let filler = vec!["0"; 18].join(" ");
        fs::write(
            base.join("stat"),
            format!("{pid} ({name}) S {filler} {start} 0 0\n"),
        )
        .unwrap();
        fs::write(
            base.join("status"),
            format!("Name:\t{name}\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\n"),
        )
        .unwrap();
        fs::write(base.join("cgroup"), format!("0::{cgroup}\n")).unwrap();
        for (index, inode) in inodes.iter().enumerate() {
            symlink(
                format!("socket:[{inode}]"),
                base.join("fd").join((index + 3).to_string()),
            )
            .unwrap();
        }
        if let Some(cwd) = cwd {
            symlink(cwd, base.join("cwd")).unwrap();
        }
    }

    /// Another user's listener: visible in the table, with an unreadable
    /// descriptor directory, exactly as an unprivileged scan finds it.
    fn hidden(&mut self, pid: u32, name: &str, uid: u32, start: u64, cgroup: &str) {
        self.process(pid, name, uid, start, cgroup, &[], None);
        fs::set_permissions(
            self.root
                .path()
                .join("proc")
                .join(pid.to_string())
                .join("fd"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }

    fn project(&self, name: &str) -> PathBuf {
        let path = self.root.path().join(name);
        fs::create_dir_all(path.join(".jj")).unwrap();
        path
    }

    fn publish(&self) {
        let header = "  sl  local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n";
        fs::write(
            self.root.path().join("proc/net/tcp"),
            format!("{header}{}", self.tcp.concat()),
        )
        .unwrap();
        fs::write(
            self.root.path().join("proc/net/tcp6"),
            format!("{header}{}", self.tcp6.concat()),
        )
        .unwrap();
    }
}

fn rows(snapshot: &Value) -> &Vec<Value> {
    snapshot["rows"].as_array().unwrap()
}

fn row(snapshot: &Value, port: u64) -> &Value {
    rows(snapshot)
        .iter()
        .find(|row| row["port"] == json!(port))
        .unwrap_or_else(|| panic!("no row for port {port}"))
}

fn send(worker: &mut Worker, message: Value) -> Vec<Value> {
    worker.request(&message.to_string())
}

/// One request, returning its reply and the snapshot that always follows it.
fn act(worker: &mut Worker, message: Value) -> (Value, Value) {
    let replies = send(worker, message);
    assert_eq!(
        replies.len(),
        2,
        "an action reply is followed by a snapshot"
    );
    assert_eq!(replies[1]["type"], json!("snapshot"));
    (replies[0].clone(), replies[1].clone())
}

fn open(worker: &mut Worker) -> Value {
    let replies = send(worker, json!({"op": "panel", "open": true}));
    replies.last().unwrap().clone()
}

/// A developer's machine: an unmanaged Node server, a user service, a root
/// service, a listener owned by a user we cannot see into, and a socket two
/// processes share.
fn machine(fixture: &mut Fixture) {
    let project = fixture.project("seele");
    fixture.listener("127.0.0.1", 3000, 1000, 41001, "0A");
    fixture.listener("0.0.0.0", 8080, 1000, 41002, "0A");
    fixture.listener("192.168.1.10", 5432, 0, 41003, "0A");
    fixture.listener("::1", 9000, 1000, 41004, "0A");
    fixture.listener("::", 4000, 1000, 41005, "0A");
    // An established connection in the same table is not a listener.
    fixture.listener("127.0.0.1", 50000, 1000, 41999, "01");
    fixture.process(
        101,
        "node",
        1000,
        1101,
        "/user.slice/user-1000.slice/session-3.scope",
        &[41001],
        Some(&project),
    );
    fixture.process(
        102,
        "vite",
        1000,
        1102,
        "/user.slice/user-1000.slice/user@1000.service/app.slice/dev.service",
        &[41002],
        Some(&project),
    );
    fixture.hidden(103, "postgres", 0, 1103, "/system.slice/postgresql.service");
    fixture.process(
        104,
        "worker",
        1000,
        1104,
        "/user.slice/user-1000.slice/session-3.scope",
        &[41004],
        None,
    );
    fixture.process(
        105,
        "worker",
        1000,
        1105,
        "/user.slice/user-1000.slice/session-3.scope",
        &[41004],
        None,
    );
    fixture.process(
        106,
        "docker-proxy",
        1000,
        1106,
        "/user.slice/user-1000.slice/session-3.scope",
        &[41005],
        None,
    );
    fixture.publish();
}

#[test]
fn every_tcp_listener_appears_with_its_actual_binding_and_udp_does_not() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);

    assert_eq!(
        snapshot["total"],
        json!(5),
        "one connection is not a listener"
    );
    let ports: Vec<u64> = rows(&snapshot)
        .iter()
        .map(|row| row["port"].as_u64().unwrap())
        .collect();
    assert_eq!(ports, vec![3000, 4000, 5432, 8080, 9000]);
    assert!(
        !rows(&snapshot)
            .iter()
            .any(|row| row["port"] == json!(5353) || row["port"] == json!(50000)),
        "UDP and established connections are excluded"
    );

    let loopback = row(&snapshot, 3000);
    assert_eq!(loopback["binding"], json!("127.0.0.1:3000"));
    assert_eq!(loopback["scope"], json!("loopback"));
    assert_eq!(loopback["destination"], json!("127.0.0.1"));
    assert_eq!(loopback["family"], json!("ipv4"));

    let wildcard = row(&snapshot, 8080);
    assert_eq!(wildcard["binding"], json!("0.0.0.0:8080"));
    assert_eq!(wildcard["scope"], json!("wildcard"));
    assert_eq!(
        wildcard["scopeLabel"],
        json!("All interfaces"),
        "a wildcard binding is never presented as localhost"
    );

    let lan = row(&snapshot, 5432);
    assert_eq!(lan["binding"], json!("192.168.1.10:5432"));
    assert_eq!(lan["scope"], json!("address"));
    assert_eq!(lan["destination"], json!("192.168.1.10"));

    let six = row(&snapshot, 9000);
    assert_eq!(six["family"], json!("ipv6"));
    assert_eq!(six["binding"], json!("[::1]:9000"));
    assert_eq!(
        six["destination"],
        json!("[::1]"),
        "an IPv6 destination stays bracketed for a URL"
    );
    assert_eq!(row(&snapshot, 4000)["destination"], json!("[::1]"));
    assert!(log.borrow().units.is_empty() && log.borrow().elevations.is_empty());
}

#[test]
fn ownership_is_reported_exactly_and_never_invented() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);

    let unmanaged = row(&snapshot, 3000);
    let owner = &unmanaged["owners"][0];
    assert_eq!(owner["pid"], json!(101));
    assert_eq!(owner["name"], json!("node"));
    assert_eq!(owner["user"], json!("silash"));
    assert_eq!(owner["project"], json!("seele"));
    assert_eq!(
        owner["service"],
        json!(""),
        "a session scope is not a service target"
    );
    assert_eq!(unmanaged["target"]["kind"], json!("process"));
    assert_eq!(unmanaged["target"]["pid"], json!(101));
    assert_eq!(unmanaged["target"]["privileged"], json!(false));

    let managed = row(&snapshot, 8080);
    assert_eq!(managed["owners"][0]["service"], json!("dev.service"));
    assert_eq!(managed["target"]["kind"], json!("service"));
    assert_eq!(managed["target"]["unit"], json!("dev.service"));
    assert_eq!(managed["target"]["scope"], json!("user"));
    assert_eq!(
        managed["target"]["privileged"],
        json!(false),
        "a user service is stopped by the user's own manager"
    );

    let foreign = row(&snapshot, 5432);
    assert_eq!(
        foreign["user"],
        json!("root"),
        "the socket's own uid is known"
    );
    assert_eq!(
        foreign["owners"].as_array().unwrap().len(),
        0,
        "an unreadable descriptor directory yields no owner rather than a guess"
    );
    assert_eq!(foreign["target"]["kind"], json!("none"));
    assert!(foreign["target"]["reason"]
        .as_str()
        .unwrap()
        .contains("Identify"));
    assert_eq!(foreign["identifiable"], json!(true));
    assert_eq!(snapshot["limited"], json!(true));

    let proxy = row(&snapshot, 4000);
    assert_eq!(
        proxy["owners"][0]["proxy"],
        json!("container"),
        "a host-side proxy is named rather than presented as the application"
    );
}

#[test]
fn a_shared_socket_requires_a_choice_before_anything_can_be_stopped() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);

    let shared = row(&snapshot, 9000);
    assert_eq!(shared["owners"].as_array().unwrap().len(), 2);
    assert_eq!(shared["target"]["kind"], json!("ambiguous"));
    assert!(shared["target"]["reason"]
        .as_str()
        .unwrap()
        .contains("Choose"));
    let id = shared["id"].as_str().unwrap().to_owned();

    let (reply, snapshot) = act(&mut worker, json!({"op": "select", "id": id, "pid": 105}));
    assert_eq!(reply["ok"], json!(true));
    let shared = row(&snapshot, 9000);
    assert_eq!(shared["target"]["kind"], json!("process"));
    assert_eq!(shared["target"]["pid"], json!(105));
    assert_eq!(shared["selected"], json!(105));

    // A PID that does not hold this socket is not a selection.
    let (reply, snapshot) = act(&mut worker, json!({"op": "select", "id": id, "pid": 101}));
    assert_eq!(reply["ok"], json!(false));
    assert_eq!(row(&snapshot, 9000)["target"]["pid"], json!(105));
}

#[test]
fn a_query_finds_a_listener_by_port_address_process_service_and_project() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    open(&mut worker);
    let mut search = |text: &str| -> Vec<u64> {
        let replies = send(&mut worker, json!({"op": "query", "text": text}));
        rows(replies.last().unwrap())
            .iter()
            .map(|row| row["port"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(search("3000"), vec![3000]);
    assert_eq!(search("localhost:3000"), vec![3000]);
    assert_eq!(search("http://localhost:3000"), vec![3000]);
    assert_eq!(search("node"), vec![3000]);
    assert_eq!(search("dev.service"), vec![8080]);
    assert_eq!(search("seele"), vec![3000, 8080]);
    assert_eq!(search("192.168.1.10"), vec![5432]);
    assert_eq!(
        search("localhost"),
        vec![3000, 4000, 8080, 9000],
        "localhost means loopback and wildcard, not one interface"
    );
    assert_eq!(search("nothing-here"), Vec::<u64>::new());
    assert_eq!(search(""), vec![3000, 4000, 5432, 8080, 9000]);
    let replies = send(
        &mut worker,
        json!({"op": "query", "text": "https://localhost:8080"}),
    );
    let snapshot = replies.last().unwrap();
    assert_eq!(snapshot["query"]["scheme"], json!("https"));
    assert_eq!(
        snapshot["query"]["explicitScheme"],
        json!(true),
        "a typed scheme is preserved for the proposed URL"
    );
    assert_eq!(
        snapshot["total"],
        json!(5),
        "filtering never hides how many listeners exist"
    );
}

#[test]
fn stopping_a_user_service_uses_the_user_manager_and_needs_no_authentication() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    log.borrow_mut().succeeds = true;
    log.borrow_mut().properties =
        "Id=dev.service\nRestart=always\nTriggeredBy=dev.socket\n".to_owned();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);
    let id = row(&snapshot, 8080)["id"].as_str().unwrap().to_owned();

    let (plan, _) = act(&mut worker, json!({"op": "plan", "id": id}));
    assert_eq!(plan["ok"], json!(true));
    assert_eq!(plan["target"]["kind"], json!("service"));
    assert_eq!(plan["restart"], json!("always"));
    assert_eq!(plan["triggeredBy"], json!("dev.socket"));
    assert_eq!(plan["canForce"], json!(false), "force is not offered first");
    let disclosure = plan["disclosure"].as_array().unwrap().clone();
    let text = disclosure
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("dev.service"), "the exact service is named");
    assert!(
        text.contains("Restart=always"),
        "restart behaviour is disclosed"
    );
    assert!(
        text.contains("dev.socket") && text.contains("does not disable"),
        "reactivation is explained instead of silently disabling a unit"
    );
    let token = plan["token"].as_str().unwrap().to_owned();

    let (stop, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "graceful"}),
    );
    // The synthetic table never changes, so this is the reappearing-listener
    // case: the panel is told the truth rather than a success.
    assert_eq!(stop["ok"], json!(false));
    assert_eq!(stop["remaining"], json!(true));
    assert_eq!(stop["canForce"], json!(true));
    assert_eq!(
        log.borrow().units,
        vec!["--user stop -- dev.service".to_owned()]
    );
    assert!(
        log.borrow().elevations.is_empty(),
        "a user service never reaches the privileged helper"
    );

    // Only now is force reachable, and it stays inside the reviewed unit.
    let (forced, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "force"}),
    );
    assert_eq!(forced["remaining"], json!(true));
    assert_eq!(
        log.borrow().units[1],
        "--user kill --kill-whom=all --signal=SIGKILL -- dev.service"
    );
    assert!(
        log.borrow().signals.is_empty(),
        "a service is never stopped by PID"
    );
}

#[test]
fn force_is_unreachable_until_a_graceful_stop_has_been_attempted() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    log.borrow_mut().succeeds = true;
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);
    let id = row(&snapshot, 3000)["id"].as_str().unwrap().to_owned();
    let (plan, _) = act(&mut worker, json!({"op": "plan", "id": id}));
    let token = plan["token"].as_str().unwrap().to_owned();

    let (refused, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "force"}),
    );
    assert_eq!(refused["error"], json!("not-escalatable"));
    assert!(
        log.borrow().signals.is_empty(),
        "an unconfirmed escalation sends no signal at all"
    );

    let (stop, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "graceful"}),
    );
    assert_eq!(stop["remaining"], json!(true));
    assert_eq!(log.borrow().signals, vec![(101, libc::SIGTERM)]);
    let (forced, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "force"}),
    );
    assert_eq!(forced["error"], json!(""));
    assert_eq!(log.borrow().signals[1], (101, libc::SIGKILL));
}

#[test]
fn a_stale_review_cannot_be_redirected_onto_a_recycled_pid_or_a_new_socket() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    log.borrow_mut().succeeds = true;
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);
    let id = row(&snapshot, 3000)["id"].as_str().unwrap().to_owned();
    let (plan, _) = act(&mut worker, json!({"op": "plan", "id": id}));
    let token = plan["token"].as_str().unwrap().to_owned();

    // While the confirmation was open, PID 101 exited and a different program
    // took its number and its port.
    let base = fixture.root.path().join("proc/101");
    fs::write(
        base.join("stat"),
        format!("101 (rm) S {} 7777 0 0\n", vec!["0"; 18].join(" ")),
    )
    .unwrap();
    fs::write(base.join("comm"), "rm\n").unwrap();

    let (stop, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "graceful"}),
    );
    assert_eq!(stop["error"], json!("changed"));
    assert_eq!(stop["ok"], json!(false));
    assert!(
        log.borrow().signals.is_empty(),
        "a recycled PID receives nothing"
    );

    // The listener itself disappearing is reported as gone, not as success.
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);
    let id = row(&snapshot, 3000)["id"].as_str().unwrap().to_owned();
    let (plan, _) = act(&mut worker, json!({"op": "plan", "id": id}));
    let token = plan["token"].as_str().unwrap().to_owned();
    fixture.tcp.clear();
    fixture.tcp6.clear();
    fixture.listener("127.0.0.1", 3000, 1000, 42001, "0A");
    fixture.publish();
    let (stop, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "graceful"}),
    );
    assert_eq!(
        stop["error"],
        json!("gone"),
        "a rebound port is a different socket, not the reviewed one"
    );
    assert!(log.borrow().signals.is_empty() && log.borrow().units.is_empty());
}

#[test]
fn a_privileged_target_goes_through_authentication_and_a_refusal_acts_on_nothing() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    log.borrow_mut().succeeds = true;
    log.borrow_mut().authorized = false;
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);
    let foreign = row(&snapshot, 5432);
    let id = foreign["id"].as_str().unwrap().to_owned();

    // Identifying another user's listener is itself an authorized read.
    let (identify, _) = act(&mut worker, json!({"op": "identify", "id": id}));
    assert_eq!(identify["ok"], json!(false));
    assert_eq!(identify["error"], json!("not-authorized"));
    assert_eq!(log.borrow().elevations.len(), 1);
    assert_eq!(log.borrow().elevations[0][0], "identify");
    assert_eq!(log.borrow().elevations[0][2], "192.168.1.10:5432");
    assert!(
        log.borrow().signals.is_empty() && log.borrow().units.is_empty(),
        "an owner lookup signals nothing"
    );

    // Authorized, it returns an identity, and the row stops claiming it is
    // unowned without inventing anything about it.
    log.borrow_mut().authorized = true;
    log.borrow_mut().owners = vec![json!({"pid": 103, "start": 1103})];
    let (identify, snapshot) = act(&mut worker, json!({"op": "identify", "id": id}));
    assert_eq!(identify["ok"], json!(true));
    let resolved = row(&snapshot, 5432);
    assert_eq!(resolved["identified"], json!(true));
    assert_eq!(resolved["owners"][0]["pid"], json!(103));
    assert_eq!(resolved["target"]["kind"], json!("service"));
    assert_eq!(resolved["target"]["unit"], json!("postgresql.service"));
    assert_eq!(resolved["target"]["privileged"], json!(true));

    let (plan, _) = act(&mut worker, json!({"op": "plan", "id": id}));
    let token = plan["token"].as_str().unwrap().to_owned();
    log.borrow_mut().authorized = false;
    let (stop, _) = act(
        &mut worker,
        json!({"op": "stop", "token": token, "mode": "graceful"}),
    );
    assert_eq!(stop["error"], json!("not-authorized"));
    assert_eq!(stop["remaining"], json!(true));
    assert!(
        log.borrow().units.is_empty(),
        "a cancelled prompt never reaches the system manager"
    );
    let last = log.borrow().elevations.last().unwrap().clone();
    assert_eq!(last[0], "service");
    assert_eq!(last[1], "graceful");
    assert_eq!(last[2], "postgresql.service");
    assert_eq!(last[4], "192.168.1.10:5432");
}

#[test]
fn closing_the_panel_forgets_everything_it_learned() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let log = recorder.log();
    log.borrow_mut().authorized = true;
    log.borrow_mut().owners = vec![json!({"pid": 103, "start": 1103})];
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let snapshot = open(&mut worker);
    let id = row(&snapshot, 5432)["id"].as_str().unwrap().to_owned();
    let (_, snapshot) = act(&mut worker, json!({"op": "identify", "id": id}));
    assert_eq!(row(&snapshot, 5432)["identified"], json!(true));

    let replies = send(&mut worker, json!({"op": "panel", "open": false}));
    let closed = replies.last().unwrap();
    assert_eq!(closed["total"], json!(0));
    assert!(rows(closed).is_empty());
    assert!(worker.tick().is_none(), "a closed panel stops scanning");

    let snapshot = open(&mut worker);
    assert_eq!(
        row(&snapshot, 5432)["identified"],
        json!(false),
        "an identity paid for with authentication is not retained across a close"
    );
}

#[test]
fn refreshing_keeps_row_identity_while_listeners_come_and_go() {
    let mut fixture = Fixture::new();
    machine(&mut fixture);
    let mut recorder = Recorder::default();
    let mut worker = Worker::new(fixture.roots(), &mut recorder, 1000);
    let first = open(&mut worker);
    let identities: HashMap<u64, String> = rows(&first)
        .iter()
        .map(|row| {
            (
                row["port"].as_u64().unwrap(),
                row["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();

    fixture.listener("127.0.0.1", 7000, 1000, 41007, "0A");
    fixture.publish();
    let replies = send(&mut worker, json!({"op": "refresh"}));
    let second = replies.last().unwrap();
    assert_eq!(second["total"], json!(6));
    for row_value in rows(second) {
        let port = row_value["port"].as_u64().unwrap();
        if let Some(previous) = identities.get(&port) {
            assert_eq!(
                row_value["id"].as_str().unwrap(),
                previous,
                "an unchanged listener keeps its identity across a refresh"
            );
        }
    }
    assert_eq!(
        row(second, 7000)["id"],
        json!("4:41007"),
        "identity is the socket inode, not the port"
    );
    assert!(second["generation"].as_u64().unwrap() > first["generation"].as_u64().unwrap());
}

#[test]
fn privileged_service_stop_rejects_same_named_user_service() {
    struct RootRecorder(bool);
    impl privileged::System for RootRecorder {
        fn systemctl(&mut self, _: &[&str]) -> bool {
            self.0 = true;
            true
        }
        fn signal(&mut self, _: u32, _: i32, _: &mut dyn FnMut() -> bool) -> bool {
            panic!("unexpected signal")
        }
    }
    let mut fixture = Fixture::new();
    fixture.listener("127.0.0.1", 8080, 1000, 41002, "0A");
    fixture.process(
        102,
        "server",
        1000,
        1102,
        "/user.slice/user-1000.slice/user@1000.service/app.slice/dev.service",
        &[41002],
        None,
    );
    fixture.publish();
    let mut host = RootRecorder(false);
    assert_eq!(
        privileged::stop_service(
            &fixture.roots(),
            &mut host,
            false,
            "dev.service",
            41002,
            "127.0.0.1:8080",
            8080
        ),
        Err("changed")
    );
    assert!(!host.0);
    fs::write(
        fixture.roots().proc.join("102/cgroup"),
        "0::/system.slice/dev.service\n",
    )
    .unwrap();
    assert!(privileged::stop_service(
        &fixture.roots(),
        &mut host,
        false,
        "dev.service",
        41002,
        "127.0.0.1:8080",
        8080
    )
    .is_ok());
    assert!(host.0);
}
