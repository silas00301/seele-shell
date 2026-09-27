# Google Calendar in Seele Shell

One Google account, read-only, inside the existing calendar popup: an agenda
under the month, one dot per calendar under each day that has events, an event
countdown beside the clock, and reminders through the shell's notification
server. Creating, editing and answering invitations happen in Google Calendar.

## Setup

Create a Google Cloud OAuth consent screen, enable the Google Calendar API, and
create an OAuth client of type **Desktop app**. While the consent screen is in
Testing, add the Google account as a test user. Seele requests only
`https://www.googleapis.com/auth/calendar.readonly`; Google may require
verification before broader publication.

Open the calendar popup, select the gear, paste the Desktop client ID and
select **Sign in with Google**. The worker opens the browser, waits up to five
minutes on a temporary `127.0.0.1` listener for the callback carrying its
random state, exchanges the code with PKCE, and stores the refresh token in the
system Secret Service wallet. Other connections to that listener, such as a
browser's speculative connection or favicon request, are answered and ignored.
The first sign-in selects the account's primary calendar. The client ID and the
choice of calendars are private user state, not secrets.

**Disconnect** removes the wallet entry and every account-specific cache entry
but keeps the client ID, so signing in again is one step; **Use another client
ID** is offered while signed out. Signing in with a different account clears the
old account's choices, cache and delivered-reminder keys. A locked wallet leaves
the calendar offline with a message that says so.

## Worker protocol

`seele-calendar` reads one JSON command per line on stdin:

| Command | Effect |
| --- | --- |
| `{"action":"setup","client_id":…}` | Saves a client ID; an empty one forgets it while signed out. |
| `signin`, `cancel` | Starts or abandons the browser sign-in. |
| `{"action":"select","id":…,"selected":bool}` | Switches one calendar on or off, idempotently. |
| `{"action":"day","date":"YYYY-MM-DD"}` | The day the agenda shows. |
| `{"action":"browse","date":…}` | A month scrolled to, so its days get dots. |
| `refresh`, `disconnect` | Refetches today's and the agenda's windows now; signs out. |

