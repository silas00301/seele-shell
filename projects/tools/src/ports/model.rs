//! What a listener is, who owns it, and what stopping it would actually do.
//!
//! Everything here is a pure function of one scan, so the panel's policy — the
//! target, whether it needs authentication, why Stop is unavailable, what a
//! confirmation has to disclose — is decided once, in Rust, and rendered
//! rather than re-derived by QML.
use super::procfs::{self, Listener, Process, Roots};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Identities resolved by an authorized `identify`, kept only in memory and
/// only as the PID and the start time that proves it is still the same
/// process. Every other field is re-read unprivileged on each refresh, so a
/// cached identity cannot outlive the process it named.
pub type Identities = HashMap<u64, Vec<(u32, u64)>>;

/// One listener with everything discovery could learn about it.
#[derive(Clone, Debug)]
pub struct Row {
    pub listener: Listener,
    pub owners: Vec<Process>,
    pub identified: bool,
    pub user: String,
}

/// What a Stop would act on. `kind` is the whole decision: a service is
/// stopped as a service, a process is signalled, and anything else is a
/// reason the panel shows instead of an action.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    pub kind: &'static str,
    pub unit: String,
    pub scope: &'static str,
    pub pid: u32,
    pub start: u64,
    pub uid: u32,
    pub privileged: bool,
    pub name: String,
    pub user: String,
    pub reason: String,
}

/// The exact identity a confirmation was shown for. A stop request carries it
/// back as one opaque token, and the worker rebuilds it from a fresh scan
/// before acting.
#[derive(Clone, Debug)]
pub struct Review {
    pub inode: u64,
    pub binding: String,
    pub port: u16,
    pub target: Target,
}

fn scope_of(value: &str) -> Option<&'static str> {
    match value {
        "" => Some(""),
        "system" => Some("system"),
        "user" => Some("user"),
        _ => None,
    }
}

fn kind_of(value: &str) -> Option<&'static str> {
    match value {
        "service" => Some("service"),
        "process" => Some("process"),
        _ => None,
    }
}

impl Review {
    /// The token is the decision, never the description: a changed label must
    /// not invalidate a review, and a changed target always must.
    pub fn token(&self) -> String {
        format!(
            "1|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.inode,
            self.binding,
            self.port,
            self.target.kind,
            self.target.unit,
            self.target.scope,
            self.target.pid,
            self.target.start,
            self.target.uid,
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        let fields: Vec<&str> = text.split('|').collect();
        if fields.len() != 10 || fields[0] != "1" {
            return None;
        }
        let unit = fields[5].to_owned();
        if !unit.is_empty() && !procfs::valid_unit(&unit) {
            return None;
        }
        Some(Self {
            inode: fields[1].parse().ok()?,
            binding: fields[2].to_owned(),
            port: fields[3].parse().ok()?,
            target: Target {
                kind: kind_of(fields[4])?,
                unit,
                scope: scope_of(fields[6])?,
                pid: fields[7].parse().ok()?,
                start: fields[8].parse().ok()?,
                uid: fields[9].parse().ok()?,
                ..Target::default()
            },
        })
    }
}

fn scope_label(listener: &Listener) -> &'static str {
    match listener.scope() {
        "loopback" => "This machine only",
        "wildcard" => "All interfaces",
        _ => "One interface",
    }
}

impl Row {
    pub fn id(&self) -> String {
        self.listener.id()
    }

    /// The process a row leads with: the single owner, or the one that was
    /// selected out of several.
    pub fn owner(&self, selected: u32) -> Option<&Process> {
        self.owners
            .iter()
            .find(|owner| owner.pid == selected)
            .or(if self.owners.len() == 1 {
                self.owners.first()
            } else {
                None
            })
    }

    /// The service every owner of this socket belongs to, if they all belong
    /// to the same one. A socket shared across units has no service target:
    /// stopping one of them would not free the port.
    pub fn service(&self) -> Option<(&str, &'static str)> {
        let first = self.owners.first()?;
        if first.service.is_empty() {
            return None;
        }
        self.owners
            .iter()
            .all(|owner| owner.service == first.service && owner.scope == first.scope)
            .then_some((first.service.as_str(), first.scope))
    }

