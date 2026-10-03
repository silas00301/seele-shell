//! Open-Meteo's side: the forecast and geocoding requests, and the projection
//! that keeps only the validated fields Seele shows. Both APIs need no key and
//! no account; requests carry a place's coordinates and nothing else.
use super::*;

pub(super) const FORECAST: &str = "https://api.open-meteo.com/v1/forecast";
pub(super) const GEOCODING: &str = "https://geocoding-api.open-meteo.com/v1/search";
/// A seven-day forecast with hourly and daily series is about 15 KiB.
pub(super) const MAX_RESPONSE: usize = 1024 * 1024;
pub(super) const FORECAST_DAYS: usize = 7;
pub(super) const MAX_HOURS: usize = 16 * 24;
pub(super) const MAX_DAYS: usize = 16;
pub(super) const MAX_RESULTS: usize = 8;
pub(super) const NAME_LIMIT: usize = 80;
pub(super) const CURRENT_FIELDS: &str = "temperature_2m,apparent_temperature,relative_humidity_2m,is_day,weather_code,wind_speed_10m,wind_direction_10m";
pub(super) const HOURLY_FIELDS: &str =
    "temperature_2m,weather_code,precipitation_probability,is_day";
pub(super) const DAILY_FIELDS: &str = "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset";

/// Where requests go. Tests point both at a local fake.
#[derive(Clone, Debug)]
pub(super) struct Endpoints {
    pub(super) forecast: Url,
    pub(super) geocoding: Url,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            forecast: Url::parse(FORECAST).expect("forecast endpoint"),
            geocoding: Url::parse(GEOCODING).expect("geocoding endpoint"),
        }
    }
}

/// One client for the worker's lifetime, so refreshes reuse pooled TLS connections.
pub(super) fn client() -> Result<reqwest::Client, &'static str> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent("seele-weather (gzip)")
        .build()
        .map_err(|_| "The weather network client is unavailable.")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Failure {
    Offline,
    Refused,
    Invalid,
}

impl Failure {
    pub(super) fn message(self) -> &'static str {
        match self {
            Failure::Offline => "Open-Meteo is unreachable.",
            Failure::Refused => "Open-Meteo refused the request.",
            Failure::Invalid => "Open-Meteo sent an unreadable answer.",
        }
    }
}

/// A request only ever goes to the endpoint it was built from: same scheme,
/// host, port and path, whatever the query says.
fn guarded(endpoint: &Url, url: &Url) -> bool {
    url.origin() == endpoint.origin() && url.path() == endpoint.path()
}

