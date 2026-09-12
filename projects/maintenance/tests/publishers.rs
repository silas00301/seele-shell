use seele_maintenance::{
    model::{Result, Urgency},
    publishers::*,
    Executor,
};
use serde_json::json;
use std::{fs, sync::Mutex, time::Duration};
struct Fixture {
    reply: (i32, String),
    calls: Mutex<Vec<Vec<String>>>,
}
impl Executor for Fixture {
    fn run(&self, args: &[String], _: &[u8], _: Duration) -> Result<(i32, String)> {
        self.calls.lock().unwrap().push(args.to_vec());
        Ok(self.reply.clone())
    }
}
fn fixture(code: i32, text: &str) -> Fixture {
    Fixture {
        reply: (code, text.into()),
        calls: Mutex::new(vec![]),
    }
}
#[test]
fn failed_service_snapshot_is_distinct_from_probe_failure() {
    let run = fixture(0, "[{\"unit\":\"example.service\"}]");
    let rows = collect(&json!({"systemd":{"scope":"user"}}), "systemd", &run, 1.0).unwrap();
    assert_eq!(rows[0].key, "user/example.service");
    assert_eq!(rows[0].actions, ["recheck", "open-logs"]);
    assert!(run.calls.lock().unwrap()[0].contains(&"--user".into()));
    assert!(collect(&json!({}), "systemd", &fixture(0, "[]"), 1.0)
        .unwrap()
        .is_empty());
    for (status, text) in [
        (1, "token=secret"),
        (0, "token=secret"),
        (0, "[{\"unit\":\"--bad.service\"}]"),
    ] {
        let error = collect(&json!({}), "systemd", &fixture(status, text), 1.0).unwrap_err();
        assert!(!error.contains("secret"));
    }
}
fn state(result: &str, status: &str, stamp: &str, load: &str) -> String {
    format!("LoadState={load}\nActiveState=inactive\nResult={result}\nExecMainStatus={status}\nExecMainExitTimestamp={stamp}\n")
}
#[test]
fn backups_require_loaded_units_recent_success_and_stable_evidence() {
    let cfg = json!({"id":"vault","label":"Vault","unit":"vault.service","maxAgeHours":24});
    assert!(backup_report(
        &cfg,
        &state("success", "0", "@100000", "loaded"),
        None,
        100100.0
    )
    .unwrap()
    .is_none());
    let failed = backup_report(
        &cfg,
        &state("exit-code", "1", "@100000", "loaded"),
        None,
        100100.0,
    )
    .unwrap()
    .unwrap();
    assert!(failed.title.ends_with("backup failed"));
    assert!(failed.actions.contains(&"retry".into()));
    assert!(backup_report(
        &cfg,
        &state("success", "0", "", "not-found"),
        None,
        100100.0
    )
    .is_err());
    let old = backup_report(
        &cfg,
        &state("success", "0", "@100000", "loaded"),
        None,
        300000.0,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&old).unwrap(),
        serde_json::to_value(
            backup_report(
                &cfg,
                &state("success", "0", "@100000", "loaded"),
                None,
                300060.0
            )
            .unwrap()
        )
        .unwrap()
    );
    let marker =
        json!({"id":"vault","label":"Vault","unit":"vault.service","successFile":"/marker"});
    assert!(backup_report(
        &marker,
        &state("success", "0", "", "loaded"),
        Some(100000.0),
        100100.0
    )
    .unwrap()
    .is_none());
    for stamp in ["NaN", "inf", "1e9", "@100000000000", "-1"] {
        assert!(backup_report(
            &cfg,
            &state("success", "0", stamp, "loaded"),
            None,
            100100.0
        )
        .is_err());
    }
}
#[test]
fn disk_uses_available_blocks_and_inodes_and_stable_threshold_bands() {
    let first = disk_report("/data", 1000, 100, 100, 100, 85.0, 95.0)
        .unwrap()
        .unwrap();
    assert_eq!(first.urgency, Urgency::Soon);
    let next = disk_report("/data", 1000, 99, 100, 100, 85.0, 95.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(next).unwrap()
    );
    let critical = disk_report("/data", 1000, 100, 100, 1, 85.0, 95.0)
        .unwrap()
        .unwrap();
    assert_eq!(critical.urgency, Urgency::Now);
    assert_eq!(first.key, critical.key);
    assert!(disk_report("/data", 1000, 500, 100, 100, 85.0, 95.0)
        .unwrap()
        .is_none());
    for values in [
        (0, 0, 85.0, 95.0),
        (100, 101, 85.0, 95.0),
        (100, 10, 95.0, 85.0),
    ] {
        assert!(disk_report("/data", values.0, values.1, 0, 0, values.2, values.3).is_err());
    }
}
#[test]
fn certificates_parse_without_locale_dependency_and_reject_invalid_calendar_dates() {
    let expires = expiry("notAfter=Oct  1 00:00:00 2026 GMT\n").unwrap();
    let cfg = json!({"certificates":{"items":[{"id":"local","label":"Local","path":"/certificate.pem"}]}});
    let run = fixture(0, "notAfter=Oct  1 00:00:00 2026 GMT\n");
    let rows = collect(&cfg, "certificates", &run, expires - 15.0 * 86400.0).unwrap();
    assert_eq!(rows[0].urgency, Urgency::Soon);
    assert_eq!(
        collect(&cfg, "certificates", &run, expires - 86400.0).unwrap()[0].urgency,
        Urgency::Now
    );
    assert!(
        collect(&cfg, "certificates", &run, expires - 60.0 * 86400.0)
            .unwrap()
            .is_empty()
    );
    for text in [
        "notAfter=Feb 31 00:00:00 2026 GMT",
        "notAfter=Oct 1 00:00:00 2026 CET",
        "notAfter=Oct 1 24:00:00 2026 GMT",
    ] {
        assert!(expiry(text).is_err());
    }
    assert!(collect(
        &cfg,
        "certificates",
        &fixture(1, "private key content"),
        expires
    )
    .is_err());
}
#[test]
fn flake_check_flags_and_errors_do_not_expose_output() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("flake.nix"), "{}").unwrap();
    let cfg = json!({"flake":{"path":root.path()}});
    let run = fixture(1, "password=secret");
    let rows = collect(&cfg, "flake", &run, 1.0).unwrap();
    assert!(!serde_json::to_string(&rows).unwrap().contains("secret"));
    let calls = run.calls.lock().unwrap();
    assert!(calls[0].contains(&"--no-build".into()));
    assert!(calls[0].contains(&"--no-write-lock-file".into()));
}
#[test]
fn input_age_and_upstream_revision_are_both_required_and_lock_is_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("flake.lock");
    let current = "a".repeat(40);
    let latest = "b".repeat(40);
    let lock = json!({"root":"root","nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs"}},"nixpkgs":{"locked":{"rev":current,"lastModified":100000},"original":{"type":"github","owner":"NixOS","repo":"nixpkgs","ref":"nixos-unstable"}}}});
    let bytes = serde_json::to_vec(&lock).unwrap();
    fs::write(&path, &bytes).unwrap();
    let cfg = json!({"inputs":{"path":root.path(),"items":[{"id":"nixpkgs","maxAgeDays":30}]}});
    let run = fixture(0, &json!({"locked":{"rev":latest}}).to_string());
    let rows = collect(&cfg, "inputs", &run, 100000.0 + 31.0 * 86400.0).unwrap();
    assert_eq!(rows[0].urgency, Urgency::Eventually);
    assert!(run.calls.lock().unwrap()[0].contains(&"github:NixOS/nixpkgs/nixos-unstable".into()));
    let recent = fixture(1, "never called");
    assert!(collect(&cfg, "inputs", &recent, 100100.0)
        .unwrap()
        .is_empty());
    assert!(recent.calls.lock().unwrap().is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let pinned =
        json!({"type":"github","owner":"NixOS","repo":"nixpkgs","ref":"unstable","rev":current});
    assert_eq!(
        original_ref(&pinned).unwrap(),
        format!("github:NixOS/nixpkgs/{}", "a".repeat(40))
    );
    for patch in [
        json!({"host":"private.invalid"}),
        json!({"dir":"nested"}),
        json!({"owner":"secret@host"}),
        json!({"repo":".."}),
    ] {
        let mut value = pinned.clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(original_ref(&value).is_err());
    }
}
#[test]
fn disabled_and_empty_sources_do_not_spawn_probes_and_configuration_is_bounded() {
    let run = fixture(1, "never called");
    for source in ["backups", "certificates"] {
        assert!(collect(&json!({}), source, &run, 1.0).unwrap().is_empty());
    }
    assert!(
        collect(&json!({"flake":{"enabled":false}}), "flake", &run, 1.0)
            .unwrap()
            .is_empty()
    );
    assert!(run.calls.lock().unwrap().is_empty());
    assert!(validate_config(&json!({"backups":{"items":[{"id":"x"},{"id":"x"}]}})).is_err());
    assert!(validate_config(&json!({"backups":{"items":vec![json!({"id":"x"});513]}})).is_err());
    assert!(validate_config(&json!({"disk":{"enabled":"yes"}})).is_err());
}
