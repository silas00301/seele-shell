//! One-account, read-only Google Calendar worker. Only public calendar data leaves this process.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    process::Command,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::mpsc,
};
use url::Url;
use uuid::Uuid;

const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";
const MAX_STATE: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 5000;
const MAX_CALENDARS: usize = 64;
const MAX_PAGES: usize = 20;
const WALLET: [&str; 4] = ["application", "seele-google-calendar", "account", "primary"];

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
    client_id: String,
    account_id: String,
    calendars: Vec<Value>,
    colors: Value,
    selected: BTreeSet<String>,
    events: Vec<Value>,
    range_start: String,
    range_end: String,
    ranges: Vec<[String; 2]>,
    refreshed_at: i64,
    checked_at: i64,
    delivered: BTreeMap<String, i64>,
}

fn state_path() -> PathBuf {
    crate::common::xdg("XDG_STATE_HOME", ".local/state").join("seele-calendar/state.json")
}

fn load() -> Result<State, &'static str> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(state_path())
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(_) => return Err("Calendar state could not be opened."),
    };
    let meta = file
        .metadata()
        .map_err(|_| "Calendar state could not be read.")?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.len() as usize > MAX_STATE
    {
        return Err("Calendar state must be a private user file.");
    }
    let mut bytes = Vec::new();
    file.take((MAX_STATE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Calendar state could not be read.")?;
    if bytes.len() > MAX_STATE {
        return Err("Calendar state is too large.");
    }
    serde_json::from_slice(&bytes).map_err(|_| "Calendar state is invalid.")
}

fn save(state: &State) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(state).map_err(|_| "Calendar state could not be encoded.")?;
    if bytes.len() > MAX_STATE {
        return Err("Calendar cache is full.");
    }
    seele_runtime::fs::atomic_write(&state_path(), &bytes)
        .map_err(|_| "Calendar state could not be saved.")
}

async fn wallet(action: &str, secret: Option<&str>) -> Result<String, &'static str> {
    let mut command = Command::new("secret-tool");
    command.arg(action);
    if action == "store" {
        command.arg("--label=Seele Google Calendar");
    }
    command.args(WALLET);
    let bytes = crate::common::command(
        command,
        secret.unwrap_or("").as_bytes().to_vec(),
        if action == "lookup" {
            Duration::from_secs(10)
        } else {
            Duration::from_secs(120)
        },
        8192,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .map_err(|_| "Unlock your system wallet and retry Google Calendar.")?;
    String::from_utf8(bytes)
        .map(|v| v.trim_end_matches('\n').to_owned())
        .map_err(|_| "The system wallet returned invalid credentials.")
}

fn client() -> Result<reqwest::Client, &'static str> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Calendar network client is unavailable.")
}

async fn access_token(client: &reqwest::Client, state: &State) -> Result<String, &'static str> {
    let refresh = zeroize::Zeroizing::new(wallet("lookup", None).await?);
    if refresh.is_empty() {
        return Err("Sign in to Google Calendar.");
    }
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", state.client_id.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|_| "Google Calendar is offline.")?;
    if !response.status().is_success() {
        return Err("Google Calendar sign-in expired. Sign in again.");
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Google returned an invalid sign-in response.")?;
    value["access_token"]
        .as_str()
        .filter(|v| v.len() < 8192)
        .map(str::to_owned)
        .ok_or("Google returned no access token.")
}

async fn get(client: &reqwest::Client, token: &str, url: Url) -> Result<Value, &'static str> {
    if url.scheme() != "https" || url.host_str() != Some("www.googleapis.com") {
        return Err("Invalid Google Calendar endpoint.");
    }
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "Google Calendar is offline.")?;
    if response.status().as_u16() == 401 {
        return Err("Google Calendar sign-in expired. Sign in again.");
    }
    if !response.status().is_success() {
        return Err("Google Calendar could not refresh events.");
    }
    if response
        .content_length()
        .is_some_and(|v| v > MAX_STATE as u64)
    {
        return Err("Google Calendar response is too large.");
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "Google Calendar response is incomplete.")?;
    if bytes.len() > MAX_STATE {
        return Err("Google Calendar response is too large.");
    }
    serde_json::from_slice(&bytes).map_err(|_| "Google Calendar response is invalid.")
}

