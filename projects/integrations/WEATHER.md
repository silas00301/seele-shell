# Local weather in Seele Shell

The clock popup, the one that holds the month and the Google Calendar agenda,
carries one weather line under its header: the current conditions, the
temperature, the place and the day's high and low. Selecting the line, or
pressing Enter or Space on it, unfolds it in place into the next eight hours,
the coming week on one shared temperature scale, and the place the forecast is
for. Escape folds it again. There is no bar item and no notification: weather
is something the popup shows when it is opened, not something the desktop
announces.

Forecasts come from [Open-Meteo](https://open-meteo.com/), whose forecast and
geocoding APIs need no key and no account. The data is licensed under
CC BY 4.0, so the unfolded card names Open-Meteo beside the time of the last
update.

## The place

By default the forecast is for the system timezone's reference city in the tz
database, the city `zone1970.tab` (or `zone.tab`) names for the zone
`/etc/localtime` points into or `TZ` names. That is the same place the theme
switcher reckons sunrise and sunset from, and the lookup is shared through
`seele_runtime::timezone`. Seele never asks for a location, never uses a
location service and never stores coordinates it derived itself; Open-Meteo
receives the city's coordinates rounded to two decimals and nothing else. When
the system timezone changes, as it does on a trip, the place follows it on the
next minute tick and the old place's forecast is dropped.

A zone without a city, such as `Etc/UTC`, leaves the card asking for a place.
The search button on the unfolded card, or the folded line in that state,
opens a place search. Typing sends one Open-Meteo geocoding request a quarter
of a second after the last key, superseding any older query; a single letter is
not sent. A result is chosen by its id, and only from the worker's last search,
so the shell never hands the worker coordinates of its own. The choice is kept
in the worker's private state file and replaces the timezone city until **Use
timezone city** clears it. Nothing about the place enters the flake.

## Worker protocol

`seele-weather` reads one JSON command per line on stdin, each at most 4 KiB:

| Command | Effect |
| --- | --- |
| `{"action":"refresh"}` | Fetches now. With `"token":N`, a `retried` answer follows when that fetch ends. |
| `{"action":"search","query":…}` | Searches for a place; an empty or one-letter query clears the results. |
| `{"action":"choose","id":…}` | Uses a place from the last search. |
| `{"action":"reset"}` | Goes back to the timezone city. |

It writes JSON lines on stdout. Each line holds only the sections whose content
changed since the previous line:

- `place`: the name and a detail line, whether it was chosen, and the timezone
  city's name for the way back.
- `status`: `no-place`, `connecting`, `online` or `offline`, whether a fetch is
  running, whether the forecast is stale, the sentence the card's footer shows
  and the one word the folded line shows for a forecast it cannot vouch for.
- `current`: the glyph, tone, condition, temperature, the day's high and low,
  and a row of facts (feels like, wind, humidity, sunrise, sunset), or null.
- `hours`: eight columns from the current hour, each with its label, glyph,
  condition, temperature and a chance of rain from 20% up.
- `days`: up to seven rows from today, each with its label, full name, glyph,
  condition, low, high, chance of rain and its span on the week's scale as
  `from` and `to` ratios.
- `search`: the query, whether it is running, an error and the results, each
  an id, a name and a region and country.
- `health`: the Integration Health state and summary, the last success in
  milliseconds and a beat that changes every two minutes, or null during the
  first fetch.
- `retried`: answers to Health Retry tokens, sent once.
- `heartbeat`: the minute, so the shell can tell a quiet worker from a hung one.

`WeatherStore.qml` assigns each section as it arrives and forwards `health` and
`retried` to Integration Health; `WeatherCard.qml` only draws. Neither parses a
forecast, converts a unit or decides what a reading means.

## Presentation

Units follow the locale's measurement system. glibc's own `LC_MEASUREMENT` data
decides, read through a private `newlocale` object so the process locale is
never changed; a locale glibc cannot load falls back to its territory under the
POSIX `LC_ALL`, `LC_MEASUREMENT`, `LANG` precedence, and anything unknown is
metric. On `nerv`, `LANG=en_US.UTF-8` with `LC_MEASUREMENT=de_DE.UTF-8` is
metric. Forecasts are fetched and cached in Celsius and km/h and converted only
for display, so the cache never depends on the locale. Temperatures are whole
degrees with a true minus sign and never `−0°`; wind carries an eight-point
compass direction unless the air is calm.

WMO weather codes map to an English condition and a Nerd Font weather glyph,
with night variants for clear and partly cloudy skies and for showers. Only a
clear or mainly clear day lights its glyph in the theme's yellow.

Times are local to the place, in 24-hour form, through the place's named zone
when Open-Meteo reports one, so a daylight-saving change inside the week lands
on the right hour; otherwise the reported fixed offset is used. The hourly
strip starts with **Now**, which shows the live reading so it agrees with the
line above it. The week's rows share one scale from the coldest low to the
warmest high, so the bars compare down the list, and a day without a range
still draws a short mark where it sits.

## Refreshing and failure

The worker is resident beside the shell. It fetches on start unless the saved
forecast is younger than its next refresh, then every thirty minutes plus up to
five minutes of jitter, and on resume from suspend once fifteen seconds have
passed for the network to come back. **Refresh** in the card and a Health
**Retry** fetch at once.

A failed fetch keeps the last good forecast on screen. The folded line then
says `Offline`, the footer says `Offline · forecast from 14:05` in the theme's
yellow, and retries back off quietly from one minute to thirty, with a little
jitter, and nothing else: no notification, no toast and no log noise. When the
saved reading is more than an hour old, "now" is taken from the cached hourly
forecast instead, without the facts the old reading held, and days that have
passed leave the list. A forecast older than two hours with no failure, as after
a long suspend, reads `Outdated`.

Integration Health registers Weather from the parent flake. It is `healthy`
while the forecast is current, `degraded` while an old one is shown, offline or
not, `disconnected` when no forecast has ever arrived, and `setup-required`
without a place. **Settings** opens the clock popup with the card unfolded, on
the search when there is no place.

## Cache

`$XDG_STATE_HOME/seele-weather/state.json` holds the chosen place, if any, and
the last good forecast with the coordinates it was requested for. It is owned by
the user, mode 0600 in a 0700 directory, written atomically through
`seele-runtime`, and bounded to 512 KiB; a symlink, a second link, another
owner, a broader mode or another version is not trusted, and the worker starts
again rather than refusing to run. Search results live only in memory.

Responses are bounded to 1 MiB and every value is validated: series whose
arrays disagree are cut to the shortest, an entry missing its time, temperature
or code is skipped, numbers outside physical ranges are dropped, times are
sorted and deduplicated, and names from the geocoder lose control and
direction-changing characters. Requests go only to the endpoint they were built
from, with redirects off.

## Validation

```sh
cargo test -p seele-integrations weather
cargo clippy -p seele-integrations --all-targets -- -D warnings
bash tests/weather-card.sh projects/shell projects/shared tests/tst_weathercard.qml "$QT_QML_IMPORT_DIR"
```

The Rust tests cover every documented WMO code, units from the locale, rounding
and the minus sign, compass points, local times across a DST change, the
projection of malformed forecasts and hostile search results, the hourly strip
and its live first column, the fallback to the hourly forecast while offline,
the week's shared scale, status and health, backoff and jitter, a private and
bounded cache, startup without a refetch, and input the shell cannot abuse.
They also run the fetch path and the worker loop against a local fake
Open-Meteo: the exact query a forecast sends, the endpoint guard, an outage that
keeps the forecast and answers a Retry, a resume that waits before refreshing,
search supersession, choosing and resetting a place, and search failures. No
test touches the real state file or the network. The QtTest fixture drives the
folded line, unfolding from the pointer and the keyboard, the stale badge, the
height bound, the place search and its states, the way back to the timezone
city, a card without a place and a card without a forecast. A running
Quickshell and the real network still need a `nerv` session.
