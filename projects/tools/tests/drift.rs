//! `seele-drift` against programs that only record what they were asked to do.
//!
//! Diff may read `resolvectl status` and `systemctl show`. Apply of one drifted
//! check may add that check's restore and nothing else. An unknown id and a
//! diff with extra arguments record no command at all.

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const DNS: &str = "\
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
";

const CATALOG: &str = r#"{"version":1,"checks":{"quad9-dot":{"dns":["9.9.9.9#dns.quad9.net","149.112.112.112#dns.quad9.net","2620:fe::fe#dns.quad9.net","2620:fe::9#dns.quad9.net"]},"podman-rootless":{},"remote-shell":{}}}"#;

const HEALTHY_UNITS: &str = r#"{
  "user:podman.socket": {"LoadState":"loaded","ActiveState":"active","UnitFileState":"enabled"},
  "system:tailscaled.service": {"LoadState":"loaded","ActiveState":"active","UnitFileState":"enabled"},
  "system:sshd.service": {"LoadState":"loaded","ActiveState":"inactive","UnitFileState":"disabled"}
}"#;

const PODMAN_UNITS: &str = r#"{
  "system:podman.socket": {"LoadState":"loaded","ActiveState":"active","UnitFileState":"enabled"},
  "user:podman.socket": {"LoadState":"loaded","ActiveState":"inactive","UnitFileState":"enabled"},
  "system:tailscaled.service": {"LoadState":"loaded","ActiveState":"active","UnitFileState":"enabled"},
  "system:sshd.service": {"LoadState":"loaded","ActiveState":"inactive","UnitFileState":"disabled"}
}"#;

struct Fixture {
    root: PathBuf,
    path: PathBuf,
    log: PathBuf,
    catalog: PathBuf,
}

impl Fixture {
    fn new(name: &str, units: &str) -> Self {
        let root = std::env::temp_dir().join(format!("seele-drift-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("bin");
        fs::create_dir_all(&path).unwrap();
        let python = Command::new("python3")
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .unwrap();
        assert!(python.status.success(), "python3 is required");
        let python = String::from_utf8(python.stdout).unwrap();
        let python = python.trim();
        let script = format!(
            r#"#!{python}
import json, os, sys
log = os.environ["SEELE_DRIFT_LOG"]
name = os.path.basename(sys.argv[0])
args = sys.argv[1:]
with open(log, "a", encoding="utf-8") as handle:
    handle.write(name + (" " + " ".join(args) if args else "") + "\n")

def fail():
    sys.exit(1)

if name == "resolvectl":
    if args != ["status"]:
        fail()
    sys.stdout.write(open(os.environ["SEELE_FAKE_RESOLVECTL"], encoding="utf-8").read())
    sys.exit(0)

if name == "run0":
    if len(args) != 2 or args[1] not in ("quad9-dot", "podman-rootless", "remote-shell"):
        fail()
    sys.exit(0)

if name == "systemctl":
    user = False
    if args and args[0] == "--user":
        user = True
        args = args[1:]
    if not args:
        fail()
    if args[0] == "show":
        if "--" not in args:
            fail()
        unit = args[-1]
        key = ("user:" if user else "system:") + unit
        units = json.loads(open(os.environ["SEELE_FAKE_UNITS"], encoding="utf-8").read())
        props = units.get(key, {{
            "LoadState": "not-found",
            "ActiveState": "inactive",
            "UnitFileState": "disabled",
        }})
        for field in ("LoadState", "ActiveState", "UnitFileState"):
            print(f"{{field}}={{props[field]}}")
        sys.exit(0)
    if user and args == ["start", "podman.socket"]:
        sys.exit(0)
    fail()

fail()
"#
        );
        for command in ["resolvectl", "systemctl", "run0"] {
            let file = path.join(command);
            fs::write(&file, &script).unwrap();
            let mut permissions = fs::metadata(&file).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&file, permissions).unwrap();
        }
        let status = root.join("resolvectl.txt");
        fs::write(&status, DNS).unwrap();
        let units_path = root.join("units.json");
        fs::write(&units_path, units).unwrap();
        let catalog = root.join("drift.json");
        fs::write(&catalog, CATALOG).unwrap();
        let log = root.join("log");
        fs::write(&log, "").unwrap();
        Self {
            root,
            path,
            log,
            catalog,
        }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_seele-drift"))
            .args(args)
            .env("PATH", &self.path)
            .env("SEELE_DRIFT_LOG", &self.log)
            .env("SEELE_DRIFT_EXPECTATIONS", &self.catalog)
            .env("SEELE_FAKE_RESOLVECTL", self.root.join("resolvectl.txt"))
            .env("SEELE_FAKE_UNITS", self.root.join("units.json"))
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_read(line: &str) -> bool {
    line == "resolvectl status" || (line.starts_with("systemctl ") && line.contains(" show "))
}

fn json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stdout)))
}

#[test]
fn diff_only_reads_and_reports_three_checks() {
    let fixture = Fixture::new("diff", HEALTHY_UNITS);
    let output = fixture.run(&["diff"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = json(&output);
    assert_eq!(body["action"], "diff");
    assert_eq!(body["ok"], true);
    assert_eq!(body["mutated"], false);
    assert_eq!(body["checks"].as_array().unwrap().len(), 3);
    assert!(body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|check| check["drifted"] == false && check["unavailable"] == false));
    let recorded = lines(&fixture.log);
    assert!(!recorded.is_empty());
    assert!(
        recorded.iter().all(|line| is_read(line)),
        "diff recorded a change: {recorded:?}"
    );
}

#[test]
fn apply_restores_only_the_selected_podman_check() {
    let fixture = Fixture::new("podman", PODMAN_UNITS);
    let output = fixture.run(&["apply", "podman-rootless"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = json(&output);
    assert_eq!(body["action"], "apply");
    assert_eq!(body["ok"], true);
    assert_eq!(body["mutated"], true);
    assert_eq!(body["restored"], serde_json::json!(["podman-rootless"]));
    let changes: Vec<_> = lines(&fixture.log)
        .into_iter()
        .filter(|line| !is_read(line))
        .collect();
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert_eq!(changes[0], "systemctl --user start podman.socket");
    assert!(changes[1].starts_with("run0 "), "{changes:?}");
    assert!(changes[1].ends_with(" podman-rootless"), "{changes:?}");
    assert!(changes[1].contains("seele-restore-drift"), "{changes:?}");
}

#[test]
fn unknown_apply_and_extra_diff_arguments_mutate_nothing() {
    let fixture = Fixture::new("refuse", HEALTHY_UNITS);
    let extra = fixture.run(&["diff", "extra"]);
    assert!(!extra.status.success());
    let body = json(&extra);
    assert_eq!(body["action"], "diff");
    assert_eq!(body["ok"], false);
    assert_eq!(body["mutated"], false);
    assert!(lines(&fixture.log).is_empty(), "{:?}", lines(&fixture.log));

    let unknown = fixture.run(&["apply", "not-a-check"]);
    assert!(!unknown.status.success());
    let body = json(&unknown);
    assert_eq!(body["action"], "apply");
    assert_eq!(body["ok"], false);
    assert_eq!(body["mutated"], false);
    assert!(lines(&fixture.log).is_empty(), "{:?}", lines(&fixture.log));
}
