use super::*;
fn thread(id: &str, time: &str, kind: &str) -> Thread {
    Thread::parse(&json!({"id":id,"repository":{"full_name":"team/project"},"reason":"subscribed","unread":true,"updated_at":time,"subject":{"type":kind,"title":"A notification","url":"https://api.github.com/repos/team/project/issues/1"}}),"github.com").unwrap()
}
fn triage(priority: &str) -> Triage {
    Triage::parse(json!({"summary":"Summary","reason":"Mentioned","attention":"Review","nextAction":"Read","priority":priority,"changes":"Initial triage"})).unwrap()
}
fn inbox() -> Inbox {
    let (tx, _) = mpsc::channel(64);
    let cancel = CancellationToken::new();
    let mut inbox = Inbox::new(
        Api {
            host: "github.com".into(),
            cancel: cancel.clone(),
        },
        tx,
        cancel,
    );
    // Reserve both background slots: these state-machine tests never execute gh.
    for id in ["fixture-a", "fixture-b"] {
        inbox
            .jobs
            .insert((999, id.into(), 0), CancellationToken::new());
    }
    inbox
}
#[test]
fn all_types_and_priorities_count_and_sort_without_suppression() {
    let mut inbox = inbox();
    for (id, time, kind) in [
        ("1", "2026-09-13T10:00:00Z", "Issue"),
        ("2", "2026-09-13T11:00:00Z", "PullRequest"),
        ("3", "2026-09-13T12:00:00Z", "UnknownFutureKind"),
        ("4", "2026-09-13T13:00:00Z", "Release"),
    ] {
        inbox.upsert(thread(id, time, kind));
    }
    inbox.entries.get_mut("1").unwrap().triage = Some(triage(model::PRIORITIES[0]));
    inbox.entries.get_mut("2").unwrap().triage = Some(triage(model::PRIORITIES[0]));
    inbox.entries.get_mut("3").unwrap().triage = Some(triage(model::PRIORITIES[3]));
    let rows = model::rows(&inbox.entries);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2", "1", "3", "4"]
    );
    inbox.entries.get_mut("2").unwrap().pending_done = true;
    assert_eq!(model::rows(&inbox.entries).len(), 3);
}
#[test]
fn exact_schema_and_no_model_selection_or_external_context() {
    let entry = Entry::new(thread("1", "now", "Issue"), 1);
    let detail = Detail::default();
    let request = triage::request("github.com:1", "octocat", &entry, &detail, 9).unwrap();
    assert_eq!(request["class"], "background");
    assert_eq!(request["context"]["viewer"], "octocat");
    assert!(request.get("model").is_none());
    assert_eq!(
        request["output"]["schema"]["properties"]["priority"]["enum"],
        json!(model::PRIORITIES)
    );
    for priority in model::PRIORITIES {
        assert!(triage(priority).rank() < 4)
    }
    assert!(Triage::parse(json!({"priority":"Urgent"})).is_err());
    let huge = Detail {
        body: "x".repeat(200 * 1024),
        ..Default::default()
    };
    assert!(triage::request("a", "octocat", &entry, &huge, 1).is_err());
}
#[test]
fn changes_keep_small_stamps_and_explicit_deltas() {
    let old = Detail {
        body: "old".into(),
        state: "open".into(),
        comments: vec![json!({"body":"first","updatedAt":"2026-09-12"})],
        ..Default::default()
    };
    let stamp = model::SourceStamp::new(&old);
    let mut new = old.clone();
    new.state = "closed".into();
    new.body = "new".into();
    new.comments
        .push(json!({"body":"second","updatedAt":"2026-09-13"}));
    let delta = model::changed(Some(&stamp), &new);
    assert_eq!(delta["bodyChanged"], true);
    assert_eq!(delta["stateBefore"], "open");
    assert_eq!(delta["stateAfter"], "closed");
    assert_eq!(delta["newOrEditedComments"].as_array().unwrap().len(), 1);
}
#[tokio::test]
async fn stale_jobs_and_account_replacement_cannot_apply_results() {
    let mut inbox = inbox();
    inbox.upsert(thread("1", "old", "Issue"));
    let token = inbox.entries["1"].token;
    inbox.upsert(thread("1", "new", "Issue"));
    inbox
        .event(Event::Triaged(
            (0, "1".into(), token),
            Ok((Detail::default(), triage(model::PRIORITIES[3]))),
        ))
        .await;
    assert!(inbox.entries["1"].triage.is_none());
    assert_eq!(inbox.entries["1"].thread.updated_at, "new");
    let token = inbox.entries["1"].token;
    inbox.reset();
    inbox
        .event(Event::Detail((0, "1".into(), token), Ok(Detail::default())))
        .await;
    assert!(inbox.entries.is_empty());
}
#[tokio::test]
async fn failed_done_rolls_back_count_and_preserves_the_confirmed_item() {
    let mut inbox = inbox();
    inbox.upsert(thread("1", "old", "Issue"));
    let entry = inbox.entries.get_mut("1").unwrap();
    entry.pending_done = true;
    let key = (0, "1".into(), entry.token);
    assert_eq!(model::rows(&inbox.entries).len(), 0);
    inbox.event(Event::Done(key, Err("unavailable"))).await;
    assert_eq!(model::rows(&inbox.entries).len(), 1);
    assert_eq!(inbox.error, "write-failed");
}
#[test]
fn completed_revision_stays_done_but_new_activity_returns() {
    let mut inbox = inbox();
    let old = thread("1", "old", "Issue");
    inbox.ledger.done.insert("1".into(), old.revision.clone());
    inbox.upsert(old);
    assert!(inbox.entries.is_empty());
    inbox.upsert(thread("1", "new", "Issue"));
    assert_eq!(inbox.entries.len(), 1);
    assert!(!inbox.ledger.done.contains_key("1"));
}
#[test]
fn url_gate_rejects_foreign_api_targets_and_path_traversal() {
    assert!(api::subject_path(
        "https://api.github.com/repos/team/project/issues/1",
        "github.com",
        "team/project"
    )
    .is_some());
    for url in [
        "https://evil.test/repos/team/project/issues/1",
        "https://api.github.com@evil.test/repos/team/project/issues/1",
        "https://api.github.com/repos/team/project/../files",
        "https://api.github.com/repos/other/project/issues/1",
        "https://api.github.com/repos/team/project/issues/1?x=y",
    ] {
        assert!(
            api::subject_path(url, "github.com", "team/project").is_none(),
            "{url}"
        );
    }
    assert!(api::safe_web(
        &json!("https://github.com.evil.test/team/project"),
        "github.com"
    )
    .is_none());
}

#[tokio::test]
async fn rate_limits_pause_jobs_and_logout_clears_cached_content() {
    let mut inbox = inbox();
    inbox.upsert(thread("1", "old", "Issue"));
    let key = (0, "1".into(), inbox.entries["1"].token);
    inbox.event(Event::Triaged(key, Err("rate-limited"))).await;
    assert_eq!(inbox.error, "rate-limited");
    assert!(inbox.next_poll > Instant::now() + Duration::from_secs(290));
    inbox.jobs.clear();
    inbox.schedule();
    assert!(inbox.jobs.is_empty());
    inbox.event(Event::Polled(0, Err("auth-required"))).await;
    assert!(inbox.entries.is_empty());
    assert!(inbox.account.is_empty());
    assert_eq!(inbox.error, "auth-required");
}
