use crate::validation::{self, Request};
use crate::{now, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

pub trait Runner: Send + Sync + 'static {
    fn infer(&self, request: &Request, model: &str, cancel: &AtomicUsize)
        -> Result<(Value, Value)>;
}
impl<F> Runner for F
where
    F: Fn(&Request, &str, &AtomicUsize) -> Result<(Value, Value)> + Send + Sync + 'static,
{
    fn infer(
        &self,
        request: &Request,
        model: &str,
        cancel: &AtomicUsize,
    ) -> Result<(Value, Value)> {
        self(request, model, cancel)
    }
}
#[derive(Clone)]
pub struct Config {
    pub model: String,
    pub concurrency: usize,
    pub capacity: usize,
    pub backoff: Duration,
    pub schema_attempts: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            model: "gpt-5.6-luna".into(),
            concurrency: 2,
            capacity: 128,
            backoff: Duration::from_secs(1),
            schema_attempts: 8,
        }
    }
}
struct Job {
    request: Arc<Request>,
    sequence: u64,
    id: String,
    state: &'static str,
    created: f64,
    updated: f64,
    started: f64,
    attempts: u32,
    transient: u32,
    invalid_outputs: u32,
    tokens: [u64; 2],
    error: &'static str,
    result: Option<Value>,
    promoted: bool,
    ready: Instant,
    cancel: Arc<AtomicUsize>,
}
impl Job {
    fn terminal(&self) -> bool {
        matches!(
            self.state,
            "succeeded" | "failed" | "cancelled" | "superseded"
        )
    }
    fn metadata(&self, model: &str) -> Value {
        json!({"id":self.id,"consumer":self.request.value["consumer"],"label":self.request.value["label"],"state":self.state,"created":self.created,"updated":self.updated,"started":self.started,"model":model,"attempts":self.attempts,"queueDuration":((if self.started == 0.0 { now() } else { self.started })-self.created).max(0.0),"tokens":{"input":self.tokens[0],"output":self.tokens[1]},"error":self.error})
    }
    fn finish(&mut self, state: &'static str, error: &'static str) {
        self.state = state;
        self.error = error;
        self.updated = now();
        if state != "succeeded" {
            self.result = None;
        }
        self.cancel.store(1, Ordering::Relaxed);
    }
}
struct State {
    jobs: HashMap<String, Job>,
    retired: Vec<Value>,
    revisions: HashMap<(String, String), u64>,
    sequence: u64,
    activity: Instant,
    closed: bool,
}
pub struct Broker {
    pub epoch: String,
    config: Config,
    state: Mutex<State>,
    changed: Notify,
    runner: Arc<dyn Runner>,
}
impl Broker {
    pub fn new(config: Config, runner: Arc<dyn Runner>) -> Arc<Self> {
        Arc::new(Self {
            epoch: uuid::Uuid::new_v4().to_string(),
            config,
            state: Mutex::new(State {
                jobs: HashMap::new(),
                retired: vec![],
                revisions: HashMap::new(),
                sequence: 0,
                activity: Instant::now(),
                closed: false,
            }),
            changed: Notify::new(),
            runner,
        })
    }
    fn queue(state: &State) -> Vec<&Job> {
        let mut queue: Vec<_> = state
            .jobs
            .values()
            .filter(|j| j.state == "queued")
            .collect();
        queue.sort_by_key(|j| (!j.promoted, !j.request.interactive(), j.sequence));
        queue
    }
    pub fn start(self: &Arc<Self>) -> Vec<tokio::task::JoinHandle<()>> {
        (0..self.config.concurrency.clamp(1, 8))
            .map(|_| {
                let broker = self.clone();
                tokio::spawn(async move { broker.work().await })
            })
            .collect()
    }
    pub fn idle(&self, duration: Duration) -> bool {
        let state = self.state.lock().unwrap();
        state.activity.elapsed() >= duration && state.jobs.values().all(Job::terminal)
    }
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        for job in state.jobs.values_mut() {
            if !job.terminal() {
                job.finish("cancelled", "");
            }
        }
        self.changed.notify_waiters();
    }
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.jobs.clear();
        state.retired.clear();
        state.revisions.clear();
    }
    async fn work(self: Arc<Self>) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (picked, delay) = {
                let mut state = self.state.lock().unwrap();
                if state.closed {
                    return;
                }
                let current = Instant::now();
                let mut due: Vec<_> = state
                    .jobs
                    .values()
                    .filter(|j| j.state == "retrying" && j.ready <= current)
                    .map(|j| (j.ready, j.sequence, j.id.clone()))
                    .collect();
                due.sort();
                for (_, _, id) in due {
                    state.sequence += 1;
                    let sequence = state.sequence;
                    let job = state.jobs.get_mut(&id).unwrap();
                    job.state = "queued";
                    job.sequence = sequence;
                }
                let delay = state
                    .jobs
                    .values()
                    .filter(|j| j.state == "retrying")
                    .map(|j| j.ready.saturating_duration_since(current))
                    .min()
                    .unwrap_or(Duration::from_secs(300));
                let id = Self::queue(&state).first().map(|j| j.id.clone());
                let picked = id.map(|id| {
                    let job = state.jobs.get_mut(&id).unwrap();
                    job.state = "running";
                    job.promoted = false;
                    if job.started == 0.0 {
                        job.started = now();
                    }
                    job.updated = now();
                    job.attempts += 1;
                    (id, job.request.clone(), job.cancel.clone())
                });
                (picked, delay)
            };
            let Some((id, request, cancel)) = picked else {
                tokio::select! { _=notified => (), _=tokio::time::sleep(delay) => () };
                continue;
            };
            self.changed.notify_waiters();
            let runner = self.runner.clone();
            let model = self.config.model.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                let (result, usage) = runner.infer(&request, &model, &cancel)?;
                let result = if validation::bounded_instance(&result)
                    && request.validator.is_valid(&result)
                {
                    Ok(result)
                } else {
                    Err("invalid_output")
                };
                Ok((result, usage))
            })
            .await
            .unwrap_or(Err("runtime_failure"));
            {
                let mut state = self.state.lock().unwrap();
                state.activity = Instant::now();
                let Some(job) = state.jobs.get_mut(&id) else {
                    continue;
                };
                if job.terminal() {
                    continue;
                }
                let outcome = outcome.and_then(|(value, usage)| {
                    for (index, key) in ["input", "output"].into_iter().enumerate() {
                        if let Some(n) = usage[key].as_u64() {
                            job.tokens[index] = job.tokens[index].saturating_add(n);
                        }
                    }
                    value
                });
                match outcome {
                    Ok(value) => {
                        job.result = Some(value);
                        job.finish("succeeded", "");
                    }
                    Err(code) => {
                        if code == "invalid_output" {
                            job.invalid_outputs += 1;
                        } else {
                            job.transient += 1;
                        }
                        if matches!(code, "isolation_failure" | "authentication_unavailable")
                            || job.transient > 2
                            || job.invalid_outputs >= self.config.schema_attempts
                        {
                            job.finish("failed", code);
                        } else {
                            job.state = "retrying";
                            job.updated = now();
                            job.ready = Instant::now()
                                + (self.config.backoff * (1 << job.transient.min(5)))
                                    .min(Duration::from_secs(30));
                        }
                    }
                }
            }
            self.changed.notify_waiters();
        }
    }
    pub async fn call(&self, message: Value) -> Result<Value> {
        let object = message.as_object().ok_or("invalid_input")?;
        let operation = message["op"].as_str().ok_or("invalid_input")?;
        let allowed: &[&str] = match operation {
            "submit" => &["op", "request"],
            "list" | "configuration" => &["op"],
            "status" | "wait" | "cancel" | "retry" | "next" | "release" => &["op", "id", "epoch"],
            _ => return Err("invalid_input"),
        };
        if object.keys().any(|k| !allowed.contains(&k.as_str())) {
            return Err("invalid_input");
        }
        if operation == "configuration" {
            return Ok(json!({"model":self.config.model}));
        }
        // Compile once, outside the shared scheduler mutex and outside the I/O
        // reactor, so schema work cannot stall unrelated IPC or model completions.
        let validated = if operation == "submit" {
            let value = message["request"].clone();
            Some(
                tokio::task::spawn_blocking(move || validation::request(value))
                    .await
                    .map_err(|_| "runtime_failure")??,
            )
        } else {
            None
        };
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let reply = {
                let mut state = self.state.lock().unwrap();
                state.activity = Instant::now();
                if state.closed {
                    return Err("broker_unavailable");
                }
                if let Some(request) = &validated {
                    if state.jobs.len() >= self.config.capacity {
                        return Err("capacity");
                    }
                    if let Some((item, revision)) = request.item() {
                        let key = (request.consumer().to_owned(), item.to_owned());
                        if state.revisions.get(&key).is_some_and(|r| *r >= revision) {
                            return Err("superseded");
                        }
                        if !state.revisions.contains_key(&key) && state.revisions.len() >= 8192 {
                            return Err("capacity");
                        }
                        state.revisions.insert(key, revision);
                        for job in state.jobs.values_mut() {
                            if job.request.consumer() == request.consumer()
                                && job.request.item().is_some_and(|(old, _)| old == item)
                            {
                                job.finish("superseded", "");
                            }
                        }
                    }
                    state.sequence += 1;
                    let job = Job {
                        request: request.clone(),
                        sequence: state.sequence,
                        id: uuid::Uuid::new_v4().to_string(),
                        state: "queued",
                        created: now(),
                        updated: now(),
                        started: 0.0,
                        attempts: 0,
                        transient: 0,
                        invalid_outputs: 0,
                        tokens: [0, 0],
                        error: "",
                        result: None,
                        promoted: false,
                        ready: Instant::now(),
                        cancel: Arc::new(AtomicUsize::new(0)),
                    };
                    let metadata = job.metadata(&self.config.model);
                    state.jobs.insert(job.id.clone(), job);
                    Some(json!({"job":metadata}))
                } else if operation == "list" {
                    state
                        .retired
                        .retain(|v| now() - v["updated"].as_f64().unwrap_or_default() < 5.0);
                    let mut running: Vec<_> = state
                        .jobs
                        .values()
                        .filter(|j| matches!(j.state, "running" | "retrying"))
                        .collect();
                    running.sort_by_key(|j| j.sequence);
                    let mut terminal: Vec<_> =
                        state.jobs.values().filter(|j| j.terminal()).collect();
                    terminal.sort_by(|a, b| {
                        b.updated
                            .total_cmp(&a.updated)
                            .then(b.sequence.cmp(&a.sequence))
                    });
                    let mut jobs: Vec<_> = running
                        .into_iter()
                        .chain(Self::queue(&state))
                        .chain(terminal)
                        .map(|j| j.metadata(&self.config.model))
                        .collect();
                    jobs.extend(state.retired.clone());
                    Some(json!({"jobs":jobs}))
                } else {
                    if message["epoch"] != self.epoch {
                        return Err("broker_restarted");
                    }
                    let id = message["id"].as_str().ok_or("unknown_job")?;
                    let job = state.jobs.get(id).ok_or("unknown_job")?;
                    if operation == "wait" && !job.terminal() {
                        None
                    } else {
                        match operation {
                            "cancel" if job.terminal() => return Err("invalid_state"),
                            "retry" if job.state != "failed" => return Err("invalid_state"),
                            "next" if job.state != "queued" => return Err("invalid_state"),
                            "release" if !job.terminal() => return Err("invalid_state"),
                            _ => (),
                        }
                        if operation == "release" {
                            let job = state.jobs.remove(id).unwrap();
                            if job.state != "failed" {
                                state.retired.push(job.metadata(&self.config.model));
                                if state.retired.len() > 128 {
                                    state.retired.remove(0);
                                }
                            }
                            Some(json!({}))
                        } else {
                            if operation == "next" {
                                for queued in state.jobs.values_mut() {
                                    queued.promoted = false;
                                }
                            }
                            let job = state.jobs.get_mut(id).unwrap();
                            match operation {
                                "cancel" => job.finish("cancelled", ""),
                                "retry" => {
                                    job.state = "queued";
                                    job.transient = 0;
                                    job.invalid_outputs = 0;
                                    job.error = "";
                                    job.cancel = Arc::new(AtomicUsize::new(0));
                                    job.updated = now();
                                }
                                "next" => {
                                    job.promoted = true;
                                    job.updated = now();
                                }
                                _ => (),
                            }
                            let mut reply = json!({"job":job.metadata(&self.config.model)});
                            if job.state == "succeeded" && matches!(operation, "status" | "wait") {
                                reply["result"] = job.result.clone().unwrap();
                            }
                            Some(reply)
                        }
                    }
                }
            };
            if let Some(reply) = reply {
                self.changed.notify_waiters();
                return Ok(reply);
            }
            notified.await;
        }
    }
}
