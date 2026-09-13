use seele_maintenance::{
    args,
    model::{self, Finding, Inbox, Result},
    service::{action_args, validate_analysis, Broker, Collector, Service},
    Executor,
};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};
fn config() -> Value {
    json!({"systemd":{"enabled":false},"backups":{"enabled":true,"items":[{"id":"daily","label":"Daily","unit":"backup.service","scope":"user"}]},"disk":{"enabled":true},"flake":{"enabled":false},"certificates":{"enabled":false},"inputs":{"enabled":false}})
}
fn finding() -> Finding {
    serde_json::from_value(json!({"key":"root","title":"Disk pressure","explanation":"Low space","urgency":"soon","actions":["recheck"],"diagnostic":"Used blocks exceed threshold"})).unwrap()
}
#[derive(Default)]
struct Execute {
    calls: Mutex<Vec<Vec<String>>>,
}
impl Executor for Execute {
    fn run(&self, arguments: &[String], _: &[u8], _: Duration) -> Result<(i32, String)> {
        self.calls.lock().unwrap().push(arguments.to_vec());
        Ok((0, String::new()))
    }
}
type BrokerFunction = dyn Fn(&Value) -> Result<Value> + Send + Sync;
struct FakeBroker {
    function: Box<BrokerFunction>,
}
impl Broker for FakeBroker {
    fn call(&self, value: &Value, _: Duration, _: bool) -> Result<Value> {
        (self.function)(value)
    }
}
fn broker(f: impl Fn(&Value) -> Result<Value> + Send + Sync + 'static) -> Arc<dyn Broker> {
    Arc::new(FakeBroker {
        function: Box::new(f),
    })
}
fn service(
    config: Value,
    collector: Arc<Collector>,
    broker: Arc<dyn Broker>,
) -> (Arc<Service>, Arc<Execute>) {
    let executor = Arc::new(Execute::default());
    let inbox = Inbox::new(model::registrations(&config), None, model::now()).unwrap();
    (
        Arc::new(Service::new(
            config,
            inbox,
            Arc::new(AtomicUsize::new(0)),
            executor.clone(),
            broker,
            collector,
        )),
        executor,
    )
}
fn no_broker() -> Arc<dyn Broker> {
    broker(|_| panic!("broker called without explicit analysis"))
}
fn empty() -> Arc<Collector> {
    Arc::new(|_, _, _, _| Ok(vec![]))
}
fn publish(service: &Arc<Service>, source: &str, value: Finding) -> Value {
    service
        .call(&json!({"op":"publish","source":source,"finding":value}))
        .unwrap();
    service.call(&json!({"op":"list"})).unwrap()["active"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["source"] == source)
        .unwrap()
        .clone()
}
#[test]
fn publication_recheck_dedup_failure_preservation_and_recovery() {
    let reports = Arc::new(Mutex::new(Ok(vec![finding()])));
    let data = reports.clone();
    let (service, execute) = service(
        config(),
        Arc::new(move |_, _, _, _| data.lock().unwrap().clone()),
        no_broker(),
    );
    service.check("disk").unwrap();
    service.check("disk").unwrap();
    assert_eq!(execute.calls.lock().unwrap().len(), 1);
    *reports.lock().unwrap() = Err("secret error");
    assert!(service.check("disk").is_err());
    let snapshot = service.call(&json!({"op":"list"})).unwrap();
    assert_eq!(snapshot["count"], 1);
    assert!(!snapshot.to_string().contains("secret"));
    *reports.lock().unwrap() = Ok(vec![]);
    service.check("disk").unwrap();
    let snapshot = service.call(&json!({"op":"list"})).unwrap();
    assert_eq!(snapshot["count"], 0);
    assert_eq!(snapshot["history"].as_array().unwrap().len(), 1);
    service.close();
}
#[test]
fn invalid_later_report_rolls_back_entire_snapshot_and_notifications() {
    let reports = Arc::new(Mutex::new(vec![finding()]));
    let data = reports.clone();
    let (service, execute) = service(
        config(),
        Arc::new(move |_, _, _, _| Ok(data.lock().unwrap().clone())),
        no_broker(),
    );
    service.check("disk").unwrap();
    let before = service.call(&json!({"op":"list"})).unwrap()["active"].clone();
    let mut valid = finding();
    valid.key = "new".into();
    let mut invalid = finding();
    invalid.key = "bad".into();
    invalid.actions = vec!["execute-command".into()];
    *reports.lock().unwrap() = vec![valid, invalid];
    assert!(service.check("disk").is_err());
    assert_eq!(
        service.call(&json!({"op":"list"})).unwrap()["active"],
        before
    );
    assert_eq!(execute.calls.lock().unwrap().len(), 1);
    service.close();
}
#[test]
fn typed_actions_require_confirmation_and_exact_registered_argv() {
    let (service, execute) = service(config(), empty(), no_broker());
    let mut value = finding();
    value.key = "daily".into();
    value.actions = args(&["retry", "open-logs"]);
    let row = publish(&service, "backups", value);
    execute.calls.lock().unwrap().clear();
    let mut request =
        json!({"op":"action","id":row["id"],"revision":row["revision"],"action":"retry"});
    assert!(service.call(&request).is_err());
    request["confirmed"] = json!(true);
    service.call(&request).unwrap();
    service.wait_jobs();
    assert_eq!(
        *execute.calls.lock().unwrap(),
        vec![args(&[
            "systemctl",
            "--user",
            "restart",
            "--",
            "backup.service"
        ])]
    );
    request["action"] = json!("sh -c anything");
    assert!(service.call(&request).is_err());
    request["action"] = json!("retry");
    request["command"] = json!("injected");
    assert!(service.call(&request).is_err());
    service.close();
}
#[test]
fn action_argv_rejects_option_injection_and_uses_running_system_for_root() {
    let mut inbox = Inbox::new(model::registrations(&json!({})), None, 1.0).unwrap();
    let mut value = finding();
    value.key = "system/backup.timer".into();
    value.actions = args(&["open-logs"]);
    let (row, _, _) = inbox.publish("systemd", value, 1.0).unwrap();
    assert!(action_args(&config(), &row, "open-logs")
        .unwrap()
        .contains(&"--unit=backup.timer".into()));
    let mut value = finding();
    value.key = "daily".into();
    value.actions = args(&["retry"]);
    let (row, _, _) = inbox.publish("backups", value, 1.0).unwrap();
    let mut cfg = config();
    cfg["backups"]["items"][0]["scope"] = json!("system");
    assert_eq!(
        action_args(&config(), &row, "open-logs").unwrap(),
        args(&[
            "systemd-run",
            "--user",
            "--collect",
            "--quiet",
            "--",
            "ghostty",
            "-e",
            "journalctl",
            "--user",
            "--unit=backup.service",
            "--no-hostname"
        ])
    );
    assert_eq!(
        action_args(&cfg, &row, "retry").unwrap(),
        args(&[
            "/run/current-system/sw/bin/run0",
            "/run/current-system/sw/bin/systemctl",
            "restart",
            "--",
            "backup.service"
        ])
    );
    for unit in [
        "--bad.service",
        "bad.service;touch /tmp/pwn",
        "../../bad.service",
        "bad.timer",
    ] {
        cfg["backups"]["items"][0]["unit"] = json!(unit);
        assert!(action_args(&cfg, &row, "retry").is_err());
    }
}
#[test]
fn analysis_is_explicit_typed_memory_only_and_does_not_execute_proposals() {
    let calls = Arc::new(Mutex::new(vec![]));
    let recorded = calls.clone();
    let (service, execute) = service(
        config(),
        empty(),
        broker(move |request| {
            recorded.lock().unwrap().push(request.clone());
            Ok(match request["op"].as_str().unwrap() {
                "submit" => json!({"ok":true,"epoch":"epoch","job":{"id":"job"}}),
                "wait" => {
                    json!({"ok":true,"result":{"cause":"Low capacity","evidence":["Threshold exceeded"],"nextSteps":["Review storage"],"actions":["recheck"]}})
                }
                _ => json!({"ok":true,"job":{"state":"succeeded"}}),
            })
        }),
    );
    let row = publish(&service, "disk", finding());
    execute.calls.lock().unwrap().clear();
    service.call(&json!({"op":"list"})).unwrap();
    assert!(calls.lock().unwrap().is_empty());
    service
        .call(&json!({"op":"analyze","id":row["id"],"revision":row["revision"]}))
        .unwrap();
    service.wait_jobs();
    let snapshot = service.call(&json!({"op":"list"})).unwrap();
    assert_eq!(
        snapshot["active"][0]["analysis"]["actions"],
        json!(["recheck"])
    );
    assert!(execute.calls.lock().unwrap().is_empty());
    let recorded = calls.lock().unwrap();
    let context = &recorded[0]["request"]["context"];
    assert_eq!(context.as_object().unwrap().len(), 2);
    assert!(context["finding"].get("outcomes").is_none());
    assert!(recorded.iter().any(|v| v["op"] == "release"));
    service.close();
}
#[test]
fn invalid_model_actions_and_extra_properties_are_rejected() {
    let allowed = args(&["recheck"]);
    let valid = json!({"cause":"","evidence":[],"nextSteps":[],"actions":["recheck"]});
    assert!(validate_analysis(&valid, &allowed).is_ok());
    for patch in [
        json!({"actions":["arbitrary-command"]}),
        json!({"actions":[{}]}),
        json!({"commands":[]}),
        json!({"evidence":["x".repeat(501)]}),
        json!({"cause":false}),
    ] {
        let mut value = valid.clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(validate_analysis(&value, &allowed).is_err());
    }
    let (service, execute) = service(
        config(),
        empty(),
        broker(|request| {
            Ok(match request["op"].as_str().unwrap() {
                "submit" => json!({"ok":true,"epoch":"epoch","job":{"id":"job"}}),
                "wait" => {
                    json!({"ok":true,"result":{"cause":"","evidence":[],"nextSteps":[],"actions":["arbitrary-command"]}})
                }
                _ => json!({"ok":true,"job":{"state":"failed"}}),
            })
        }),
    );
    let row = publish(&service, "disk", finding());
    execute.calls.lock().unwrap().clear();
    service
        .call(&json!({"op":"analyze","id":row["id"],"revision":row["revision"]}))
        .unwrap();
    service.wait_jobs();
    let snapshot = service.call(&json!({"op":"list"})).unwrap();
    assert!(snapshot["active"][0]["analysis"].is_null());
    assert!(execute.calls.lock().unwrap().is_empty());
    assert!(snapshot["active"][0]["outcomes"][0]["result"]
        .as_str()
        .unwrap()
        .contains("unavailable"));
    service.close();
}
#[derive(Default)]
struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl Gate {
    fn enter(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            state = self.changed.wait(state).unwrap();
        }
    }
    fn started(&self) {
        let state = self.state.lock().unwrap();
        let (state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| !s.0)
            .unwrap();
        assert!(state.0 && !timed.timed_out());
    }
    fn open(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}
