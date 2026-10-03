//! Local weather for the clock popup, from Open-Meteo.
//!
//! The worker owns the place, fetching, the private cache, units, conditions,
//! local times and every other presentation decision. It publishes small
//! projected sections to the shell, each only when it changed, so QML draws
//! finished labels and never parses a forecast.
//!
//! The default place is the system timezone's reference city in the tz
//! database, the same place the theme switcher reckons sunrise and sunset
//! from. Nothing asks for or derives a more precise location. A place chosen
//! through the popup's search replaces it in the private state file until
//! "Use timezone city" clears it.
//!
//! A forecast is refreshed every half hour with a few minutes of jitter and
//! after a resume. A failed refresh keeps the last good forecast on screen and
//! marks it stale; retries back off quietly, with no notification of any kind.
use crate::common::{clean, next_tick, suspend_offset, Publisher};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use seele_runtime::timezone;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    hash::BuildHasher,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc,
    task::JoinHandle,
};
use url::Url;

mod cache;
mod open_meteo;
#[cfg(test)]
mod tests;
mod units;
mod view;

use cache::*;
use open_meteo::*;
use units::*;

pub(super) const REFRESH_EVERY: i64 = 30 * 60;
/// Spread over five minutes, so a fleet of machines does not ask at once.
pub(super) const JITTER: i64 = 5 * 60;
pub(super) const RETRY_FIRST: i64 = 60;
/// A forecast this old is stale even when nothing failed, as after a suspend.
pub(super) const STALE_AFTER: i64 = 2 * 3600;
/// After a resume the network usually needs a moment before it answers.
pub(super) const WAKE_SETTLE: i64 = 15;
pub(super) const SEARCH_SETTLE: Duration = Duration::from_millis(250);
pub(super) const QUERY_LIMIT: usize = 80;
pub(super) const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Integration Health hears from the worker at least this often.
pub(super) const HEALTH_EVERY: i64 = 120;
pub(super) const MAX_INPUT: usize = 4096;

enum Message {
    Forecast(u64, Result<Forecast, Failure>),
    Search(u64, Result<Vec<Found>, Failure>),
}

/// The place forecasts are for, from the chosen place or the timezone city.
#[derive(Clone, Debug, PartialEq)]
struct Target {
    name: String,
    detail: String,
    chosen: bool,
    latitude: f64,
    longitude: f64,
}

impl Target {
    /// The coordinates as they are sent, which a cached forecast is kept against.
    fn key(&self) -> String {
        format!(
            "{},{}",
            coordinate(self.latitude),
            coordinate(self.longitude)
        )
    }
}

#[derive(Default)]
struct Search {
    query: String,
    serial: u64,
    task: Option<JoinHandle<()>>,
    busy: bool,
    results: Vec<Found>,
    error: String,
}

/// A uniformly spread number of seconds below `limit`, from the standard
/// library's randomly keyed hasher; jitter needs no stronger source.
fn jitter(limit: i64) -> i64 {
    let random = std::collections::hash_map::RandomState::new().hash_one(limit);
    (random % limit.max(1) as u64) as i64
}

struct Worker {
    state: State,
    path: PathBuf,
    http: Option<reqwest::Client>,
    endpoints: Endpoints,
    units: Units,
    messages: mpsc::UnboundedSender<Message>,
    /// Whether ticks follow the system timezone; tests pin the place instead.
    follow_zone: bool,
    zone: Option<String>,
    city: Option<timezone::Place>,
    fetch: Option<JoinHandle<()>>,
    fetch_serial: u64,
    fetch_key: String,
    due: i64,
    failures: u32,
    failed: bool,
    /// Health Retry tokens waiting for the refresh they started, and the
    /// answers not yet published.
    retries: Vec<u64>,
    answered: Vec<(u64, bool)>,
    search: Search,
    error: String,
    publisher: Publisher,
}

impl Worker {
    fn new(
        state: State,
        path: PathBuf,
        messages: mpsc::UnboundedSender<Message>,
        units: Units,
        problem: &str,
    ) -> Self {
        let http = client();
        // A forecast saved by the previous run is refreshed on its own
        // schedule rather than again the moment the worker starts.
        let due = state.cached.as_ref().map_or(0, |cached| {
            cached.fetched_at + REFRESH_EVERY + jitter(JITTER)
        });
        Worker {
            error: http.as_ref().err().copied().unwrap_or(problem).to_owned(),
            http: http.ok(),
            state,
            path,
            endpoints: Endpoints::default(),
            units,
            messages,
            follow_zone: true,
            zone: None,
            city: None,
            fetch: None,
            fetch_serial: 0,
            fetch_key: String::new(),
            due,
            failures: 0,
            failed: false,
            retries: Vec::new(),
            answered: Vec::new(),
            search: Search::default(),
            publisher: Publisher::default(),
        }
    }

