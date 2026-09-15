//! The privileged half of the port inspector.
//!
//! Every path this executable uses is fixed by the executable itself. It
//! accepts a narrowly typed target — never a command, a path or an argument
//! list — and resolves that target from the kernel again after authentication,
//! because a confirmation can stay open long enough for a PID to be recycled or
//! a port to be rebound by something else entirely.
use crate::procfs::{self, Listener, Process, Roots};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;

pub type Result<T = Value> = std::result::Result<T, &'static str>;

const SYSTEMCTL: &str = "/run/current-system/sw/bin/systemctl";
const MAX_OWNERS: usize = 16;
const NAME_LIMIT: usize = 64;
const PATH_LIMIT: usize = 256;

/// The two effects this helper is allowed to have. Everything else it does is
/// a read.
pub trait System {
    fn systemctl(&mut self, arguments: &[&str]) -> bool;
    fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool;
}

fn clip(value: &str, limit: usize) -> String {
    match value.char_indices().nth(limit) {
        Some((index, _)) => value[..index].to_owned(),
        None => value.to_owned(),
    }
}

/// Strict decimal syntax. A leading zero, a sign, whitespace or a trailing
/// character is a rejected argument rather than a tolerated one.
fn decimal(value: &str, limit: usize) -> Result<u64> {
    if value.is_empty()
        || value.len() > limit
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("invalid_target");
    }
    value.parse().map_err(|_| "invalid_target")
}

/// A binding is compared against the kernel's own rendering of the socket and
/// never used as a path or an argument, but it is still bounded to the
/// characters an address can contain.
fn binding(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || b"[].:".contains(&byte))
    {
        return Err("invalid_target");
    }
    Ok(value)
}

fn port(value: &str) -> Result<u16> {
    u16::try_from(decimal(value, 5)?)
        .ok()
        .filter(|port| *port > 0)
        .ok_or("invalid_target")
}

/// A service this helper will stop. The user's own session manager is excluded
/// deliberately: stopping it would end the session rather than a listener.
fn service(value: &str) -> Result<&str> {
    if !procfs::valid_unit(value)
        || !value.ends_with(".service")
        || (value.starts_with("user@") && value.ends_with(".service"))
    {
        return Err("invalid_target");
    }
    Ok(value)
}

fn listener(roots: &Roots, inode: u64, address: &str, number: u16) -> Result<Listener> {
    procfs::find_listener(roots, inode, address, number).ok_or("gone")
}

fn owners(roots: &Roots, inode: u64) -> Vec<Process> {
    let wanted = HashSet::from([inode]);
    let users = procfs::users(roots);
    procfs::socket_owners(roots, &wanted)
        .remove(&inode)
        .unwrap_or_default()
        .into_iter()
        .take(MAX_OWNERS)
        .filter_map(|pid| procfs::process(roots, pid, &users))
        .collect()
}

fn owner_value(owner: &Process) -> Value {
    json!({
        "pid": owner.pid,
        "start": owner.start,
        "name": clip(&owner.name, NAME_LIMIT),
        "uid": owner.uid,
        "user": clip(&owner.user, NAME_LIMIT),
        "unit": clip(&owner.unit, NAME_LIMIT * 4),
        "scope": owner.scope,
        "service": clip(&owner.service, NAME_LIMIT * 4),
        "cwd": clip(&owner.cwd, PATH_LIMIT),
        "project": clip(&owner.project, NAME_LIMIT),
        "projectPath": clip(&owner.project_path, PATH_LIMIT),
        "proxy": owner.proxy,
    })
}

/// Resolve the owners of a listener the desktop user cannot see.
///
/// `/proc/<pid>/fd` is readable only by the process owner, so another user's
/// listener has no identifiable owner without this. It is a deliberate,
/// separately authorized read: nothing is signalled or stopped here, and the
/// result is what the confirmation afterwards discloses.
pub fn identify(roots: &Roots, inode: u64, address: &str, number: u16, uid: u32) -> Result {
    let found = listener(roots, inode, address, number)?;
    // The reviewed row said whose listener this is. If that changed, the row
    // the user was looking at is not the socket in front of us.
    if found.uid != uid {
        return Err("changed");
    }
    let resolved = owners(roots, inode);
    Ok(json!({
        "ok": true,
        "owners": resolved.iter().map(owner_value).collect::<Vec<_>>(),
    }))
}

