//! One-account, read-only Google Calendar worker.
//!
//! The worker owns the account, the private cache and every calendar policy:
//! what a day's agenda holds, which dots a month draws, the bar indicator and
//! reminder delivery. It publishes small projected sections to the shell, each
//! only when it changed, so no event list crosses into QML and no binding
//! filters events. Network work runs beside the command loop, so browsing,
//! calendar selection and reminders never wait for Google. The planner gets
//! bounded timed busy blocks, never the source event cache.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, TimeZone, Utc};
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

mod cache;
mod content;
mod google;
mod reminders;
#[cfg(test)]
mod tests;
mod view;

use crate::common::{next_tick, suspend_offset, Publisher};
use cache::*;
use content::*;
use google::*;
use reminders::*;
use view::*;

const REFRESH_EVERY: i64 = 300;
/// A window the popup looks at is refetched in the background once it is this old.
const REVALIDATE_AFTER: i64 = 900;
const STALE_AFTER: i64 = 600;
const RETRY_AFTER: i64 = 30;
const PALETTE_EVERY: i64 = 86_400;

enum Message {
    Auth(u64, Result<bool, &'static str>),
    Sync(u64, Result<Fetched, Failure>),
    Wallet(Result<(), &'static str>),
}

struct Worker {
    state: State,
    http: Option<reqwest::Client>,
    tokens: Tokens,
    base: Option<Url>,
    path: PathBuf,
    messages: mpsc::UnboundedSender<Message>,
    auth: Option<JoinHandle<()>>,
    auth_serial: u64,
    sync: Option<JoinHandle<()>>,
    sync_serial: u64,
    clearing: Option<JoinHandle<()>>,
    running: Option<Job>,
    wanted: BTreeSet<(NaiveDate, NaiveDate)>,
    force: bool,
    online: Option<bool>,
    expired: bool,
    failures: u32,
    last_attempt: i64,
    cursor_saved: i64,
    error: String,
    today: NaiveDate,
    agenda_day: NaiveDate,
    rev: u64,
    index: Index,
    index_rev: u64,
    dots_rev: u64,
    publisher: Publisher,
}

impl Worker {
    fn new(state: State, messages: mpsc::UnboundedSender<Message>, error: &str) -> Self {
        let http = client();
        let today = Local::now().date_naive();
        Worker {
            error: http.as_ref().err().copied().unwrap_or(error).to_owned(),
            http: http.ok(),
            cursor_saved: state.checked_at,
            state,
            tokens: Tokens::default(),
            base: Url::parse(API).ok(),
            path: state_path(),
            messages,
            auth: None,
            auth_serial: 0,
            sync: None,
            sync_serial: 0,
            clearing: None,
            running: None,
            wanted: BTreeSet::new(),
            force: false,
            online: None,
            expired: false,
            failures: 0,
            last_attempt: 0,
            today,
            agenda_day: today,
            rev: 1,
            index: Index::default(),
            index_rev: 0,
            dots_rev: 0,
            publisher: Publisher::default(),
        }
    }

    fn status(&self) -> &'static str {
        // Signing in again over an expired session keeps that session's status,
        // so its saved events stay on screen; `signing_in` reports the attempt.
        if self.state.client_id.is_empty() {
            "setup"
        } else if !self.state.signed_in && self.auth.is_some() {
            "signing-in"
        } else if !self.state.signed_in {
            "signed-out"
        } else if self.expired {
            "expired"
        } else {
            match self.online {
                None => "connecting",
                Some(true) => "online",
                Some(false) => "offline",
            }
        }
    }

    fn persist(&mut self) -> Result<(), &'static str> {
        loop {
            match save(&self.path, &self.state) {
                Err("Calendar cache is full.")
                    if evict(&mut self.state, &[self.today, self.agenda_day]) =>
                {
                    self.rev += 1;
                }
                result => return result,
            }
        }
    }