    fn target(&self) -> Option<Target> {
        if let Some(chosen) = &self.state.chosen {
            return Some(Target {
                name: chosen.name.clone(),
                detail: chosen.detail.clone(),
                chosen: true,
                latitude: chosen.latitude,
                longitude: chosen.longitude,
            });
        }
        self.city.as_ref().map(|city| Target {
            name: city.label.clone(),
            detail: format!("Timezone city · {}", city.zone),
            chosen: false,
            latitude: city.latitude,
            longitude: city.longitude,
        })
    }

    /// The cached forecast, while it is for the place now shown.
    fn cached(&self) -> Option<&Cached> {
        let key = self.target()?.key();
        self.state
            .cached
            .as_ref()
            .filter(|cached| cached.key == key)
    }

    fn persist_or_report(&mut self) {
        match save(&self.path, &self.state) {
            Ok(()) => {
                if self.error.starts_with("Weather state") {
                    self.error.clear();
                }
            }
            Err(error) => self.error = error.into(),
        }
    }

    fn cancel_fetch(&mut self) {
        self.fetch_serial += 1;
        if let Some(task) = self.fetch.take() {
            task.abort();
        }
    }

    /// The place changed: a forecast for the old one is dropped, and the new
    /// one is fetched at once. Returns whether anything was dropped and saved.
    fn relocate(&mut self, now: i64) -> bool {
        let key = self.target().map(|target| target.key());
        let cached = self.state.cached.as_ref().map(|c| c.key.clone());
        if self.fetch.is_some() && key.as_ref() != Some(&self.fetch_key) {
            self.cancel_fetch();
        }
        if cached == key {
            return false;
        }
        self.due = now;
        self.failures = 0;
        self.failed = false;
        if cached.is_none() {
            return false;
        }
        self.state.cached = None;
        self.persist_or_report();
        true
    }

    /// Follows the system timezone, which moves when the machine travels.
    fn locate(&mut self, now: i64) {
        let zone = timezone::zone();
        if zone == self.zone {
            return;
        }
        self.city = zone
            .as_deref()
            .and_then(|zone| timezone::place_in(&timezone::zoneinfo(), zone));
        self.zone = zone;
        if self.state.chosen.is_none() {
            self.relocate(now);
        }
    }

    fn backoff(&self) -> i64 {
        (RETRY_FIRST << self.failures.saturating_sub(1).min(5)).min(REFRESH_EVERY)
    }

    fn input(&mut self, line: &str) {
        if line.len() > MAX_INPUT {
            return;
        }
        let Ok(command) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let now = Utc::now().timestamp();
        match command["action"].as_str().unwrap_or("") {
            "refresh" => {
                let token = command["token"].as_u64();
                if self.target().is_none() {
                    self.answered.extend(token.map(|token| (token, false)));
                } else {
                    self.due = self.due.min(now);
                    self.retries.extend(token);
                }
            }
            "search" => self.search_for(command["query"].as_str().unwrap_or("")),
            "choose" => {
                if let Some(id) = command["id"].as_u64() {
                    self.choose(id, now);
                }
            }
            "reset" => self.reset(now),
            _ => {}
        }
        self.pump(now);
    }

    fn clear_search(&mut self) {
        self.search.serial += 1;
        if let Some(task) = self.search.task.take() {
            task.abort();
        }
        self.search.query.clear();
        self.search.busy = false;
        self.search.results.clear();
        self.search.error.clear();
    }

    /// A newer query supersedes an older one, and the request waits a moment
    /// so typing a name sends one search rather than one per letter.
    fn search_for(&mut self, query: &str) {
        let query = clean(&Value::String(query.to_owned()), "", QUERY_LIMIT)
            .trim()
            .to_owned();
        if query == self.search.query {
            return;
        }
        self.clear_search();
        self.search.query = query.clone();
        // Open-Meteo answers a single letter with nothing at all.
        if query.chars().count() < 2 {
            return;
        }
        let Some(http) = self.http.clone() else {
            self.search.error = Failure::Offline.message().into();
            return;
        };
        self.search.busy = true;
        let (serial, endpoints, messages) = (
            self.search.serial,
            self.endpoints.clone(),
            self.messages.clone(),
        );
        self.search.task = Some(tokio::spawn(async move {
            tokio::time::sleep(SEARCH_SETTLE).await;
            let result = tokio::time::timeout(FETCH_TIMEOUT, search(&http, &endpoints, &query))
                .await
                .unwrap_or(Err(Failure::Offline));
            let _ = messages.send(Message::Search(serial, result));
        }));
    }