It writes JSON lines on stdout. Each line holds only the sections whose content
changed since the previous line: `account` (status, account, syncing, error,
last sync), `calendars` (name, colour, role, selected, still loading),
`dots` (day to up to three calendar colours and a count of the rest),
`agenda` (the requested day, whether it is cached or loading, and its rows,
already sorted and labelled), `indicator` (the bar's event, or null) and a
minute `heartbeat`. No event list crosses into QML and no binding filters
events: `CalendarStore.qml` assigns a property only when its section arrives,
and the popup and bar only draw.

Status is one of `setup`, `signed-out`, `signing-in`, `connecting`, `online`,
`offline` or `expired`. The store maps it to Integration Health, republishing an
unchanged state every two minutes, well inside the registered ten-minute
deadline, and answers a Health **Retry** only when the refresh it started ends.

## Synchronisation

Network work runs in one background task at a time, beside the command loop, so
selecting a day or a calendar and delivering reminders never wait for Google.
Requests made while a sync runs are coalesced into the next one. A sync reads
the calendar list, Google's event colour palette at most once a day, and the
events of each selected calendar for each requested window, up to four requests
at a time. Every request uses a partial response (`fields=…`) and
`maxAttendees=1`, so Google returns only the signed-in attendee and never the
guest list, asks for gzip, and reuses one pooled connection. The access token is
kept in memory until shortly before it expires, by wall-clock time so suspend
counts; a request Google rejects with 401 renews it once. Requests are built
from the API base and refused if they would leave its origin and path.

A window is 91 days, centred on a day. The cache keeps at most three: today's,
the agenda's and one the month list browsed to, evicting the least recently
viewed and never the ones on screen. Today's window is refreshed every five
minutes and on resume from suspend. A cached window the popup looks at is
refetched in the background once it is fifteen minutes old, while its events
stay on screen. A day outside every window is fetched on demand and says it is
loading, or, when offline, that its events have not been downloaded, rather
than presenting an empty day. After a failed sync, automatic retries back off
from 30 seconds to five minutes; an expired sign-in stops them until the user
signs in or refreshes.

Switching a calendar off removes its events at once, offline or not. Switching
one on fetches it for every cached window immediately and shows it loading
until its events arrive. Each fetched window replaces the cached copy of that
window for the calendars fetched, so changed, moved and cancelled events and
their reminders reconcile on the next sync.

## Cache

`$XDG_STATE_HOME/seele-calendar/state.json` is owned by the user, mode 0600,
written atomically and bounded to 8 MiB: at most 64 calendars and 5,000
expanded event instances across its windows, with descriptions reduced to
plain text of at most 4,000 characters. When it is full the least recently
viewed window is dropped. Delivered reminder keys are kept for 45 days. A cache
that is unreadable, not private or from an incompatible version is started
again rather than stopping the worker; an older version keeps the account,
calendar choice and delivered keys.

## Events, days and the bar

Google expands recurring events and their exceptions through `singleEvents`.
Cancelled instances and invitations the user declined are dropped when fetched;
events without an invitation stay. Timed events use their RFC 3339 offsets or
their named time zone. All-day events use their dates, and the calendar's time
zone for reminder times, including days whose midnight a DST change skips.

A day's agenda lists all-day events first, then timed events by start, end and
title. An event that crosses midnight reads `From 22:00`, `Until 02:00` or `All
day` on the days it covers, and its unfolded row gives the full dated range and
duration. Past rows are dimmed, running ones say `Now`, and a meeting link
becomes a **Join** button on the row itself that lights up in the fifteen
minutes before it starts. Unfolding a row shows the time, location, calendar,
the user's response, the description and **Open in Google Calendar**.

Day dots name each selected calendar with events that day once, in its Google
colour, three at most followed by `+N`; multi-day events mark every day they
occupy. Event accents honour an event's own colour; the rest of the popup stays
on Seele's theme tokens.

The bar shows the next timed event starting within fifteen minutes, otherwise
the running event ending soonest, with `+N` for the other eligible events and
the calendar and event identity as a tie-break. All-day events, and timed
events lasting a day or more, stay out of it. Its colour is a dot, so a pale
calendar colour never makes the title unreadable; only the title elides.
Clicking it opens the popup on that event's day with the event unfolded.

## Reminders

Popup reminders follow Google's calendar defaults and each event's overrides.
They are checked on each wall-clock minute boundary, and at least every 15
seconds so a resume is noticed promptly. A reminder's key is its calendar, its
expanded occurrence and its scheduled time, and keys are saved before the
notification is sent, so refreshes, reconnects and restarts never repeat one.
One notification is sent per check: a single reminder names its event and says
when it starts; several, or any missed during sleep, including those for events
that have already ended, arrive as one summary. Delivery uses the shell's
notification server through `notify-send`, which applies Do Not Disturb. While
the focus timer runs, Calendar reminders enter the notification panel without a
toast; this temporary rule does not change saved notification preferences.

## Links

Join links come from the event's video conference entry, its Google Meet link,
or the first link to a known meeting service (Google Meet, Zoom, Microsoft
Teams, Webex, Whereby, Jitsi Meet, GoTo Meeting, Amazon Chime) in its location
or description. Links must be HTTPS without user information; **Open in Google
Calendar** accepts only Google's own event hosts.

## Validation

```sh
cargo test -p seele-integrations calendar
cargo clippy -p seele-integrations --all-targets -- -D warnings
bash tests/calendar-panel.sh projects/shell projects/shared tests/tst_calendarpanel.qml "$QT_QML_IMPORT_DIR"
```

The Rust tests cover projection, description text, meeting links, agenda
labels across midnight, dots, the indicator, reminder identity and wording,
DST, window eviction and cache upgrade. They also run the fetch path and the
worker loop against a local fake Calendar API with a fixture token: pagination,
partial responses and gzip, filtering, primary-calendar adoption, one request
per calendar per window, the origin guard, a loop that answers while a sync
runs, and immediate calendar switching. No test touches the real cache, wallet
or network. The QtTest fixture drives every agenda state, keyboard unfolding,
Join visibility, reveal from the bar, the setup and sign-in steps, whole-row
calendar toggles and the confirmed disconnect. The interactive OAuth round trip,
Secret Service unlock, notification DND behaviour and the popup inside a running
Quickshell still need a `nerv` session.
