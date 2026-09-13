use seele_maintenance::model::*;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};
fn inbox() -> Inbox {
    Inbox::new(registrations(&json!({})), None, 1_000_000.0).unwrap()
}
fn finding() -> Finding {
    serde_json::from_value(json!({"key":"root","title":"Disk pressure","explanation":"Space is low","urgency":"soon","actions":["recheck"]})).unwrap()
}
#[test]
fn dedup_recurrence_order_and_snooze_escalation() {
    let mut inbox = inbox();
    let now = 1_000_000.0;
    let (row, notify, changed) = inbox.publish("disk", finding(), now).unwrap();
    assert!(notify && changed);
    let (same, notify, changed) = inbox.publish("disk", finding(), now + 1.0).unwrap();
    assert!(!notify && !changed);
    assert_eq!(same.updated, row.updated);
    assert!(inbox
        .operation(&row.id, row.revision, "done", None, now)
        .is_err());
    inbox
        .operation(&row.id, row.revision, "snooze", Some(3600), now)
        .unwrap();
    assert_eq!(inbox.snapshot(now)["count"], 0);
    let mut value = finding();
    value.urgency = Urgency::Now;
    let (escalated, notify, _) = inbox.publish("disk", value, now + 2.0).unwrap();
    assert!(notify);
    assert_eq!(escalated.snoozed_until, 0.0);
    assert!(inbox
        .operation(&row.id, row.revision, "snooze", Some(60), now)
        .is_err());
    inbox.resolve("disk", "root", now + 3.0).unwrap();
    let (recurred, notify, _) = inbox.publish("disk", finding(), now + 4.0).unwrap();
    assert!(notify);
    assert_eq!(recurred.recurrence, 1);
    assert_eq!(recurred.first_seen, now);
}
#[test]
fn notices_expire_and_resolved_metadata_and_outcomes_age_out() {
    let mut inbox = inbox();
    let mut value = finding();
    value.lifecycle = Lifecycle::Notice;
    value.urgency = Urgency::Informational;
    let (row, notify, _) = inbox.publish("disk", value, 1_000_000.0).unwrap();
    assert!(!notify);
    inbox
        .operation(&row.id, row.revision, "snooze", Some(60), 1_000_000.0)
        .unwrap();
    assert_eq!(
        inbox.snapshot(1_000_061.0)["active"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    inbox.outcome(&row.id, row.revision, "recheck", "Completed", 1_000_000.0);
    inbox.prune(1_000_000.0 + WEEK + 1.0);
    assert!(inbox.items[&row.id].outcomes.is_empty());
    inbox
        .operation(
            &row.id,
            row.revision,
            "done",
            None,
            1_000_000.0 + WEEK + 1.0,
        )
        .unwrap();
    assert_eq!(
        inbox.snapshot(1_000_000.0 + WEEK + 2.0)["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        inbox.snapshot(1_000_000.0 + WEEK * 2.0 + 2.0)["history"],
        json!([])
    );
}
#[test]
fn persistence_only_metadata_private_permissions_and_stale_analysis() {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = root.path().join("inbox.json");
    let mut inbox = Inbox::new(registrations(&json!({})), Some(path.clone()), 1_000_000.0).unwrap();
    let mut value = finding();
    value.diagnostic = Some("private diagnostic".into());
    value.details = "token=supersecret".into();
    let (row, _, _) = inbox.publish("disk", value, 1_000_000.0).unwrap();
    inbox.analysis.insert(
        row.id.clone(),
        json!({"revision":row.revision,"cause":"private output"}),
    );
    assert_eq!(
        inbox.snapshot(1_000_000.0)["active"][0]["analysisStale"],
        false
    );
    let mut update = finding();
    update.urgency = Urgency::Now;
    inbox.publish("disk", update, 1_000_001.0).unwrap();
    assert_eq!(
        inbox.snapshot(1_000_001.0)["active"][0]["analysisStale"],
        true
    );
    inbox.persist(1_000_001.0).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    for secret in [
        "private diagnostic",
        "private output",
        "supersecret",
        "busy",
    ] {
        assert!(!text.contains(secret));
    }
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut reloaded = Inbox::new(registrations(&json!({})), Some(path), 1_000_002.0).unwrap();
    assert!(reloaded.diagnostics.is_empty());
    assert_eq!(
        reloaded.snapshot(1_000_002.0)["active"][0]["canAnalyze"],
        false
    );
}
#[test]
fn persisted_corruption_and_arbitrary_extra_payloads_are_dropped() {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = root.path().join("inbox.json");
    let mut original = inbox();
    let (row, _, _) = original.publish("disk", finding(), 1_000_000.0).unwrap();
    let mut value = serde_json::to_value(row).unwrap();
    value["id"] = json!("spoofed");
    value["diagnostic"] = json!("private");
    value["analysis"] = json!({"raw":"private"});
    value["unknown"] = json!("private");
    let mut corrupt = value.clone();
    corrupt["updated"] = json!("invalid");
    corrupt["key"] = json!("broken");
    seele_runtime::fs::atomic_write(
        &path,
        &serde_json::to_vec(&json!([value,corrupt,{"malformed":true}])).unwrap(),
    )
    .unwrap();
    let mut reloaded =
        Inbox::new(registrations(&json!({})), Some(path.clone()), 1_000_000.0).unwrap();
    assert_eq!(reloaded.items.len(), 1);
    assert!(reloaded.items.contains_key("disk:root"));
    reloaded.persist(1_000_000.0).unwrap();
    assert!(!fs::read_to_string(path).unwrap().contains("private"));
}
#[test]
fn persisted_symlinks_and_nonprivate_files_fail_closed() {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = root.path().join("state");
    let victim = root.path().join("victim");
    fs::write(&victim, "[]").unwrap();
    symlink(&victim, &path).unwrap();
    assert!(Inbox::new(registrations(&json!({})), Some(path), 1.0).is_err());
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Inbox::new(registrations(&json!({})), Some(victim), 1.0).is_err());
}
#[test]
fn protocol_types_and_duplicate_actions_are_rejected_before_mutation() {
    let mut inbox = inbox();
    for patch in [
        json!({"key":2}),
        json!({"actions":[{}]}),
        json!({"urgency":[]}),
        json!({"title":{}}),
        json!({"command":"execute"}),
    ] {
        let mut value = serde_json::to_value(finding()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(serde_json::from_value::<Finding>(value).is_err());
    }
    let mut value = finding();
    value.actions = vec!["recheck".into(), "recheck".into()];
    assert!(inbox.publish("disk", value, 1.0).is_err());
    let mut value = finding();
    value.diagnostic = Some("x".repeat(16385));
    assert!(inbox.publish("disk", value, 1.0).is_err());
    assert!(inbox.publish("unknown", finding(), 1.0).is_err());
    assert!(inbox.items.is_empty());
}
#[test]
fn redaction_handles_quoted_tokens_multiline_keys_links_and_direction_controls() {
    let text = clean("password=abc token: \"space separated secret\" Bearer abc https://secret.example/?token=abc \u{202e}safe\n-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----",4096);
    for secret in [
        "abc",
        "space separated",
        "secret.example",
        "PRIVATE KEY",
        "\u{202e}",
    ] {
        assert!(!text.contains(secret), "{text}");
    }
    assert!(text.contains("safe"));
    assert_eq!(clean("äöü", 2), "äö");
    assert_eq!(
        clean("-----BEGIN PRIVATE KEY-----\nincomplete", 4096),
        "[REDACTED PRIVATE MATERIAL]"
    );
}
#[test]
fn capacity_is_bounded_without_evicting_active_findings() {
    let mut inbox = inbox();
    for index in 0..CAPACITY {
        let mut value = finding();
        value.key = index.to_string();
        inbox.publish("disk", value, 1.0).unwrap();
    }
    assert!(inbox.publish("disk", finding(), 2.0).is_err());
    assert_eq!(inbox.items.len(), CAPACITY);
}

#[test]
fn fingerprint_matches_existing_python_state_without_repeat_notifications() {
    assert_eq!(
        fingerprint(&finding()),
        "195f3c67327f085f8996b333c95d5981d1f1bbbe8ec71e76b4c4ad6414a08167"
    );
}
