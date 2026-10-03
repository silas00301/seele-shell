use super::view::{condition, MIN_SPAN};
use super::*;
use chrono_tz::Europe::Berlin;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn berlin(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Berlin
        .with_ymd_and_hms(y, m, d, h, min, 0)
        .earliest()
        .unwrap()
        .timestamp()
}

fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Utc.with_ymd_and_hms(y, m, d, h, min, 0)
        .single()
        .unwrap()
        .timestamp()
}

/// Open-Meteo's answer for Berlin around `now`, in its own shape: hourly from
/// local midnight today for a week, daily for seven days, unixtime throughout.
fn fixture_json(now: i64) -> Value {
    let today = Utc
        .timestamp_opt(now, 0)
        .unwrap()
        .with_timezone(&Berlin)
        .date_naive();
    let midnight = |offset: i64| {
        let day = today + chrono::Duration::days(offset);
        Berlin
            .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp()
    };
    let start = midnight(0);
    let hours: Vec<i64> = (0..7 * 24).map(|h| start + h * 3600).collect();
    let hour_of = |i: usize| (i % 24) as i64;
    let days: Vec<i64> = (0..7).map(midnight).collect();
    json!({
        "latitude": 52.5, "longitude": 13.379999, "utc_offset_seconds": 7200,
        "timezone": "Europe/Berlin", "timezone_abbreviation": "GMT+2",
        "current": {"time": now - now % 900, "interval": 900, "temperature_2m": 15.1,
            "apparent_temperature": 13.9, "relative_humidity_2m": 67, "is_day": 1,
            "weather_code": 3, "wind_speed_10m": 6.5, "wind_direction_10m": 135},
        "hourly": {
            "time": hours,
            "temperature_2m": (0..hours.len()).map(|i| 10.0 + hour_of(i) as f64 * 0.5).collect::<Vec<_>>(),
            "weather_code": (0..hours.len()).map(|i| if hour_of(i) < 12 { 1 } else { 61 }).collect::<Vec<_>>(),
            "precipitation_probability": (0..hours.len()).map(|i| hour_of(i) * 4).collect::<Vec<_>>(),
            "is_day": (0..hours.len()).map(|i| i64::from((7..19).contains(&hour_of(i)))).collect::<Vec<_>>(),
        },
        "daily": {
            "time": days,
            "weather_code": [3, 3, 61, 3, 2, 80, 61],
            "temperature_2m_max": [18.0, 21.1, 21.9, 22.4, 24.6, 19.0, 13.2],
            "temperature_2m_min": [13.5, 13.6, 14.1, 10.1, 12.6, 11.0, 8.0],
            "precipitation_probability_max": [33, 0, 3, 5, 0, 34, 41],
            "sunrise": days.iter().map(|d| d + 7 * 3600 + 9 * 60).collect::<Vec<_>>(),
            "sunset": days.iter().map(|d| d + 18 * 3600 + 41 * 60).collect::<Vec<_>>(),
        }
    })
}

fn fixture_forecast(now: i64) -> Forecast {
    project_forecast(&fixture_json(now)).unwrap()
}

fn berlin_city() -> timezone::Place {
    timezone::Place {
        zone: "Europe/Berlin".into(),
        label: "Berlin".into(),
        latitude: 52.5,
        longitude: 13.0 + 22.0 / 60.0,
    }
}

fn names(facts: &Value) -> Vec<&str> {
    facts
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect()
}

fn labels(hours: &Value) -> Vec<&str> {
    hours
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["label"].as_str().unwrap())
        .collect()
}

// --- Conditions, units and local time -------------------------------------

#[test]
fn every_code_open_meteo_documents_has_a_condition() {
    let documented = [
        0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82, 85,
        86, 95, 96, 99,
    ];
    for code in documented {
        for day in [true, false] {
            let condition = condition(code, day);
            assert_ne!(condition.label, "Unknown conditions", "code {code}");
            assert!(!condition.glyph.is_empty());
        }
    }
    assert_eq!(condition(0, true).tone, "sun");
    assert_eq!(condition(1, true).tone, "sun");
    assert_eq!(
        condition(0, false).tone,
        "plain",
        "the sun does not shine at night"
    );
    assert_ne!(condition(0, true).glyph, condition(0, false).glyph);
    assert_eq!(condition(2, false).glyph, "\u{f0f31}");
    assert_eq!(condition(65, true).label, "Heavy rain");
    assert_eq!(condition(99, true).label, "Thunderstorm with heavy hail");
    assert_eq!(condition(4, true).label, "Unknown conditions");
}

