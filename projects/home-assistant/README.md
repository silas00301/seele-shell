# Home Assistant

The optional house icon opens selected entity states. Lights, switches and input
booleans offer explicit on/off controls; all other domains are read-only. Click a
switch or focus its row with Tab and press Space/Enter. Escape closes the panel.
Refresh checks immediately; an idle shell checks every 30 seconds. Failed refreshes
keep the last values marked stale and disable controls until a successful read.

Create `$XDG_CONFIG_HOME/seele-shell/home-assistant.json` (normally
`~/.config/seele-shell/home-assistant.json`) outside this repository. The file must
be owned by your user, private (`chmod 600`), and a regular file rather than a
symlink. `SEELE_HOME_ASSISTANT_CONFIG` can select another private file. For example,
replace these placeholders locally:

```json
{
  "url": "https://home.example",
  "token": "YOUR_LONG_LIVED_ACCESS_TOKEN",
  "entities": [
    {"entity_id": "light.desk", "name": "Desk light"},
    "switch.monitor",
    "input_boolean.reading",
    "sensor.living_room_temperature"
  ]
}
```

The URL is the instance base URL, without `/api`, credentials, query or fragment.
Use HTTPS when traffic is not confined to a trusted local connection. Obtain the
long-lived access token in your Home Assistant profile. Never put the token in a
Nix expression, command argument or shell history. Removing the file disables the
integration on the next refresh; without a file it makes no network requests and
the bar entry stays hidden. You can also open the panel with
`seele-shellctl control home-assistant` before configuring it.

`control.py` uses the [Home Assistant REST API](https://developers.home-assistant.io/docs/api/rest/).
It reads `/api/states`, exports only the selected entities and display fields, and
posts single-entity `turn_on`/`turn_off` service requests. It never calls toggle,
locks, alarms, scripts, scenes or arbitrary services. An explicit on/off request
is safe to retry after an ambiguous network failure. A successful service request
refreshes state; devices that update asynchronously may retain their prior state
until the next refresh.

The Python helper alone reads credentials. It does not use ambient HTTP proxies,
follow redirects, cache responses, or log errors containing server content. The
file and server response have size limits; socket operations time out after four
seconds and a helper operation after twelve seconds. QML adds a fifteen-second
watchdog. At most 32 entities can be selected. Credentials and other attributes
are excluded from QML output; local display names override server names.

Run the isolated tests with existing Python and Node:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tests/home-assistant.py projects/home-assistant/control.py
node tests/home-assistant-store.js projects/shell/HomeAssistantStore.qml
```

The HTTP tests create only a temporary private fixture and a loopback mock server;
they never load a real connection or control a device. Both suites run in
`test-shell` and the package install checks; the package also lints the store QML.
For live UI verification, open a new shell with a configured test light and sensor,
check keyboard/mouse control, disconnect the server, and confirm stale values are
read-only. Runtime QML and Nix package validation require the normal development
environment.