async fn pages(
    client: &reqwest::Client,
    token: &str,
    url: Url,
    max: usize,
) -> Result<Vec<Value>, &'static str> {
    let mut all = Vec::new();
    let mut page_token = None::<String>;
    for _ in 0..MAX_PAGES {
        let mut page_url = url.clone();
        if let Some(token) = page_token.take() {
            page_url.query_pairs_mut().append_pair("pageToken", &token);
        }
        let page = get(client, token, page_url).await?;
        let items = page["items"]
            .as_array()
            .ok_or("Google Calendar list is invalid.")?;
        if all.len() + items.len() > max {
            return Err("Google Calendar list exceeds the cache limit.");
        }
        all.extend(items.iter().cloned());
        if let Some(next) = page["nextPageToken"].as_str() {
            page_token = Some(next.to_owned());
        } else {
            return Ok(all);
        }
    }
    Err("Google Calendar returned too many pages.")
}

fn visible(event: &Value) -> bool {
    event["status"] != "cancelled"
        && event["attendees"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["self"] == true))
            .and_then(|v| v["responseStatus"].as_str())
            != Some("declined")
}

fn project_calendar(value: &Value) -> Option<Value> {
    let id = value["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 512)?;
    Some(json!({
        "id": id,
        "summary": crate::common::clean(&value["summary"], "", 160),
        "primary": value["primary"] == true,
        "backgroundColor": value["backgroundColor"],
        "timeZone": value["timeZone"],
        "defaultReminders": value["defaultReminders"],
    }))
}

fn project_event(value: &Value, calendar_id: &str) -> Option<Value> {
    let id = value["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 512)?;
    let self_attendee = value["attendees"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["self"] == true))
        .map(|item| json!({"self":true,"responseStatus":item["responseStatus"]}));
    let video = value["conferenceData"]["entryPoints"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["entryPointType"] == "video"))
        .map(|item| json!({"entryPointType":"video","uri":item["uri"]}));
    Some(json!({
        "id":id,"calendar_id":calendar_id,"status":value["status"],
        "summary":crate::common::clean(&value["summary"],"",240),
        "start":value["start"],"end":value["end"],
        "location":crate::common::clean(&value["location"],"",512),
        "description":crate::common::clean(&value["description"],"",8192),
        "htmlLink":value["htmlLink"],"hangoutLink":value["hangoutLink"],
        "conferenceData":{"entryPoints":video.into_iter().collect::<Vec<_>>()},
        "attendees":self_attendee.into_iter().collect::<Vec<_>>(),
        "reminders":value["reminders"],"colorId":value["colorId"],
    }))
}

fn date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn covers(state: &State, day: NaiveDate) -> bool {
    let value = day.to_string();
    state
        .ranges
        .iter()
        .any(|range| range[0] <= value && value < range[1])
        || (state.ranges.is_empty() && state.range_start <= value && value < state.range_end)
}

fn overlaps(event: &Value, start: NaiveDate, end: NaiveDate) -> bool {
    if let (Some(a), Some(b)) = (
        event["start"]["date"].as_str(),
        event["end"]["date"].as_str(),
    ) {
        return a < end.to_string().as_str() && b > start.to_string().as_str();
    }
    let a = event["start"]["dateTime"]
        .as_str()
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok());
    let b = event["end"]["dateTime"]
        .as_str()
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok());
    let start = start
        .and_hms_opt(0, 0, 0)
        .map(|v| v.and_utc().timestamp())
        .unwrap_or_default();
    let end = end
        .and_hms_opt(0, 0, 0)
        .map(|v| v.and_utc().timestamp())
        .unwrap_or_default();
    a.is_some_and(|v| v.timestamp() < end) && b.is_some_and(|v| v.timestamp() > start)
}

fn midnight(day: NaiveDate, zone: chrono_tz::Tz) -> Option<i64> {
    for minute in 0..180 {
        let local = day.and_hms_opt(0, minute / 60, minute % 60)?;
        if let Some(value) = zone.from_local_datetime(&local).earliest() {
            return Some(value.timestamp());
        }
    }
    None
}

