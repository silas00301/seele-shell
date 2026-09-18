//! The kernel interfaces behind the port inspector.
//!
//! The unprivileged worker and the privileged stop helper read exactly these
//! files, so this module stays free of the tools library and of every
//! dependency but libc and the shared runtime's bounded reader. Every read is
//! bounded and every failure leaves its field explicitly unknown rather than
//! guessing an owner.
//!
//! Both executables include this file directly and each uses a different part
//! of it, so what one of them does not call is not unused code.
#![allow(dead_code)]
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

/// A refresh runs while a panel is open, so every scan is bounded: a broken or
/// hostile process table must not turn one refresh into an unbounded walk.
pub const MAX_LISTENERS: usize = 512;
const MAX_PROCESSES: usize = 4096;
const MAX_DESCRIPTORS: usize = 1024;
const MAX_OWNERS: usize = 16;
const FILE_LIMIT: usize = 512 * 1024;
const PROJECT_DEPTH: usize = 12;

/// Every path discovery reads. Tests supply a synthetic tree; production uses
/// the host's own `/proc`, which is also the network namespace this inspector
/// is scoped to.
#[derive(Clone, Debug)]
pub struct Roots {
    pub proc: PathBuf,
    pub passwd: PathBuf,
}

impl Default for Roots {
    fn default() -> Self {
        Self {
            proc: PathBuf::from("/proc"),
            passwd: PathBuf::from("/etc/passwd"),
        }
    }
}

fn read(path: &Path) -> Option<String> {
    let bytes = seele_runtime::fs::read_bounded(path, FILE_LIMIT, false).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A TCP listening socket exactly as the kernel reports it. The binding is
/// never normalized here: a wildcard stays a wildcard in the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listener {
    pub address: IpAddr,
    pub port: u16,
    pub uid: u32,
    pub inode: u64,
}

impl Listener {
    /// The socket inode is the row's identity. A listener that goes away and
    /// another that binds the same port afterwards are different rows, so a
    /// confirmation left open cannot act on the replacement.
    pub fn id(&self) -> String {
        format!(
            "{}:{}",
            if self.address.is_ipv4() { 4 } else { 6 },
            self.inode
        )
    }

    pub fn family(&self) -> &'static str {
        if self.address.is_ipv4() {
            "ipv4"
        } else {
            "ipv6"
        }
    }

    pub fn wildcard(&self) -> bool {
        match self.address {
            IpAddr::V4(address) => address.is_unspecified(),
            IpAddr::V6(address) => {
                address.is_unspecified()
                    || address
                        .to_ipv4_mapped()
                        .is_some_and(|v4| v4.is_unspecified())
            }
        }
    }

    pub fn loopback(&self) -> bool {
        match self.address {
            IpAddr::V4(address) => address.is_loopback(),
            IpAddr::V6(address) => {
                address.is_loopback() || address.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
            }
        }
    }

    /// `loopback`, `wildcard` or `address`: what the binding actually reaches,
    /// which is not the same question as which host a browser should be sent
    /// to.
    pub fn scope(&self) -> &'static str {
        if self.loopback() {
            "loopback"
        } else if self.wildcard() {
            "wildcard"
        } else {
            "address"
        }
    }

    /// The host part of a URL that actually reaches this listener, already
    /// bracketed where a colon would otherwise be read as the port separator.
    /// A wildcard binding is converted to the loopback address of its own
    /// family rather than to `localhost`, which resolves to whichever family
    /// the resolver prefers and can miss a single-family listener entirely.
    pub fn destination(&self) -> String {
        match self.address {
            IpAddr::V4(_) if self.wildcard() || self.loopback() => "127.0.0.1".to_owned(),
            IpAddr::V4(address) => address.to_string(),
            IpAddr::V6(address) => {
                if let Some(mapped) = address.to_ipv4_mapped() {
                    if mapped.is_unspecified() || mapped.is_loopback() {
                        "127.0.0.1".to_owned()
                    } else {
                        mapped.to_string()
                    }
                } else if self.wildcard() || self.loopback() {
                    "[::1]".to_owned()
                } else {
                    format!("[{address}]")
                }
            }
        }
    }

    /// The binding as it is written, which is what the row shows.
    pub fn binding(&self) -> String {
        match self.address {
            IpAddr::V4(address) => format!("{address}:{}", self.port),
            IpAddr::V6(address) => format!("[{address}]:{}", self.port),
        }
    }
}