#[test]
fn changed_findings_mark_analysis_stale_and_resolved_findings_do_not_accept_results() {
    for resolve in [false, true] {
        let gate = Arc::new(Gate::default());
        let blocked = gate.clone();
        let (service, _) = service(
            config(),
            empty(),
            broker(move |r| {
                Ok(match r["op"].as_str().unwrap() {
                    "submit" => json!({"ok":true,"epoch":"epoch","job":{"id":"job"}}),
                    "wait" => {
                        blocked.enter();
                        json!({"ok":true,"result":{"cause":"cause","evidence":[],"nextSteps":[],"actions":["recheck"]}})
                    }
                    _ => json!({"ok":true,"job":{"state":"succeeded"}}),
                })
            }),
        );
        let row = publish(&service, "disk", finding());
        service
            .call(&json!({"op":"analyze","id":row["id"],"revision":row["revision"]}))
            .unwrap();
        gate.started();
        if resolve {
            service
                .call(&json!({"op":"resolve","source":"disk","key":"root"}))
                .unwrap();
        } else {
            let mut changed = finding();
            changed.urgency = model::Urgency::Now;
            publish(&service, "disk", changed);
        }
        gate.open();
        service.wait_jobs();
        let snapshot = service.call(&json!({"op":"list"})).unwrap();
        if resolve {
            assert!(snapshot["history"][0]["analysis"].is_null());
        } else {
            assert_eq!(snapshot["active"][0]["analysisStale"], true);
        }
        service.close();
    }
}
#[test]
fn shutdown_recovers_accepted_submit_then_cancels_and_releases() {
    let gate = Arc::new(Gate::default());
    let submitted = gate.clone();
    let calls = Arc::new(Mutex::new(vec![]));
    let recorded = calls.clone();
    let (service, _) = service(
        config(),
        empty(),
        broker(move |r| {
            recorded
                .lock()
                .unwrap()
                .push(r["op"].as_str().unwrap().to_owned());
            Ok(match r["op"].as_str().unwrap() {
                "submit" => {
                    submitted.enter();
                    json!({"ok":true,"epoch":"epoch","job":{"id":"job"}})
                }
                "status" => json!({"ok":true,"job":{"state":"queued"}}),
                _ => json!({"ok":true}),
            })
        }),
    );
    let row = publish(&service, "disk", finding());
    service
        .call(&json!({"op":"analyze","id":row["id"],"revision":row["revision"]}))
        .unwrap();
    gate.started();
    service.stop.store(1, Ordering::Relaxed);
    gate.open();
    service.close();
    let calls = calls.lock().unwrap();
    assert!(calls.contains(&"cancel".into()));
    assert!(calls.contains(&"release".into()));
    assert!(!calls.contains(&"wait".into()));
}
#[test]
fn concurrent_rechecks_share_a_complete_probe() {
    let gate = Arc::new(Gate::default());
    let blocked = gate.clone();
    let probes = Arc::new(AtomicUsize::new(0));
    let count = probes.clone();
    let (service, execute) = service(
        config(),
        Arc::new(move |_, _, _, _| {
            count.fetch_add(1, Ordering::Relaxed);
            blocked.enter();
            Ok(vec![finding()])
        }),
        no_broker(),
    );
    let first = {
        let service = service.clone();
        thread::spawn(move || service.check("disk"))
    };
    gate.started();
    let second = {
        let service = service.clone();
        thread::spawn(move || service.check("disk"))
    };
    thread::sleep(Duration::from_millis(20));
    assert!(!second.is_finished());
    gate.open();
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    assert_eq!(probes.load(Ordering::Relaxed), 1);
    assert_eq!(execute.calls.lock().unwrap().len(), 1);
    service.close();
}
#[test]
fn bounded_action_admission_and_source_owned_conditions() {
    let gate = Arc::new(Gate::default());
    let blocked = gate.clone();
    let (service, _) = service(
        config(),
        empty(),
        broker(move |r| {
            Ok(match r["op"].as_str().unwrap() {
                "submit" => {
                    blocked.enter();
                    json!({"ok":true,"epoch":"epoch","job":{"id":"job"}})
                }
                "wait" => json!({"ok":false}),
                _ => json!({"ok":true,"job":{"state":"succeeded"}}),
            })
        }),
    );
    let mut requests = vec![];
    for index in 0..9 {
        let mut value = finding();
        value.key = index.to_string();
        let row = publish(&service, "disk", value);
        requests.push(json!({"op":"analyze","id":row["id"],"revision":row["revision"]}));
    }
    for request in &requests[..8] {
        service.call(request).unwrap();
    }
    assert!(service.call(&requests[8]).is_err());
    assert!(service.call(&requests[0]).is_err());
    gate.open();
    service.wait_jobs();
    let row = service.call(&json!({"op":"list"})).unwrap()["active"][0].clone();
    assert!(service
        .call(&json!({"op":"done","id":row["id"],"revision":row["revision"]}))
        .is_err());
    service.close();
}