    /// Decide what Stop would act on, or why it is unavailable.
    pub fn target(&self, selected: u32, self_uid: u32) -> Target {
        let unavailable = |reason: &str| Target {
            kind: "none",
            reason: reason.to_owned(),
            uid: self.listener.uid,
            ..Target::default()
        };
        if self.owners.is_empty() {
            return if self.listener.uid == self_uid {
                unavailable("The owning process is no longer visible. Refresh to check again.")
            } else {
                unavailable(
                    "Another user owns this listener. Identify its owner to see what stopping it would do.",
                )
            };
        }
        if let Some((unit, scope)) = self.service() {
            return Target {
                kind: "service",
                unit: unit.to_owned(),
                scope,
                uid: self.listener.uid,
                // A system unit is the system manager's to stop, whoever runs
                // its processes.
                privileged: scope == "system",
                name: self.owners.first().map(|o| o.name.clone()).unwrap_or_default(),
                user: self.owners.first().map(|o| o.user.clone()).unwrap_or_default(),
                ..Target::default()
            };
        }
        let Some(owner) = self.owner(selected) else {
            return Target {
                kind: "ambiguous",
                reason: "Several processes share this socket. Choose the one to stop.".to_owned(),
                uid: self.listener.uid,
                ..Target::default()
            };
        };
        Target {
            kind: "process",
            pid: owner.pid,
            start: owner.start,
            uid: owner.uid,
            privileged: owner.uid != self_uid,
            name: owner.name.clone(),
            user: owner.user.clone(),
            ..Target::default()
        }
    }

    pub fn review(&self, selected: u32, self_uid: u32) -> Review {
        Review {
            inode: self.listener.inode,
            binding: self.listener.binding(),
            port: self.listener.port,
            target: self.target(selected, self_uid),
        }
    }

    /// Whether an explicit, separately authorized owner lookup is the only way
    /// to learn who holds this socket.
    pub fn identifiable(&self, self_uid: u32) -> bool {
        self.owners.is_empty() && self.listener.uid != self_uid
    }

    fn haystack(&self) -> String {
        let mut text = format!(
            "{} {} {} {}",
            self.listener.binding(),
            self.listener.address,
            self.listener.port,
            self.user,
        );
        for owner in &self.owners {
            text.push_str(&format!(
                " {} {} {} {} {}",
                owner.name, owner.unit, owner.user, owner.project, owner.project_path
            ));
        }
        text.to_ascii_lowercase()
    }
}

/// What the user typed, understood the way a local address is written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub port: Option<u16>,
    pub host: String,
    pub scheme: String,
    pub explicit_scheme: bool,
}

/// Accept a port, an address, a `host:port` pair or a local URL. A scheme the
/// user actually typed is preserved and becomes the row's proposed scheme; a
/// port number on its own is never read as proof of HTTP.
pub fn parse_query(text: &str) -> Query {
    let trimmed = text.trim();
    let mut query = Query {
        text: trimmed.to_owned(),
        scheme: "http".to_owned(),
        ..Query::default()
    };
    if trimmed.is_empty() {
        return query;
    }
    let mut rest = trimmed;
    for scheme in ["https", "http"] {
        if let Some(tail) = rest
            .strip_prefix(&format!("{scheme}://"))
            .or_else(|| rest.strip_prefix(&format!("{scheme}:")))
        {
            query.scheme = scheme.to_owned();
            query.explicit_scheme = true;
            rest = tail;
            break;
        }
    }
    rest = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ if rest.bytes().all(|b| b.is_ascii_digit()) && !rest.is_empty() => ("", Some(rest)),
        _ => (rest, None),
    };
    query.port = port.and_then(|value| value.parse::<u16>().ok()).filter(|p| *p > 0);
    query.host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    // A port that did not parse is still text the user is searching for.
    if query.port.is_none() && !rest.is_empty() && query.host.is_empty() {
        query.host = rest.to_ascii_lowercase();
    }
    query
}

impl Query {
    pub fn empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn matches(&self, row: &Row) -> bool {
        if self.empty() {
            return true;
        }
        if let Some(port) = self.port {
            if row.listener.port != port {
                return false;
            }
        }
        if self.host.is_empty() {
            return true;
        }
        if self.host == "localhost" {
            return row.listener.loopback() || row.listener.wildcard();
        }
        row.haystack().contains(&self.host)
    }
}