fn hex_address(text: &str) -> Option<IpAddr> {
    // Each 32-bit word is printed in host order, so on a little-endian machine
    // every word's bytes are reversed against the wire order.
    match text.len() {
        8 => Some(IpAddr::V4(Ipv4Addr::from(
            u32::from_str_radix(text, 16).ok()?.swap_bytes(),
        ))),
        32 => {
            let mut octets = [0u8; 16];
            for index in 0..4 {
                let word = u32::from_str_radix(&text[index * 8..index * 8 + 8], 16).ok()?;
                octets[index * 4..index * 4 + 4].copy_from_slice(&word.swap_bytes().to_be_bytes());
            }
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

/// Parse one `/proc/net/tcp`-style table, keeping only listening sockets. UDP
/// is deliberately absent: a datagram socket has no listening state to report.
pub fn parse_listeners(text: &str) -> Vec<Listener> {
    let mut listeners = Vec::new();
    for line in text.lines().skip(1) {
        if listeners.len() >= MAX_LISTENERS {
            break;
        }
        let mut fields = line.split_ascii_whitespace();
        let (Some(_slot), Some(local), Some(_remote), Some(state)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        // 0A is TCP_LISTEN. Every other state is an ordinary connection.
        if state != "0A" {
            continue;
        }
        let mut rest = fields.skip(3);
        let (Some(uid), Some(_timeout), Some(inode)) = (rest.next(), rest.next(), rest.next())
        else {
            continue;
        };
        let Some((host, port)) = local.split_once(':') else {
            continue;
        };
        let (Some(address), Ok(port), Ok(uid), Ok(inode)) = (
            hex_address(host),
            u16::from_str_radix(port, 16),
            uid.parse::<u32>(),
            inode.parse::<u64>(),
        ) else {
            continue;
        };
        listeners.push(Listener {
            address,
            port,
            uid,
            inode,
        });
    }
    listeners
}

/// Every TCP listener in this network namespace, IPv4 and IPv6 alike.
pub fn listeners(roots: &Roots) -> Vec<Listener> {
    let mut all = Vec::new();
    for table in ["net/tcp", "net/tcp6"] {
        if let Some(text) = read(&roots.proc.join(table)) {
            all.extend(parse_listeners(&text));
        }
    }
    all.truncate(MAX_LISTENERS);
    all.sort_by_key(|listener| (listener.port, listener.binding(), listener.inode));
    all
}

fn socket_inode(target: &Path) -> Option<u64> {
    target
        .to_str()?
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Map the wanted socket inodes to the processes holding them.
///
/// `/proc/<pid>/fd` is readable only for our own processes, so another user's
/// listener simply has no owners here. That absence is reported as unknown
/// ownership; it never becomes a guess, and it never hides the listener.
pub fn socket_owners(roots: &Roots, inodes: &HashSet<u64>) -> HashMap<u64, Vec<u32>> {
    let mut owners: HashMap<u64, Vec<u32>> = HashMap::new();
    if inodes.is_empty() {
        return owners;
    }
    let Ok(entries) = fs::read_dir(&roots.proc) else {
        return owners;
    };
    let mut scanned = 0usize;
    for entry in entries.flatten() {
        if scanned >= MAX_PROCESSES {
            break;
        }
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        scanned += 1;
        let Ok(descriptors) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten().take(MAX_DESCRIPTORS) {
            let Ok(target) = fs::read_link(descriptor.path()) else {
                continue;
            };
            let Some(inode) = socket_inode(&target) else {
                continue;
            };
            if !inodes.contains(&inode) {
                continue;
            }
            let holders = owners.entry(inode).or_default();
            if !holders.contains(&pid) && holders.len() < MAX_OWNERS {
                holders.push(pid);
            }
        }
    }
    for holders in owners.values_mut() {
        holders.sort_unstable();
    }
    owners
}

/// A host-side proxy is not the application it forwards to. Naming the proxy
/// keeps the row honest instead of presenting the container's process.
pub fn proxy_kind(name: &str) -> &'static str {
    match name {
        "docker-proxy" | "rootlessport" | "rootlesskit" | "slirp4netns" | "pasta" | "passt" => {
            "container"
        }
        "systemd-socket-proxyd" => "socket",
        _ => "",
    }
}

fn is_user_manager(component: &str) -> bool {
    component.starts_with("user@") && component.ends_with(".service")
}

/// A unit name Seele is willing to hand to `systemctl`. The argv it lands in is
/// fixed and terminated by `--`, and this keeps the value itself unambiguous.
pub fn valid_unit(unit: &str) -> bool {
    !unit.is_empty()
        && unit.len() <= 255
        && unit.as_bytes()[0].is_ascii_alphanumeric()
        && unit
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@_.\\-".contains(&byte))
}

/// The unit a process belongs to, the manager that owns it, and the service
/// that may be stopped on its behalf.
///
/// Only a `.service` becomes a stop target. A `.scope` is a session or an
/// application the compositor launched, and stopping one would take a terminal
/// or a whole login session with it, so a scope's listener targets its process.
/// `user@<uid>.service` is excluded for the same reason: it is the user's
/// entire session manager.
pub fn cgroup_unit(text: &str) -> (String, &'static str, String) {
    for line in text.lines() {
        let Some(path) = line.splitn(3, ':').nth(2) else {
            continue;
        };
        let mut unit = String::new();
        let mut scope = "";
        let mut service = String::new();
        let mut user_manager = false;
        for component in path.split('/').filter(|part| !part.is_empty()) {
            if is_user_manager(component) {
                user_manager = true;
                unit = component.to_owned();
                scope = "system";
                service.clear();
            } else if component.ends_with(".service") || component.ends_with(".scope") {
                unit = component.to_owned();
                scope = if user_manager { "user" } else { "system" };
                service = if component.ends_with(".service") && valid_unit(component) {
                    component.to_owned()
                } else {
                    String::new()
                };
            }
        }
        if !unit.is_empty() {
            return (unit, scope, service);
        }
    }
    (String::new(), "", String::new())
}

/// The repository or project a process is working in, from its working
/// directory alone. A working directory Seele cannot read stays unknown.
pub fn project(cwd: &Path) -> Option<(String, String)> {
    const VCS: [&str; 2] = [".jj", ".git"];
    const MARKERS: [&str; 5] = [
        "flake.nix",
        "Cargo.toml",
        "package.json",
        "go.mod",
        "pyproject.toml",
    ];
    fn named(path: &Path) -> Option<(String, String)> {
        Some((
            path.file_name()?.to_str()?.to_owned(),
            path.to_str()?.to_owned(),
        ))
    }
    let mut marker: Option<&Path> = None;
    let mut directory = Some(cwd);
    let mut depth = 0;
    while let Some(current) = directory {
        if depth >= PROJECT_DEPTH || current.parent().is_none() {
            break;
        }
        depth += 1;
        if VCS.iter().any(|entry| current.join(entry).exists()) {
            return named(current);
        }
        if marker.is_none() && MARKERS.iter().any(|entry| current.join(entry).exists()) {
            marker = Some(current);
        }
        directory = current.parent();
    }
    marker.and_then(named)
}

/// Everything known about one owning process. An unreadable field is empty,
/// and `start` is what makes a later action refuse a recycled PID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub start: u64,
    pub name: String,
    pub uid: u32,
    pub user: String,
    pub unit: String,
    pub scope: &'static str,
    pub service: String,
    pub cwd: String,
    pub project: String,
    pub project_path: String,
    pub proxy: &'static str,
}

/// The process start time, in clock ticks since boot. Together with the PID it
/// identifies one process for as long as that process lives.
pub fn start_time(roots: &Roots, pid: u32) -> Option<u64> {
    let stat = read(&roots.proc.join(pid.to_string()).join("stat"))?;
    // The command name is parenthesized and may itself contain spaces and
    // parentheses, so fields are counted from the last closing parenthesis.
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_ascii_whitespace().nth(19)?.parse().ok()
}

/// The real UID a process runs as. Ownership is revalidated with this after
/// authentication, so it is read on its own rather than through a full record.
pub fn process_uid(roots: &Roots, pid: u32) -> Option<u32> {
    status_uid(&read(&roots.proc.join(pid.to_string()).join("status"))?)
}

fn status_uid(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_ascii_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Read one owning process. `None` means the process is gone; an empty field
/// means the kernel would not tell us.
pub fn process(roots: &Roots, pid: u32, users: &HashMap<u32, String>) -> Option<Process> {
    let base = roots.proc.join(pid.to_string());
    let start = start_time(roots, pid)?;
    let name = read(&base.join("comm"))
        .map(|value| value.trim().to_owned())
        .unwrap_or_default();
    let uid = read(&base.join("status"))
        .as_deref()
        .and_then(status_uid)
        .unwrap_or(u32::MAX);
    let (unit, scope, service) = read(&base.join("cgroup"))
        .as_deref()
        .map(cgroup_unit)
        .unwrap_or_default();
    let cwd = fs::read_link(base.join("cwd")).ok();
    let (project, project_path) = cwd
        .as_deref()
        .and_then(project)
        .unwrap_or_else(|| (String::new(), String::new()));
    Some(Process {
        pid,
        start,
        proxy: proxy_kind(&name),
        name,
        uid,
        user: users.get(&uid).cloned().unwrap_or_default(),
        unit,
        scope,
        service,
        cwd: cwd
            .as_deref()
            .and_then(Path::to_str)
            .unwrap_or_default()
            .to_owned(),
        project,
        project_path,
    })
}

/// User names for the UIDs discovery reports. Read from the password file
/// rather than through NSS so one refresh cannot block on a network directory.
pub fn users(roots: &Roots) -> HashMap<u32, String> {
    let mut users = HashMap::new();
    let Some(text) = read(&roots.passwd) else {
        return users;
    };
    for line in text.lines().take(8192) {
        let mut fields = line.split(':');
        let (Some(name), Some(_password), Some(uid)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if let Ok(uid) = uid.parse::<u32>() {
            users.entry(uid).or_insert_with(|| name.to_owned());
        }
    }
    users
}

/// A user name for display, falling back to the bare number so a UID without a
/// password entry is still reported exactly rather than as an empty owner.
pub fn user_label(users: &HashMap<u32, String>, uid: u32) -> String {
    if uid == u32::MAX {
        return String::new();
    }
    users
        .get(&uid)
        .cloned()
        .unwrap_or_else(|| format!("uid {uid}"))
}

/// Does this process still hold this exact listening socket?
///
/// This is the check every action repeats immediately before acting, and the
/// privileged helper repeats again after authentication.
pub fn holds_socket(roots: &Roots, pid: u32, inode: u64) -> bool {
    let Ok(descriptors) = fs::read_dir(roots.proc.join(pid.to_string()).join("fd")) else {
        return false;
    };
    for descriptor in descriptors.flatten().take(MAX_DESCRIPTORS) {
        if fs::read_link(descriptor.path())
            .ok()
            .as_deref()
            .and_then(socket_inode)
            == Some(inode)
        {
            return true;
        }
    }
    false
}

/// Find one listener again by the identity a review was built from. A vanished
/// socket, a rebound port or a changed address all answer `None`.
pub fn find_listener(roots: &Roots, inode: u64, binding: &str, port: u16) -> Option<Listener> {
    listeners(roots).into_iter().find(|listener| {
        listener.inode == inode && listener.port == port && listener.binding() == binding
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_listening_sockets_of_both_families_in_wire_order() {
        let table = concat!(
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n",
            "   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41253 1 0 100 0 0 10 0\n",
            "   1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 41300 1 0 100 0 0 10 0\n",
            "   2: 0100007F:C350 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 41999 1 0 100 0 0 10 0\n",
        );
        let rows = parse_listeners(table);
        assert_eq!(rows.len(), 2, "established connections are not listeners");
        assert_eq!(rows[0].address, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(rows[0].port, 8080);
        assert_eq!(rows[0].uid, 1000);
        assert_eq!(rows[0].inode, 41253);
        assert_eq!(rows[0].scope(), "loopback");
        assert_eq!(rows[1].port, 80);
        assert_eq!(rows[1].uid, 0);
        assert_eq!(rows[1].scope(), "wildcard");

        let six = concat!(
            "  sl  local_address                         remote_address                        st ... uid timeout inode\n",
            "   0: 00000000000000000000000000000000:0BB8 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 52001 1 0 100 0 0 10 0\n",
            "   1: 00000000000000000000000001000000:1F91 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 52002 1 0 100 0 0 10 0\n",
        );
        let rows = parse_listeners(six);
        assert_eq!(rows[0].address, "::".parse::<IpAddr>().unwrap());
        assert_eq!(rows[0].port, 3000);
        assert_eq!(rows[0].scope(), "wildcard");
        assert_eq!(rows[0].destination(), "[::1]");
        assert_eq!(rows[1].address, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(rows[1].binding(), "[::1]:8081");
        assert_eq!(rows[1].id(), "6:52002");
    }

    #[test]
    fn wildcard_bindings_open_a_destination_of_their_own_family() {
        let row = |address: &str| Listener {
            address: address.parse().unwrap(),
            port: 3000,
            uid: 1000,
            inode: 1,
        };
        assert_eq!(row("0.0.0.0").destination(), "127.0.0.1");
        assert_eq!(row("127.0.0.1").destination(), "127.0.0.1");
        assert_eq!(row("192.168.1.10").destination(), "192.168.1.10");
        assert_eq!(row("::").destination(), "[::1]");
        assert_eq!(row("::1").destination(), "[::1]");
        assert_eq!(row("fd00::5").destination(), "[fd00::5]");
        assert_eq!(row("fd00::5").binding(), "[fd00::5]:3000");
        assert_eq!(row("::ffff:0.0.0.0").destination(), "127.0.0.1");
        assert_eq!(row("192.168.1.10").scope(), "address");
    }

    #[test]
    fn only_services_become_stop_targets() {
        assert_eq!(
            cgroup_unit("0::/system.slice/nginx.service"),
            ("nginx.service".into(), "system", "nginx.service".into())
        );
        assert_eq!(
            cgroup_unit("0::/user.slice/user-1000.slice/user@1000.service/app.slice/dev.service"),
            ("dev.service".into(), "user", "dev.service".into())
        );
        // A session or application scope is not a service: stopping it would
        // take the whole session or window with it.
        assert_eq!(
            cgroup_unit("0::/user.slice/user-1000.slice/session-3.scope"),
            ("session-3.scope".into(), "system", String::new())
        );
        assert_eq!(
            cgroup_unit("0::/user.slice/user-1000.slice/user@1000.service"),
            ("user@1000.service".into(), "system", String::new())
        );
        assert_eq!(cgroup_unit("0::/"), (String::new(), "", String::new()));
    }

    #[test]
    fn unit_names_are_validated_before_they_reach_systemctl() {
        assert!(valid_unit("nginx.service"));
        assert!(valid_unit("getty@tty1.service"));
        for bad in [
            "",
            "-x.service",
            "unit;reboot.service",
            "unit name.service",
            "unit\n.service",
            "../escape.service",
        ] {
            assert!(!valid_unit(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn container_proxies_are_named_rather_than_presented_as_the_application() {
        assert_eq!(proxy_kind("docker-proxy"), "container");
        assert_eq!(proxy_kind("rootlesskit"), "container");
        assert_eq!(proxy_kind("node"), "");
    }
}
