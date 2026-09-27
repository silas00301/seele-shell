//! Google's side: sign-in, tokens, the Calendar API and the projection that
//! keeps only the fields Seele shows.
use super::*;

pub(super) const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";
pub(super) const API: &str = "https://www.googleapis.com/calendar/v3/";
pub(super) const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
pub(super) const MAX_RESPONSE: usize = 4 * 1024 * 1024;
pub(super) const MAX_EVENTS: usize = 5000;
pub(super) const MAX_CALENDARS: usize = 64;
pub(super) const MAX_PAGES: usize = 20;
pub(super) const FETCH_CONCURRENCY: usize = 4;
pub(super) const DESCRIPTION_LIMIT: usize = 4000;
pub(super) const WALLET: [&str; 4] = ["application", "seele-google-calendar", "account", "primary"];
// Partial responses keep payloads to the fields the worker projects. With
// `maxAttendees=1` Google returns only the signed-in attendee, never the guest list.
pub(super) const CALENDAR_FIELDS: &str = "nextPageToken,items(id,summary,summaryOverride,primary,backgroundColor,accessRole,timeZone,defaultReminders)";
pub(super) const EVENT_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,location,description,htmlLink,hangoutLink,conferenceData(conferenceSolution(name),entryPoints(entryPointType,uri)),attendees(self,responseStatus),reminders,colorId,transparency,eventType)";

pub(super) async fn wallet(action: &str, secret: Option<&str>) -> Result<String, &'static str> {
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

/// One client for the worker's lifetime, so refreshes reuse pooled TLS connections.
pub(super) fn client() -> Result<reqwest::Client, &'static str> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent("seele-calendar (gzip)")
        .build()
        .map_err(|_| "Calendar network client is unavailable.")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Failure {
    Offline,
    Expired,
    Wallet,
    Other(&'static str),
}

impl Failure {
    pub(super) fn message(self) -> &'static str {
        match self {
            Failure::Offline => "Google Calendar is unreachable.",
            Failure::Expired => "Google sign-in expired. Sign in again.",
            Failure::Wallet => "Unlock your system wallet to reach Google Calendar.",
            Failure::Other(message) => message,
        }
    }
}

pub(super) async fn read_json(mut response: reqwest::Response) -> Result<Value, Failure> {
    if response
        .content_length()
        .is_some_and(|v| v > MAX_RESPONSE as u64)
    {
        return Err(Failure::Other("Google Calendar response is too large."));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Offline)? {
        if chunk.len() > MAX_RESPONSE - bytes.len() {
            return Err(Failure::Other("Google Calendar response is too large."));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| Failure::Other("Google Calendar response is invalid."))
}

/// A short-lived access token, kept in memory so a refresh costs no wallet
/// lookup or token exchange. Expiry is wall-clock time, which suspend advances.
#[derive(Default)]
pub(super) struct Token {
    pub(super) value: Zeroizing<String>,
    pub(super) until: i64,
}

pub(super) type Tokens = Arc<Mutex<Token>>;

pub(super) async fn access_token(
    http: &reqwest::Client,
    tokens: &Tokens,
    client_id: &str,
    renew: bool,
) -> Result<Zeroizing<String>, Failure> {
    // Holding the lock serialises concurrent fetches onto one exchange.
    let mut token = tokens.lock().await;
    let now = Utc::now().timestamp();
    if !renew && !token.value.is_empty() && now < token.until {
        return Ok(token.value.clone());
    }
    let refresh = Zeroizing::new(wallet("lookup", None).await.map_err(|_| Failure::Wallet)?);
    if refresh.is_empty() {
        return Err(Failure::Expired);
    }
    let response = http
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|_| Failure::Offline)?;
    match response.status().as_u16() {
        200..=299 => {}
        // `invalid_grant`: the refresh token was revoked or has expired.
        400 | 401 => return Err(Failure::Expired),
        _ => return Err(Failure::Offline),
    }
    let value = read_json(response).await?;
    let access = value["access_token"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() < 8192)
        .ok_or(Failure::Other("Google returned no access token."))?;
    let lifetime = value["expires_in"]
        .as_i64()
        .unwrap_or(3600)
        .clamp(120, 3600);
    token.value = Zeroizing::new(access.to_owned());
    token.until = now + lifetime - 60;
    Ok(token.value.clone())
}

/// One sync's connection to the Calendar API. Every request is built from
/// `base` and must stay on its origin and path, so nothing a response carries
/// can send a bearer token anywhere else.
pub(super) struct Google<'a> {
    pub(super) http: &'a reqwest::Client,
    pub(super) tokens: &'a Tokens,
    pub(super) client_id: &'a str,
    pub(super) base: &'a Url,
}