    /// Only a place the last search returned can be chosen; the shell names
    /// it by id and never supplies coordinates of its own.
    fn choose(&mut self, id: u64, now: i64) {
        let Some(found) = self.search.results.iter().find(|f| f.id == id).cloned() else {
            return;
        };
        self.state.chosen = Some(Chosen {
            id: found.id,
            name: found.name,
            detail: found.detail,
            latitude: found.latitude,
            longitude: found.longitude,
        });
        self.clear_search();
        if !self.relocate(now) {
            self.persist_or_report();
        }
    }

    /// "Use timezone city": the chosen place is forgotten.
    fn reset(&mut self, now: i64) {
        if self.state.chosen.take().is_none() {
            return;
        }
        self.clear_search();
        if !self.relocate(now) {
            self.persist_or_report();
        }
    }

    fn pump(&mut self, now: i64) {
        if self.fetch.is_some() || now < self.due {
            return;
        }
        let (Some(target), Some(http)) = (self.target(), self.http.clone()) else {
            return;
        };
        self.fetch_serial += 1;
        self.fetch_key = target.key();
        let (serial, endpoints, messages) = (
            self.fetch_serial,
            self.endpoints.clone(),
            self.messages.clone(),
        );
        self.fetch = Some(tokio::spawn(async move {
            let result = tokio::time::timeout(
                FETCH_TIMEOUT,
                forecast(&http, &endpoints, target.latitude, target.longitude),
            )
            .await
            .unwrap_or(Err(Failure::Offline));
            let _ = messages.send(Message::Forecast(serial, result));
        }));
    }

    fn receive(&mut self, message: Message) {
        let now = Utc::now().timestamp();
        match message {
            Message::Forecast(serial, result) => {
                if serial != self.fetch_serial {
                    return;
                }
                self.fetch = None;
                if self.target().map(|t| t.key()).as_deref() != Some(self.fetch_key.as_str()) {
                    self.due = now;
                    self.pump(now);
                    return;
                }
                let ok = result.is_ok();
                match result {
                    Ok(forecast) => {
                        self.state.cached = Some(Cached {
                            key: self.fetch_key.clone(),
                            fetched_at: now,
                            forecast,
                        });
                        self.failed = false;
                        self.failures = 0;
                        self.due = now + REFRESH_EVERY + jitter(JITTER);
                        self.persist_or_report();
                    }
                    Err(_) => {
                        // Quiet: no notification, and the last forecast stays.
                        self.failed = true;
                        self.failures = self.failures.saturating_add(1);
                        self.due = now + self.backoff() + jitter(RETRY_FIRST / 2);
                    }
                }
                self.answered
                    .extend(self.retries.drain(..).map(|token| (token, ok)));
            }
            Message::Search(serial, result) => {
                if serial != self.search.serial {
                    return;
                }
                self.search.task = None;
                self.search.busy = false;
                match result {
                    Ok(results) => {
                        self.search.results = results;
                        self.search.error.clear();
                    }
                    Err(Failure::Offline) => {
                        self.search.results.clear();
                        self.search.error = "Place search needs a connection.".into();
                    }
                    Err(failure) => {
                        self.search.results.clear();
                        self.search.error = failure.message().into();
                    }
                }
            }
        }
        self.pump(now);
    }

    fn tick(&mut self, woke: bool) {
        let now = Utc::now().timestamp();
        if self.follow_zone {
            self.locate(now);
        }
        if woke {
            // The network is probably coming back; a backoff from before the
            // suspend says nothing now, but give it a moment to answer.
            self.failures = 0;
            let aged = self
                .cached()
                .is_none_or(|cached| now - cached.fetched_at >= REFRESH_EVERY);
            if aged || self.failed {
                self.due = now + WAKE_SETTLE;
            }
        }
        self.pump(now);
    }