/// Stop the service that owns the listener.
///
/// Every process holding the socket has to belong to the reviewed service.
/// Otherwise the confirmation named a target that would not free the port, and
/// stopping it would interrupt something the user was never shown.
pub fn stop_service(
    roots: &Roots,
    system: &mut dyn System,
    force: bool,
    unit: &str,
    inode: u64,
    address: &str,
    number: u16,
) -> Result {
    listener(roots, inode, address, number)?;
    let resolved = owners(roots, inode);
    if resolved.is_empty()
        || resolved
            .iter()
            .any(|owner| owner.service != unit || owner.scope != "system")
    {
        return Err("changed");
    }
    // Force stays inside the reviewed unit. It never falls back to a PID, which
    // by this point may belong to something else entirely.
    let arguments: Vec<&str> = if force {
        vec!["kill", "--kill-whom=all", "--signal=SIGKILL", "--", unit]
    } else {
        vec!["stop", "--", unit]
    };
    if !system.systemctl(&arguments) {
        return Err("failed");
    }
    Ok(json!({"ok": true, "target": "service", "unit": unit}))
}

/// Stop the exact process that was reviewed.
///
/// Pin the process before repeating identity checks, so PID reuse between
/// validation and signalling cannot redirect the action.
#[allow(clippy::too_many_arguments)]
pub fn stop_process(
    roots: &Roots,
    system: &mut dyn System,
    force: bool,
    pid: u32,
    start: u64,
    uid: u32,
    inode: u64,
    address: &str,
    number: u16,
) -> Result {
    if pid < 2 {
        return Err("invalid_target");
    }
    listener(roots, inode, address, number)?;
    if procfs::start_time(roots, pid) != Some(start)
        || procfs::process_uid(roots, pid) != Some(uid)
        || !procfs::holds_socket(roots, pid, inode)
    {
        return Err("changed");
    }
    if !system.signal(
        pid,
        if force { libc::SIGKILL } else { libc::SIGTERM },
        &mut || {
            procfs::start_time(roots, pid) == Some(start)
                && procfs::process_uid(roots, pid) == Some(uid)
                && procfs::holds_socket(roots, pid, inode)
        },
    ) {
        return Err("failed");
    }
    Ok(json!({"ok": true, "target": "process", "pid": pid}))
}

fn mode(value: &str) -> Result<bool> {
    match value {
        "graceful" => Ok(false),
        "force" => Ok(true),
        _ => Err("invalid_target"),
    }
}

/// The complete argument surface. Anything else is rejected before a single
/// file is read.
pub fn dispatch(arguments: &[String], roots: &Roots, system: &mut dyn System) -> Result {
    let argument = |index: usize| arguments.get(index).map(String::as_str).unwrap_or("");
    match (argument(0), arguments.len()) {
        ("identify", 5) => identify(
            roots,
            decimal(argument(1), 20)?,
            binding(argument(2))?,
            port(argument(3))?,
            u32::try_from(decimal(argument(4), 10)?).map_err(|_| "invalid_target")?,
        ),
        ("service", 6) => stop_service(
            roots,
            system,
            mode(argument(1))?,
            service(argument(2))?,
            decimal(argument(3), 20)?,
            binding(argument(4))?,
            port(argument(5))?,
        ),
        ("process", 8) => stop_process(
            roots,
            system,
            mode(argument(1))?,
            u32::try_from(decimal(argument(2), 10)?).map_err(|_| "invalid_target")?,
            decimal(argument(3), 20)?,
            u32::try_from(decimal(argument(4), 10)?).map_err(|_| "invalid_target")?,
            decimal(argument(5), 20)?,
            binding(argument(6))?,
            port(argument(7))?,
        ),
        _ => Err("invalid_target"),
    }
}

pub fn exit_code(error: &str) -> i32 {
    match error {
        "invalid_target" => 64,
        "changed" => 65,
        "gone" => 66,
        _ => 67,
    }
}