impl Google<'_> {
    fn endpoint(&self, path: &str) -> Result<Url, Failure> {
        self.base
            .join(path)
            .map_err(|_| Failure::Other("Invalid Google Calendar endpoint."))
    }

    pub(super) async fn get(&self, url: &Url) -> Result<Value, Failure> {
        if url.origin() != self.base.origin() || !url.path().starts_with(self.base.path()) {
            return Err(Failure::Other("Invalid Google Calendar endpoint."));
        }
        for renew in [false, true] {
            let token = access_token(self.http, self.tokens, self.client_id, renew).await?;
            let response = self
                .http
                .get(url.clone())
                .bearer_auth(token.as_str())
                .send()
                .await
                .map_err(|_| Failure::Offline)?;
            return match response.status().as_u16() {
                200..=299 => read_json(response).await,
                // An access token Google no longer accepts is renewed once.
                401 if !renew => continue,
                401 => Err(Failure::Expired),
                403 => Err(Failure::Other(
                    "Google refused the request. Check that the Calendar API is enabled for this client.",
                )),
                429 => Err(Failure::Other(
                    "Google Calendar asked Seele to slow down. It retries shortly.",
                )),
                500..=599 => Err(Failure::Offline),
                _ => Err(Failure::Other("Google Calendar could not refresh events.")),
            };
        }
        Err(Failure::Expired)
    }

    async fn pages(&self, url: Url, max: usize) -> Result<Vec<Value>, Failure> {
        let mut all = Vec::new();
        let mut page_token = None::<String>;
        for _ in 0..MAX_PAGES {
            let mut page_url = url.clone();
            if let Some(token) = page_token.take() {
                page_url.query_pairs_mut().append_pair("pageToken", &token);
            }
            let page = self.get(&page_url).await?;
            let items = page["items"]
                .as_array()
                .ok_or(Failure::Other("Google Calendar list is invalid."))?;
            if all.len() + items.len() > max {
                return Err(Failure::Other(
                    "Google Calendar returned more than the cache keeps.",
                ));
            }
            all.extend(items.iter().cloned());
            match page["nextPageToken"].as_str() {
                Some(next) => page_token = Some(next.to_owned()),
                None => return Ok(all),
            }
        }
        Err(Failure::Other("Google Calendar returned too many pages."))
    }

    async fn calendars(&self) -> Result<Vec<Value>, Failure> {
        let mut url = self.endpoint("users/me/calendarList")?;
        url.query_pairs_mut()
            .append_pair("maxResults", "250")
            .append_pair("fields", CALENDAR_FIELDS);
        Ok(self
            .pages(url, MAX_CALENDARS)
            .await?
            .iter()
            .filter_map(project_calendar)
            .collect())
    }

    async fn palette(&self) -> Result<BTreeMap<String, String>, Failure> {
        let mut url = self.endpoint("colors")?;
        url.query_pairs_mut().append_pair("fields", "event");
        let value = self.get(&url).await?;
        Ok(value["event"]
            .as_object()
            .map(|items| {
                items
                    .iter()
                    .filter(|(id, _)| id.len() <= 8)
                    .filter_map(|(id, color)| {
                        let hex = hex_color(&color["background"]);
                        (!hex.is_empty()).then(|| (id.clone(), hex))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn events(
        &self,
        calendar_id: &str,
        zone: &str,
        (start, end): (NaiveDate, NaiveDate),
    ) -> Result<Vec<Value>, Failure> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| Failure::Other("Invalid calendar endpoint."))?
            .pop_if_empty()
            .push("calendars")
            .push(calendar_id)
            .push("events");
        url.query_pairs_mut()
            .append_pair("singleEvents", "true")
            .append_pair("showDeleted", "false")
            .append_pair("maxResults", "2500")
            .append_pair("maxAttendees", "1")
            .append_pair("timeMin", &format!("{}T00:00:00Z", start - days(1)))
            .append_pair("timeMax", &format!("{}T00:00:00Z", end + days(1)))
            .append_pair("fields", EVENT_FIELDS);
        Ok(self
            .pages(url, MAX_EVENTS)
            .await?
            .into_iter()
            .filter(visible)
            .filter_map(|mut event| {
                normalize_time(&mut event["start"], zone);
                normalize_time(&mut event["end"], zone);
                project_event(&event, calendar_id)
            })
            .collect())
    }
}

pub(super) fn visible(event: &Value) -> bool {
    event["status"] != "cancelled"
        && event["attendees"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["self"] == true))
            .and_then(|v| v["responseStatus"].as_str())
            != Some("declined")
}

pub(super) fn hex_color(value: &Value) -> String {
    value
        .as_str()
        .filter(|v| {
            v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

pub(super) fn popup_reminders(value: &Value) -> Value {
    Value::Array(
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|v| v["method"] == "popup")
                    .filter_map(|v| v["minutes"].as_i64())
                    .map(|minutes| json!({"method": "popup", "minutes": minutes}))
                    .collect()
            })
            .unwrap_or_default(),
    )
}

pub(super) fn project_calendar(value: &Value) -> Option<Value> {
    let id = value["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 512)?;
    // The name the user gave a calendar in Google wins over its owner's name.
    let mut name = crate::common::clean(&value["summaryOverride"], "", 160);
    if name.is_empty() {
        name = crate::common::clean(&value["summary"], "", 160);
    }
    if name.is_empty() {
        name = crate::common::clean(&json!(id), "", 160);
    }
    Some(json!({
        "id": id,
        "name": name,
        "primary": value["primary"] == true,
        "color": hex_color(&value["backgroundColor"]),
        "role": crate::common::clean(&value["accessRole"], "", 32),
        "timeZone": crate::common::clean(&value["timeZone"], "", 64),
        "defaultReminders": popup_reminders(&value["defaultReminders"]),
    }))
}

pub(super) fn endpoint(value: &Value) -> Value {
    let mut out = Map::new();
    for key in ["dateTime", "date", "timeZone"] {
        if let Some(text) = value[key].as_str().filter(|v| v.len() <= 64) {
            out.insert(key.to_owned(), json!(text));
        }
    }
    Value::Object(out)
}

pub(super) fn project_event(value: &Value, calendar_id: &str) -> Option<Value> {
    let id = value["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 512)?;
    let rsvp = value["attendees"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["self"] == true))
        .and_then(|item| item["responseStatus"].as_str())
        .filter(|v| matches!(*v, "accepted" | "tentative" | "needsAction"))
        .unwrap_or("");
    let (join, join_label) = meeting(value).unwrap_or_default();
    let link = value["htmlLink"]
        .as_str()
        .and_then(safe_link)
        .filter(|url| {
            matches!(
                url.host_str(),
                Some("www.google.com" | "calendar.google.com")
            )
        })
        .map(String::from)
        .unwrap_or_default();
    let overrides = popup_reminders(&value["reminders"]["overrides"]);
    Some(json!({
        "id": id,
        "calendar_id": calendar_id,
        "summary": crate::common::clean(&value["summary"], "", 240),
        "start": endpoint(&value["start"]),
        "end": endpoint(&value["end"]),
        "location": crate::common::clean(&value["location"], "", 512),
        "description": plain_text(value["description"].as_str().unwrap_or(""), DESCRIPTION_LIMIT),
        "link": link,
        "join": join,
        "join_label": join_label,
        "rsvp": rsvp,
        "reminders": {"useDefault": value["reminders"]["useDefault"] != false, "overrides": overrides},
        "color_id": crate::common::clean(&value["colorId"], "", 8),
        "transparency": value["transparency"].as_str().unwrap_or(""),
        "event_type": value["eventType"].as_str().unwrap_or(""),
    }))
}

pub(super) fn normalize_time(endpoint: &mut Value, calendar_zone: &str) {
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

#[derive(Clone, Debug)]
pub(super) struct Job {
    pub(super) serial: u64,
    pub(super) client_id: String,
    pub(super) account_id: String,
    pub(super) calendars: Vec<String>,
    pub(super) windows: Vec<(NaiveDate, NaiveDate)>,
    pub(super) palette: bool,
    /// Where the Calendar API lives; a fixture substitutes its own server.
    pub(super) base: Url,
    /// The job covers every cached window, so it completes the selection.
    pub(super) full: bool,
}

#[derive(Debug, Default)]
pub(super) struct Fetched {
    pub(super) calendars: Vec<Value>,
    pub(super) palette: Option<BTreeMap<String, String>>,
    /// Calendars whose events were fetched for every window of the job.
    pub(super) fetched: Vec<String>,
    /// The primary calendar chosen for a first sign-in or another account.
    pub(super) adopted: Option<String>,
    pub(super) events: Vec<Vec<Value>>,
}

pub(super) async fn fetch(
    http: reqwest::Client,
    tokens: Tokens,
    job: Job,
) -> Result<Fetched, Failure> {
    let google = Google {
        http: &http,
        tokens: &tokens,
        client_id: &job.client_id,
        base: &job.base,
    };
    let calendars = google.calendars().await?;
    let primary = calendars
        .iter()
        .find(|c| c["primary"] == true)
        .and_then(|c| c["id"].as_str())
        .map(str::to_owned);
    // Another Google account, or a first sign-in with nothing chosen yet, starts
    // from its primary calendar instead of an empty agenda.
    let switched = primary
        .as_deref()
        .is_some_and(|p| !job.account_id.is_empty() && job.account_id != p);
    let adopted = if switched || (job.account_id.is_empty() && job.calendars.is_empty()) {
        primary
    } else {
        None
    };
    let zones: HashMap<&str, &str> = calendars
        .iter()
        .filter_map(|c| Some((c["id"].as_str()?, c["timeZone"].as_str().unwrap_or("UTC"))))
        .collect();
    let targets: Vec<String> = match &adopted {
        Some(id) => vec![id.clone()],
        None => job
            .calendars
            .iter()
            .filter(|id| zones.contains_key(id.as_str()))
            .cloned()
            .collect(),
    };
    let palette = if job.palette {
        Some(google.palette().await?)
    } else {
        None
    };
    let pairs: Vec<(usize, &str, &str)> = (0..job.windows.len())
        .flat_map(|index| targets.iter().map(move |id| (index, id.as_str())))
        .map(|(index, id)| (index, id, zones.get(id).copied().unwrap_or("UTC")))
        .collect();
    // The requests are built before they are streamed: a closure inside the
    // stream would make the spawned future's `Send` bound higher-ranked.
    let requests: Vec<_> = pairs
        .into_iter()
        .map(|(index, id, zone)| {
            let request = google.events(id, zone, job.windows[index]);
            async move { request.await.map(|events| (index, events)) }
        })
        .collect();
    let results: Vec<(usize, Vec<Value>)> = stream::iter(requests)
        .buffer_unordered(FETCH_CONCURRENCY)
        .try_collect()
        .await?;
    let mut events = vec![Vec::new(); job.windows.len()];
    let mut total = 0usize;
    for (index, items) in results {
        total += items.len();
        if total > MAX_EVENTS {
            return Err(Failure::Other(
                "Selected calendars hold more events than the cache keeps.",
            ));
        }
        events[index].extend(items);
    }
    Ok(Fetched {
        calendars,
        palette,
        fetched: targets,
        adopted,
        events,
    })
}

pub(super) const SIGNED_IN_PAGE: &str =
    "Google Calendar is connected. You can close this tab and return to Seele.";
pub(super) const CANCELLED_PAGE: &str = "Sign-in was cancelled. You can close this tab.";

pub(super) async fn respond(stream: &mut TcpStream, status: &str, message: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Seele Calendar</title><body style=\"font:16px system-ui,sans-serif;margin:4em auto;max-width:32em;text-align:center\"><p>{message}</p>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

pub(super) async fn request_path(stream: &mut TcpStream) -> Option<String> {
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !bytes.windows(2).any(|w| w == b"\r\n") && bytes.len() < 4096 {
        let count = tokio::time::timeout(Duration::from_secs(30), stream.read(&mut chunk))
            .await
            .ok()?
            .ok()?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let line = std::str::from_utf8(&bytes).ok()?.lines().next()?;
    let mut parts = line.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().map(str::to_owned))?
}

pub(super) async fn signin(client_id: String) -> Result<Zeroizing<String>, &'static str> {
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
    let mut browser = Command::new("xdg-open");
    browser.arg(url.as_str());
    crate::common::launch(
        browser,
        Duration::from_secs(10),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .map_err(|_| "Could not open a browser for Google sign-in.")?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    // A browser may connect speculatively or ask for a favicon first; only the
    // callback carrying this attempt's state completes sign-in.
    let code = loop {
        let (mut stream, peer) = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| "Google sign-in timed out.")?
            .map_err(|_| "Google callback failed.")?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let Some(path) = request_path(&mut stream).await else {
            continue;
        };
        let Ok(callback) = Url::parse(&format!("http://127.0.0.1:{port}{path}")) else {
            respond(
                &mut stream,
                "400 Bad Request",
                "This is not a Google sign-in response.",
            )
            .await;
            continue;
        };
        let params: BTreeMap<_, _> = callback
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if callback.path() != "/callback" || params.get("state") != Some(&nonce) {
            respond(
                &mut stream,
                "404 Not Found",
                "This is not a Google sign-in response.",
            )
            .await;
            continue;
        }
        if let Some(error) = params.get("error") {
            respond(&mut stream, "200 OK", CANCELLED_PAGE).await;
            return Err(if error == "access_denied" {
                "Google sign-in was cancelled."
            } else {
                "Google refused the sign-in."
            });
        }
        let Some(code) = params.get("code").cloned() else {
            respond(&mut stream, "200 OK", CANCELLED_PAGE).await;
            return Err("Google sign-in was cancelled.");
        };
        respond(&mut stream, "200 OK", SIGNED_IN_PAGE).await;
        break Zeroizing::new(code);
    };
    let response = client()?
        .post(TOKEN_ENDPOINT)
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
    let mut value = read_json(response)
        .await
        .map_err(|_| "Google sign-in response was invalid.")?;
    match value["refresh_token"].take() {
        Value::String(refresh) if !refresh.is_empty() && refresh.len() <= 8192 => {
            Ok(Zeroizing::new(refresh))
        }
        _ => Err("Google did not grant offline access."),
    }
}
