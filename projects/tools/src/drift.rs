//! Declarative drift for nerv: three checks the flake already states, a dry-run
//! that only reads, and a restore that puts the selected drifted checks back.
//!
//! `seele-drift diff` never calls a mutating program. `seele-drift apply`
//! restores only the ids it was given, and only while they are still drifted.
//! System changes go through `seele-restore-drift`, which accepts one of the
//! three ids and a fixed command for that id, never a caller-supplied command.

use crate::command;
use seele_runtime::process::{self, Limits};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const VERSION: u32 = 1;
const CATALOG_LIMIT: usize = 65_536;
const STATUS_LIMIT: usize = 256 * 1024;
const SYSTEM_CATALOG: &str = "/etc/seele/drift.json";
const SYSTEMCTL: &str = "/run/current-system/sw/bin/systemctl";
const RESOLVECTL: &str = "/run/current-system/sw/bin/resolvectl";
const HELPER_NAME: &str = "seele-restore-drift";

const QUAD9: &str = "quad9-dot";
const PODMAN: &str = "podman-rootless";
const REMOTE: &str = "remote-shell";

#[derive(Debug)]
struct DriftError {
    message: String,
    mutated: bool,
    restored: Vec<String>,
}

fn quiet(message: impl Into<String>) -> DriftError {
    DriftError {
        message: message.into(),
        mutated: false,
        restored: Vec::new(),
    }
}

struct Catalog {
    dns: Vec<String>,
    quad9: bool,
    podman: bool,
    remote: bool,
}

impl Catalog {
    fn enabled(&self, id: &str) -> bool {
        match id {
            QUAD9 => self.quad9,
            PODMAN => self.podman,
            REMOTE => self.remote,
            _ => false,
        }
    }
}

#[derive(Deserialize)]
struct RawCatalog {
    version: u32,
    checks: BTreeMap<String, RawCheck>,
}

#[derive(Deserialize, Default)]
struct RawCheck {
    #[serde(default)]
    dns: Vec<String>,
}

#[derive(Clone, Copy)]
struct Unit {
    present: bool,
    not_found: bool,
    active: bool,
    enabled: bool,
}

enum Probe<T> {
    Ready(T),
    Failed,
}

enum Class {
    Loaded(Unit),
    Missing,
    Unreadable,
}

fn classify(probe: Probe<Unit>) -> Class {
    match probe {
        Probe::Failed => Class::Unreadable,
        Probe::Ready(unit) if unit.not_found => Class::Missing,
        Probe::Ready(unit) if unit.present => Class::Loaded(unit),
        Probe::Ready(_) => Class::Unreadable,
    }
}

trait Reads {
    fn resolvectl_status(&self) -> Probe<String>;
    fn unit(&self, user: bool, name: &str) -> Probe<Unit>;
}

trait Acts: Reads {
    fn elevate(&mut self, id: &str) -> Result<(), String>;
    fn start_user_podman(&mut self) -> Result<(), String>;
}

#[derive(Clone, Debug, serde::Serialize)]
struct CheckView {
    id: String,
    title: String,
    drifted: bool,
    unavailable: bool,
    before: String,
    after: String,
    restore: String,
}

struct Analysis {
    view: CheckView,
    ops: Vec<SystemOp>,
    start_user: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
struct Report {
    version: u32,
    action: String,
    ok: bool,
    mutated: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    restored: Vec<String>,
    checks: Vec<CheckView>,
}

#[derive(Clone)]
enum SystemOp {
    RestartResolved,
    RevertLink(String),
    StopPodmanSocket,
    DisablePodmanSocket,
    StopDockerSocket,
    DisableDockerSocket,
    DisableSshd,
    StartTailscaled,
    EnableTailscaled,
}

impl SystemOp {
    fn program(&self) -> &'static str {
        match self {
            Self::RevertLink(_) => RESOLVECTL,
            _ => SYSTEMCTL,
        }
    }

    fn args(&self) -> Vec<String> {
        match self {
            Self::RestartResolved => vec!["restart".into(), "systemd-resolved.service".into()],
            Self::RevertLink(name) => vec!["revert".into(), name.clone()],
            Self::StopPodmanSocket => vec!["stop".into(), "podman.socket".into()],
            Self::DisablePodmanSocket => vec!["disable".into(), "podman.socket".into()],
            Self::StopDockerSocket => vec!["stop".into(), "docker.socket".into()],
            Self::DisableDockerSocket => vec!["disable".into(), "docker.socket".into()],
            Self::DisableSshd => vec!["disable".into(), "sshd.service".into()],
            Self::StartTailscaled => vec!["start".into(), "tailscaled.service".into()],
            Self::EnableTailscaled => vec!["enable".into(), "tailscaled.service".into()],
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::RestartResolved => "Restart systemd-resolved".into(),
            Self::RevertLink(name) => format!("Revert {name}"),
            Self::StopPodmanSocket => "Stop the rootful podman.socket".into(),
            Self::DisablePodmanSocket => "Disable the rootful podman.socket at boot".into(),
            Self::StopDockerSocket => "Stop docker.socket".into(),
            Self::DisableDockerSocket => "Disable docker.socket at boot".into(),
            Self::DisableSshd => "Disable OpenSSH at boot".into(),
            Self::StartTailscaled => "Start Tailscale".into(),
            Self::EnableTailscaled => "Enable Tailscale at boot".into(),
        }
    }
}