    fn status(&self, now: i64) -> Value {
        let cached = self.cached();
        let state = match (self.target(), cached) {
            (None, _) => "no-place",
            (Some(_), _) if self.failed => "offline",
            (Some(_), None) => "connecting",
            (Some(_), Some(_)) => "online",
        };
        let fetched = cached
            .map(|c| view::fetched(c.fetched_at, now))
            .unwrap_or_default();
        let stale = cached.is_none_or(|c| self.failed || now - c.fetched_at > STALE_AFTER);
        let label = match state {
            "no-place" => "Choose a place for the forecast".to_owned(),
            "connecting" => "Loading the forecast…".to_owned(),
            "offline" if cached.is_some() => format!("Offline · forecast from {fetched}"),
            "offline" => "Offline".to_owned(),
            _ if stale => format!("Forecast from {fetched}"),
            _ => format!("Updated {fetched}"),
        };
        // The folded line has room for one word about a forecast it cannot vouch for.
        let badge = match (cached, state) {
            (None, _) => "",
            (Some(_), "offline") => "Offline",
            (Some(_), _) if stale => "Outdated",
            _ => "",
        };
        json!({
            "state": state,
            "fetching": self.fetch.is_some(),
            "stale": stale,
            "label": label,
            "badge": badge,
            "error": self.error,
        })
    }

    fn health(&self, now: i64) -> Value {
        let cached = self.cached();
        let (state, summary) = match (self.target(), cached) {
            (None, _) => ("setup-required", "Choose a place for the weather"),
            (Some(_), None) if self.failed => ("disconnected", "Open-Meteo is unreachable"),
            // The first fetch is a transition, not a health state.
            (Some(_), None) => return Value::Null,
            (Some(_), Some(_)) if self.failed => {
                ("degraded", "Offline · showing the last forecast")
            }
            (Some(_), Some(c)) if now - c.fetched_at > STALE_AFTER => {
                ("degraded", "Not refreshed recently")
            }
            (Some(_), Some(_)) => ("healthy", "Forecast up to date"),
        };
        json!({
            "state": state,
            "summary": summary,
            "last_success": cached.map(|c| c.fetched_at * 1000).unwrap_or(0),
            // Republished on this cadence so a quiet worker is not a stale one.
            "beat": now / HEALTH_EVERY,
        })
    }

    /// The sections that changed since the last line, or nothing.
    fn changes(&mut self) -> Option<Value> {
        let now = Utc::now().timestamp();
        let target = self.target();
        let forecast = self.cached().map(|cached| &cached.forecast);
        let mut sections = vec![
            (
                "place",
                json!({
                    "name": target.as_ref().map(|t| t.name.as_str()).unwrap_or(""),
                    "detail": target.as_ref().map(|t| t.detail.as_str()).unwrap_or(""),
                    "chosen": target.as_ref().is_some_and(|t| t.chosen),
                    "city": self.city.as_ref().map(|c| c.label.as_str()).unwrap_or(""),
                }),
            ),
            ("status", self.status(now)),
            (
                "current",
                forecast.map_or(Value::Null, |f| view::current(f, now, self.units)),
            ),
            (
                "hours",
                forecast.map_or(json!([]), |f| view::hours(f, now, self.units)),
            ),
            (
                "days",
                forecast.map_or(json!([]), |f| view::days(f, now, self.units)),
            ),
            (
                "search",
                json!({
                    "query": self.search.query,
                    "busy": self.search.busy,
                    "error": self.search.error,
                    "results": self.search.results.iter().map(|found| json!({
                        "id": found.id,
                        "name": found.name,
                        "detail": found.detail,
                    })).collect::<Vec<_>>(),
                }),
            ),
            ("health", self.health(now)),
            // A minute heartbeat lets the shell tell a quiet worker from a hung one.
            ("heartbeat", json!(now / 60)),
        ];
        if !self.answered.is_empty() {
            let answered: Vec<Value> = self
                .answered
                .drain(..)
                .map(|(token, ok)| json!({"token": token, "ok": ok}))
                .collect();
            sections.push(("retried", Value::Array(answered)));
        }
        self.publisher.changes(sections)
    }

    /// Writes the changed sections as one line. False once the shell has
    /// closed its end, which ends the worker rather than panicking on a pipe.
    fn emit(&mut self) -> bool {
        let Some(line) = self.changes() else {
            return true;
        };
        let mut out = std::io::stdout().lock();
        writeln!(out, "{line}").and_then(|()| out.flush()).is_ok()
    }
}

pub async fn run() {
    let path = state_path();
    let (state, problem) = load(&path);
    let (sender, mut messages) = mpsc::unbounded_channel();
    let mut worker = Worker::new(state, path, sender, units::system(), problem);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut previous_offset = suspend_offset();
    worker.tick(false);
    if !worker.emit() {
        return;
    }
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
        if !worker.emit() {
            break;
        }
    }
}