fn start_time(event: &Value, calendar: &Value) -> Option<i64> {
    let zone = event["start"]["timeZone"]
        .as_str()
        .or_else(|| calendar["timeZone"].as_str())
        .and_then(|v| v.parse::<chrono_tz::Tz>().ok());
    if let Some(value) = event["start"]["dateTime"].as_str() {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
            return Some(parsed.timestamp());
        }
        let local = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
        return zone?
            .from_local_datetime(&local)
            .earliest()
            .map(|v| v.timestamp());
    }
    let day = date(event["start"]["date"].as_str()?)?;
    if let Some(zone) = zone {
        return midnight(day, zone);
    }
    Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|v| v.timestamp())
}

fn normalize_time(endpoint: &mut Value, calendar_zone: &str) {
    let Some(value) = endpoint["dateTime"].as_str() else {
        return;
    };
    if DateTime::parse_from_rfc3339(value).is_ok() {
        return;
    }
    let Some(zone) = endpoint["timeZone"]
        .as_str()
        .or(Some(calendar_zone))
        .and_then(|v| v.parse::<chrono_tz::Tz>().ok())
    else {
        return;
    };
    let Some(local) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").ok()
    else {
        return;
    };
    if let Some(instant) = zone.from_local_datetime(&local).earliest() {
        endpoint["dateTime"] = json!(instant.with_timezone(&Utc).to_rfc3339());
    }
}

fn reminder_minutes(event: &Value, calendar: &Value) -> Vec<i64> {
    let source = if event["reminders"]["useDefault"] == false {
        &event["reminders"]["overrides"]
    } else {
        &calendar["defaultReminders"]
    };
    source
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|v| v["method"] == "popup")
                .filter_map(|v| v["minutes"].as_i64())
                .filter(|v| (0..=40320).contains(v))
                .collect()
        })
        .unwrap_or_default()
}

fn reminders(state: &mut State, now: i64, woke: bool) -> (Vec<String>, bool) {
    let previous = if state.checked_at == 0 {
        now - 60
    } else {
        state.checked_at
    };
    let mut due = Vec::new();
    for event in &state.events {
        if !visible(event) {
            continue;
        }
        let Some(calendar_id) = event["calendar_id"].as_str() else {
            continue;
        };
        if !state.selected.contains(calendar_id) {
            continue;
        }
        let Some(calendar) = state.calendars.iter().find(|v| v["id"] == calendar_id) else {
            continue;
        };
        let Some(start) = start_time(event, calendar) else {
            continue;
        };
        let Some(id) = event["id"].as_str() else {
            continue;
        };
        for minutes in reminder_minutes(event, calendar) {
            let at = start - minutes * 60;
            let key = format!("{calendar_id}:{id}:{at}");
            if at <= now && at > previous && !state.delivered.contains_key(&key) {
                state.delivered.insert(key, now);
                let title = crate::common::clean(&event["summary"], "", 120);
                due.push(if title.is_empty() {
                    "Event".into()
                } else {
                    title
                });
            }
        }
    }
    state.checked_at = now;
    state
        .delivered
        .retain(|_, at| *at > now - 60 * 60 * 24 * 45);
    let resume = woke || now - previous > 120;
    (due, resume)
}

#[cfg(target_os = "linux")]
fn suspend_offset() -> Option<i64> {
    fn read(clock: libc::clockid_t) -> Option<i64> {
        let mut value = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(clock, &mut value) } == 0 {
            Some(value.tv_sec * 1_000_000_000 + value.tv_nsec)
        } else {
            None
        }
    }
    Some(read(libc::CLOCK_BOOTTIME)? - read(libc::CLOCK_MONOTONIC)?)
}

#[cfg(not(target_os = "linux"))]
fn suspend_offset() -> Option<i64> {
    None
}

async fn notify(title: &str, body: &str) {
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("notify-send")
            .args(["-a", "Seele Calendar", "--", title, body])
            .kill_on_drop(true)
            .status(),
    )
    .await;
}