#[test]
fn units_follow_the_measurement_locale_and_default_to_metric() {
    let locale = |pairs: &[(&str, &str)]| {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        from_locale(move |name| map.get(name).cloned())
    };
    assert_eq!(locale(&[]), Units::Metric);
    assert_eq!(locale(&[("LANG", "C.UTF-8")]), Units::Metric);
    assert_eq!(locale(&[("LANG", "en_US.UTF-8")]), Units::Imperial);
    // nerv: an English desktop that measures in German.
    assert_eq!(
        locale(&[("LANG", "en_US.UTF-8"), ("LC_MEASUREMENT", "de_DE.UTF-8")]),
        Units::Metric
    );
    assert_eq!(
        locale(&[("LC_ALL", "en_US"), ("LC_MEASUREMENT", "de_DE.UTF-8")]),
        Units::Imperial,
        "LC_ALL overrides every category"
    );
    assert_eq!(
        locale(&[("LC_ALL", ""), ("LC_MEASUREMENT", "en_LR.UTF-8")]),
        Units::Imperial,
        "an empty variable does not count"
    );
    assert_eq!(locale(&[("LANG", "en_GB.UTF-8@euro")]), Units::Metric);
    assert_eq!(locale(&[("LANG", "my_MM")]), Units::Imperial);
}

#[test]
fn readings_round_to_whole_units_without_a_minus_zero() {
    assert_eq!(Units::Metric.temperature(14.5), "15°");
    assert_eq!(Units::Metric.temperature(-0.4), "0°");
    assert_eq!(Units::Metric.temperature(-2.6), "\u{2212}3°");
    assert_eq!(Units::Imperial.temperature(0.0), "32°");
    assert_eq!(Units::Imperial.temperature(-40.0), "\u{2212}40°");
    assert_eq!(Units::Metric.speed(9.6), "10 km/h");
    assert_eq!(Units::Imperial.speed(16.093_44), "10 mph");
    assert_eq!(Units::Imperial.speed_value(0.7), 0);
    for (degrees, point) in [
        (0.0, "N"),
        (22.4, "N"),
        (22.6, "NE"),
        (135.0, "SE"),
        (180.0, "S"),
        (270.0, "W"),
        (337.6, "N"),
        (360.0, "N"),
    ] {
        assert_eq!(compass(degrees), point, "{degrees}");
    }
}

#[test]
fn local_times_follow_the_named_zone_across_a_dst_change() {
    // Berlin leaves summer time at 01:00 UTC on 25 October 2026; the offset
    // reported at the fetch is still summer time's.
    let hours: Vec<Hour> = (0..3)
        .map(|h| Hour {
            time: utc(2026, 10, 25, 0, 0) + h * 3600,
            temperature: 8.0,
            code: 3,
            day: false,
            precipitation: None,
        })
        .collect();
    let forecast = Forecast {
        timezone: "Europe/Berlin".into(),
        utc_offset: 7200,
        hours,
        ..Forecast::default()
    };
    let now = utc(2026, 10, 25, 0, 10);
    assert_eq!(
        labels(&view::hours(&forecast, now, Units::Metric)),
        ["Now", "02:00", "03:00"],
        "02:00 happens twice"
    );
    let fixed = Forecast {
        timezone: String::new(),
        ..forecast
    };
    assert_eq!(
        labels(&view::hours(&fixed, now, Units::Metric)),
        ["Now", "03:00", "04:00"],
        "without a zone the reported offset is all there is"
    );
}

// --- Projection of Open-Meteo's answers -----------------------------------

#[test]
fn the_forecast_projection_keeps_valid_entries_in_order() {
    let now = berlin(2026, 10, 2, 14, 20);
    let forecast = fixture_forecast(now);
    assert_eq!(forecast.timezone, "Europe/Berlin");
    assert_eq!(forecast.utc_offset, 7200);
    assert_eq!(forecast.hours.len(), 7 * 24);
    assert_eq!(forecast.days.len(), 7);
    let current = forecast.current.unwrap();
    assert_eq!(
        (current.code, current.day, current.humidity),
        (3, true, Some(67.0))
    );

    let mut broken = fixture_json(now);
    broken["timezone"] = json!("Europe/Berlin\u{202e}");
    broken["hourly"]["temperature_2m"][0] = Value::Null;
    broken["hourly"]["weather_code"][1] = json!(400);
    broken["hourly"]["temperature_2m"][2] = json!(f64::MAX);
    broken["hourly"]["is_day"]
        .as_array_mut()
        .unwrap()
        .truncate(10);
    broken["daily"]["time"][3] = broken["daily"]["time"][0].clone();
    broken["daily"]["temperature_2m_max"][1] = json!(5.0);
    broken["current"]["relative_humidity_2m"] = json!(140);
    let projected = project_forecast(&broken).unwrap();
    assert_eq!(projected.timezone, "", "an invalid zone name is dropped");
    assert_eq!(
        projected.hours.len(),
        7,
        "series are cut to the shortest array and bad entries are skipped"
    );
    assert!(projected.hours.windows(2).all(|w| w[0].time < w[1].time));
    assert_eq!(projected.days.len(), 6, "a repeated day appears once");
    let swapped = projected
        .days
        .iter()
        .find(|d| d.time == forecast.days[1].time)
        .unwrap();
    assert!(
        (swapped.low, swapped.high) == (5.0, 13.6),
        "a maximum under its minimum is swapped"
    );
    assert_eq!(projected.current.unwrap().humidity, None);

    assert_eq!(
        project_forecast(&json!({"error": true, "reason": "x"})),
        None
    );
    assert_eq!(project_forecast(&json!([])), None);
}