/// One complete scan: every listener in this namespace with whatever ownership
/// the kernel was willing to show.
pub fn scan(roots: &Roots, identities: &Identities, self_uid: u32) -> Vec<Row> {
    let listeners = procfs::listeners(roots);
    let inodes: HashSet<u64> = listeners.iter().map(|listener| listener.inode).collect();
    let owners = procfs::socket_owners(roots, &inodes);
    let users = procfs::users(roots);
    listeners
        .into_iter()
        .map(|listener| {
            let mut identified = false;
            let mut pids = owners.get(&listener.inode).cloned().unwrap_or_default();
            if pids.is_empty() {
                if let Some(remembered) = identities.get(&listener.inode) {
                    // A remembered identity is only as good as the process it
                    // named: the start time has to still match.
                    pids = remembered
                        .iter()
                        .filter(|(pid, start)| procfs::start_time(roots, *pid) == Some(*start))
                        .map(|(pid, _)| *pid)
                        .collect();
                    identified = !pids.is_empty();
                }
            }
            let processes = pids
                .into_iter()
                .filter_map(|pid| procfs::process(roots, pid, &users))
                .collect();
            Row {
                user: procfs::user_label(&users, listener.uid),
                listener,
                owners: processes,
                identified,
            }
        })
        .collect()
}

fn owner_value(owner: &Process) -> Value {
    json!({
        "pid": owner.pid,
        "start": owner.start,
        "name": owner.name,
        "uid": owner.uid,
        "user": owner.user,
        "unit": owner.unit,
        "unitScope": owner.scope,
        "service": owner.service,
        "cwd": owner.cwd,
        "project": owner.project,
        "projectPath": owner.project_path,
        "proxy": owner.proxy,
    })
}

fn target_value(target: &Target) -> Value {
    json!({
        "kind": target.kind,
        "unit": target.unit,
        "scope": target.scope,
        "pid": target.pid,
        "start": target.start,
        "uid": target.uid,
        "user": target.user,
        "name": target.name,
        "privileged": target.privileged,
        "reason": target.reason,
    })
}