    fn persist_or_report(&mut self) {
        if let Err(error) = self.persist() {
            self.error = error.into();
        }
    }

    fn covered(&self, day: NaiveDate) -> bool {
        self.state.windows.iter().any(|w| w.covers(day))
    }

    fn backoff(&self) -> i64 {
        (RETRY_AFTER << self.failures.saturating_sub(1).min(4)).min(REFRESH_EVERY)
    }

    /// Asks for the window holding `day`: a new one when it is not cached, a
    /// background refetch when the cached copy has aged. Either way the cached
    /// events stay on screen meanwhile.
    fn visit(&mut self, day: NaiveDate, now: i64, force: bool) {
        match self.state.windows.iter_mut().find(|w| w.covers(day)) {
            Some(window) => {
                window.used_at = now;
                if force || now - window.fetched_at >= REVALIDATE_AFTER {
                    if let Some(bounds) = window.bounds() {
                        self.wanted.insert(bounds);
                    }
                }
            }
            None => {
                self.wanted.insert(window_around(day));
            }
        }
    }

    fn input(&mut self, line: &str) {
        if line.len() > 8192 {
            return;
        }
        let Ok(command) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let now = Utc::now().timestamp();
        let day = command["date"].as_str().and_then(date);
        match command["action"].as_str().unwrap_or("") {
            "setup" => self.setup(command["client_id"].as_str().unwrap_or("")),
            "signin" => self.signin(command["client_secret"].as_str().unwrap_or("")),
            "cancel" => self.cancel_signin(),
            "select" => {
                if let Some(id) = command["id"].as_str() {
                    self.select(id, command["selected"] == true);
                }
            }
            "day" => {
                if let Some(day) = day {
                    self.agenda_day = day;
                    self.visit(day, now, false);
                }
            }
            "browse" => {
                if let Some(day) = day {
                    self.visit(day, now, false);
                }
            }
            "refresh" => {
                self.force = true;
                self.expired = false;
                self.visit(self.today, now, true);
                self.visit(self.agenda_day, now, true);
            }
            "disconnect" => self.disconnect(),
            _ => {}
        }
        self.pump(now);
    }

