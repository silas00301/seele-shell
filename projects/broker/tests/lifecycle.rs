use seele_broker::lifecycle::{Broker, Config, Runner};
use seele_broker::validation::Request;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
fn payload(consumer: &str) -> Value {
    json!({"consumer":consumer,"label":"Fixture job","prompt":"Private prompt","context":{"value":1},"input":{"version":"1","schema":{"type":"object","required":["value"]}},"output":{"version":"1","schema":{"type":"integer"}}})
}
fn config() -> Config {
    Config {
        concurrency: 1,
        backoff: Duration::from_millis(1),
        ..Config::default()
    }
}
async fn submit(b: &Broker, value: Value) -> Value {
    b.call(json!({"op":"submit","request":value}))
        .await
        .unwrap()["job"]
        .clone()
}
async fn call(b: &Broker, op: &str, job: &Value) -> Value {
    b.call(json!({"op":op,"id":job["id"],"epoch":b.epoch}))
        .await
        .unwrap()
}
async fn done(b: &Broker, job: &Value) -> Value {
    tokio::time::timeout(Duration::from_secs(3), call(b, "wait", job))
        .await
        .unwrap()
}
async fn eventually(check: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !check() {
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
async fn shutdown(b: &Broker, workers: Vec<tokio::task::JoinHandle<()>>) {
    b.close();
    for worker in workers {
        worker.await.unwrap();
    }
    b.clear();
}
#[tokio::test]
async fn queue_order_promotion_and_nonpreemption() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let record = calls.clone();
    let gate = Arc::new(AtomicUsize::new(0));
    let wait = gate.clone();
    let runner: Arc<dyn Runner> = Arc::new(move |r: &Request, _: &str, cancel: &AtomicUsize| {
        record.lock().unwrap().push(r.consumer().to_owned());
        while wait.load(Ordering::Relaxed) == 0 && cancel.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok((json!(42), json!({"input":2,"output":1})))
    });
    let b = Broker::new(config(), runner);
    let workers = b.start();
    submit(&b, payload("first")).await;
    eventually(|| calls.lock().unwrap().len() == 1).await;
    let a = submit(&b, payload("background")).await;
    let mut p = payload("interactive");
    p["class"] = json!("interactive");
    let second = submit(&b, p).await;
    let mut p = payload("second-interactive");
    p["class"] = json!("interactive");
    let c = submit(&b, p).await;
    let listing = b.call(json!({"op":"list"})).await.unwrap();
    let ids: Vec<_> = listing["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|v| v["id"].clone())
        .collect();
    assert_eq!(
        ids,
        vec![second["id"].clone(), c["id"].clone(), a["id"].clone()]
    );
    call(&b, "next", &a).await;
    assert_eq!(*calls.lock().unwrap(), ["first"]);
    gate.store(1, Ordering::Relaxed);
    done(&b, &c).await;
    assert_eq!(
        *calls.lock().unwrap(),
        ["first", "background", "interactive", "second-interactive"]
    );
    shutdown(&b, workers).await;
}
#[tokio::test]
async fn supersession_cancel_release_and_watermarks() {
    let gate = Arc::new(AtomicUsize::new(0));
    let wait = gate.clone();
    let active = Arc::new(AtomicUsize::new(0));
    let observed = active.clone();
    let runner: Arc<dyn Runner> = Arc::new(move |_: &Request, _: &str, cancel: &AtomicUsize| {
        observed.fetch_add(1, Ordering::Relaxed);
        while wait.load(Ordering::Relaxed) == 0 && cancel.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok((json!(42), json!({})))
    });
    let b = Broker::new(config(), runner);
    let workers = b.start();
    let mut p = payload("one");
    p["item"] = json!("item");
    p["revision"] = json!(1);
    let old = submit(&b, p.clone()).await;
    eventually(|| active.load(Ordering::Relaxed) > 0).await;
    p["revision"] = json!(2);
    let new = submit(&b, p.clone()).await;
    gate.store(1, Ordering::Relaxed);
    assert_eq!(done(&b, &new).await["result"], 42);
    let result = call(&b, "status", &old).await;
    assert_eq!(result["job"]["state"], "superseded");
    assert!(result.get("result").is_none());
    call(&b, "release", &old).await;
    call(&b, "release", &new).await;
    assert_eq!(
        b.call(json!({"op":"submit","request":p}))
            .await
            .unwrap_err(),
        "superseded"
    );
    let listing = b.call(json!({"op":"list"})).await.unwrap().to_string();
    assert!(!listing.contains("Private prompt"));
    assert!(!listing.contains("result"));
    assert!(!listing.contains("context"));
    shutdown(&b, workers).await;
}
#[tokio::test]
async fn retries_are_bounded_and_isolation_never_retries() {
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let runner: Arc<dyn Runner> = Arc::new(move |r: &Request, _: &str, _: &AtomicUsize| {
        observed.fetch_add(1, Ordering::Relaxed);
        match r.consumer() {
            "transient" => Err("runtime_failure"),
            "isolation" => Err("isolation_failure"),
            _ => Ok((json!("invalid"), json!({"input":2,"output":1}))),
        }
    });
    let b = Broker::new(config(), runner);
    let workers = b.start();
    for (name, attempts, error) in [
        ("invalid", 8, "invalid_output"),
        ("transient", 3, "runtime_failure"),
        ("isolation", 1, "isolation_failure"),
    ] {
        let job = submit(&b, payload(name)).await;
        let result = done(&b, &job).await;
        assert_eq!(result["job"]["attempts"], attempts);
        assert_eq!(result["job"]["error"], error);
        assert_eq!(result["job"]["state"], "failed");
        if name == "invalid" {
            assert_eq!(result["job"]["tokens"]["input"], 16);
        }
    }
    shutdown(&b, workers).await;
}
#[tokio::test]
async fn failed_retry_and_cancelled_backoff_remain_explicit() {
    let mode = Arc::new(AtomicUsize::new(0));
    let selected = mode.clone();
    let runner: Arc<dyn Runner> = Arc::new(move |_: &Request, _: &str, _: &AtomicUsize| {
        match selected.load(Ordering::Relaxed) {
            0 => Err("model_failure"),
            1 => Ok((json!(7), json!({}))),
            _ => Ok((json!("bad"), json!({}))),
        }
    });
    let b = Broker::new(config(), runner);
    let workers = b.start();
    let job = submit(&b, payload("retry")).await;
    assert_eq!(done(&b, &job).await["job"]["state"], "failed");
    mode.store(1, Ordering::Relaxed);
    call(&b, "retry", &job).await;
    assert_eq!(done(&b, &job).await["result"], 7);
    mode.store(2, Ordering::Relaxed);
    let job = submit(&b, payload("cancel")).await;
    call(&b, "cancel", &job).await;
    assert_eq!(done(&b, &job).await["job"]["state"], "cancelled");
    call(&b, "release", &job).await;
    assert_eq!(
        b.call(json!({"op":"status","id":job["id"],"epoch":"old"}))
            .await
            .unwrap_err(),
        "broker_restarted"
    );
    assert_eq!(
        b.call(json!({"op":"status","id":job["id"],"epoch":b.epoch}))
            .await
            .unwrap_err(),
        "unknown_job"
    );
    shutdown(&b, workers).await;
}
#[tokio::test]
async fn capacity_and_unknown_fields_fail_before_execution() {
    let runner: Arc<dyn Runner> =
        Arc::new(|_: &Request, _: &str, _: &AtomicUsize| Ok((json!(1), json!({}))));
    let mut cfg = config();
    cfg.capacity = 1;
    let b = Broker::new(cfg, runner);
    submit(&b, payload("one")).await;
    assert_eq!(
        b.call(json!({"op":"submit","request":payload("two")}))
            .await
            .unwrap_err(),
        "capacity"
    );
    assert_eq!(
        b.call(json!({"op":"list","context":"private"}))
            .await
            .unwrap_err(),
        "invalid_input"
    );
    b.close();
    b.clear();
}