/// The host's two effects, with a clean environment and fixed absolute paths.
pub struct Host {
    cancel: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Host {
    pub fn new() -> Self {
        Self {
            cancel: seele_runtime::process::termination_signal()
                .unwrap_or_else(|_| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
        }
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

/// The running system's own systemctl, checked for the ownership and
/// permissions a root-executed program must have before it is executed.
fn trusted(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && metadata.uid() == 0
            && metadata.mode() & 0o022 == 0
            && metadata.mode() & 0o111 != 0
    })
}

impl System for Host {
    fn systemctl(&mut self, arguments: &[&str]) -> bool {
        let path = Path::new(SYSTEMCTL);
        if !trusted(path) {
            return false;
        }
        let mut command = std::process::Command::new(path);
        command
            .env_clear()
            .env("PATH", "/run/current-system/sw/bin")
            .env("LANG", "C.UTF-8")
            .current_dir("/")
            .args(arguments);
        seele_runtime::process::discard(
            &mut command,
            b"",
            seele_runtime::process::Limits {
                timeout: std::time::Duration::from_secs(150),
                output: 0,
            },
            &*self.cancel,
        )
        .is_ok_and(|status| status.success())
    }

    fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool {
        seele_runtime::process::signal_checked(pid, number, validate)
    }
}

/// One JSON line on stdout either way, so a caller can tell a refused action
/// apart from an authentication that never happened at all.
pub fn main() -> std::process::ExitCode {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } != 0 {
        println!("{}", json!({"ok": false, "error": "not_privileged"}));
        return std::process::ExitCode::from(64);
    }
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let mut host = Host::new();
    match dispatch(&arguments, &Roots::default(), &mut host) {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!({"ok": false, "error": error}));
            std::process::ExitCode::from(exit_code(error) as u8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        units: Vec<Vec<String>>,
        signals: Vec<(u32, i32)>,
    }
    impl System for Recorder {
        fn systemctl(&mut self, arguments: &[&str]) -> bool {
            self.units
                .push(arguments.iter().map(|value| (*value).to_owned()).collect());
            true
        }
        fn signal(&mut self, pid: u32, number: i32, validate: &mut dyn FnMut() -> bool) -> bool {
            if !validate() {
                return false;
            }
            self.signals.push((pid, number));
            true
        }
    }

    /// A root with no process table. Nothing in this module's own tests reads
    /// the host's `/proc`, authenticates or runs a system manager.
    fn roots() -> Roots {
        Roots {
            proc: std::path::PathBuf::from("/nonexistent/seele-ports/proc"),
            passwd: std::path::PathBuf::from("/nonexistent/seele-ports/passwd"),
        }
    }

    #[test]
    fn rejects_every_argument_shape_that_is_not_a_typed_target() {
        let roots = roots();
        let mut recorder = Recorder::default();
        for arguments in [
            vec![],
            vec!["identify"],
            vec!["service", "graceful", "nginx.service"],
            vec![
                "service",
                "reboot",
                "nginx.service",
                "1",
                "0.0.0.0:80",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "nginx.socket",
                "1",
                "0.0.0.0:80",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "user@1000.service",
                "1",
                "0.0.0.0:80",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "a;reboot.service",
                "1",
                "0.0.0.0:80",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "nginx.service",
                "01",
                "0.0.0.0:80",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "nginx.service",
                "-1",
                "0.0.0.0:80",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "nginx.service",
                "1",
                "0.0.0.0:80; rm",
                "80",
            ],
            vec![
                "service",
                "graceful",
                "nginx.service",
                "1",
                "0.0.0.0:80",
                "0",
            ],
            vec![
                "service",
                "graceful",
                "nginx.service",
                "1",
                "0.0.0.0:80",
                "65536",
            ],
            vec![
                "process",
                "graceful",
                "1",
                "1",
                "0",
                "1",
                "0.0.0.0:80",
                "80",
            ],
            vec!["shutdown", "now"],
        ] {
            let values = arguments
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<String>>();
            let outcome = match &dispatch(&values, &roots, &mut recorder) {
                Ok(_) => "accepted",
                Err(error) => error,
            };
            assert!(
                matches!(outcome, "invalid_target" | "gone"),
                "{values:?} was not rejected: {outcome}"
            );
        }
        assert!(recorder.units.is_empty() && recorder.signals.is_empty());
    }

    #[test]
    fn strict_decimal_and_binding_syntax() {
        assert_eq!(decimal("41253", 20), Ok(41253));
        assert_eq!(decimal("0", 20), Ok(0));
        for bad in [
            "",
            "01",
            "+1",
            " 1",
            "1 ",
            "1e3",
            "-1",
            "18446744073709551616",
        ] {
            assert!(decimal(bad, 20).is_err(), "{bad} must be rejected");
        }
        assert_eq!(binding("[fd00::5]:3000"), Ok("[fd00::5]:3000"));
        assert_eq!(binding("127.0.0.1:8080"), Ok("127.0.0.1:8080"));
        for bad in ["", "127.0.0.1:8080 ", "$(reboot)", "host:80"] {
            assert!(binding(bad).is_err(), "{bad} must be rejected");
        }
        assert_eq!(port("65535"), Ok(65535));
        assert!(port("0").is_err() && port("65536").is_err());
    }

    #[test]
    fn every_refusal_keeps_its_own_exit_code() {
        assert_eq!(exit_code("invalid_target"), 64);
        assert_eq!(exit_code("changed"), 65);
        assert_eq!(exit_code("gone"), 66);
        assert_eq!(exit_code("failed"), 67);
    }
}