pub(super) async fn get(
    http: &reqwest::Client,
    endpoint: &Url,
    url: Url,
) -> Result<Value, Failure> {
    if !guarded(endpoint, &url) {
        return Err(Failure::Refused);
    }
    let mut response = http.get(url).send().await.map_err(|_| Failure::Offline)?;
    if !response.status().is_success() {
        return Err(Failure::Refused);
    }
    if response
        .content_length()
        .is_some_and(|v| v > MAX_RESPONSE as u64)
    {
        return Err(Failure::Invalid);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Offline)? {
        if chunk.len() > MAX_RESPONSE - bytes.len() {
            return Err(Failure::Invalid);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Failure::Invalid)
}

/// Coordinates sent to Open-Meteo, rounded to two decimals (about a
/// kilometre): a forecast cell is coarser than that anyway.
pub(super) fn coordinate(value: f64) -> String {
    format!("{value:.2}")
}

pub(super) fn forecast_url(endpoint: &Url, latitude: f64, longitude: f64) -> Url {
    let mut url = endpoint.clone();
    url.query_pairs_mut()
        .clear()
        .append_pair("latitude", &coordinate(latitude))
        .append_pair("longitude", &coordinate(longitude))
        .append_pair("current", CURRENT_FIELDS)
        .append_pair("hourly", HOURLY_FIELDS)
        .append_pair("daily", DAILY_FIELDS)
        .append_pair("timezone", "auto")
        .append_pair("timeformat", "unixtime")
        .append_pair("forecast_days", &FORECAST_DAYS.to_string())
        .append_pair("temperature_unit", "celsius")
        .append_pair("wind_speed_unit", "kmh");
    url
}

pub(super) fn search_url(endpoint: &Url, query: &str) -> Url {
    let mut url = endpoint.clone();
    url.query_pairs_mut()
        .clear()
        .append_pair("name", query)
        .append_pair("count", &MAX_RESULTS.to_string())
        .append_pair("language", "en")
        .append_pair("format", "json");
    url
}

pub(super) async fn forecast(
    http: &reqwest::Client,
    endpoints: &Endpoints,
    latitude: f64,
    longitude: f64,
) -> Result<Forecast, Failure> {
    let url = forecast_url(&endpoints.forecast, latitude, longitude);
    let value = get(http, &endpoints.forecast, url).await?;
    project_forecast(&value).ok_or(Failure::Invalid)
}

pub(super) async fn search(
    http: &reqwest::Client,
    endpoints: &Endpoints,
    query: &str,
) -> Result<Vec<Found>, Failure> {
    let url = search_url(&endpoints.geocoding, query);
    let value = get(http, &endpoints.geocoding, url).await?;
    Ok(project_results(&value))
}

/// The current reading. Optional fields are left out of the facts line rather
/// than invented when Open-Meteo has no value for them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Current {
    pub(super) time: i64,
    pub(super) temperature: f64,
    pub(super) code: u8,
    pub(super) day: bool,
    pub(super) apparent: Option<f64>,
    pub(super) humidity: Option<f64>,
    pub(super) wind_speed: Option<f64>,
    pub(super) wind_direction: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Hour {
    pub(super) time: i64,
    pub(super) temperature: f64,
    pub(super) code: u8,
    pub(super) day: bool,
    pub(super) precipitation: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Day {
    /// Local midnight at the place, as an instant.
    pub(super) time: i64,
    pub(super) code: u8,
    pub(super) high: f64,
    pub(super) low: f64,
    pub(super) precipitation: Option<f64>,
    pub(super) sunrise: Option<i64>,
    pub(super) sunset: Option<i64>,
}

/// Everything kept of one forecast, in metric. Times are instants; `timezone`
/// is the place's own zone, which local times are shown in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Forecast {
    pub(super) timezone: String,
    pub(super) utc_offset: i32,
    pub(super) current: Option<Current>,
    pub(super) hours: Vec<Hour>,
    pub(super) days: Vec<Day>,
}

/// A place a search found. Only these fields survive, cleaned of control and
/// direction characters, so nothing from the response can forge interface text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Found {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) detail: String,
    pub(super) latitude: f64,
    pub(super) longitude: f64,
}

fn number(value: &Value, low: f64, high: f64) -> Option<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && (low..=high).contains(v))
}

/// Instants between 2000 and 2200; anything else is not a forecast time.
fn instant(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .filter(|v| (946_684_800..7_258_118_400).contains(v))
}

fn temperature(value: &Value) -> Option<f64> {
    number(value, -100.0, 70.0)
}

fn code(value: &Value) -> Option<u8> {
    value
        .as_u64()
        .filter(|v| *v <= 99)
        .and_then(|v| u8::try_from(v).ok())
}

fn percent(value: &Value) -> Option<f64> {
    number(value, 0.0, 100.0)
}

/// A named zone as the tz database spells one; anything else is dropped and
/// the fixed offset Open-Meteo also sends is used instead.
pub(super) fn zone_name(value: &Value) -> String {
    let text = value.as_str().unwrap_or("");
    let valid = !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b));
    if valid {
        text.to_owned()
    } else {
        String::new()
    }
}