    fn setup(&mut self, value: &str) {
        if self.state.signed_in || self.auth.is_some() {
            return;
        }
        let id = value.trim();
        if id.is_empty() {
            if !self.state.client_id.is_empty() {
                self.state.client_id.clear();
                self.error.clear();
                self.persist_or_report();
            }
            return;
        }
        let valid = id.len() <= 512
            && id.ends_with(".apps.googleusercontent.com")
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b));
        if !valid {
            self.error =
                "That is not a Desktop OAuth client ID. It ends in .apps.googleusercontent.com."
                    .into();
            return;
        }
        self.error.clear();
        if self.state.client_id != id {
            self.state.client_id = id.to_owned();
            self.persist_or_report();
        }
    }

    fn signin(&mut self, secret: &str) {
        if self.state.client_id.is_empty() {
            self.error = "Enter a Google Desktop OAuth client ID first.".into();
            return;
        }
        if self.auth.is_some() {
            return;
        }
        if secret.len() > 4096 || secret.chars().any(char::is_control) {
            self.error = "That Google client secret is invalid.".into();
            return;
        }
        // A token stored now could be erased by the disconnect still clearing the wallet.
        if self
            .clearing
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            self.error = "Still disconnecting. Try again in a moment.".into();
            return;
        }
        self.auth_serial += 1;
        self.error.clear();
        let (serial, messages, client_id, saved_secret) = (
            self.auth_serial,
            self.messages.clone(),
            self.state.client_id.clone(),
            self.state.has_client_secret,
        );
        let provided = Zeroizing::new(secret.to_owned());
        self.auth = Some(tokio::spawn(async move {
            let result: Result<bool, &'static str> = async {
                let secret = if !provided.is_empty() {
                    provided
                } else if saved_secret {
                    Zeroizing::new(client_secret_wallet("lookup", None).await?)
                } else {
                    provided
                };
                if saved_secret && secret.is_empty() {
                    return Err("Enter the Google client secret again.");
                }
                let refresh = signin(client_id, &secret).await?;
                wallet("store", Some(&refresh)).await?;
                if !secret.is_empty() {
                    if let Err(error) = client_secret_wallet("store", Some(&secret)).await {
                        let _ = wallet("clear", None).await;
                        return Err(error);
                    }
                }
                Ok(!secret.is_empty())
            }
            .await;
            let _ = messages.send(Message::Auth(serial, result));
        }));
    }

    fn cancel_signin(&mut self) {
        self.auth_serial += 1;
        if let Some(task) = self.auth.take() {
            task.abort();
        }
        self.error.clear();
    }

    fn select(&mut self, id: &str, on: bool) {
        if !self.state.calendars.iter().any(|c| c["id"] == id) {
            return;
        }
        let changed = if on {
            self.state.selected.insert(id.to_owned())
        } else {
            self.state.selected.remove(id)
        };
        if !changed {
            return;
        }
        if on {
            // A calendar someone just switched on is worth one attempt now.
            self.force = true;
        } else {
            self.state.fetched.remove(id);
            self.state.events.retain(|event| calendar_of(event) != id);
        }
        self.rev += 1;
        self.persist_or_report();
    }

    fn disconnect(&mut self) {
        self.auth_serial += 1;
        self.sync_serial += 1;
        for task in [self.auth.take(), self.sync.take()].into_iter().flatten() {
            task.abort();
        }
        self.running = None;
        self.wanted.clear();
        self.force = false;
        self.online = None;
        self.expired = false;
        self.failures = 0;
        self.error.clear();
        self.tokens = Tokens::default();
        let had_client_secret = self.state.has_client_secret;
        // The client ID is not a secret; keeping it makes signing in again one step.
        self.state = State {
            client_id: std::mem::take(&mut self.state.client_id),
            ..fresh()
        };
        self.rev += 1;
        self.persist_or_report();
        let messages = self.messages.clone();
        self.clearing = Some(tokio::spawn(async move {
            let token = wallet("clear", None).await;
            let secret = if had_client_secret {
                client_secret_wallet("clear", None).await.map(|_| ())
            } else {
                Ok(())
            };
            let result = if token.is_ok() && secret.is_ok() {
                Ok(())
            } else {
                Err("Google credentials could not be removed from the system wallet.")
            };
            let _ = messages.send(Message::Wallet(result));
        }));
    }

    fn pump(&mut self, now: i64) {
        if self.running.is_some() || self.auth.is_some() {
            return;
        }
        if self.state.client_id.is_empty() || !self.state.signed_in {
            self.wanted.clear();
            self.force = false;
            return;
        }
        if !self.force
            && (self.expired || (self.failures > 0 && now - self.last_attempt < self.backoff()))
        {
            return;
        }
        let Some(http) = self.http.clone() else {
            return;
        };
        let unfetched = self
            .state
            .selected
            .iter()
            .any(|id| !self.state.fetched.contains(id));
        let full = unfetched || self.state.account_id.is_empty();
        let mut windows = std::mem::take(&mut self.wanted);
        if full {
            windows.extend(self.state.windows.iter().filter_map(Window::bounds));
            windows.insert(window_around(self.today));
        }
        self.force = false;
        if windows.is_empty() {
            return;
        }
        let Some(base) = self.base.clone() else {
            return;
        };
        self.last_attempt = now;
        self.sync_serial += 1;
        let job = Job {
            serial: self.sync_serial,
            client_id: self.state.client_id.clone(),
            has_client_secret: self.state.has_client_secret,
            account_id: self.state.account_id.clone(),
            calendars: self.state.selected.iter().cloned().collect(),
            windows: windows.into_iter().collect(),
            palette: self.state.palette.is_empty() || now - self.state.palette_at >= PALETTE_EVERY,
            base,
            full,
        };
        self.running = Some(job.clone());
        let (messages, tokens) = (self.messages.clone(), self.tokens.clone());
        self.sync = Some(tokio::spawn(async move {
            let serial = job.serial;
            let result = tokio::time::timeout(Duration::from_secs(60), fetch(http, tokens, job))
                .await
                .unwrap_or(Err(Failure::Other("Google Calendar refresh timed out.")));
            let _ = messages.send(Message::Sync(serial, result));
        }));
    }

    fn receive(&mut self, message: Message) {
        let now = Utc::now().timestamp();
        match message {
            Message::Sync(serial, result) => {
                if serial != self.sync_serial {
                    return;
                }
                self.sync = None;
                let Some(job) = self.running.take() else {
                    return;
                };
                match result {
                    Ok(fetched) => {
                        merge(
                            &mut self.state,
                            &job,
                            fetched,
                            now,
                            &[self.today, self.agenda_day],
                        );
                        self.online = Some(true);
                        self.expired = false;
                        self.failures = 0;
                        self.error.clear();
                        self.rev += 1;
                        self.persist_or_report();
                    }
                    Err(failure) => {
                        self.online = Some(false);
                        self.failures = self.failures.saturating_add(1);
                        self.error = failure.message().into();
                        if failure == Failure::Expired {
                            self.expired = true;
                            self.wanted.clear();
                        } else {
                            self.wanted.extend(job.windows);
                        }
                    }
                }
            }
            Message::Auth(serial, result) => {
                if serial != self.auth_serial {
                    return;
                }
                self.auth = None;
                match result {
                    Ok(has_client_secret) => {
                        self.state.signed_in = true;
                        self.state.has_client_secret = has_client_secret;
                        self.expired = false;
                        self.online = None;
                        self.failures = 0;
                        self.error.clear();
                        self.tokens = Tokens::default();
                        self.force = true;
                        self.wanted.insert(window_around(self.today));
                        self.persist_or_report();
                    }
                    Err(error) => self.error = error.into(),
                }
            }
            Message::Wallet(result) => {
                self.clearing = None;
                if let Err(error) = result {
                    self.error = error.into();
                }
            }
        }
        self.pump(now);
    }

    fn tick(&mut self, woke: bool) {
        let now = Utc::now().timestamp();
        self.today = Local::now().date_naive();
        self.deliver_reminders(now, woke);
        if woke {
            // The network is probably back; do not wait out an offline backoff.
            self.failures = 0;
        }
        if self.state.signed_in
            && !self.expired
            && (woke || now - self.last_attempt >= REFRESH_EVERY)
        {
            self.wanted.insert(window_around(self.today));
            if !self.covered(self.agenda_day) {
                self.wanted.insert(window_around(self.agenda_day));
            }
        }
        self.pump(now);
    }

    fn deliver_reminders(&mut self, now: i64, woke: bool) {
        let checked = self.state.checked_at;
        let previous = if checked == 0 { now - 60 } else { checked };
        let due = due_reminders(&self.state, previous, now);
        let missed = woke || now - previous > 120;
        self.state.checked_at = now;
        self.state
            .delivered
            .retain(|_, at| *at > now - 60 * 60 * 24 * 45);
        if due.is_empty() {
            if now - self.cursor_saved >= 300 && self.persist().is_ok() {
                self.cursor_saved = now;
            }
            return;
        }
        for reminder in &due {
            self.state.delivered.insert(reminder.key.clone(), now);
        }
        // Delivery keys are committed before notifying, so a restart cannot repeat them.
        match self.persist() {
            Ok(()) => {
                self.cursor_saved = now;
                let (title, body) = reminder_text(&due, now, self.today, missed);
                tokio::spawn(notify(title, body));
            }
            Err(_) => {
                for reminder in &due {
                    self.state.delivered.remove(&reminder.key);
                }
                self.state.checked_at = checked;
                self.error = "Calendar reminders could not be saved.".into();
            }
        }
    }

    fn emit(&mut self) {
        if let Some(line) = self.changes() {
            println!("{line}");
        }
    }

    /// The sections that changed since the last line, or nothing.
    fn changes(&mut self) -> Option<Value> {
        let now = Utc::now().timestamp();
        if self.index_rev != self.rev {
            self.index = index(&self.state);
            self.index_rev = self.rev;
        }
        let status = self.status();
        let covered = self.covered(self.agenda_day);
        // A day being fetched, or queued behind the fetch now running, is loading;
        // one held back by an offline backoff is not.
        let day = self.agenda_day;
        let holds = |&(a, b): &(NaiveDate, NaiveDate)| a <= day && day < b;
        let loading = !covered
            && self
                .running
                .as_ref()
                .is_some_and(|job| job.windows.iter().any(holds) || self.wanted.iter().any(holds));
        let refreshed = self.state.refreshed_at;
        let updated = match local_date(refreshed) {
            _ if refreshed == 0 => String::new(),
            Some(day) if day == self.today => hm(refreshed),
            _ => Local
                .timestamp_opt(refreshed, 0)
                .single()
                .map(|v| v.format("%-d %b %H:%M").to_string())
                .unwrap_or_default(),
        };
        let syncing = self.running.is_some();
        let mut sections = vec![
            (
                "account",
                json!({
                    "status": status,
                    "configured": !self.state.client_id.is_empty(),
                    "account": self.state.account_id,
                    "syncing": syncing,
                    "signing_in": self.auth.is_some(),
                    "error": self.error,
                    "refreshed_at": refreshed,
                    "updated": updated,
                    "stale": status != "online" || refreshed == 0 || now - refreshed > STALE_AFTER,
                }),
            ),
            (
                "calendars",
                Value::Array(
                    self.state
                        .calendars
                        .iter()
                        .map(|c| {
                            let id = text(&c["id"]);
                            let selected = self.state.selected.contains(id);
                            json!({
                                "id": id,
                                "name": text(&c["name"]),
                                "color": text(&c["color"]),
                                "role": role_label(c),
                                "selected": selected,
                                "loading": selected && !self.state.fetched.contains(id),
                            })
                        })
                        .collect(),
                ),
            ),
            (
                "agenda",
                json!({
                    "day": self.agenda_day.to_string(),
                    "covered": covered,
                    "loading": loading,
                    "items": (if covered { agenda_items(&self.state, &self.index, self.agenda_day) } else { Vec::new() }),
                }),
            ),
            (
                "indicator",
                indicator(&self.state, &self.index, now, self.today),
            ),
        ];
        // A minute heartbeat lets the shell tell a quiet worker from a hung one.
        sections.push(("heartbeat", json!(now / 60)));
        if self.dots_rev != self.rev {
            self.dots_rev = self.rev;
            sections.push(("dots", dots(&self.state, &self.index)));
            sections.push(("busy", busy(&self.state, &self.index)));
            sections.push((
                "coverage",
                json!(self
                    .state
                    .windows
                    .iter()
                    .map(|w| (&w.start, &w.end))
                    .collect::<Vec<_>>()),
            ));
        }
        self.publisher.changes(sections)
    }
}

pub async fn run() {
    let (state, problem) = load();
    let (sender, mut messages) = mpsc::unbounded_channel();
    let mut worker = Worker::new(state, sender, problem);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut previous_offset = suspend_offset();
    worker.tick(false);
    worker.emit();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break; };
                worker.input(&line);
            }
            Some(message) = messages.recv() => worker.receive(message),
            _ = tokio::time::sleep(next_tick()) => {
                let offset = suspend_offset();
                let woke = offset.zip(previous_offset).is_some_and(|(a, b)| a - b > 5_000_000_000);
                previous_offset = offset;
                worker.tick(woke);
            }
        }
        worker.emit();
    }
}