/// A row exactly as the panel draws it. Unknown metadata is an empty field the
/// panel names as unknown, never an invented owner.
pub fn row_value(row: &Row, query: &Query, selected: u32, self_uid: u32) -> Value {
    let target = row.target(selected, self_uid);
    json!({
        "id": row.id(),
        "inode": row.listener.inode,
        "family": row.listener.family(),
        "address": row.listener.address.to_string(),
        "binding": row.listener.binding(),
        "port": row.listener.port,
        "scope": row.listener.scope(),
        "scopeLabel": scope_label(&row.listener),
        "uid": row.listener.uid,
        "user": row.user,
        "destination": row.listener.destination(),
        "scheme": query.scheme,
        "explicitScheme": query.explicit_scheme,
        "owners": row.owners.iter().map(owner_value).collect::<Vec<_>>(),
        "selected": row.owner(selected).map(|owner| owner.pid).unwrap_or(0),
        "identified": row.identified,
        "identifiable": row.identifiable(self_uid),
        "target": target_value(&target),
        "token": row.review(selected, self_uid).token(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(address: &str, port: u16, uid: u32, inode: u64) -> Listener {
        Listener {
            address: address.parse().unwrap(),
            port,
            uid,
            inode,
        }
    }

    fn process(pid: u32, name: &str, uid: u32, service: &str, scope: &'static str) -> Process {
        Process {
            pid,
            start: 100 + u64::from(pid),
            name: name.to_owned(),
            uid,
            user: if uid == 0 { "root".into() } else { "silash".into() },
            unit: service.to_owned(),
            scope,
            service: service.to_owned(),
            ..Process::default()
        }
    }

    fn row(owners: Vec<Process>) -> Row {
        Row {
            listener: listener("0.0.0.0", 3000, 1000, 41),
            owners,
            identified: false,
            user: "silash".into(),
        }
    }

    #[test]
    fn a_shared_service_is_the_target_and_a_shared_socket_alone_is_not() {
        let managed = row(vec![
            process(10, "nginx", 0, "nginx.service", "system"),
            process(11, "nginx", 0, "nginx.service", "system"),
        ]);
        let target = managed.target(0, 1000);
        assert_eq!(target.kind, "service");
        assert_eq!(target.unit, "nginx.service");
        assert!(target.privileged, "a system unit needs authentication");

        let mixed = row(vec![
            process(10, "node", 1000, "", ""),
            process(11, "node", 1000, "", ""),
        ]);
        assert_eq!(mixed.target(0, 1000).kind, "ambiguous");
        // Choosing one owner resolves it without widening the action.
        let chosen = mixed.target(11, 1000);
        assert_eq!(chosen.kind, "process");
        assert_eq!(chosen.pid, 11);
        assert!(!chosen.privileged);

        let split = row(vec![
            process(10, "node", 1000, "a.service", "user"),
            process(11, "node", 1000, "b.service", "user"),
        ]);
        assert_eq!(
            split.target(0, 1000).kind,
            "ambiguous",
            "two units sharing a socket have no single service target"
        );
    }

    #[test]
    fn a_user_service_needs_no_authentication_and_another_user_needs_one() {
        let user = row(vec![process(10, "node", 1000, "dev.service", "user")]);
        let target = user.target(0, 1000);
        assert_eq!(target.kind, "service");
        assert!(!target.privileged);

        let foreign = row(vec![process(10, "postgres", 0, "", "")]);
        let target = foreign.target(0, 1000);
        assert_eq!(target.kind, "process");
        assert!(target.privileged);
    }

    #[test]
    fn an_unidentifiable_owner_disables_stop_with_a_reason() {
        let mut hidden = row(vec![]);
        hidden.listener.uid = 0;
        let target = hidden.target(0, 1000);
        assert_eq!(target.kind, "none");
        assert!(target.reason.contains("Identify"));
        assert!(hidden.identifiable(1000));

        let gone = row(vec![]);
        assert_eq!(gone.target(0, 1000).kind, "none");
        assert!(!gone.identifiable(1000), "our own listener needs no lookup");
    }

    #[test]
    fn a_review_token_round_trips_its_decision_and_nothing_else() {
        let managed = row(vec![process(10, "nginx", 0, "nginx.service", "system")]);
        let review = managed.review(0, 1000);
        let token = review.token();
        let parsed = Review::parse(&token).expect("token parses");
        assert_eq!(parsed.token(), token);
        assert_eq!(parsed.target.kind, "service");
        assert_eq!(parsed.target.unit, "nginx.service");
        assert_eq!(parsed.binding, "0.0.0.0:3000");
        for bad in [
            "",
            "2|41|0.0.0.0:3000|3000|service|nginx.service|system|0|0|0",
            "1|41|0.0.0.0:3000|3000|reboot|nginx.service|system|0|0|0",
            "1|41|0.0.0.0:3000|3000|service|nginx.service|root|0|0|0",
            "1|41|0.0.0.0:3000|3000|service|a b.service|system|0|0|0",
            "1|41|0.0.0.0:3000|3000|service|nginx.service|system|0|0",
        ] {
            assert!(Review::parse(bad).is_none(), "{bad} must not parse");
        }
        // A process review that named a PID keeps its start time, which is what
        // refuses a recycled number later.
        let single = row(vec![process(10, "node", 1000, "", "")]);
        let parsed = Review::parse(&single.review(0, 1000).token()).unwrap();
        assert_eq!((parsed.target.pid, parsed.target.start), (10, 110));
    }

    #[test]
    fn queries_read_ports_addresses_and_local_urls() {
        let plain = parse_query("3000");
        assert_eq!(plain.port, Some(3000));
        assert_eq!(plain.host, "");
        assert!(!plain.explicit_scheme);
        assert_eq!(plain.scheme, "http");

        let pair = parse_query("localhost:3000");
        assert_eq!((pair.port, pair.host.as_str()), (Some(3000), "localhost"));

        let url = parse_query("https://localhost:8443/admin");
        assert_eq!(url.port, Some(8443));
        assert_eq!(url.scheme, "https");
        assert!(url.explicit_scheme, "a typed scheme is preserved");

        let six = parse_query("[::1]:9000");
        assert_eq!((six.port, six.host.as_str()), (Some(9000), "::1"));

        let name = parse_query("Seele");
        assert_eq!(name.port, None);
        assert_eq!(name.host, "seele");
    }

    #[test]
    fn matching_covers_port_address_process_service_and_project() {
        let mut candidate = row(vec![process(10, "node", 1000, "dev.service", "user")]);
        candidate.owners[0].project = "seele".into();
        candidate.owners[0].project_path = "/home/silash/Developer/seele".into();
        for text in [
            "3000",
            "localhost:3000",
            "http://localhost:3000",
            "node",
            "dev.service",
            "seele",
            "silash",
            "0.0.0.0",
        ] {
            assert!(
                parse_query(text).matches(&candidate),
                "{text} should match the row"
            );
        }
        for text in ["3001", "nginx", "postgres:3000"] {
            assert!(
                !parse_query(text).matches(&candidate),
                "{text} should not match the row"
            );
        }
        assert!(parse_query("   ").matches(&candidate), "an empty query keeps every row");
    }
}