async fn sync(state: &mut State, day: NaiveDate) -> Result<(), &'static str> {
    let client = client()?;
    let token = zeroize::Zeroizing::new(access_token(&client, state).await?);
    let mut next = state.clone();
    let calendar_url =
        Url::parse("https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=250")
            .map_err(|_| "Invalid calendar endpoint.")?;
    let calendars = pages(&client, &token, calendar_url, MAX_CALENDARS).await?;
    next.calendars = calendars.iter().filter_map(project_calendar).collect();
    if let Some(primary) = next
        .calendars
        .iter()
        .find(|v| v["primary"] == true)
        .and_then(|v| v["id"].as_str())
        .map(str::to_owned)
    {
        if !next.account_id.is_empty() && next.account_id != primary {
            next.selected.clear();
            next.events.clear();
            next.ranges.clear();
            next.delivered.clear();
            next.range_start.clear();
            next.range_end.clear();
        }
        next.account_id = primary;
    }
    next.colors = get(
        &client,
        &token,
        Url::parse("https://www.googleapis.com/calendar/v3/colors")
            .map_err(|_| "Invalid color endpoint.")?,
    )
    .await?;
    let start = day - ChronoDuration::days(45);
    let end = day + ChronoDuration::days(46);
    let query_start = start - ChronoDuration::days(1);
    let query_end = end + ChronoDuration::days(1);
    let mut events = Vec::new();
    for calendar in &next.calendars {
        let Some(id) = calendar["id"].as_str() else {
            continue;
        };
        if !next.selected.contains(id) {
            continue;
        }
        let mut url = Url::parse("https://www.googleapis.com/calendar/v3/calendars/")
            .map_err(|_| "Invalid calendar endpoint.")?;
        url.path_segments_mut()
            .map_err(|_| "Invalid calendar endpoint.")?
            .push(id)
            .push("events");
        url.query_pairs_mut()
            .append_pair("singleEvents", "true")
            .append_pair("showDeleted", "false")
            .append_pair("maxResults", "2500")
            .append_pair("timeMin", &format!("{}T00:00:00Z", query_start))
            .append_pair("timeMax", &format!("{}T00:00:00Z", query_end));
        for mut event in pages(
            &client,
            &token,
            url,
            MAX_EVENTS.saturating_sub(events.len()),
        )
        .await?
        {
            if !visible(&event) {
                continue;
            }
            let zone = calendar["timeZone"].as_str().unwrap_or("UTC");
            normalize_time(&mut event["start"], zone);
            normalize_time(&mut event["end"], zone);
            if let Some(event) = project_event(&event, id) {
                events.push(event);
            }
        }
    }
    next.events.retain(|event| !overlaps(event, start, end));
    next.events.extend(events);
    let window = [start.to_string(), end.to_string()];
    if day == Local::now().date_naive() || next.ranges.is_empty() {
        if next.ranges.is_empty() {
            next.ranges.push(window);
        } else {
            next.ranges[0] = window;
        }
    } else if next.ranges.len() == 1 {
        next.ranges.push(window);
    } else {
        next.ranges[1] = window;
    }
    next.events.retain(|event| {
        next.ranges.iter().any(|range| {
            date(&range[0])
                .zip(date(&range[1]))
                .is_some_and(|(a, b)| overlaps(event, a, b))
        })
    });
    next.range_start = start.to_string();
    next.range_end = end.to_string();
    next.refreshed_at = Utc::now().timestamp();
    save(&next)?;
    *state = next;
    Ok(())
}

async fn sync_bounded(state: &mut State, day: NaiveDate) -> Result<(), &'static str> {
    tokio::time::timeout(Duration::from_secs(25), sync(state, day))
        .await
        .map_err(|_| "Google Calendar refresh timed out.")?
}

async fn refresh_views(state: &mut State, selected_day: NaiveDate) -> Result<(), &'static str> {
    let today = Local::now().date_naive();
    sync_bounded(state, today).await?;
    if (selected_day - today).num_days().abs() > 45 {
        sync_bounded(state, selected_day).await?;
    }
    Ok(())
}

fn snapshot(state: &State, connected: bool, error: &str) -> Value {
    json!({"configured": !state.client_id.is_empty(), "connected":connected, "error":error,
        "calendars":state.calendars,"selected":state.selected,"events":state.events,"colors":state.colors,
        "range_start":state.range_start,"range_end":state.range_end,"ranges":state.ranges,"refreshed_at":state.refreshed_at})
}