#[test]
fn search_results_are_cleaned_and_bounded() {
    let mut results = vec![
        json!({"id": 2911298, "name": "Hamburg", "latitude": 53.55073, "longitude": 9.99302,
               "admin1": "Hamburg", "country": "Germany", "timezone": "Europe/Berlin"}),
        json!({"id": 1, "name": "Evil\u{202e}town\n", "latitude": 1.0, "longitude": 2.0,
               "admin1": "Spoofed\u{2066}Region", "country": "\u{0007}Nowhere"}),
        json!({"name": "No id", "latitude": 1.0, "longitude": 2.0}),
        json!({"id": 3, "name": "Off the map", "latitude": 91.0, "longitude": 2.0}),
        json!({"id": 4, "name": "   ", "latitude": 1.0, "longitude": 2.0}),
    ];
    for id in 10..30 {
        results.push(
            json!({"id": id, "name": format!("Place {id}"), "latitude": 0.0, "longitude": 0.0}),
        );
    }
    let found = project_results(&json!({ "results": results }));
    assert_eq!(found.len(), MAX_RESULTS);
    assert_eq!(found[0].name, "Hamburg");
    assert_eq!(
        found[0].detail, "Germany",
        "a region named like the place is left out"
    );
    assert_eq!(found[1].name, "Eviltown");
    assert_eq!(found[1].detail, "SpoofedRegion, Nowhere");
    assert_eq!(found[2].id, 10, "invalid entries are skipped");
    assert!(project_results(&json!({"generationtime_ms": 0.1})).is_empty());
}

// --- What the popup draws ---------------------------------------------------

#[test]
fn now_starts_the_strip_and_agrees_with_the_headline() {
    let now = berlin(2026, 10, 2, 14, 20);
    let forecast = fixture_forecast(now);
    let current = view::current(&forecast, now, Units::Metric);
    assert_eq!(current["temperature"], "15°");
    assert_eq!(current["condition"], "Overcast");
    assert_eq!(current["high"], "18°");
    assert_eq!(current["low"], "14°");
    assert_eq!(
        names(&current["facts"]),
        [
            "Feels like 14°",
            "Wind 7 km/h SE",
            "Humidity 67%",
            "Sunrise 07:09",
            "Sunset 18:41"
        ]
    );
    let hours = view::hours(&forecast, now, Units::Metric);
    assert_eq!(
        labels(&hours),
        ["Now", "15:00", "16:00", "17:00", "18:00", "19:00", "20:00", "21:00"]
    );
    assert_eq!(
        hours[0]["temperature"], "15°",
        "the live reading, not the hour's forecast"
    );
    assert_eq!(hours[1]["temperature"], "18°");
    assert_eq!(hours[1]["rain"], "60%");
    let night = view::hours(&forecast, berlin(2026, 10, 2, 2, 0), Units::Metric);
    assert_eq!(night[0]["rain"], "", "a chance under 20% is not shown");
    assert_eq!(
        night[0]["temperature"], "11°",
        "an old reading gives way to the hour"
    );
    assert_eq!(
        night[0]["glyph"], "\u{f0594}",
        "a clear night shows the moon"
    );
}

#[test]
fn an_old_reading_gives_way_to_the_cached_hourly_forecast() {
    let fetched = berlin(2026, 10, 2, 9, 0);
    let forecast = fixture_forecast(fetched);
    // Three hours offline: the reading is old, the hourly forecast is not.
    let later = berlin(2026, 10, 2, 12, 30);
    let current = view::current(&forecast, later, Units::Imperial);
    assert_eq!(
        current["temperature"], "61°",
        "16 °C at noon, in Fahrenheit"
    );
    assert_eq!(current["condition"], "Light rain");
    assert_eq!(
        names(&current["facts"]),
        ["Sunrise 07:09", "Sunset 18:41"],
        "facts the old reading held are left out rather than shown as current"
    );
    let weeks_later = berlin(2026, 10, 20, 12, 0);
    assert_eq!(
        view::current(&forecast, weeks_later, Units::Metric),
        Value::Null
    );
    assert_eq!(view::days(&forecast, weeks_later, Units::Metric), json!([]));
}

