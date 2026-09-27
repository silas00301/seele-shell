use super::*;

fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Local
        .with_ymd_and_hms(y, m, d, h, min, 0)
        .earliest()
        .unwrap()
        .timestamp()
}

fn rfc(secs: i64) -> String {
    Utc.timestamp_opt(secs, 0).unwrap().to_rfc3339()
}

fn timed(id: &str, calendar: &str, start: i64, end: i64) -> Value {
    json!({"id": id, "calendar_id": calendar, "summary": id, "start": {"dateTime": rfc(start)}, "end": {"dateTime": rfc(end)}})
}

fn all_day(id: &str, calendar: &str, start: &str, end: &str) -> Value {
    json!({"id": id, "calendar_id": calendar, "summary": id, "start": {"date": start}, "end": {"date": end}})
}

fn state_with(calendars: &[(&str, &str)], events: Vec<Value>) -> State {
    let mut state = fresh();
    for (id, color) in calendars {
        state.calendars.push(json!({"id": id, "name": id, "color": color, "timeZone": "UTC", "defaultReminders": []}));
        state.selected.insert((*id).into());
    }
    state.events = events;
    state
}

fn day(value: &str) -> NaiveDate {
    date(value).unwrap()
}

#[test]
fn visibility_and_occurrence_reminder_identity() {
    let mut state = State::default();
    state.selected.insert("a".into());
    state
        .calendars
        .push(json!({"id":"a","defaultReminders":[{"method":"popup","minutes":10}]}));
    state.events.push(json!({"id":"series_20260926T100000Z","calendar_id":"a","summary":"Meeting","start":{"dateTime":"2026-09-26T10:00:00Z"}}));
    let at = DateTime::parse_from_rfc3339("2026-09-26T09:50:00Z")
        .unwrap()
        .timestamp();
    let due = due_reminders(&state, at - 1, at);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].title, "Meeting");
    state.delivered.insert(due[0].key.clone(), at);
    assert!(due_reminders(&state, at - 1, at).is_empty());
    let declined =
        json!({"status":"confirmed","attendees":[{"self":true,"responseStatus":"declined"}]});
    assert!(!visible(&declined));
    assert!(!visible(&json!({"status":"cancelled"})));
    assert!(visible(&json!({"status":"confirmed"})));
}