async fn signin(client_id: String) -> Result<zeroize::Zeroizing<String>, &'static str> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "Local Google sign-in callback is unavailable.")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Local callback is unavailable.")?
        .port();
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let nonce = Uuid::new_v4().to_string();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|_| "Invalid sign-in endpoint.")?;
    url.query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &nonce);
    // URL contains only a public client ID and a one-time state, never a credential.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("xdg-open")
            .arg(url.as_str())
            .kill_on_drop(true)
            .status(),
    )
    .await;
    let (mut stream, peer) = tokio::time::timeout(Duration::from_secs(180), listener.accept())
        .await
        .map_err(|_| "Google sign-in timed out.")?
        .map_err(|_| "Google callback failed.")?;
    if !peer.ip().is_loopback() {
        return Err("Google callback was not local.");
    }
    let mut bytes = [0u8; 4096];
    let count = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut bytes))
        .await
        .map_err(|_| "Google callback timed out.")?
        .map_err(|_| "Google callback failed.")?;
    let request =
        std::str::from_utf8(&bytes[..count]).map_err(|_| "Google callback was invalid.")?;
    let path = request
        .split_whitespace()
        .nth(1)
        .ok_or("Google callback was invalid.")?;
    let callback = Url::parse(&format!("http://127.0.0.1:{port}{path}"))
        .map_err(|_| "Google callback was invalid.")?;
    if callback.path() != "/callback" {
        return Err("Google callback path was invalid.");
    }
    let params: BTreeMap<_, _> = callback
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if params.get("state") != Some(&nonce) {
        return Err("Google sign-in state did not match.");
    }
    let code = params.get("code").ok_or("Google sign-in was cancelled.")?;
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nGoogle Calendar sign-in received. Return to Seele Shell.").await;
    let response = client()?
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| "Google sign-in exchange failed.")?;
    if !response.status().is_success() {
        return Err("Google rejected the sign-in exchange.");
    }
    let mut value: Value = response
        .json()
        .await
        .map_err(|_| "Google sign-in response was invalid.")?;
    match value["refresh_token"].take() {
        Value::String(refresh) if !refresh.is_empty() && refresh.len() <= 8192 => {
            Ok(zeroize::Zeroizing::new(refresh))
        }
        _ => Err("Google did not grant offline access."),
    }
}