#[test]
fn the_week_shares_one_temperature_scale() {
    let now = berlin(2026, 10, 2, 14, 20);
    let forecast = fixture_forecast(now);
    let days = view::days(&forecast, now, Units::Metric);
    assert_eq!(
        labels(&days),
        ["Today", "Sat", "Sun", "Mon", "Tue", "Wed", "Thu"]
    );
    assert_eq!(days[0]["name"], "Friday 2 October");
    assert_eq!(days[0]["rain"], "33%");
    assert_eq!(days[1]["rain"], "");
    assert_eq!(days[6]["from"], 0.0, "the coldest night starts the scale");
    assert_eq!(days[4]["to"], 1.0, "the warmest day ends it");
    for day in days.as_array().unwrap() {
        assert!(day["from"].as_f64().unwrap() <= day["to"].as_f64().unwrap());
    }
    // A day later, offline: yesterday leaves the list instead of reading "Today".
    let tomorrow = view::days(&forecast, berlin(2026, 10, 3, 9, 0), Units::Metric);
    assert_eq!(tomorrow[0]["label"], "Today");
    assert_eq!(tomorrow[0]["name"], "Saturday 3 October");
    assert_eq!(tomorrow.as_array().unwrap().len(), 6);
    let flat = Forecast {
        days: vec![Day {
            time: forecast.days[0].time,
            high: 12.0,
            low: 12.0,
            ..Day::default()
        }],
        ..forecast.clone()
    };
    let flat = view::days(&flat, now, Units::Metric);
    assert_eq!(
        (flat[0]["from"].as_f64(), flat[0]["to"].as_f64()),
        (Some(0.0), Some(1.0))
    );
    let mut still = forecast.clone();
    still.days[6].high = still.days[6].low;
    let still = view::days(&still, now, Units::Metric);
    let (from, to) = (
        still[6]["from"].as_f64().unwrap(),
        still[6]["to"].as_f64().unwrap(),
    );
    assert!(
        from == 0.0 && (to - MIN_SPAN).abs() < 1e-9,
        "a day without a range still draws a mark: {from}..{to}"
    );
}

// --- The worker without a network ------------------------------------------

fn offline_worker() -> (Worker, mpsc::UnboundedReceiver<Message>, tempfile::TempDir) {
    // Nothing in a test may touch the real state file. The worker creates its
    // own private directory, as it does under XDG_STATE_HOME, whatever the umask.
    let directory = tempfile::tempdir().unwrap();
    let (sender, receiver) = mpsc::unbounded_channel();
    let mut worker = Worker::new(
        fresh(),
        directory.path().join("seele-weather/state.json"),
        sender,
        Units::Metric,
        "",
    );
    worker.follow_zone = false;
    (worker, receiver, directory)
}

#[test]
fn status_and_health_say_what_the_forecast_on_screen_is() {
    let (mut worker, _messages, _directory) = offline_worker();
    let now = Utc::now().timestamp();
    assert_eq!(worker.status(now)["state"], "no-place");
    assert_eq!(worker.health(now)["state"], "setup-required");
    worker.city = Some(berlin_city());
    assert_eq!(worker.status(now)["state"], "connecting");
    assert_eq!(
        worker.health(now),
        Value::Null,
        "a first fetch is a transition"
    );
    worker.failed = true;
    assert_eq!(worker.status(now)["label"], "Offline");
    assert_eq!(worker.health(now)["state"], "disconnected");
    worker.failed = false;
    let key = worker.target().unwrap().key();
    worker.state.cached = Some(Cached {
        key,
        fetched_at: now - 60,
        forecast: fixture_forecast(now),
    });
    let status = worker.status(now);
    assert_eq!(status["state"], "online");
    assert_eq!(status["stale"], false);
    assert_eq!(status["badge"], "");
    assert!(status["label"].as_str().unwrap().starts_with("Updated "));
    let health = worker.health(now);
    assert_eq!(health["state"], "healthy");
    assert_eq!(health["last_success"], (now - 60) * 1000);
    worker.failed = true;
    let status = worker.status(now);
    assert_eq!(status["stale"], true);
    assert_eq!(status["badge"], "Offline");
    assert!(status["label"]
        .as_str()
        .unwrap()
        .starts_with("Offline · forecast from "));
    assert_eq!(worker.health(now)["state"], "degraded");
    worker.failed = false;
    worker.state.cached.as_mut().unwrap().fetched_at = now - STALE_AFTER - 1;
    assert!(worker.status(now)["label"]
        .as_str()
        .unwrap()
        .starts_with("Forecast from "));
    assert_eq!(worker.status(now)["badge"], "Outdated");
    assert_eq!(worker.health(now)["summary"], "Not refreshed recently");
    // A forecast for another place is never shown for this one.
    worker.state.cached.as_mut().unwrap().key = "0.00,0.00".into();
    assert_eq!(worker.status(now)["state"], "connecting");
}