#[test]
fn all_day_dst_and_override_reminders() {
    let calendar =
        json!({"timeZone":"Europe/Berlin","defaultReminders":[{"method":"popup","minutes":60}]});
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
fn missed_reminders_are_summarized_once_per_occurrence() {
    let mut state = State::default();
    state.selected.insert("primary".into());
    state.calendars.push(json!({"id":"primary","timeZone":"UTC","defaultReminders":[{"method":"popup","minutes":0},{"method":"popup","minutes":10}]}));
    state.events.push(json!({"id":"occurrence-2","calendar_id":"primary","summary":"Ended while sleeping","start":{"dateTime":"2026-09-26T10:00:00Z"},"end":{"dateTime":"2026-09-26T10:30:00Z"}}));
    let start = DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z")
        .unwrap()
        .timestamp();
    let due = due_reminders(&state, start - 3600, start + 3600);
    assert_eq!(
        due.len(),
        2,
        "both configured reminder times are distinct keys"
    );
    let today = local_date(start).unwrap();
    let (title, body) = reminder_text(&due, start + 3600, today, true);
    assert_eq!(title, "Ended while sleeping");
    assert!(body.starts_with("Missed reminder · Started at"), "{body}");
    let mut second = due[0].clone();
    second.occurrence = "other".into();
    second.title = "Standup".into();
    let (title, _) = reminder_text(&[due[0].clone(), second], start + 3600, today, true);
    assert_eq!(title, "2 missed calendar reminders");
}

#[test]
fn reminder_body_counts_down_to_the_start() {
    let start = local(2026, 9, 27, 9, 0);
    let due = Due {
        key: "k".into(),
        occurrence: "o".into(),
        title: "Standup".into(),
        start,
        end: start + 1800,
        all_day: false,
    };
    let today = local_date(start).unwrap();
    assert_eq!(
        starts(&due, start - 600, today),
        "Starts in 10 min · 09:00–09:30"
    );
    assert_eq!(starts(&due, start, today), "Starts now · 09:00–09:30");
    let party = Due {
        all_day: true,
        ..due
    };
    assert_eq!(starts(&party, start - 600, today), "Today · all day");
}

#[test]
fn window_coverage_and_multiday_overlap() {
    let window = Window {
        start: "2026-09-01".into(),
        end: "2026-10-01".into(),
        ..Window::default()
    };
    assert!(window.covers(day("2026-09-30")));
    assert!(!window.covers(day("2026-10-01")));
    let event = json!({"start":{"date":"2026-09-29"},"end":{"date":"2026-10-03"}});
    assert!(overlaps(&event, day("2026-10-01"), day("2026-11-01")));
    assert!(!overlaps(&event, day("2026-10-03"), day("2026-11-01")));
}

#[test]
fn projection_keeps_only_public_fields() {
    let source = json!({"id":"occurrence","summary":"Review","start":{"dateTime":"2026-09-26T10:00:00Z"},"end":{"dateTime":"2026-09-26T11:00:00Z"},
        "attendees":[{"self":true,"email":"private@example.com","responseStatus":"tentative"},{"email":"other@example.com"}],
        "extendedProperties":{"private":{"secret":"hidden"}},
        "htmlLink":"https://www.google.com/calendar/event?eid=abc",
        "description":"<p>Agenda &amp; notes</p><ul><li>One</li><li>Two</li></ul>",
        "reminders":{"useDefault":false,"overrides":[{"method":"email","minutes":30},{"method":"popup","minutes":5}]}});
    let event = project_event(&source, "primary").unwrap();
    let encoded = event.to_string();
    assert!(!encoded.contains("private@example.com"));
    assert!(!encoded.contains("other@example.com"));
    assert!(!encoded.contains("hidden"));
    assert_eq!(event["rsvp"], "tentative");
    assert_eq!(event["description"], "Agenda & notes\n• One\n• Two");
    assert_eq!(
        event["link"],
        "https://www.google.com/calendar/event?eid=abc"
    );
    assert_eq!(
        event["reminders"]["overrides"],
        json!([{"method":"popup","minutes":5}])
    );
    let foreign = project_event(
        &json!({"id":"x","htmlLink":"https://evil.example/calendar"}),
        "primary",
    )
    .unwrap();
    assert_eq!(foreign["link"], "");
}

#[test]
fn description_text_keeps_prose_and_drops_markup() {
    assert_eq!(plain_text("a < b and c > d", 100), "a < b and c > d");
    assert_eq!(
        plain_text("Line<br>next<br/>third", 100),
        "Line\nnext\nthird"
    );
    assert_eq!(plain_text("<p>One</p><p>Two</p>", 100), "One\nTwo");
    assert_eq!(
        plain_text("<b>Bold</b> &#x41;&#66; &nbsp;x", 100),
        "Bold AB x"
    );
    assert_eq!(plain_text("One\n\n\n\nTwo", 100), "One\n\nTwo");
    assert_eq!(plain_text("\u{202e}evil\u{0007}", 100), "evil");
    assert_eq!(plain_text("abcdef", 3), "abc…");
    assert_eq!(plain_text("&unknown; &", 100), "&unknown; &");
}

#[test]
fn meeting_links_come_from_conference_data_or_known_services() {
    let meet = json!({"hangoutLink":"https://meet.google.com/abc-defg-hij"});
    assert_eq!(
        meeting(&meet),
        Some((
            "https://meet.google.com/abc-defg-hij".into(),
            "Google Meet".into()
        ))
    );
    let zoom = json!({"description":"<a href=\"https://us06web.zoom.us/j/123?pwd=x\">Join</a>"});
    assert_eq!(meeting(&zoom).unwrap().1, "Zoom");
    let teams = json!({"location":"Room 4, https://teams.microsoft.com/l/meetup-join/abc."});
    assert_eq!(
        meeting(&teams).unwrap().0,
        "https://teams.microsoft.com/l/meetup-join/abc"
    );
    assert_eq!(
        meeting(&json!({"description":"https://example.com/notes"})),
        None
    );
    assert_eq!(meeting(&json!({"hangoutLink":"javascript:alert(1)"})), None);
    assert_eq!(
        meeting(&json!({"hangoutLink":"https://evil@meet.google.com/"})),
        None
    );
    let video = json!({"conferenceData":{"conferenceSolution":{"name":"Webex"},"entryPoints":[{"entryPointType":"phone","uri":"tel:+1"},{"entryPointType":"video","uri":"https://acme.webex.com/meet/x"}]}});
    assert_eq!(
        meeting(&video),
        Some(("https://acme.webex.com/meet/x".into(), "Webex".into()))
    );
}

#[test]
// Mid-July has no daylight-saving change in any zone the tests run under.
fn agenda_lists_all_day_first_and_labels_parts_of_long_events() {
    let d = day("2026-07-15");
    let state = state_with(
        &[("one", "#112233")],
        vec![
            timed(
                "late",
                "one",
                local(2026, 7, 15, 14, 0),
                local(2026, 7, 15, 15, 0),
            ),
            timed(
                "early",
                "one",
                local(2026, 7, 15, 9, 0),
                local(2026, 7, 15, 10, 0),
            ),
            timed(
                "overnight",
                "one",
                local(2026, 7, 14, 22, 0),
                local(2026, 7, 15, 2, 0),
            ),
            timed(
                "tonight",
                "one",
                local(2026, 7, 15, 23, 0),
                local(2026, 7, 16, 1, 0),
            ),
            all_day("trip", "one", "2026-07-14", "2026-07-17"),
            all_day("tomorrow", "one", "2026-07-16", "2026-07-17"),
        ],
    );
    let index = index(&state);
    let items = agenda_items(&state, &index, d);
    let titles: Vec<&str> = items.iter().map(|i| i["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["trip", "overnight", "early", "late", "tonight"]);
    assert_eq!(items[0]["time"], "All day · 14–16 Jul");
    assert_eq!(items[0]["when"], "Tue 14–Thu 16 Jul · 3 days");
    assert_eq!(items[1]["time"], "Until 02:00");
    assert_eq!(items[2]["time"], "09:00–10:00");
    assert_eq!(items[2]["when"], "09:00–10:00 · 1 h");
    assert_eq!(items[4]["time"], "From 23:00");
    assert_eq!(items[0]["color"], "#112233");
    assert!(agenda_items(&state, &index, day("2026-07-17")).is_empty());
}

#[test]
fn dots_name_each_calendar_once_and_count_the_rest() {
    let at = local(2026, 9, 27, 9, 0);
    let calendars = [
        ("a", "#aaaaaa"),
        ("b", "#bbbbbb"),
        ("c", "#cccccc"),
        ("d", "#dddddd"),
        ("e", "#eeeeee"),
    ];
    let mut events: Vec<Value> = calendars
        .iter()
        .map(|(id, _)| timed(&format!("{id}-1"), id, at, at + 3600))
        .collect();
    events.push(timed("a-2", "a", at + 3600, at + 7200));
    events.push(all_day("span", "b", "2026-09-26", "2026-09-28"));
    let state = state_with(&calendars, events);
    let dots = dots(&state, &index(&state));
    assert_eq!(
        dots["2026-09-27"]["colors"],
        json!(["#aaaaaa", "#bbbbbb", "#cccccc"])
    );
    assert_eq!(dots["2026-09-27"]["more"], 2);
    assert_eq!(dots["2026-09-26"]["colors"], json!(["#bbbbbb"]));
    assert!(dots.get("2026-09-28").is_none());
}

#[test]
fn event_colour_overrides_the_calendar_colour() {
    let mut state = state_with(&[("one", "#123456")], vec![]);
    state.palette.insert("4".into(), "#fedcba".into());
    let calendar = state.calendars[0].clone();
    assert_eq!(
        event_color(&state, &json!({"color_id": "4"}), &calendar),
        "#fedcba"
    );
    assert_eq!(
        event_color(&state, &json!({"color_id": ""}), &calendar),
        "#123456"
    );
    assert_eq!(hex_color(&json!("red")), "");
    assert_eq!(hex_color(&json!("#ABCDEF")), "#abcdef");
}

#[test]
fn indicator_counts_down_then_follows_the_soonest_ending_event() {
    let nine = local(2026, 9, 27, 9, 0);
    let today = local_date(nine).unwrap();
    let state = state_with(
        &[("one", "#123456")],
        vec![
            timed("a", "one", nine, nine + 3600),
            timed("b", "one", nine + 600, nine + 4200),
            timed("long", "one", nine - 7200, nine + 86_400),
            all_day("party", "one", "2026-09-27", "2026-09-28"),
        ],
    );
    let index = index(&state);
    assert_eq!(indicator(&state, &index, nine - 901, today), Value::Null);
    let soon = indicator(&state, &index, nine - 900, today);
    assert_eq!(soon["key"], "one:a");
    assert_eq!(soon["label"], "in 15m");
    assert_eq!(soon["extra"], 0);
    assert_eq!(
        indicator(&state, &index, nine - 30, today)["label"],
        "in 1m"
    );
    let both = indicator(&state, &index, nine - 60, today);
    assert_eq!(
        (both["key"].as_str(), both["extra"].as_i64()),
        (Some("one:a"), Some(1))
    );
    let ongoing = indicator(&state, &index, nine + 300, today);
    assert_eq!(
        ongoing["key"], "one:b",
        "the next start wins over what is running"
    );
    assert_eq!(ongoing["label"], "in 5m");
    let running = indicator(&state, &index, nine + 700, today);
    assert_eq!(
        (running["key"].as_str(), running["label"].as_str()),
        (Some("one:a"), Some("now"))
    );
    assert_eq!(running["detail"], "09:00–10:00 · 1 more event");
    assert_eq!(indicator(&state, &index, nine + 4200, today), Value::Null);
}

#[test]
fn equal_times_break_ties_by_identity() {
    let nine = local(2026, 9, 27, 9, 0);
    let today = local_date(nine).unwrap();
    let state = state_with(
        &[("one", ""), ("two", "")],
        vec![
            timed("z", "two", nine, nine + 600),
            timed("z", "one", nine, nine + 600),
        ],
    );
    assert_eq!(
        indicator(&state, &index(&state), nine - 60, today)["key"],
        "one:z"
    );
}

fn job(calendars: &[&str], windows: &[(NaiveDate, NaiveDate)], full: bool) -> Job {
    Job {
        serial: 1,
        client_id: "id".into(),
        account_id: String::new(),
        calendars: calendars.iter().map(|v| (*v).into()).collect(),
        windows: windows.to_vec(),
        palette: false,
        base: Url::parse(API).unwrap(),
        full,
    }
}

fn calendars(ids: &[&str]) -> Vec<Value> {
    ids.iter()
        .enumerate()
        .map(|(at, id)| json!({"id": id, "name": id, "primary": at == 0, "color": "#000000"}))
        .collect()
}

#[test]
fn merge_reconciles_changed_and_cancelled_events() {
    let window = window_around(day("2026-09-27"));
    let mut state = state_with(&[("me@example.com", "")], vec![]);
    state.account_id = "me@example.com".into();
    let at = local(2026, 9, 27, 9, 0);
    state.events = vec![
        timed("gone", "me@example.com", at, at + 60),
        timed("moved", "me@example.com", at, at + 60),
    ];
    let fetched = Fetched {
        calendars: calendars(&["me@example.com"]),
        fetched: vec!["me@example.com".into()],
        events: vec![vec![timed("moved", "me@example.com", at + 3600, at + 7200)]],
        ..Fetched::default()
    };
    merge(
        &mut state,
        &job(&["me@example.com"], &[window], true),
        fetched,
        100,
        &[],
    );
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[0]["start"]["dateTime"], rfc(at + 3600));
    assert!(state.fetched.contains("me@example.com"));
    assert_eq!(state.windows.len(), 1);
    assert_eq!(state.refreshed_at, 100);
}

#[test]
fn first_sign_in_adopts_the_primary_calendar_and_another_account_resets() {
    let window = window_around(day("2026-09-27"));
    let at = local(2026, 9, 27, 9, 0);
    let mut state = fresh();
    let fetched = Fetched {
        calendars: calendars(&["me@example.com", "team"]),
        fetched: vec!["me@example.com".into()],
        adopted: Some("me@example.com".into()),
        events: vec![vec![timed("a", "me@example.com", at, at + 60)]],
        ..Fetched::default()
    };
    merge(&mut state, &job(&[], &[window], false), fetched, 100, &[]);
    assert_eq!(state.account_id, "me@example.com");
    assert_eq!(
        state.selected.iter().collect::<Vec<_>>(),
        ["me@example.com"]
    );
    assert_eq!(state.fetched.iter().collect::<Vec<_>>(), ["me@example.com"]);
    assert_eq!(state.events.len(), 1);
    state.delivered.insert("me@example.com:a:1".into(), 1);
    state.selected.insert("team".into());
    let other = Fetched {
        calendars: calendars(&["you@example.com"]),
        fetched: vec!["you@example.com".into()],
        adopted: Some("you@example.com".into()),
        events: vec![vec![]],
        ..Fetched::default()
    };
    merge(
        &mut state,
        &job(&["me@example.com", "team"], &[window], false),
        other,
        200,
        &[],
    );
    assert_eq!(state.account_id, "you@example.com");
    assert_eq!(
        state.selected.iter().collect::<Vec<_>>(),
        ["you@example.com"]
    );
    assert!(state.events.is_empty() && state.delivered.is_empty());
}

#[test]
fn a_calendar_switched_on_mid_sync_stays_pending() {
    let window = window_around(day("2026-09-27"));
    let mut state = state_with(&[("me@example.com", ""), ("team", "")], vec![]);
    state.account_id = "me@example.com".into();
    let fetched = Fetched {
        calendars: calendars(&["me@example.com", "team"]),
        fetched: vec!["me@example.com".into()],
        events: vec![vec![]],
        ..Fetched::default()
    };
    merge(
        &mut state,
        &job(&["me@example.com"], &[window], true),
        fetched,
        100,
        &[],
    );
    assert!(state.selected.contains("team"));
    assert!(!state.fetched.contains("team"));
}

#[test]
fn windows_are_bounded_and_keep_what_is_on_screen() {
    let mut state = fresh();
    let today = day("2026-09-27");
    let (a, b) = window_around(today);
    upsert_window(&mut state, a, b, 1, &[today]);
    let (c, d) = window_around(day("2026-12-27"));
    upsert_window(&mut state, c, d, 2, &[today]);
    let (e, f) = window_around(day("2027-03-27"));
    upsert_window(&mut state, e, f, 3, &[today]);
    let (g, h) = window_around(day("2027-06-27"));
    upsert_window(&mut state, g, h, 4, &[today, day("2027-06-27")]);
    assert_eq!(state.windows.len(), MAX_WINDOWS);
    assert!(state.windows.iter().any(|w| w.covers(today)));
    assert!(!state.windows.iter().any(|w| w.covers(day("2026-12-27"))));
    let (i, j) = window_around(day("2026-09-28"));
    upsert_window(&mut state, i, j, 5, &[today]);
    assert_eq!(
        state.windows.iter().filter(|w| w.covers(today)).count(),
        1,
        "a window shifted by a day replaces its predecessor"
    );
}

#[test]
fn earlier_cache_versions_keep_the_account_and_refetch() {
    let old: State = serde_json::from_value(json!({
        "client_id": "x.apps.googleusercontent.com", "account_id": "me@example.com",
        "selected": ["me@example.com"], "events": [{"id": "old"}],
        "ranges": [["2026-09-01", "2026-10-01"]], "delivered": {"k": 1}
    }))
    .unwrap();
    let state = upgrade(old);
    assert_eq!(state.version, STATE_VERSION);
    assert!(state.signed_in);
    assert!(state.events.is_empty() && state.windows.is_empty());
    assert_eq!(state.delivered.len(), 1);
}

#[test]
fn publisher_sends_only_changed_sections() {
    let mut publisher = Publisher::default();
    let first = publisher
        .changes(vec![("a", json!(1)), ("b", json!(2))])
        .unwrap();
    assert_eq!(first, json!({"a": 1, "b": 2}));
    assert_eq!(
        publisher.changes(vec![("a", json!(1)), ("b", json!(2))]),
        None
    );
    assert_eq!(
        publisher.changes(vec![("a", json!(1)), ("b", json!(3))]),
        Some(json!({"b": 3}))
    );
}

#[test]
fn durations_read_naturally() {
    assert_eq!(duration(1800), "30 min");
    assert_eq!(duration(3600), "1 h");
    assert_eq!(duration(5400), "1 h 30 min");
    assert_eq!(duration(86_400 * 2), "2 days");
}

/// A local stand-in for the Calendar API. It answers from fixed pages, refuses
/// any request without the fixture bearer token, and records each request
/// line and whether it offered gzip, so the fetch path runs without a network.
type Log = Arc<std::sync::Mutex<Vec<(String, bool)>>>;

fn fixture_route(target: &str) -> (u16, Value) {
    let url = Url::parse(&format!("http://fixture{target}")).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    // Events fall on the day a window was requested around, so a fixture
    // holds whatever "today" is when the tests run.
    let centre = query
        .get("timeMin")
        .and_then(|v| date(&v[..10]))
        .map(|d| d + days(1 + WINDOW_BEFORE));
    let on = |offset: i64| {
        centre
            .map(|d| (d + days(offset)).to_string())
            .unwrap_or_default()
    };
    let attendees = json!([
        {"self": true, "email": "me@example.com", "responseStatus": "tentative"},
        {"email": "guest@example.com", "responseStatus": "accepted"}
    ]);
    match url.path() {
        "/calendar/v3/users/me/calendarList" if !query.contains_key("pageToken") => (
            200,
            json!({"nextPageToken": "second", "items": [
                {"id": "me@example.com", "summary": "me@example.com", "primary": true, "backgroundColor": "#4986E7", "accessRole": "owner", "timeZone": "Europe/Berlin",
                 "defaultReminders": [{"method": "popup", "minutes": 10}, {"method": "email", "minutes": 60}]}
            ]}),
        ),
        "/calendar/v3/users/me/calendarList" => (
            200,
            json!({"items": [
                {"id": "team", "summary": "Team", "summaryOverride": "Work", "backgroundColor": "#f83a22", "accessRole": "writer", "timeZone": "UTC"},
                {"id": "broken", "backgroundColor": "red"}
            ]}),
        ),
        "/calendar/v3/colors" => (
            200,
            json!({"event": {"11": {"background": "#DC2127"}, "2": {"background": "green"}}}),
        ),
        "/calendar/v3/calendars/me@example.com/events" => (
            200,
            json!({"items": [
                {"id": format!("kept-{}", on(0)), "status": "confirmed", "summary": "Review", "attendees": attendees,
                 "start": {"dateTime": format!("{}T12:00:00+02:00", on(0))}, "end": {"dateTime": format!("{}T13:00:00+02:00", on(0))},
                 "hangoutLink": "https://meet.google.com/abc-defg-hij"},
                {"id": format!("cancelled-{}", on(0)), "status": "cancelled", "start": {"dateTime": format!("{}T12:00:00Z", on(0))}, "end": {"dateTime": format!("{}T13:00:00Z", on(0))}},
                {"id": format!("declined-{}", on(0)), "status": "confirmed", "attendees": [{"self": true, "responseStatus": "declined"}],
                 "start": {"dateTime": format!("{}T14:00:00Z", on(0))}, "end": {"dateTime": format!("{}T15:00:00Z", on(0))}},
                {"id": format!("local-{}", on(0)), "status": "confirmed", "start": {"dateTime": format!("{}T09:00:00", on(1)), "timeZone": "America/New_York"}, "end": {"dateTime": format!("{}T10:00:00", on(1)), "timeZone": "America/New_York"}}
            ]}),
        ),
        "/calendar/v3/calendars/team/events" => (
            200,
            json!({"items": [{"id": format!("team-{}", &query["timeMin"][..10]), "status": "confirmed", "start": {"date": on(0)}, "end": {"date": on(1)}}]}),
        ),
        _ => (404, json!({})),
    }
}

async fn fake_google() -> (Url, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = Url::parse(&format!(
        "http://{}/calendar/v3/",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let log: Log = Log::default();
    let record = log.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let record = record.clone();
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
                let header = |name: &str| {
                    head.lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(key, _)| key.trim().eq_ignore_ascii_case(name))
                        .map(|(_, value)| value.trim().to_owned())
                        .unwrap_or_default()
                };
                let gzip = header("accept-encoding").contains("gzip");
                record.lock().unwrap().push((target.clone(), gzip));
                let (status, body) = if header("authorization") == "Bearer fixture-token" {
                    fixture_route(&target)
                } else {
                    (401, json!({}))
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
    (base, log)
}

fn fixture_tokens() -> Tokens {
    Arc::new(Mutex::new(Token {
        value: Zeroizing::new("fixture-token".into()),
        until: i64::MAX,
    }))
}

#[tokio::test]
async fn first_sync_pages_projects_and_adopts_the_primary_calendar() {
    let (base, log) = fake_google().await;
    let window = window_around(day("2026-09-27"));
    let job = Job {
        palette: true,
        base,
        ..job(&[], &[window], false)
    };
    let fetched = fetch(client().unwrap(), fixture_tokens(), job)
        .await
        .unwrap();
    let ids: Vec<&str> = fetched.calendars.iter().map(|c| text(&c["id"])).collect();
    assert_eq!(
        ids,
        ["me@example.com", "team", "broken"],
        "both calendar pages are read"
    );
    assert_eq!(
        fetched.calendars[1]["name"], "Work",
        "the user's own name wins"
    );
    assert_eq!(fetched.calendars[0]["color"], "#4986e7");
    assert_eq!(fetched.calendars[2]["color"], "");
    assert_eq!(
        fetched.calendars[0]["defaultReminders"],
        json!([{"method": "popup", "minutes": 10}])
    );
    assert_eq!(fetched.adopted.as_deref(), Some("me@example.com"));
    assert_eq!(fetched.fetched, ["me@example.com"]);
    assert_eq!(
        fetched.palette.unwrap().into_iter().collect::<Vec<_>>(),
        [("11".to_owned(), "#dc2127".to_owned())]
    );
    let events = &fetched.events[0];
    let kept: Vec<&str> = events
        .iter()
        .map(|e| text(&e["id"]).split('-').next().unwrap())
        .collect();
    assert_eq!(
        kept,
        ["kept", "local"],
        "cancelled and declined events are dropped"
    );
    assert_eq!(events[0]["rsvp"], "tentative");
    assert_eq!(events[0]["join_label"], "Google Meet");
    assert!(!events[0].to_string().contains("guest@example.com"));
    assert_eq!(
        events[1]["start"]["dateTime"], "2026-09-28T13:00:00+00:00",
        "a named zone becomes an instant"
    );
    let log = log.lock().unwrap().clone();
    assert!(
        log.iter().all(|(_, gzip)| *gzip),
        "every request offers gzip"
    );
    let events_request = log
        .iter()
        .find(|(target, _)| target.contains("/events"))
        .unwrap();
    let query: HashMap<String, String> = Url::parse(&format!("http://fixture{}", events_request.0))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(query["singleEvents"], "true");
    assert_eq!(query["maxAttendees"], "1");
    assert_eq!(query["fields"], EVENT_FIELDS);
    assert_eq!(
        query["timeMin"],
        format!("{}T00:00:00Z", window.0 - days(1))
    );
}

#[tokio::test]
async fn a_sync_fetches_each_selected_calendar_for_each_window() {
    let (base, log) = fake_google().await;
    let windows = [
        window_around(day("2026-09-27")),
        window_around(day("2027-01-15")),
    ];
    let job = Job {
        account_id: "me@example.com".into(),
        base,
        ..job(&["me@example.com", "team", "unsubscribed"], &windows, true)
    };
    let fetched = fetch(client().unwrap(), fixture_tokens(), job)
        .await
        .unwrap();
    assert_eq!(fetched.adopted, None);
    assert_eq!(
        fetched.fetched,
        ["me@example.com", "team"],
        "a calendar gone from the list is skipped"
    );
    assert_eq!(fetched.palette, None);
    let requests = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(t, _)| t.contains("/events"))
        .count();
    assert_eq!(requests, 4);
    let team: Vec<Vec<&str>> = fetched
        .events
        .iter()
        .map(|events| {
            events
                .iter()
                .filter(|e| calendar_of(e) == "team")
                .map(|e| text(&e["id"]))
                .collect()
        })
        .collect();
    assert_eq!(
        team[0],
        [format!("team-{}", windows[0].0 - days(1)).as_str()]
    );
    assert_eq!(
        team[1],
        [format!("team-{}", windows[1].0 - days(1)).as_str()],
        "results land in their own window"
    );
}

#[tokio::test]
async fn requests_never_leave_the_api_base() {
    let (base, log) = fake_google().await;
    let (http, tokens) = (client().unwrap(), fixture_tokens());
    let google = Google {
        http: &http,
        tokens: &tokens,
        client_id: "id",
        base: &base,
    };
    let foreign = Url::parse("http://127.0.0.1:9/calendar/v3/colors").unwrap();
    assert_eq!(
        google.get(&foreign).await,
        Err(Failure::Other("Invalid Google Calendar endpoint."))
    );
    let outside = base.join("/oauth2/v4/token").unwrap();
    assert_eq!(
        google.get(&outside).await,
        Err(Failure::Other("Invalid Google Calendar endpoint."))
    );
    assert!(log.lock().unwrap().is_empty());
}

fn fixture_worker(base: Url) -> (Worker, mpsc::UnboundedReceiver<Message>, tempfile::TempDir) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let state = State {
        client_id: "fixture.apps.googleusercontent.com".into(),
        signed_in: true,
        ..fresh()
    };
    let mut worker = Worker::new(state, sender, "");
    worker.base = Some(base);
    worker.tokens = fixture_tokens();
    // Nothing in a test may touch the real cache file.
    let directory = tempfile::tempdir().unwrap();
    worker.path = directory.path().join("state.json");
    (worker, receiver, directory)
}

async fn next(messages: &mut mpsc::UnboundedReceiver<Message>) -> Message {
    tokio::time::timeout(Duration::from_secs(10), messages.recv())
        .await
        .unwrap()
        .unwrap()
}

fn calendar<'a>(line: &'a Value, id: &str) -> &'a Value {
    line["calendars"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
}

#[tokio::test]
async fn the_loop_answers_while_a_sync_runs_and_adopts_the_primary_calendar() {
    let (base, _log) = fake_google().await;
    let (mut worker, mut messages, directory) = fixture_worker(base);
    let today = worker.today.to_string();
    worker.tick(false);
    let first = worker.changes().unwrap();
    assert_eq!(first["account"]["status"], "connecting");
    assert_eq!(first["account"]["syncing"], true);
    assert_eq!(first["agenda"]["covered"], false);
    assert_eq!(
        first["agenda"]["loading"], true,
        "an uncached day says it is loading"
    );
    // The loop is not waiting on Google: a far day is answered at once and
    // reads as loading while it waits behind the running fetch.
    let far = (worker.today + days(200)).to_string();
    worker.input(&format!(r#"{{"action":"day","date":"{far}"}}"#));
    let queued = worker.changes().unwrap();
    assert_eq!(queued["agenda"]["day"], far.as_str());
    assert_eq!(queued["agenda"]["loading"], true);
    let message = next(&mut messages).await;
    worker.receive(message);
    assert!(worker.running.is_some(), "the far window follows today's");
    let message = next(&mut messages).await;
    worker.receive(message);
    let synced = worker.changes().unwrap();
    assert_eq!(synced["account"]["status"], "online");
    assert_eq!(synced["account"]["account"], "me@example.com");
    assert_eq!(synced["agenda"]["covered"], true);
    assert_eq!(
        calendar(&synced, "me@example.com")["selected"],
        true,
        "the primary calendar is chosen on first sign-in"
    );
    assert_eq!(calendar(&synced, "team")["selected"], false);
    assert_eq!(synced["dots"][today.as_str()]["colors"], json!(["#4986e7"]));
    worker.input(&format!(r#"{{"action":"day","date":"{today}"}}"#));
    let back = worker.changes().unwrap();
    let titles: Vec<&str> = back["agenda"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| text(&i["title"]))
        .collect();
    assert_eq!(titles, ["Review"]);
    // Switching a calendar off is immediate and needs no network.
    worker.input(r#"{"action":"select","id":"me@example.com","selected":false}"#);
    let off = worker.changes().unwrap();
    assert_eq!(off["agenda"]["items"], json!([]));
    assert_eq!(off["dots"], json!({}));
    assert!(worker.running.is_none());
    // Switching one on shows it loading until its events arrive.
    worker.input(r#"{"action":"select","id":"team","selected":true}"#);
    let on = worker.changes().unwrap();
    assert_eq!(calendar(&on, "team")["loading"], true);
    assert!(
        worker.running.is_some(),
        "a calendar switched on is fetched at once"
    );
    let message = next(&mut messages).await;
    worker.receive(message);
    let loaded = worker.changes().unwrap();
    assert_eq!(calendar(&loaded, "team")["loading"], false);
    assert_eq!(loaded["agenda"]["items"][0]["title"], "(No title)");
    // Nothing changed, so at most the minute heartbeat is sent.
    let quiet = worker.changes();
    assert!(
        quiet.as_ref().is_none_or(|line| line
            .as_object()
            .unwrap()
            .keys()
            .all(|k| k == "heartbeat")),
        "{quiet:?}"
    );
    let saved: State =
        serde_json::from_slice(&std::fs::read(directory.path().join("state.json")).unwrap())
            .unwrap();
    assert_eq!(saved.selected.iter().collect::<Vec<_>>(), ["team"]);
    assert_eq!(saved.account_id, "me@example.com");
    assert_eq!(saved.windows.len(), 2);
}