fn sentence(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn view(
    id: &str,
    title: &str,
    before: String,
    after: String,
    restore: String,
    drifted: bool,
    unavailable: bool,
) -> CheckView {
    let before = if !drifted && !unavailable {
        after.clone()
    } else {
        before
    };
    CheckView {
        id: id.into(),
        title: title.into(),
        drifted,
        unavailable,
        before,
        after,
        restore: if drifted { restore } else { String::new() },
    }
}

fn unavailable(id: &str, title: &str, detail: &str) -> Analysis {
    Analysis {
        view: view(
            id,
            title,
            detail.into(),
            detail.into(),
            String::new(),
            false,
            true,
        ),
        ops: Vec::new(),
        start_user: false,
    }
}

fn dns_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 80
        && token.contains('#')
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'#'))
}

fn load_catalog(path: &Path) -> Result<Catalog, DriftError> {
    let text = fs::read_to_string(path).map_err(|_| {
        quiet(format!(
            "This machine has no drift catalog at {}.",
            path.display()
        ))
    })?;
    if text.len() > CATALOG_LIMIT {
        return Err(quiet("The drift catalog is too large."));
    }
    let raw: RawCatalog =
        serde_json::from_str(&text).map_err(|_| quiet("The drift catalog is unreadable."))?;
    if raw.version != VERSION {
        return Err(quiet("The drift catalog version is not supported."));
    }
    let mut catalog = Catalog {
        dns: Vec::new(),
        quad9: false,
        podman: false,
        remote: false,
    };
    if let Some(check) = raw.checks.get(QUAD9) {
        if check.dns.is_empty() || !check.dns.iter().all(|token| dns_token(token)) {
            return Err(quiet("The Quad9 check has no resolvers."));
        }
        catalog.quad9 = true;
        catalog.dns = check.dns.clone();
    }
    catalog.podman = raw.checks.contains_key(PODMAN);
    catalog.remote = raw.checks.contains_key(REMOTE);
    if !catalog.quad9 && !catalog.podman && !catalog.remote {
        return Err(quiet("The drift catalog has no checks."));
    }
    Ok(catalog)
}

fn user_catalog_path() -> PathBuf {
    env::var_os("SEELE_DRIFT_EXPECTATIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SYSTEM_CATALOG))
}

struct Scope {
    global: bool,
    name: String,
    protocols: String,
    setting: String,
    servers: Vec<String>,
    domains: Vec<String>,
    last: LastField,
}

#[derive(Clone, Copy, PartialEq)]
enum LastField {
    None,
    Servers,
    Domains,
}

fn field_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let (key, value) = trimmed.split_once(':')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .next()
            .is_some_and(|char| char.is_ascii_alphabetic())
        || key
            .chars()
            .any(|char| !(char.is_ascii_alphabetic() || char == ' '))
    {
        return None;
    }
    Some((key, value.trim()))
}

fn tokens(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .take(32)
        .map(str::to_string)
        .collect()
}