#[test]
fn retries_back_off_quietly_up_to_the_refresh_interval() {
    let (mut worker, _messages, _directory) = offline_worker();
    let waits: Vec<i64> = (1..=8)
        .map(|failures| {
            worker.failures = failures;
            worker.backoff()
        })
        .collect();
    assert_eq!(waits, [60, 120, 240, 480, 960, 1800, 1800, 1800]);
    for _ in 0..200 {
        assert!((0..JITTER).contains(&jitter(JITTER)));
    }
    assert_eq!(jitter(0), 0);
}

#[test]
fn a_saved_forecast_is_not_fetched_again_at_startup() {
    let directory = tempfile::tempdir().unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel();
    let now = Utc::now().timestamp();
    let state = State {
        cached: Some(Cached {
            key: "52.50,13.37".into(),
            fetched_at: now - 60,
            forecast: fixture_forecast(now),
        }),
        ..fresh()
    };
    let worker = Worker::new(
        state,
        directory.path().join("seele-weather/state.json"),
        sender,
        Units::Metric,
        "",
    );
    assert!(worker.due >= now - 60 + REFRESH_EVERY);
}

#[test]
fn the_shell_cannot_supply_coordinates_or_oversized_input() {
    let (mut worker, _messages, directory) = offline_worker();
    worker.city = Some(berlin_city());
    worker.http = None;
    worker.input(r#"{"action":"choose","id":2911298,"latitude":1,"longitude":2}"#);
    assert_eq!(
        worker.state.chosen, None,
        "only a found place can be chosen"
    );
    worker.input(&format!(
        r#"{{"action":"search","query":"{}"}}"#,
        "x".repeat(MAX_INPUT)
    ));
    assert_eq!(worker.search.query, "", "an oversized line is ignored");
    // JSON escapes, so the source itself carries no direction control.
    worker.input(r#"{"action":"search","query":"\u0007Ha\u202em"}"#);
    assert_eq!(
        worker.search.query, "Ham",
        "control and direction characters are dropped"
    );
    assert_eq!(
        worker.search.error, "Open-Meteo is unreachable.",
        "a search without a network client says so"
    );
    worker.input(r#"{"action":"search","query":"H"}"#);
    assert!(!worker.search.busy, "a single letter is not sent");
    worker.input(r#"{"action":"reset"}"#);
    assert!(
        !directory.path().join("seele-weather").exists(),
        "resetting with nothing chosen writes nothing"
    );
    worker.input(r#"{"action":"refresh","token":4}"#);
    assert!(worker.answered.is_empty(), "the retry waits for its fetch");
    let found = |id: u64, name: &str| Found {
        id,
        name: name.into(),
        detail: "Germany".into(),
        latitude: 53.0,
        longitude: 10.0,
    };
    worker.search.results = vec![found(1, "Hamburg"), found(2, "Hanover")];
    worker.input(r#"{"action":"choose","id":2}"#);
    assert_eq!(
        worker.state.chosen.as_ref().map(|c| c.name.as_str()),
        Some("Hanover"),
        "the place named by its id, not merely the first"
    );
    let (mut lost, _messages, _directory) = offline_worker();
    lost.input(r#"{"action":"refresh","token":5}"#);
    assert_eq!(
        lost.answered,
        [(5, false)],
        "a retry with no place fails at once"
    );
}

#[test]
fn the_cache_is_private_bounded_and_disposable() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("weather/state.json");
    let now = berlin(2026, 10, 2, 14, 20);
    let state = State {
        chosen: Some(Chosen {
            id: 2911298,
            name: "Hamburg".into(),
            detail: "Germany".into(),
            latitude: 53.55,
            longitude: 9.99,
        }),
        cached: Some(Cached {
            key: "53.55,9.99".into(),
            fetched_at: now,
            forecast: fixture_forecast(now),
        }),
        ..fresh()
    };
    save(&path, &state).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert_eq!(load(&path), (state.clone(), ""));

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (loaded, problem) = load(&path);
    assert_eq!(loaded, fresh(), "a file others can read is not trusted");
    assert!(!problem.is_empty());

    let elsewhere = directory.path().join("elsewhere.json");
    std::fs::write(&elsewhere, serde_json::to_vec(&state).unwrap()).unwrap();
    std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    assert_eq!(load(&path).0, fresh(), "a symlink is not followed");

    std::fs::remove_file(&path).unwrap();
    save(
        &path,
        &State {
            version: 99,
            ..state.clone()
        },
    )
    .unwrap();
    assert_eq!(
        load(&path),
        (fresh(), ""),
        "another version starts again quietly"
    );

    std::fs::remove_file(&path).unwrap();
    assert_eq!(load(&path), (fresh(), ""));
    let mut huge = state;
    huge.cached.as_mut().unwrap().forecast.timezone = "x".repeat(MAX_STATE);
    assert_eq!(save(&path, &huge), Err("Weather state is too large."));
}

// --- Against a local fake Open-Meteo ----------------------------------------

/// Every request target the fake answered.
type Log = Arc<std::sync::Mutex<Vec<String>>>;

struct Fake {
    endpoints: Endpoints,
    log: Log,
    down: Arc<AtomicBool>,
}

fn query_of(target: &str) -> HashMap<String, String> {
    Url::parse(&format!("http://fixture{target}"))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

fn fake_route(target: &str) -> (u16, Value) {
    let query = query_of(target);
    let path = target.split('?').next().unwrap_or("");
    match (path, query.get("name").map(String::as_str)) {
        ("/v1/forecast", _) => (200, fixture_json(Utc::now().timestamp())),
        ("/v1/search", Some("Hamb")) => (
            200,
            json!({"generationtime_ms": 0.5, "results": [
                {"id": 2911298, "name": "Hamburg", "latitude": 53.55073, "longitude": 9.99302,
                 "country": "Germany", "admin1": "Free and Hanseatic City of Hamburg", "timezone": "Europe/Berlin"},
                {"id": 2911369, "name": "Hamb", "latitude": 51.57096, "longitude": 6.38873,
                 "country": "Germany", "admin1": "North Rhine-Westphalia"}
            ]}),
        ),
        ("/v1/search", _) => (200, json!({"generationtime_ms": 0.5})),
        _ => (404, json!({})),
    }
}

async fn fake_open_meteo() -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let endpoints = Endpoints {
        forecast: Url::parse(&format!("http://{address}/v1/forecast")).unwrap(),
        geocoding: Url::parse(&format!("http://{address}/v1/search")).unwrap(),
    };
    let log: Log = Log::default();
    let down = Arc::new(AtomicBool::new(false));
    let (record, outage) = (log.clone(), down.clone());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let (record, outage) = (record.clone(), outage.clone());
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut chunk = [0u8; 4096];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(count) => head.extend_from_slice(&chunk[..count]),
                    }
                }
                let head = String::from_utf8_lossy(&head).to_string();
                let target = head.split_whitespace().nth(1).unwrap_or("").to_owned();
                record.lock().unwrap().push(target.clone());
                let (status, body) = if outage.load(Ordering::SeqCst) {
                    (503, json!({"error": true, "reason": "maintenance"}))
                } else {
                    fake_route(&target)
                };
                let body = body.to_string();
                let reply = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    });
    Fake {
        endpoints,
        log,
        down,
    }
}

async fn next(messages: &mut mpsc::UnboundedReceiver<Message>) -> Message {
    tokio::time::timeout(Duration::from_secs(10), messages.recv())
        .await
        .unwrap()
        .unwrap()
}

/// Hands every message to the worker until a search answer has arrived.
async fn settle_search(worker: &mut Worker, messages: &mut mpsc::UnboundedReceiver<Message>) {
    loop {
        let message = next(messages).await;
        let search = matches!(message, Message::Search(..));
        worker.receive(message);
        if search {
            return;
        }
    }
}

fn fake_worker(fake: &Fake) -> (Worker, mpsc::UnboundedReceiver<Message>, tempfile::TempDir) {
    let (mut worker, messages, directory) = offline_worker();
    worker.endpoints = fake.endpoints.clone();
    worker.city = Some(berlin_city());
    (worker, messages, directory)
}

fn section<'a>(line: &'a Option<Value>, name: &str) -> &'a Value {
    &line.as_ref().unwrap()[name]
}