pub async fn run() {
    let mut state = match load() {
        Ok(v) => v,
        Err(error) => {
            println!("{}", json!({"error":error}));
            return;
        }
    };
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let (sender, mut auth) =
        mpsc::channel::<(u64, Result<zeroize::Zeroizing<String>, &'static str>)>(1);
    let mut auth_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut auth_serial = 0u64;
    let mut connected = false;
    let mut error = String::new();
    let mut selected_day = Local::now().date_naive();
    let mut last_sync_attempt = 0i64;
    let mut last_cursor_save = state.checked_at;
    let mut previous_suspend_offset = suspend_offset();
    println!("{}", snapshot(&state, connected, &error));
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break; };
                if line.len() > 8192 { continue; }
                let Ok(command): Result<Value, _> = serde_json::from_str(&line) else { continue; };
                match command["action"].as_str().unwrap_or("") {
                    "setup" => {
                        if let Some(id) = command["client_id"].as_str().filter(|v| v.len() <= 512 && v.ends_with(".apps.googleusercontent.com") && v.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))) {
                            state.client_id = id.to_owned(); error.clear();
                            if let Err(e) = save(&state) { error = e.into(); }
                        } else { error = "Enter a Google Desktop OAuth client ID.".into(); }
                    }
                    "signin" => {
                        if state.client_id.is_empty() { error = "Enter a Google Desktop OAuth client ID first.".into(); }
                        else if auth_task.is_none() { auth_serial+=1; let serial=auth_serial; let tx = sender.clone(); let id = state.client_id.clone(); auth_task=Some(tokio::spawn(async move { let _ = tx.send((serial,signin(id).await)).await; })); error = "Waiting for Google sign-in…".into(); }
                    }
                    "select" => {
                        let allowed: BTreeSet<String> = state.calendars.iter().filter_map(|v| v["id"].as_str().map(str::to_owned)).collect();
                        if !connected { error = "Reconnect before changing calendar selection.".into(); }
                        else if let Some(ids) = command["ids"].as_array().filter(|v| v.len() <= MAX_CALENDARS) {
                            let mut proposed = state.clone();
                            proposed.selected = ids.iter().filter_map(|v| v.as_str()).filter(|v| allowed.contains(*v)).map(str::to_owned).collect();
                            proposed.events.clear(); proposed.ranges.clear(); proposed.range_start.clear(); proposed.range_end.clear();
                            match refresh_views(&mut proposed, selected_day).await {
                                Ok(()) => {state=proposed; connected=true;error.clear();},
                                Err(e) => {let _=save(&state); connected=false;error=e.into();}
                            }
                        }
                    }
                    "day" => {
                        if let Some(day) = command["date"].as_str().and_then(date) { selected_day = day; }
                        if !covers(&state, selected_day) && !state.client_id.is_empty() {
                            match sync_bounded(&mut state, selected_day).await { Ok(()) => { connected=true;error.clear(); }, Err(e) => { connected=false;error=e.into(); } }
                        }
                    }
                    "refresh" => if !state.client_id.is_empty() { match refresh_views(&mut state, selected_day).await { Ok(()) => {connected=true;error.clear();}, Err(e) => {connected=false;error=e.into();} } },
                    "disconnect" => {
                        auth_serial+=1;
                        if let Some(task)=auth_task.take() { task.abort(); }
                        match wallet("clear", None).await {
                            Ok(_) => { state = State::default(); connected=false; error.clear(); if let Err(e) = save(&state) { error=e.into(); } },
                            Err(e) => error=e.into(),
                        }
                    }
                    _ => {}
                }
                if matches!(command["action"].as_str(), Some("select" | "day" | "refresh")) {
                    last_sync_attempt = Utc::now().timestamp();
                }
                println!("{}", snapshot(&state, connected, &error));
            }
            result = auth.recv() => {
                if let Some((serial,result)) = result {
                    if serial != auth_serial { continue; }
                    if auth_task.take().is_none() { continue; }
                    match result {
                        Ok(refresh) => match wallet("store", Some(&refresh)).await {
                            Ok(_) => match refresh_views(&mut state, selected_day).await { Ok(()) => { connected=true;error.clear(); }, Err(e) => { connected=false;error=e.into(); } },
                            Err(e) => error=e.into(),
                        },
                        Err(e) => error=e.into(),
                    }
                    last_sync_attempt = Utc::now().timestamp();
                    println!("{}", snapshot(&state, connected, &error));
                }
            }
            _ = tick.tick() => {
                let now = Utc::now().timestamp();
                let current_offset = suspend_offset();
                let woke = current_offset.zip(previous_suspend_offset).is_some_and(|(a,b)| a - b > 5_000_000_000);
                previous_suspend_offset = current_offset;
                let previous_checked = state.checked_at;
                let previous_delivered = state.delivered.clone();
                let (due, resume) = reminders(&mut state, now, woke);
                if !due.is_empty() {
                    // Commit delivery keys before notifying; a worker restart cannot repeat them.
                    if save(&state).is_ok() {
                        last_cursor_save = now;
                        if resume || due.len() > 1 {
                            let mut body = due.iter().take(5).cloned().collect::<Vec<_>>().join(" · ");
                            if due.len() > 5 { body.push_str(&format!(" · +{} more", due.len()-5)); }
                            notify(&format!("{} calendar reminders", due.len()), &body).await;
                        }
                        else { notify("Calendar reminder", &due[0]).await; }
                    } else {
                        state.checked_at = previous_checked;
                        state.delivered = previous_delivered;
                        error = "Calendar reminders could not be saved.".into();
                        println!("{}", snapshot(&state, connected, &error));
                    }
                } else if now - last_cursor_save >= 300 && save(&state).is_ok() {
                    last_cursor_save = now;
                }
                if !state.client_id.is_empty() && (last_sync_attempt == 0 || now - last_sync_attempt >= 300) {
                    last_sync_attempt = now;
                    match refresh_views(&mut state, selected_day).await { Ok(()) => { connected=true;error.clear(); }, Err(e) => { connected=false;error=e.into(); } }
                    println!("{}", snapshot(&state, connected, &error));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visibility_and_occurrence_reminder_identity() {
        let mut state = State::default();
        state.selected.insert("a".into());
        state
            .calendars
            .push(json!({"id":"a","defaultReminders":[{"method":"popup","minutes":10}]}));
        state.events.push(json!({"id":"series_20260926T100000Z","calendar_id":"a","summary":"Meeting","start":{"dateTime":"2026-09-26T10:00:00Z"},"status":"confirmed"}));
        let at = DateTime::parse_from_rfc3339("2026-09-26T09:50:00Z")
            .unwrap()
            .timestamp();
        state.checked_at = at - 1;
        assert_eq!(reminders(&mut state, at, false).0, vec!["Meeting"]);
        assert!(reminders(&mut state, at + 30, false).0.is_empty());
        state.events[0]["attendees"] = json!([{"self":true,"responseStatus":"declined"}]);
        assert!(!visible(&state.events[0]));
    }

    #[test]
    fn all_day_dst_and_override_reminders() {
        let calendar = json!({"timeZone":"Europe/Berlin","defaultReminders":[{"method":"popup","minutes":60}]});
        let before = json!({"start":{"date":"2026-03-29"}});
        let after = json!({"start":{"date":"2026-03-30"},"reminders":{"useDefault":false,"overrides":[{"method":"popup","minutes":10}]}});
        assert_eq!(
            start_time(&after, &calendar).unwrap() - start_time(&before, &calendar).unwrap(),
            23 * 3600
        );
        assert_eq!(reminder_minutes(&before, &calendar), vec![60]);
        assert_eq!(reminder_minutes(&after, &calendar), vec![10]);
    }

    #[test]
    fn missed_reminder_for_ended_event_is_summarized_once() {
        let mut state = State::default();
        state.selected.insert("primary".into());
        state.calendars.push(json!({"id":"primary","timeZone":"UTC","defaultReminders":[{"method":"popup","minutes":0}]}));
        state.events.push(json!({"id":"occurrence-2","calendar_id":"primary","summary":"Ended while sleeping","start":{"dateTime":"2026-09-26T10:00:00Z"},"end":{"dateTime":"2026-09-26T10:30:00Z"}}));
        let start = DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z")
            .unwrap()
            .timestamp();
        state.checked_at = start - 10;
        let (due, summary) = reminders(&mut state, start + 3600, true);
        assert_eq!(due, vec!["Ended while sleeping"]);
        assert!(summary);
        assert!(reminders(&mut state, start + 3601, false).0.is_empty());
    }

    #[test]
    fn cache_coverage_and_multiday_overlap() {
        let mut state = State::default();
        state
            .ranges
            .push(["2026-09-01".into(), "2026-10-01".into()]);
        assert!(covers(&state, date("2026-09-30").unwrap()));
        assert!(!covers(&state, date("2026-10-01").unwrap()));
        let event = json!({"start":{"date":"2026-09-29"},"end":{"date":"2026-10-03"}});
        assert!(overlaps(
            &event,
            date("2026-10-01").unwrap(),
            date("2026-11-01").unwrap()
        ));
    }

    #[test]
    fn public_snapshot_omits_unneeded_attendee_and_extended_data() {
        let source = json!({"id":"occurrence","summary":"Review","start":{"dateTime":"2026-09-26T10:00:00Z"},"end":{"dateTime":"2026-09-26T11:00:00Z"},
            "attendees":[{"self":true,"email":"private@example.com","responseStatus":"tentative"},{"email":"other@example.com"}],
            "extendedProperties":{"private":{"secret":"hidden"}}});
        let event = project_event(&source, "primary").unwrap();
        let encoded = event.to_string();
        assert!(!encoded.contains("private@example.com"));
        assert!(!encoded.contains("other@example.com"));
        assert!(!encoded.contains("hidden"));
        assert_eq!(event["attendees"][0]["responseStatus"], "tentative");
    }
}