fn link_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=15).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn parse_status(text: &str) -> Vec<Scope> {
    let text = match text.char_indices().nth(STATUS_LIMIT) {
        Some((index, _)) => &text[..index],
        None => text,
    };
    let mut scopes = Vec::new();
    let mut current: Option<Scope> = None;
    for line in text.lines() {
        if scopes.len() >= 64 {
            break;
        }
        let trimmed = line.trim();
        if trimmed == "Global" {
            if let Some(scope) = current.take() {
                scopes.push(scope);
            }
            current = Some(Scope {
                global: true,
                name: String::new(),
                protocols: String::new(),
                setting: String::new(),
                servers: Vec::new(),
                domains: Vec::new(),
                last: LastField::None,
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Link ") {
            if let Some((_, after)) = rest.split_once('(') {
                if let Some((name, _)) = after.split_once(')') {
                    if let Some(scope) = current.take() {
                        scopes.push(scope);
                    }
                    current = Some(Scope {
                        global: false,
                        name: name.trim().to_string(),
                        protocols: String::new(),
                        setting: String::new(),
                        servers: Vec::new(),
                        domains: Vec::new(),
                        last: LastField::None,
                    });
                    continue;
                }
            }
        }
        let Some(scope) = current.as_mut() else {
            continue;
        };
        if let Some((key, value)) = field_line(line) {
            match key {
                "Protocols" => scope.protocols = value.to_string(),
                "DNSOverTLS setting" => scope.setting = value.to_string(),
                "DNS Servers" => {
                    scope.servers = tokens(value);
                    scope.last = LastField::Servers;
                }
                "DNS Domain" => {
                    scope.domains = tokens(value);
                    scope.last = LastField::Domains;
                }
                _ => scope.last = LastField::None,
            }
            continue;
        }
        match scope.last {
            LastField::Servers => scope.servers.extend(tokens(trimmed)),
            LastField::Domains => scope.domains.extend(tokens(trimmed)),
            LastField::None => {}
        }
    }
    if let Some(scope) = current {
        scopes.push(scope);
    }
    scopes
}

fn strict_dot(scope: &Scope) -> Option<bool> {
    if !scope.setting.is_empty() {
        return Some(matches!(scope.setting.as_str(), "yes" | "true"));
    }
    if scope.protocols.contains("+DNSOverTLS") {
        return Some(true);
    }
    if scope.protocols.contains("-DNSOverTLS") || scope.protocols.contains("DNSOverTLS=no") {
        return Some(false);
    }
    None
}

fn plain(token: &str) -> String {
    token
        .chars()
        .filter(|char| !char.is_control())
        .take(80)
        .collect()
}

fn show_server(token: &str) -> String {
    match token.split_once('#') {
        Some((address, "dns.quad9.net")) => plain(address),
        Some((address, name)) => format!("{} ({})", plain(address), plain(name)),
        None => format!("{} unauthenticated", plain(token)),
    }
}

fn holds_root(domains: &[String]) -> bool {
    domains.iter().any(|domain| domain == "~.")
}

fn quad9_after(dns: &[String]) -> String {
    let addresses = dns
        .iter()
        .map(|token| {
            token
                .split_once('#')
                .map(|(address, _)| address)
                .unwrap_or(token)
        })
        .map(plain)
        .collect::<Vec<_>>()
        .join(", ");
    format!("Strict DNS-over-TLS through Quad9 ({addresses}), with ~. routed there")
}

fn analyze_quad9(catalog: &Catalog, reads: &impl Reads) -> Analysis {
    const TITLE: &str = "Quad9 DNS-over-TLS";
    let after = quad9_after(&catalog.dns);
    let Probe::Ready(status) = reads.resolvectl_status() else {
        return unavailable(QUAD9, TITLE, "Could not read systemd-resolved");
    };
    let scopes = parse_status(&status);
    let Some(global) = scopes.iter().find(|scope| scope.global) else {
        return unavailable(QUAD9, TITLE, "Could not read the system resolver");
    };
    let mut problems = Vec::new();
    let mut ops = Vec::new();
    let global_wrong = match strict_dot(global) {
        Some(true) => false,
        Some(false) if global.setting == "opportunistic" => {
            problems.push("DNS-over-TLS is opportunistic".into());
            true
        }
        Some(false) => {
            problems.push("DNS-over-TLS is off".into());
            true
        }
        None => {
            problems.push("DNS-over-TLS was not reported".into());
            true
        }
    };
    let live: BTreeSet<_> = global.servers.iter().cloned().collect();
    let expected: BTreeSet<_> = catalog.dns.iter().cloned().collect();
    if live != expected {
        let shown = if global.servers.is_empty() {
            "unset".into()
        } else {
            global
                .servers
                .iter()
                .take(8)
                .map(|token| show_server(token))
                .collect::<Vec<_>>()
                .join(", ")
        };
        problems.push(format!("Resolvers are {shown}"));
    }
    if !holds_root(&global.domains) {
        problems.push("Public names have no Quad9 route".into());
    }
    if global_wrong || live != expected || !holds_root(&global.domains) {
        ops.push(SystemOp::RestartResolved);
    }
    for scope in scopes
        .iter()
        .filter(|scope| !scope.global && holds_root(&scope.domains))
    {
        if link_name(&scope.name) {
            problems.push(format!("{} holds ~.", scope.name));
            ops.push(SystemOp::RevertLink(scope.name.clone()));
        } else {
            problems.push("A link holds ~.".into());
        }
    }
    let drifted = !ops.is_empty();
    Analysis {
        view: view(
            QUAD9,
            TITLE,
            sentence(problems),
            after,
            sentence(ops.iter().map(SystemOp::describe)),
            drifted,
            false,
        ),
        ops,
        start_user: false,
    }
}

fn socket_drift(
    label: &str,
    unit: &Unit,
    stop: SystemOp,
    disable: SystemOp,
) -> (Option<String>, Vec<SystemOp>) {
    if unit.not_found || (!unit.present && !unit.active && !unit.enabled) {
        return (None, Vec::new());
    }
    let mut ops = Vec::new();
    if unit.active {
        ops.push(stop);
    }
    if unit.enabled {
        ops.push(disable);
    }
    let text = match (unit.active, unit.enabled) {
        (true, true) => Some(format!("{label} is running and starts at boot")),
        (true, false) => Some(format!("{label} is running")),
        (false, true) => Some(format!("{label} starts at boot")),
        (false, false) => None,
    };
    (text, ops)
}

fn analyze_podman(reads: &impl Reads) -> Analysis {
    const TITLE: &str = "Podman stays rootless";
    const AFTER: &str =
        "Rootful Podman and Docker sockets are stopped and manual. Your Podman socket is running.";
    let root = match classify(reads.unit(false, "podman.socket")) {
        Class::Unreadable => {
            return unavailable(PODMAN, TITLE, "Could not read the rootful Podman socket");
        }
        Class::Missing => Unit {
            present: false,
            not_found: true,
            active: false,
            enabled: false,
        },
        Class::Loaded(unit) => unit,
    };
    let docker = match classify(reads.unit(false, "docker.socket")) {
        Class::Unreadable => {
            return unavailable(PODMAN, TITLE, "Could not read the Docker socket");
        }
        Class::Missing => Unit {
            present: false,
            not_found: true,
            active: false,
            enabled: false,
        },
        Class::Loaded(unit) => unit,
    };
    let user = classify(reads.unit(true, "podman.socket"));
    let (root_text, mut ops) = socket_drift(
        "Rootful podman.socket",
        &root,
        SystemOp::StopPodmanSocket,
        SystemOp::DisablePodmanSocket,
    );
    let (docker_text, docker_ops) = socket_drift(
        "docker.socket",
        &docker,
        SystemOp::StopDockerSocket,
        SystemOp::DisableDockerSocket,
    );
    ops.extend(docker_ops);
    let mut problems = Vec::new();
    if let Some(text) = root_text {
        problems.push(text);
    }
    if let Some(text) = docker_text {
        problems.push(text);
    }
    let mut start_user = false;
    match user {
        Class::Loaded(unit) if !unit.active => {
            problems.push("Your Podman socket is stopped".into());
            start_user = true;
        }
        Class::Loaded(_) => {}
        Class::Missing => problems.push("Your Podman socket is not installed".into()),
        Class::Unreadable => problems.push("Your Podman socket could not be read".into()),
    }
    let user_blocked = matches!(user, Class::Missing | Class::Unreadable);
    if problems.is_empty() {
        return Analysis {
            view: view(
                PODMAN,
                TITLE,
                String::new(),
                AFTER.into(),
                String::new(),
                false,
                false,
            ),
            ops,
            start_user: false,
        };
    }
    if ops.is_empty() && !start_user && user_blocked {
        return unavailable(PODMAN, TITLE, &problems.join(" · "));
    }
    let mut restore = ops.iter().map(SystemOp::describe).collect::<Vec<_>>();
    if start_user {
        restore.push("Start your Podman socket".into());
    }
    Analysis {
        view: view(
            PODMAN,
            TITLE,
            sentence(problems),
            AFTER.into(),
            sentence(restore),
            true,
            false,
        ),
        ops,
        start_user,
    }
}

fn remote_after(ssh_active: bool, disabling_live_session: bool) -> String {
    let ssh = if disabling_live_session {
        "OpenSSH is manual. This session keeps running."
    } else if ssh_active {
        "OpenSSH is on for this session and is manual."
    } else {
        "OpenSSH is manual."
    };
    format!("Tailscale is running and starts at boot. {ssh}")
}

fn analyze_remote(reads: &impl Reads) -> Analysis {
    const TITLE: &str = "Remote shell stays declarative";
    let tail = classify(reads.unit(false, "tailscaled.service"));
    let ssh = classify(reads.unit(false, "sshd.service"));
    let mut problems = Vec::new();
    let mut ops = Vec::new();
    match &tail {
        Class::Loaded(unit) => {
            if !unit.active {
                problems.push("Tailscale is stopped".into());
                ops.push(SystemOp::StartTailscaled);
            }
            if !unit.enabled {
                problems.push("Tailscale is disabled at boot".into());
                ops.push(SystemOp::EnableTailscaled);
            }
        }
        Class::Missing => problems.push("Tailscale is not installed".into()),
        Class::Unreadable => problems.push("Tailscale could not be read".into()),
    }
    let ssh_active = matches!(ssh, Class::Loaded(unit) if unit.active);
    let ssh_enabled = matches!(ssh, Class::Loaded(unit) if unit.enabled);
    match &ssh {
        Class::Loaded(_) if ssh_enabled => {
            problems.push("OpenSSH starts at boot".into());
            ops.push(SystemOp::DisableSshd);
        }
        Class::Loaded(_) | Class::Missing => {}
        Class::Unreadable => problems.push("OpenSSH could not be read".into()),
    }
    let tail_blocked = !matches!(tail, Class::Loaded(_));
    let ssh_blocked = matches!(ssh, Class::Unreadable);
    if ops.is_empty() && (tail_blocked || ssh_blocked) && !problems.is_empty() {
        return unavailable(REMOTE, TITLE, &sentence(problems.clone()));
    }
    if ops.is_empty()
        && problems.iter().any(|problem| {
            problem.contains("not installed") || problem.contains("could not be read")
        })
        && !ssh_enabled
    {
        return unavailable(REMOTE, TITLE, &sentence(problems));
    }
    let after = remote_after(ssh_active, ssh_enabled && ssh_active);
    let drifted = !ops.is_empty();
    Analysis {
        view: view(
            REMOTE,
            TITLE,
            sentence(problems),
            after,
            sentence(ops.iter().map(SystemOp::describe)),
            drifted,
            false,
        ),
        ops,
        start_user: false,
    }
}

fn analyze_all(catalog: &Catalog, reads: &impl Reads) -> Vec<Analysis> {
    let mut checks = Vec::new();
    if catalog.quad9 {
        checks.push(analyze_quad9(catalog, reads));
    }
    if catalog.podman {
        checks.push(analyze_podman(reads));
    }
    if catalog.remote {
        checks.push(analyze_remote(reads));
    }
    checks
}

fn inspect(catalog: &Catalog, reads: &impl Reads) -> Report {
    Report {
        version: VERSION,
        action: "diff".into(),
        ok: true,
        mutated: false,
        error: String::new(),
        restored: Vec::new(),
        checks: analyze_all(catalog, reads)
            .into_iter()
            .map(|analysis| analysis.view)
            .collect(),
    }
}

fn user_podman_down(reads: &impl Reads) -> Result<bool, DriftError> {
    match classify(reads.unit(true, "podman.socket")) {
        Class::Loaded(unit) => Ok(!unit.active),
        Class::Missing => Ok(false),
        Class::Unreadable => Err(quiet("Could not read your Podman socket.")),
    }
}

fn apply(catalog: &Catalog, ids: &[String], machine: &mut impl Acts) -> Result<Report, DriftError> {
    if ids.is_empty() {
        return Err(quiet("Name the checks to restore."));
    }
    let mut ordered = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids {
        if !valid_id(id) || !catalog.enabled(id) {
            return Err(quiet(format!("Unknown check {id}.")));
        }
        if seen.insert(id.clone()) {
            ordered.push(id.clone());
        }
    }
    let planned = analyze_all(catalog, machine);
    let mut restored = Vec::new();
    let mut mutated = false;
    for id in &ordered {
        let Some(item) = planned.iter().find(|item| item.view.id == *id) else {
            continue;
        };
        if !item.view.drifted {
            continue;
        }
        if item.start_user && user_podman_down(machine)? {
            if let Err(message) = machine.start_user_podman() {
                return Err(DriftError {
                    message,
                    mutated: true,
                    restored: restored.clone(),
                });
            }
            mutated = true;
        }
        if !item.ops.is_empty() {
            if let Err(message) = machine.elevate(id) {
                return Err(DriftError {
                    message,
                    mutated: true,
                    restored: restored.clone(),
                });
            }
            mutated = true;
        }
        restored.push(id.clone());
    }
    let mut report = inspect(catalog, machine);
    report.action = "apply".into();
    report.mutated = mutated;
    report.restored = restored;
    Ok(report)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn system_plan(catalog: &Catalog, id: &str, reads: &impl Reads) -> Vec<SystemOp> {
    analyze_all(catalog, reads)
        .into_iter()
        .find(|item| item.view.id == id)
        .map(|item| item.ops)
        .unwrap_or_default()
}

fn run_program(program: &str, args: &[String], timeout: Duration) -> bool {
    let mut command = Command::new(program);
    command.args(args);
    process::discard(
        &mut command,
        b"",
        Limits { timeout, output: 0 },
        &command::shutdown_signal(),
    )
    .is_ok_and(|status| status.success())
}

fn helper_path() -> Result<PathBuf, String> {
    use std::os::unix::fs::MetadataExt;
    let executable =
        fs::canonicalize(env::current_exe().map_err(|_| "The drift helper is missing.")?)
            .map_err(|_| "The drift helper is missing.")?;
    let path = executable
        .parent()
        .ok_or("The drift helper is missing.")?
        .join(HELPER_NAME);
    let metadata = fs::metadata(&path).map_err(|_| "The drift helper is missing.")?;
    if metadata.is_file() && metadata.mode() & 0o111 != 0 && metadata.mode() & 0o022 == 0 {
        Ok(path)
    } else {
        Err("The drift helper is missing.".into())
    }
}

struct Live;

impl Reads for Live {
    fn resolvectl_status(&self) -> Probe<String> {
        match command::output("resolvectl", ["status"]) {
            Some(text) => Probe::Ready(text),
            None => Probe::Failed,
        }
    }

    fn unit(&self, user: bool, name: &str) -> Probe<Unit> {
        let mut args = Vec::new();
        if user {
            args.push("--user".to_string());
        }
        args.extend(
            [
                "show",
                "-p",
                "LoadState",
                "-p",
                "ActiveState",
                "-p",
                "UnitFileState",
                "--",
                name,
            ]
            .into_iter()
            .map(str::to_string),
        );
        let Some(text) = command::output("systemctl", &args) else {
            return Probe::Failed;
        };
        parse_show(&text).map(Probe::Ready).unwrap_or(Probe::Failed)
    }
}

impl Acts for Live {
    fn elevate(&mut self, id: &str) -> Result<(), String> {
        if !matches!(id, QUAD9 | PODMAN | REMOTE) {
            return Err(format!("Unknown check {id}."));
        }
        let helper = helper_path()?;
        let args = vec![helper.to_string_lossy().into_owned(), id.to_string()];
        if run_program("run0", &args, Duration::from_secs(150)) {
            Ok(())
        } else {
            Err("The restore was not authorized.".into())
        }
    }

    fn start_user_podman(&mut self) -> Result<(), String> {
        let args = ["--user", "start", "podman.socket"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if run_program("systemctl", &args, Duration::from_secs(30)) {
            Ok(())
        } else {
            Err("Could not start your Podman socket.".into())
        }
    }
}

fn parse_show(text: &str) -> Option<Unit> {
    let mut load = None;
    let mut active = None;
    let mut file = None;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        match key {
            "LoadState" => load = Some(value.to_string()),
            "ActiveState" => active = Some(value.to_string()),
            "UnitFileState" => file = Some(value.to_string()),
            _ => {}
        }
    }
    let load = load?;
    let active = active.unwrap_or_default();
    let file = file.unwrap_or_default();
    Some(Unit {
        present: load == "loaded",
        not_found: load == "not-found",
        active: active == "active",
        enabled: file == "enabled" || file == "enabled-runtime",
    })
}

fn emit(result: Result<Report, DriftError>) -> Result<(), String> {
    match result {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Err(error) => {
            let body = json!({
                "version": VERSION,
                "action": "diff",
                "ok": false,
                "mutated": error.mutated,
                "error": error.message,
                "restored": error.restored,
                "checks": [],
            });
            println!("{body}");
            Err(error.message)
        }
    }
}

pub fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "diff".into());
    match command.as_str() {
        "diff" => {
            if args.next().is_some() {
                return emit(Err(quiet("diff takes no arguments.")));
            }
            let catalog = match load_catalog(&user_catalog_path()) {
                Ok(catalog) => catalog,
                Err(error) => return emit(Err(error)),
            };
            emit(Ok(inspect(&catalog, &Live)))
        }
        "apply" => {
            let ids = args.collect::<Vec<_>>();
            let catalog = match load_catalog(&user_catalog_path()) {
                Ok(catalog) => catalog,
                Err(error) => {
                    let body = json!({
                        "version": VERSION,
                        "action": "apply",
                        "ok": false,
                        "mutated": false,
                        "error": error.message,
                        "restored": [],
                        "checks": [],
                    });
                    println!("{body}");
                    return Err(error.message);
                }
            };
            let mut live = Live;
            match apply(&catalog, &ids, &mut live) {
                Ok(report) => emit(Ok(report)),
                Err(mut error) => {
                    let body = json!({
                        "version": VERSION,
                        "action": "apply",
                        "ok": false,
                        "mutated": error.mutated,
                        "error": error.message,
                        "restored": error.restored,
                        "checks": [],
                    });
                    println!("{body}");
                    let message = std::mem::take(&mut error.message);
                    Err(message)
                }
            }
        }
        _ => emit(Err(quiet("Use diff or apply."))),
    }
}

pub fn restore() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(id) = args.next() else {
        return Err("Name one check to restore.".into());
    };
    if args.next().is_some() || !matches!(id.as_str(), QUAD9 | PODMAN | REMOTE) {
        return Err("Name one of the drift checks.".into());
    }
    let catalog = load_catalog(Path::new(SYSTEM_CATALOG)).map_err(|error| error.message)?;
    if !catalog.enabled(&id) {
        return Err("The drift catalog has no such check.".into());
    }
    let ops = system_plan(&catalog, &id, &Live);
    for op in &ops {
        if !run_program(op.program(), &op.args(), Duration::from_secs(30)) {
            let body = json!({"ok": false, "id": id, "error": "restore failed"});
            println!("{body}");
            return Err(format!("Could not restore {id}."));
        }
    }
    println!(
        "{}",
        json!({"ok": true, "id": id, "mutated": !ops.is_empty()})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const DNS: &[&str] = &[
        "9.9.9.9#dns.quad9.net",
        "149.112.112.112#dns.quad9.net",
        "2620:fe::fe#dns.quad9.net",
        "2620:fe::9#dns.quad9.net",
    ];

    const HEALTHY: &str = "\
Global
           Protocols: -LLMNR -mDNS +DNSOverTLS DNSSEC=no/unsupported
    resolv.conf mode: stub
Current DNS Server: 9.9.9.9#dns.quad9.net
       DNS Servers: 9.9.9.9#dns.quad9.net 149.112.112.112#dns.quad9.net
                    2620:fe::fe#dns.quad9.net 2620:fe::9#dns.quad9.net
        DNS Domain: ~.

Link 2 (enp6s0)
    Current Scopes: DNS
         Protocols: -DefaultRoute -LLMNR -mDNS -DNSOverTLS DNSSEC=no/unsupported
Current DNS Server: 192.168.1.1
       DNS Servers: 192.168.1.1
        DNS Domain: home

Link 3 (tailscale0)
    Current Scopes: DNS
         Protocols: -DefaultRoute -LLMNR -mDNS -DNSOverTLS DNSSEC=no/unsupported
Current DNS Server: 100.100.100.100
       DNS Servers: 100.100.100.100
        DNS Domain: ~ts.net
";

    const DRIFTED_DNS: &str = "\
Global
           Protocols: -LLMNR -mDNS -DNSOverTLS DNSSEC=no/unsupported
    resolv.conf mode: stub
Current DNS Server: 192.168.1.1
       DNS Servers: 192.168.1.1
        DNS Domain: ~.

Link 2 (enp6s0)
    Current Scopes: DNS
         Protocols: +DefaultRoute -LLMNR -mDNS -DNSOverTLS DNSSEC=no/unsupported
Current DNS Server: 192.168.1.1
       DNS Servers: 192.168.1.1
        DNS Domain: ~. home
";

    fn catalog() -> Catalog {
        Catalog {
            dns: DNS.iter().map(|token| (*token).to_string()).collect(),
            quad9: true,
            podman: true,
            remote: true,
        }
    }

    fn loaded(active: bool, enabled: bool) -> Unit {
        Unit {
            present: true,
            not_found: false,
            active,
            enabled,
        }
    }

    fn missing() -> Unit {
        Unit {
            present: false,
            not_found: true,
            active: false,
            enabled: false,
        }
    }

    struct Fake {
        status: Probe<String>,
        units: HashMap<(bool, String), Probe<Unit>>,
        elevated: Vec<String>,
        started_user: u32,
        fail_elevate: bool,
    }

    impl Fake {
        fn healthy() -> Self {
            let mut units = HashMap::new();
            units.insert((false, "podman.socket".into()), Probe::Ready(missing()));
            units.insert((false, "docker.socket".into()), Probe::Ready(missing()));
            units.insert(
                (true, "podman.socket".into()),
                Probe::Ready(loaded(true, true)),
            );
            units.insert(
                (false, "tailscaled.service".into()),
                Probe::Ready(loaded(true, true)),
            );
            units.insert(
                (false, "sshd.service".into()),
                Probe::Ready(loaded(false, false)),
            );
            Self {
                status: Probe::Ready(HEALTHY.into()),
                units,
                elevated: Vec::new(),
                started_user: 0,
                fail_elevate: false,
            }
        }
    }

    impl Reads for Fake {
        fn resolvectl_status(&self) -> Probe<String> {
            match &self.status {
                Probe::Ready(text) => Probe::Ready(text.clone()),
                Probe::Failed => Probe::Failed,
            }
        }

        fn unit(&self, user: bool, name: &str) -> Probe<Unit> {
            match self.units.get(&(user, name.to_string())) {
                Some(Probe::Ready(unit)) => Probe::Ready(*unit),
                Some(Probe::Failed) | None => Probe::Failed,
            }
        }
    }

    impl Acts for Fake {
        fn elevate(&mut self, id: &str) -> Result<(), String> {
            self.elevated.push(id.to_string());
            if self.fail_elevate {
                Err("not authorized".into())
            } else {
                Ok(())
            }
        }

        fn start_user_podman(&mut self) -> Result<(), String> {
            self.started_user += 1;
            Ok(())
        }
    }

    #[test]
    fn dry_run_reads_without_recording_a_change() {
        let machine = Fake::healthy();
        let report = inspect(&catalog(), &machine);
        assert!(!report.mutated);
        assert_eq!(report.action, "diff");
        assert!(report.checks.iter().all(|check| !check.drifted));
        assert!(machine.elevated.is_empty());
        assert_eq!(machine.started_user, 0);
        let quad9 = report
            .checks
            .iter()
            .find(|check| check.id == QUAD9)
            .unwrap();
        assert_eq!(quad9.before, quad9.after);
        assert!(quad9.after.contains("Quad9"));
    }

    #[test]
    fn quad9_drift_names_the_live_resolver_and_the_flake() {
        let mut machine = Fake::healthy();
        machine.status = Probe::Ready(DRIFTED_DNS.into());
        let report = inspect(&catalog(), &machine);
        let quad9 = report
            .checks
            .iter()
            .find(|check| check.id == QUAD9)
            .unwrap();
        assert!(quad9.drifted);
        assert!(quad9.before.contains("DNS-over-TLS is off"));
        assert!(quad9.before.contains("192.168.1.1"));
        assert!(quad9.before.contains("enp6s0 holds ~."));
        assert!(quad9.after.contains("Strict DNS-over-TLS through Quad9"));
        assert!(quad9.restore.contains("Restart systemd-resolved"));
        assert!(quad9.restore.contains("Revert enp6s0"));
        assert!(machine.elevated.is_empty());
    }

    #[test]
    fn opportunistic_dns_is_not_the_strict_flake_mode() {
        let mut machine = Fake::healthy();
        machine.status = Probe::Ready(
            "Global\nDNSOverTLS setting: opportunistic\nProtocols: +DNSOverTLS\nDNS Servers: 9.9.9.9#dns.quad9.net 149.112.112.112#dns.quad9.net 2620:fe::fe#dns.quad9.net 2620:fe::9#dns.quad9.net\nDNS Domain: ~.\n"
                .into(),
        );
        let item = analyze_quad9(&catalog(), &machine);
        assert!(item.view.drifted);
        assert!(item.view.before.contains("opportunistic"));
        assert!(item
            .ops
            .iter()
            .any(|op| matches!(op, SystemOp::RestartResolved)));
    }

    #[test]
    fn apply_restores_only_the_named_drifted_check() {
        let mut machine = Fake::healthy();
        machine.status = Probe::Ready(DRIFTED_DNS.into());
        machine.units.insert(
            (false, "podman.socket".into()),
            Probe::Ready(loaded(true, true)),
        );
        machine.units.insert(
            (true, "podman.socket".into()),
            Probe::Ready(loaded(false, true)),
        );
        machine.units.insert(
            (false, "sshd.service".into()),
            Probe::Ready(loaded(true, true)),
        );
        machine.units.insert(
            (false, "tailscaled.service".into()),
            Probe::Ready(loaded(false, false)),
        );
        let report = apply(&catalog(), &[PODMAN.into()], &mut machine).unwrap();
        assert!(report.mutated);
        assert_eq!(machine.elevated, vec![PODMAN.to_string()]);
        assert_eq!(machine.started_user, 1);
        assert_eq!(report.restored, vec![PODMAN.to_string()]);
    }

    #[test]
    fn apply_of_a_matching_check_does_not_change_anything() {
        let mut machine = Fake::healthy();
        let report = apply(&catalog(), &[QUAD9.into(), REMOTE.into()], &mut machine).unwrap();
        assert!(!report.mutated);
        assert!(machine.elevated.is_empty());
        assert_eq!(machine.started_user, 0);
        assert!(report.restored.is_empty());
    }

    #[test]
    fn unknown_or_empty_apply_mutates_nothing() {
        let mut machine = Fake::healthy();
        machine.status = Probe::Ready(DRIFTED_DNS.into());
        assert!(apply(&catalog(), &[], &mut machine).is_err());
        assert!(apply(&catalog(), &["nope".into()], &mut machine).is_err());
        assert!(machine.elevated.is_empty());
        assert_eq!(machine.started_user, 0);
    }

    #[test]
    fn a_failed_restore_does_not_continue_into_later_checks() {
        let mut machine = Fake::healthy();
        machine.status = Probe::Ready(DRIFTED_DNS.into());
        machine.units.insert(
            (false, "sshd.service".into()),
            Probe::Ready(loaded(false, true)),
        );
        machine.fail_elevate = true;
        let error = apply(&catalog(), &[QUAD9.into(), REMOTE.into()], &mut machine).unwrap_err();
        assert!(error.mutated);
        assert_eq!(machine.elevated, vec![QUAD9.to_string()]);
        assert!(error.restored.is_empty());
    }

    #[test]
    fn openssh_restore_disables_boot_and_leaves_a_running_session() {
        let mut machine = Fake::healthy();
        machine.units.insert(
            (false, "sshd.service".into()),
            Probe::Ready(loaded(true, true)),
        );
        let item = analyze_remote(&machine);
        assert!(item.view.drifted);
        assert!(item.view.before.contains("OpenSSH starts at boot"));
        assert!(item.view.after.contains("This session keeps running"));
        assert!(item
            .ops
            .iter()
            .any(|op| matches!(op, SystemOp::DisableSshd)));
        assert!(!item
            .ops
            .iter()
            .any(|op| op.args().iter().any(|arg| arg == "stop")));
        let disable = SystemOp::DisableSshd;
        assert_eq!(
            disable.args(),
            vec!["disable".to_string(), "sshd.service".to_string()]
        );
    }

    #[test]
    fn rootful_podman_stop_and_disable_are_fixed_commands() {
        let mut machine = Fake::healthy();
        machine.units.insert(
            (false, "podman.socket".into()),
            Probe::Ready(loaded(true, true)),
        );
        let item = analyze_podman(&machine);
        let args = item.ops.iter().map(|op| op.args()).collect::<Vec<_>>();
        assert!(args
            .iter()
            .any(|argv| argv == &vec!["stop".to_string(), "podman.socket".to_string()]));
        assert!(args
            .iter()
            .any(|argv| argv == &vec!["disable".to_string(), "podman.socket".to_string()]));
        assert!(item
            .ops
            .iter()
            .all(|op| op.program() == SYSTEMCTL || op.program() == RESOLVECTL));
    }

    #[test]
    fn a_link_name_cannot_become_a_command() {
        assert!(!link_name("enp6s0;reboot"));
        assert!(!link_name("-bad"));
        assert!(link_name("enp6s0"));
        assert!(link_name("tailscale0"));
        let revert = SystemOp::RevertLink("enp6s0".into());
        assert_eq!(revert.program(), RESOLVECTL);
        assert_eq!(
            revert.args(),
            vec!["revert".to_string(), "enp6s0".to_string()]
        );
    }

    #[test]
    fn catalog_rejects_a_resolver_that_is_not_a_token() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("seele-drift-catalog-{}", std::process::id()));
        fs::write(
            &path,
            r#"{"version":1,"checks":{"quad9-dot":{"dns":["9.9.9.9;id"]}}}"#,
        )
        .unwrap();
        assert!(load_catalog(&path).is_err());
        let _ = fs::remove_file(path);
    }
}