fn saved(directory: &tempfile::TempDir) -> State {
    let path = directory.path().join("seele-weather/state.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
async fn a_forecast_request_carries_rounded_coordinates_and_nothing_else() {
    let fake = fake_open_meteo().await;
    let forecast = forecast(
        &client().unwrap(),
        &fake.endpoints,
        52.5,
        13.0 + 22.0 / 60.0,
    )
    .await
    .unwrap();
    assert_eq!(forecast.days.len(), 7);
    let log = fake.log.lock().unwrap().clone();
    assert_eq!(log.len(), 1);
    let query = query_of(&log[0]);
    let mut keys: Vec<&str> = query.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "current",
            "daily",
            "forecast_days",
            "hourly",
            "latitude",
            "longitude",
            "temperature_unit",
            "timeformat",
            "timezone",
            "wind_speed_unit"
        ],
        "no key, account or identifier is sent"
    );
    assert_eq!(query["latitude"], "52.50");
    assert_eq!(query["longitude"], "13.37");
    assert_eq!(query["timezone"], "auto");
    assert_eq!(query["timeformat"], "unixtime");
    assert_eq!(query["temperature_unit"], "celsius");
    assert_eq!(query["current"], CURRENT_FIELDS);
}

#[tokio::test]
async fn requests_never_leave_their_endpoint() {
    let fake = fake_open_meteo().await;
    let http = client().unwrap();
    let elsewhere = Url::parse("http://127.0.0.1:9/v1/forecast").unwrap();
    assert_eq!(
        get(&http, &fake.endpoints.forecast, elsewhere).await,
        Err(Failure::Refused)
    );
    let sideways = fake.endpoints.forecast.join("/v1/archive").unwrap();
    assert_eq!(
        get(&http, &fake.endpoints.forecast, sideways).await,
        Err(Failure::Refused)
    );
    assert!(fake.log.lock().unwrap().is_empty());
    fake.down.store(true, Ordering::SeqCst);
    assert_eq!(
        forecast(&http, &fake.endpoints, 1.0, 2.0).await,
        Err(Failure::Refused),
        "an error status is a refusal, not a forecast"
    );
}