/// Series whose arrays disagree in length are cut to the shortest, and an
/// entry missing its time, temperature or code is skipped rather than guessed.
fn series<'a>(block: &'a Value, fields: &[&str], limit: usize) -> Vec<Vec<&'a Value>> {
    let columns: Vec<&Vec<Value>> = fields
        .iter()
        .filter_map(|field| block[*field].as_array())
        .collect();
    if columns.len() != fields.len() {
        return Vec::new();
    }
    let rows = columns
        .iter()
        .map(|c| c.len())
        .min()
        .unwrap_or(0)
        .min(limit);
    (0..rows)
        .map(|row| columns.iter().map(|column| &column[row]).collect())
        .collect()
}

pub(super) fn project_forecast(value: &Value) -> Option<Forecast> {
    let current = &value["current"];
    let current = (|| {
        Some(Current {
            time: instant(&current["time"])?,
            temperature: temperature(&current["temperature_2m"])?,
            code: code(&current["weather_code"])?,
            day: current["is_day"].as_i64() != Some(0),
            apparent: temperature(&current["apparent_temperature"]),
            humidity: percent(&current["relative_humidity_2m"]),
            wind_speed: number(&current["wind_speed_10m"], 0.0, 500.0),
            wind_direction: number(&current["wind_direction_10m"], 0.0, 360.0),
        })
    })();
    let mut hours: Vec<Hour> = series(
        &value["hourly"],
        &[
            "time",
            "temperature_2m",
            "weather_code",
            "precipitation_probability",
            "is_day",
        ],
        MAX_HOURS,
    )
    .into_iter()
    .filter_map(|row| {
        Some(Hour {
            time: instant(row[0])?,
            temperature: temperature(row[1])?,
            code: code(row[2])?,
            precipitation: percent(row[3]),
            day: row[4].as_i64() != Some(0),
        })
    })
    .collect();
    let mut days: Vec<Day> = series(
        &value["daily"],
        &[
            "time",
            "weather_code",
            "temperature_2m_max",
            "temperature_2m_min",
            "precipitation_probability_max",
            "sunrise",
            "sunset",
        ],
        MAX_DAYS,
    )
    .into_iter()
    .filter_map(|row| {
        let (high, low) = (temperature(row[2])?, temperature(row[3])?);
        Some(Day {
            time: instant(row[0])?,
            code: code(row[1])?,
            high: high.max(low),
            low: low.min(high),
            precipitation: percent(row[4]),
            sunrise: instant(row[5]),
            sunset: instant(row[6]),
        })
    })
    .collect();
    // Out-of-order or repeated times would draw a strip that runs backwards.
    hours.sort_by_key(|hour| hour.time);
    hours.dedup_by_key(|hour| hour.time);
    days.sort_by_key(|day| day.time);
    days.dedup_by_key(|day| day.time);
    if current.is_none() && hours.is_empty() && days.is_empty() {
        return None;
    }
    let utc_offset = value["utc_offset_seconds"]
        .as_i64()
        .filter(|v| v.abs() <= 18 * 3600)
        .unwrap_or(0) as i32;
    Some(Forecast {
        timezone: zone_name(&value["timezone"]),
        utc_offset,
        current,
        hours,
        days,
    })
}

pub(super) fn project_results(value: &Value) -> Vec<Found> {
    let Some(results) = value["results"].as_array() else {
        return Vec::new();
    };
    results
        .iter()
        .filter_map(|result| {
            let name = clean(&result["name"], "", NAME_LIMIT);
            if name.trim().is_empty() {
                return None;
            }
            let region = clean(&result["admin1"], "", NAME_LIMIT);
            let country = clean(&result["country"], "", NAME_LIMIT);
            // "Hamburg, Germany" rather than repeating the name as its region.
            let detail = [region, country]
                .into_iter()
                .filter(|part| !part.trim().is_empty() && *part != name)
                .collect::<Vec<_>>()
                .join(", ");
            Some(Found {
                id: result["id"].as_u64().filter(|id| *id > 0)?,
                name: name.trim().to_owned(),
                detail,
                latitude: number(&result["latitude"], -90.0, 90.0)?,
                longitude: number(&result["longitude"], -180.0, 180.0)?,
            })
        })
        .take(MAX_RESULTS)
        .collect()
}
