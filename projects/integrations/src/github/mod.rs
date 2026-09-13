//! Resident inbox owner. Qt renders snapshots and forwards explicit intent;
//! pagination, reconciliation, retries, triage and mutations belong here.
mod api;
mod model;
mod triage;
use crate::common::{self, Result};
use api::Api;
use model::{Detail, Entry, Ledger, Thread, Triage};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{io::BufReader, sync::mpsc, task::JoinSet};
use tokio_util::sync::CancellationToken;

const LIMIT: usize = 10_000;
const WIRE: usize = 64 * 1024 * 1024;
type JobKey = (u64, String, u64);
enum Event {
    Command(Value),
    Eof,
    Identity(u64, String, String),
    Page(u64, Vec<Thread>),
    Polled(u64, Result<()>),
    Detail(JobKey, Result<Detail>),
    Triaged(JobKey, Result<(Detail, Triage)>),
    Done(JobKey, Result<()>),
    Focus(u64, String),
    Notified(u64, String, String, bool),
}
struct Inbox {
    api: Api,
    account: String,
    viewer: String,
    epoch: u64,
    serial: u64,
    poll: u64,
    polling: bool,
    complete: bool,
    entries: BTreeMap<String, Entry>,
    seen: HashSet<String>,
    ledger: Ledger,
    ledger_path: Option<PathBuf>,
    alerts: HashSet<String>,
    alert_count: usize,
    selected: String,
    error: &'static str,
    notice: String,
    last_poll: Instant,
    next_poll: Instant,
    jobs: HashMap<JobKey, CancellationToken>,
    selection_job: Option<CancellationToken>,
    tx: mpsc::Sender<Event>,
    tasks: JoinSet<()>,
    cancel: CancellationToken,
}
impl Inbox {
    fn new(api: Api, tx: mpsc::Sender<Event>, cancel: CancellationToken) -> Self {
        let now = Instant::now();
        Self {
            api,
            account: String::new(),
            viewer: String::new(),
            epoch: 0,
            serial: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
            poll: 0,
            polling: false,
            complete: false,
            entries: BTreeMap::new(),
            seen: HashSet::new(),
            ledger: Ledger::default(),
            ledger_path: None,
            alerts: HashSet::new(),
            alert_count: 0,
            selected: String::new(),
            error: "",
            notice: String::new(),
            last_poll: now - Duration::from_secs(300),
            next_poll: now,
            jobs: HashMap::new(),
            selection_job: None,
            tx,
            tasks: JoinSet::new(),
            cancel,
        }
    }
    fn token(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }
    fn current(&self, key: &JobKey) -> bool {
        key.0 == self.epoch && self.entries.get(&key.1).is_some_and(|e| e.token == key.2)
    }
    fn reset(&mut self) {
        for (_, token) in self.jobs.drain() {
            token.cancel()
        }
        if let Some(token) = self.selection_job.take() {
            token.cancel()
        }
        self.epoch += 1;
        self.alerts.clear();
        self.entries.clear();
        self.selected.clear();
        self.complete = false;
        self.ledger = Ledger::default();
        self.ledger_path = None;
    }
    fn load_ledger(&mut self) {
        let directory = common::xdg("XDG_STATE_HOME", ".local/state").join("seele-github");
        if seele_runtime::fs::private_directory(&directory).is_err() {
            self.error = "state-unavailable";
            return;
        }
        let path = directory.join(format!("{}.json", model::digest(&self.account)));
        match seele_runtime::fs::read_private(&path, 4 * 1024 * 1024) {
            Ok(bytes) => match serde_json::from_slice::<Ledger>(&bytes) {
                Ok(ledger) if ledger.done.len() + ledger.notified.len() <= LIMIT * 2 => {
                    self.ledger = ledger
                }
                _ => self.error = "state-unavailable",
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => self.error = "state-unavailable",
        }
        self.ledger_path = Some(path);
    }
    fn persist(&mut self) {
        let saved = self.ledger_path.as_ref().is_some_and(|path| {
            serde_json::to_vec(&self.ledger)
                .ok()
                .is_some_and(|bytes| seele_runtime::fs::atomic_write(path, &bytes).is_ok())
        });
        if !saved {
            self.error = "state-unavailable"
        }
    }
    async fn publish(&self) -> std::io::Result<()> {
        let rows = model::rows(&self.entries);
        let value = json!({"event":"snapshot","host":self.api.host,"viewer":self.viewer,"account":self.account,"items":rows,"count":rows.len(),"complete":self.complete,"refreshing":self.polling,"error":model::message(self.error),"errorCode":self.error,"notice":self.notice,"selected":self.selected,"detail":self.entries.get(&self.selected).map(Entry::view)});
        seele_runtime::reactor::write_json(
            &mut common::FdIo::stdout()?,
            &value,
            WIRE,
            Duration::from_secs(5),
        )
        .await
    }
    fn refresh(&mut self, manual: bool) {
        let now = Instant::now();
        if self.polling
            || now.duration_since(self.last_poll) < Duration::from_secs(5)
            || (!manual && now < self.next_poll)
        {
            return;
        }
        self.poll += 1;
        let poll = self.poll;
        self.polling = true;
        self.seen.clear();
        self.last_poll = now;
        self.next_poll = now + Duration::from_secs(60);
        let api = self.api.clone();
        let tx = self.tx.clone();
        self.tasks.spawn(async move {
            let result = async {
                let (account, viewer) = api.identity().await?;
                tx.send(Event::Identity(poll, account, viewer))
                    .await
                    .map_err(|_| "cancelled")?;
                for page in 1..=LIMIT / 50 {
                    let batch = api.page(page).await?;
                    let count = batch.len();
                    tx.send(Event::Page(poll, batch))
                        .await
                        .map_err(|_| "cancelled")?;
                    if count < 50 {
                        return Ok(());
                    }
                }
                Err("inbox-too-large")
            }
            .await;
            let _ = tx.send(Event::Polled(poll, result)).await;
        });
    }
    fn upsert(&mut self, thread: Thread) {
        self.seen.insert(thread.id.clone());
        if self.ledger.done.get(&thread.id) == Some(&thread.revision) {
            return;
        }
        self.ledger.done.remove(&thread.id);
        if let Some(entry) = self.entries.get_mut(&thread.id) {
            if entry.thread.revision == thread.revision {
                entry.thread = thread;
                if entry.state == "ready" && entry.checked_at.elapsed() >= Duration::from_secs(300)
                {
                    entry.state = "pending";
                    entry.attempts = 0;
                }
                return;
            }
        }
        let token = self.token();
        let id = thread.id.clone();
        let mut fresh = Entry::new(thread, token);
        if let Some(old) = self.entries.remove(&id) {
            fresh.detail = old.detail;
            fresh.analyzed = old.analyzed;
            fresh.triage = old.triage;
            fresh.previous = old.previous;
        }
        for (key, cancel) in &self.jobs {
            if key.1 == id {
                cancel.cancel()
            }
        }
        self.entries.insert(id, fresh);
    }
    fn schedule(&mut self) {
        if matches!(
            self.error,
            "auth-required" | "access-denied" | "rate-limited"
        ) {
            return;
        }
        while self.jobs.len() < 2 {
            let id = self
                .entries
                .iter()
                .filter(|(_, e)| {
                    !e.pending_done
                        && (e.state == "pending"
                            || (e.state == "failed"
                                && e.attempts < 3
                                && Instant::now() >= e.retry_at))
                })
                .min_by_key(|(id, e)| {
                    (
                        if **id == self.selected { 0 } else { 1 },
                        std::cmp::Reverse(e.thread.updated_at.clone()),
                    )
                })
                .map(|(id, _)| id.clone());
            let Some(id) = id else { break };
            let revision = self.token();
            let entry = self.entries.get_mut(&id).unwrap();
            entry.state = "triaging";
            entry.error = "";
            entry.attempts += 1;
            let entry = entry.clone();
            let key = (self.epoch, id, entry.token);
            let cancel = self.cancel.child_token();
            self.jobs.insert(key.clone(), cancel.clone());
            let api = Api {
                cancel: cancel.clone(),
                ..self.api.clone()
            };
            let tx = self.tx.clone();
            let account = self.account.clone();
            let viewer = self.viewer.clone();
            self.tasks.spawn(async move {
                let result = async {
                    let detail = api.detail(&entry.thread).await?;
                    let _ = tx
                        .send(Event::Detail(key.clone(), Ok(detail.clone())))
                        .await;
                    if let (Some(old), Some(triage)) = (&entry.analyzed, &entry.triage) {
                        if old.fingerprint == model::digest(&detail) {
                            return Ok((detail, triage.clone()));
                        }
                    }
                    let triage =
                        triage::run(&account, &viewer, &entry, &detail, revision, cancel).await?;
                    Ok((detail, triage))
                }
                .await;
                let _ = tx.send(Event::Triaged(key, result)).await;
            });
        }
    }
    fn select(&mut self, id: &str) {
        let Some(entry) = self.entries.get(id) else {
            return;
        };
        self.selected = id.into();
        if let Some(cancel) = self.selection_job.take() {
            cancel.cancel()
        }
        // Detail fetch has its own single slot: a slow model never holds the
        // selected raw thread behind background inference.
        if entry.detail.is_none() {
            let cancel = self.cancel.child_token();
            self.selection_job = Some(cancel.clone());
            let api = Api {
                cancel,
                ..self.api.clone()
            };
            let thread = entry.thread.clone();
            let key = (self.epoch, id.to_owned(), entry.token);
            let tx = self.tx.clone();
            self.tasks.spawn(async move {
                let result = api.detail(&thread).await;
                let _ = tx.send(Event::Detail(key, result)).await;
            });
        }
    }
    fn done(&mut self, id: &str) {
        let Some(entry) = self.entries.get_mut(id) else {
            return;
        };
        if entry.pending_done || self.account.is_empty() {
            return;
        }
        entry.pending_done = true;
        self.notice = "Marking Done…".into();
        let key = (self.epoch, id.to_owned(), entry.token);
        let api = self.api.clone();
        let tx = self.tx.clone();
        let account = self.account.clone();
        self.tasks.spawn(async move {
            let result = async {
                if api.identity().await?.0 != account {
                    return Err("auth-required");
                };
                api.done(&key.1).await
            }
            .await;
            let _ = tx.send(Event::Done(key, result)).await;
        });
    }
    fn deliver(&mut self, id: &str) {
        let Some(entry) = self.entries.get(id) else {
            return;
        };
        let Some(triage) = &entry.triage else { return };
        if triage.rank() > 1 || entry.pending_done {
            return;
        }
        let fingerprint = model::digest(&(
            entry.thread.revision.clone(),
            entry.analyzed.as_ref().map(|s| &s.fingerprint),
        ));
        if self.ledger.notified.get(id) == Some(&fingerprint) {
            return;
        }
        self.ledger.notified.insert(id.into(), fingerprint.clone());
        self.alert_count += 1;
        let title = entry.thread.title.clone();
        let summary = triage.summary.clone();
        let urgent = triage.rank() == 0;
        self.persist();
        let cancel = self.cancel.clone();
        let tx = self.tx.clone();
        let id = id.to_owned();
        let epoch = self.epoch;
        // A bounded waiter owns the action for the native notification service.
        self.tasks.spawn(async move {
            let escape = |s: String| {
                s.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            };
            let mut command = Command::new("notify-send");
            command
                .args([
                    "--app-name=Seele GitHub",
                    "--icon=github",
                    "--wait",
                    "--action=default=Open in Seele",
                    "--expire-time=30000",
                    if urgent {
                        "--urgency=critical"
                    } else {
                        "--urgency=normal"
                    },
                    "--",
                ])
                .arg(escape(title))
                .arg(escape(summary));
            let result =
                common::command(command, vec![], Duration::from_secs(86400), 1024, cancel).await;
            if result
                .as_ref()
                .is_ok_and(|output| output == b"default\n" || output == b"default")
            {
                let _ = tx.send(Event::Focus(epoch, id.clone())).await;
            }
            let _ = tx
                .send(Event::Notified(epoch, id, fingerprint, result.is_ok()))
                .await;
        });
    }
    fn command(&mut self, value: Value) {
        let id = value["id"].as_str().unwrap_or_default();
        match value["op"].as_str() {
            Some("refresh") => self.refresh(true),
            Some("select") => self.select(id),
            Some("back") => self.selected.clear(),
            Some("done") => self.done(id),
            Some("retry") => {
                if let Some(entry) = self.entries.get_mut(id) {
                    if entry.state == "failed" || entry.state == "ready" {
                        entry.state = "pending";
                        entry.attempts = 0;
                        entry.error = "";
                    }
                }
            }
            Some("open") => {
                let url = self.entries.get(id).and_then(|e| {
                    api::safe_web(
                        &json!(e
                            .detail
                            .as_ref()
                            .map(|d| d.url.as_str())
                            .filter(|url| !url.is_empty())
                            .unwrap_or(&e.thread.url)),
                        &self.api.host,
                    )
                });
                if let Some(url) = url {
                    let cancel = self.cancel.clone();
                    self.tasks.spawn(async move {
                        let mut c = Command::new("xdg-open");
                        c.arg(url);
                        let _ = common::launch(c, Duration::from_secs(5), cancel).await;
                    });
                }
            }
            Some("inbox") => {
                let cancel = self.cancel.clone();
                let url = format!("https://{}/notifications", self.api.host);
                self.tasks.spawn(async move {
                    let mut c = Command::new("xdg-open");
                    c.arg(url);
                    let _ = common::launch(c, Duration::from_secs(5), cancel).await;
                });
            }
            _ => {}
        }
    }
    async fn event(&mut self, event: Event) -> bool {
        match event {
            Event::Eof => return false,
            Event::Command(value) => self.command(value),
            Event::Identity(poll, account, viewer) if poll == self.poll => {
                if account != self.account {
                    self.reset();
                    self.account = account;
                    self.load_ledger();
                }
                self.viewer = viewer;
            }
            Event::Page(poll, threads) if poll == self.poll => {
                for thread in threads {
                    self.upsert(thread)
                }
            }
            Event::Polled(poll, result) if poll == self.poll => {
                self.polling = false;
                match result {
                    Ok(()) => {
                        self.complete = true;
                        if self.error != "state-unavailable" {
                            self.error = ""
                        };
                        self.entries
                            .retain(|id, e| self.seen.contains(id) || e.pending_done);
                        self.ledger.done.retain(|id, _| self.seen.contains(id));
                        self.ledger.notified.retain(|id, _| self.seen.contains(id));
                        self.persist();
                    }
                    Err(error) => {
                        self.complete = false;
                        self.error = error;
                        self.next_poll = Instant::now()
                            + Duration::from_secs(if error == "rate-limited" { 300 } else { 60 });
                        if error == "auth-required" {
                            self.reset();
                            self.account.clear();
                            self.viewer.clear();
                        }
                    }
                }
            }
            Event::Detail(key, result) if self.current(&key) => {
                let entry = self.entries.get_mut(&key.1).unwrap();
                match result {
                    Ok(detail) => {
                        if entry.state != "ready" || entry.detail.is_none() {
                            entry.detail = Some(detail)
                        }
                    }
                    Err(error) => entry.error = error,
                }
            }
            Event::Triaged(key, result) => {
                self.jobs.remove(&key);
                if self.current(&key) {
                    let entry = self.entries.get_mut(&key.1).unwrap();
                    match result {
                        Ok((detail, triage)) => {
                            if entry.triage.as_ref().is_some_and(|old| {
                                serde_json::to_value(old).ok() != serde_json::to_value(&triage).ok()
                            }) {
                                entry.previous = entry.triage.take();
                            }
                            entry.analyzed = Some(model::SourceStamp::new(&detail));
                            entry.detail = Some(detail);
                            entry.triage = Some(triage);
                            entry.checked_at = Instant::now();
                            entry.state = "ready";
                            entry.error = "";
                            self.alerts.insert(key.1.clone());
                        }
                        Err(error) => {
                            entry.state = "failed";
                            entry.error = error;
                            entry.retry_at = Instant::now()
                                + Duration::from_secs(30 * u64::from(entry.attempts).pow(2));
                            if matches!(error, "rate-limited" | "auth-required" | "access-denied") {
                                self.error = error;
                                self.next_poll = Instant::now() + Duration::from_secs(300);
                            }
                        }
                    }
                }
            }
            Event::Done(key, result) if self.current(&key) => match result {
                Ok(()) => {
                    if let Some(entry) = self.entries.remove(&key.1) {
                        self.ledger
                            .done
                            .insert(key.1.clone(), entry.thread.revision);
                    }
                    for (job, cancel) in &self.jobs {
                        if job.1 == key.1 {
                            cancel.cancel()
                        }
                    }
                    self.persist();
                    if self.error == "write-failed" {
                        self.error = "";
                    }
                    self.notice =
                        "Marked Done on GitHub. Restoring it is available in GitHub's web inbox."
                            .into();
                    if self.selected == key.1 {
                        self.selected.clear();
                    }
                }
                Err(error) => {
                    self.entries.get_mut(&key.1).unwrap().pending_done = false;
                    self.error =
                        if matches!(error, "auth-required" | "rate-limited" | "access-denied") {
                            error
                        } else {
                            "write-failed"
                        };
                    self.notice.clear();
                }
            },
            Event::Notified(epoch, id, fingerprint, success) => {
                self.alert_count = self.alert_count.saturating_sub(1);
                if epoch == self.epoch
                    && !success
                    && self.ledger.notified.get(&id) == Some(&fingerprint)
                {
                    self.ledger.notified.remove(&id);
                    self.persist();
                    self.notice="A desktop alert could not be delivered. The notification remains in this inbox.".into();
                }
            }
            Event::Focus(epoch, id) if epoch == self.epoch && self.entries.contains_key(&id) => {
                self.select(&id);
                let value = json!({"event":"focus","id":id});
                let _ = seele_runtime::reactor::write_json(
                    &mut common::FdIo::stdout().unwrap(),
                    &value,
                    WIRE,
                    Duration::from_secs(5),
                )
                .await;
            }
            _ => {}
        }
        // GitHub remains the durable source. Retain at most sixteen full
        // threads, plus live jobs and the selected one; re-open fetches evicted
        // content. Small comparison stamps and triage results remain available.
        let mut cached = 0;
        for (id, entry) in &mut self.entries {
            if entry.detail.is_some() && *id != self.selected && entry.state != "triaging" {
                cached += 1;
                if cached > 16 {
                    entry.detail = None;
                }
            }
        }
        while self.alert_count < 16 {
            let Some(id) = self.alerts.iter().next().cloned() else {
                break;
            };
            self.alerts.remove(&id);
            self.deliver(&id);
        }
        self.schedule();
        true
    }
}
pub async fn watch(cancel: CancellationToken) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(64);
    let api = Api::new(cancel.clone())?;
    let mut inbox = Inbox::new(api, tx.clone(), cancel.clone());
    let reader_cancel = cancel.clone();
    let reader = tokio::spawn(async move {
        let Ok(stdin) = common::FdIo::stdin() else {
            let _ = tx.send(Event::Eof).await;
            return;
        };
        let mut reader = BufReader::new(stdin);
        loop {
            let line = tokio::select! {line=common::read_frame(&mut reader,16*1024)=>line,_=reader_cancel.cancelled()=>break};
            match line {
                Ok(Some(bytes)) => {
                    if let Ok(v) = serde_json::from_slice(&bytes) {
                        if tx.send(Event::Command(v)).await.is_err() {
                            break;
                        }
                    }
                }
                _ => {
                    let _ = tx.send(Event::Eof).await;
                    break;
                }
            }
        }
    });
    inbox.refresh(false);
    inbox.publish().await.map_err(|_| "unavailable")?;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _=cancel.cancelled()=>break,
            Some(event)=rx.recv()=>{
                if !inbox.event(event).await { break; }
                if inbox.publish().await.is_err() { break; }
            },
            _=tick.tick()=>{let before=(inbox.polling,inbox.jobs.len());inbox.refresh(false);inbox.schedule();if before!=(inbox.polling,inbox.jobs.len()) && inbox.publish().await.is_err(){break}},
            Some(_)=inbox.tasks.join_next(),if !inbox.tasks.is_empty()=>{},
        }
    }
    cancel.cancel();
    reader.abort();
    inbox.tasks.abort_all();
    while inbox.tasks.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests;