#[tokio::test]
async fn the_loop_keeps_the_last_forecast_through_an_outage() {
    let fake = fake_open_meteo().await;
    let (mut worker, mut messages, directory) = fake_worker(&fake);
    worker.tick(false);
    let first = worker.changes();
    assert_eq!(section(&first, "place")["name"], "Berlin");
    assert_eq!(section(&first, "status")["state"], "connecting");
    assert_eq!(section(&first, "status")["fetching"], true);
    assert_eq!(section(&first, "current"), &Value::Null);
    let message = next(&mut messages).await;
    worker.receive(message);
    let online = worker.changes();
    assert_eq!(section(&online, "status")["state"], "online");
    assert_eq!(section(&online, "current")["temperature"], "15°");
    assert_eq!(section(&online, "days").as_array().unwrap().len(), 7);
    assert_eq!(section(&online, "health")["state"], "healthy");
    let now = Utc::now().timestamp();
    assert!((now + REFRESH_EVERY - 5..now + REFRESH_EVERY + JITTER + 5).contains(&worker.due));
    assert_eq!(worker.error, "", "the forecast was saved");
    assert_eq!(saved(&directory).cached.unwrap().key, "52.50,13.37");

    // Nothing changed, so at most the heartbeats are sent.
    let quiet = worker.changes();
    assert!(
        quiet.as_ref().is_none_or(|line| line
            .as_object()
            .unwrap()
            .keys()
            .all(|k| k == "heartbeat" || k == "health")),
        "{quiet:?}"
    );

    // An outage: a Health Retry fails, and the forecast stays, marked stale.
    fake.down.store(true, Ordering::SeqCst);
    worker.input(r#"{"action":"refresh","token":7}"#);
    let message = next(&mut messages).await;
    worker.receive(message);
    let offline = worker.changes();
    assert_eq!(section(&offline, "status")["state"], "offline");
    assert_eq!(section(&offline, "status")["stale"], true);
    assert!(
        offline.as_ref().unwrap().get("current").is_none(),
        "the forecast on screen is unchanged"
    );
    assert_eq!(worker.cached().unwrap().forecast.days.len(), 7);
    assert_eq!(section(&offline, "health")["state"], "degraded");
    assert_eq!(
        section(&offline, "retried"),
        &json!([{"token": 7, "ok": false}])
    );
    assert!(worker.due >= Utc::now().timestamp() + RETRY_FIRST - 1);
    assert!(
        worker.fetch.is_none(),
        "the next attempt waits out the backoff"
    );

    // Back online, the next Retry succeeds.
    fake.down.store(false, Ordering::SeqCst);
    worker.input(r#"{"action":"refresh","token":8}"#);
    let message = next(&mut messages).await;
    worker.receive(message);
    let back = worker.changes();
    assert_eq!(section(&back, "status")["state"], "online");
    assert_eq!(section(&back, "status")["stale"], false);
    assert_eq!(
        section(&back, "retried"),
        &json!([{"token": 8, "ok": true}])
    );
    assert_eq!(fake.log.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn a_resume_waits_a_moment_and_then_refreshes() {
    let fake = fake_open_meteo().await;
    let (mut worker, mut messages, _directory) = fake_worker(&fake);
    worker.tick(false);
    let message = next(&mut messages).await;
    worker.receive(message);
    // Back from a night's sleep: the forecast is hours old.
    worker.state.cached.as_mut().unwrap().fetched_at -= 8 * 3600;
    worker.due -= 8 * 3600;
    worker.tick(true);
    assert!(worker.fetch.is_none(), "the network gets a moment first");
    assert!(worker.due > Utc::now().timestamp());
    worker.due = Utc::now().timestamp();
    worker.tick(false);
    assert!(worker.fetch.is_some());
}

#[tokio::test]
async fn a_chosen_place_replaces_the_timezone_city_until_reset() {
    let fake = fake_open_meteo().await;
    let (mut worker, mut messages, directory) = fake_worker(&fake);
    worker.tick(false);
    let message = next(&mut messages).await;
    worker.receive(message);
    worker.changes();

    worker.input(r#"{"action":"search","query":"Nowhere"}"#);
    settle_search(&mut worker, &mut messages).await;
    let nothing = worker.changes();
    assert_eq!(section(&nothing, "search")["results"], json!([]));
    assert_eq!(section(&nothing, "search")["error"], "");

    worker.input(r#"{"action":"search","query":"Ham"}"#);
    worker.input(r#"{"action":"search","query":"Hamb"}"#);
    assert_eq!(
        section(&worker.changes(), "search")["busy"],
        true,
        "a search says it is running"
    );
    settle_search(&mut worker, &mut messages).await;
    let found = worker.changes();
    assert_eq!(section(&found, "search")["query"], "Hamb");
    assert_eq!(
        section(&found, "search")["results"],
        json!([
            {"id": 2911298, "name": "Hamburg", "detail": "Free and Hanseatic City of Hamburg, Germany"},
            {"id": 2911369, "name": "Hamb", "detail": "North Rhine-Westphalia, Germany"}
        ])
    );
    let searches = fake
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|t| t.starts_with("/v1/search"))
        .count();
    assert_eq!(searches, 2, "the superseded query was never sent");

    worker.input(r#"{"action":"choose","id":2911298}"#);
    let chosen = worker.changes();
    assert_eq!(section(&chosen, "place")["name"], "Hamburg");
    assert_eq!(section(&chosen, "place")["chosen"], true);
    assert_eq!(section(&chosen, "place")["city"], "Berlin");
    assert_eq!(
        section(&chosen, "current"),
        &Value::Null,
        "Berlin's forecast is not Hamburg's"
    );
    assert_eq!(section(&chosen, "search")["results"], json!([]));
    let message = next(&mut messages).await;
    worker.receive(message);
    assert_eq!(section(&worker.changes(), "status")["state"], "online");
    let last = fake.log.lock().unwrap().last().unwrap().clone();
    let query = query_of(&last);
    assert_eq!(
        (query["latitude"].as_str(), query["longitude"].as_str()),
        ("53.55", "9.99")
    );
    let state = saved(&directory);
    assert_eq!(state.chosen.unwrap().name, "Hamburg");
    assert_eq!(state.cached.unwrap().key, "53.55,9.99");

    worker.input(r#"{"action":"reset"}"#);
    let reset = worker.changes();
    assert_eq!(section(&reset, "place")["name"], "Berlin");
    assert_eq!(section(&reset, "place")["chosen"], false);
    let message = next(&mut messages).await;
    worker.receive(message);
    let state = saved(&directory);
    assert_eq!(state.chosen, None);
    assert_eq!(state.cached.unwrap().key, "52.50,13.37");
}

#[tokio::test]
async fn search_failures_are_named_and_keep_the_place() {
    let fake = fake_open_meteo().await;
    let (mut worker, mut messages, _directory) = fake_worker(&fake);
    fake.down.store(true, Ordering::SeqCst);
    worker.input(r#"{"action":"search","query":"Hamb"}"#);
    settle_search(&mut worker, &mut messages).await;
    assert_eq!(worker.search.error, "Open-Meteo refused the request.");
    // A port nothing listens on.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    worker.endpoints.geocoding = Url::parse(&format!("http://{address}/v1/search")).unwrap();
    worker.input(r#"{"action":"search","query":"Hambu"}"#);
    settle_search(&mut worker, &mut messages).await;
    assert_eq!(worker.search.error, "Place search needs a connection.");
    assert!(!worker.search.busy);
    assert_eq!(worker.target().unwrap().name, "Berlin");
}
