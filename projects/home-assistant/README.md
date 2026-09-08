# Home Assistant

The house icon stays in the menu bar, including before setup. Open it to connect
with a server URL and a long-lived access token from your Home Assistant profile.
The connection is checked before saving. The system keyring may ask to unlock.
Tokens go through libsecret's `secret-tool`, using the standard
[Secret Service API](https://specifications.freedesktop.org/secret-service/).
KWallet and other compatible providers work without provider-specific code.
The token travels on the helper's stdin, never in command arguments or logs, and
the password field clears after submission and when the panel closes.

Choose **Devices** to search available entities and select up to 32. **Edit** opens
a selected entity's display name, room override, favorite, menu bar reading, and
ordering controls. Blank names and rooms use Home Assistant's values. Rooms come
from the entity, device and area registries when the account can read them;
manual room overrides also work with restricted accounts. Favorites appear first,
then the other selected entities grouped by room. Selected temperature and humidity
readings appear in the room summary. Choose **Menu bar** again to clear its reading.

Lights, switches and input booleans have explicit on/off controls. Expand supported
lights for brightness and warm/cool sliders. Slider changes send on release, with
arrow-key control available. Tab reaches controls, Space/Enter activates them,
and Escape backs out or closes the panel. Scenes and arbitrary services are not
exposed. Other domains remain read-only.

The resident worker uses Home Assistant's
[WebSocket API](https://developers.home-assistant.io/docs/api/websocket/) for live
state events and explicit single-entity service calls. Each device has its own
pending state. A successful service response alone does not clear that state;
the worker waits for the reported device state, with a bounded timeout. Other
devices remain usable. Disconnections preserve stale readings and disable device
controls, then reconnect with backoff. The optional menu bar reading turns amber
while stale. Closing stdin stops the worker and its connection.

## Private metadata

`$XDG_CONFIG_HOME/seele-shell/home-assistant.json`, normally
`~/.config/seele-shell/home-assistant.json`, stores only the URL and display
preferences. Writes use a private temporary file and atomic replacement. The
file must be owned by the user with mode 0600 and cannot be a symlink.
`SEELE_HOME_ASSISTANT_CONFIG` overrides its path for isolated tests.

```json
{
  "url": "https://home.example",
  "entities": [
    {"entity_id": "light.desk", "name": "Desk", "room": "Office", "favorite": true},
    {"entity_id": "sensor.temperature", "name": "", "room": "Office", "favorite": false}
  ],
  "summary": "sensor.temperature"
}
```

Existing private files with a `token` migrate on worker startup. The worker saves
it to the keyring before atomically replacing the file without that field. A
failed migration leaves the original file intact and offers a keyring retry.
Setup also lets the user replace the connection. Missing configuration makes no
Home Assistant requests. URLs and credentials never enter the Nix store.

The HTTP and WebSocket clients reject redirects and ignore ambient proxies.
Remote response bodies and exception details do not cross stdout. State,
capabilities and registry names are projected into bounded display fields;
arbitrary entity attributes stay in the worker. Catalog data is sent only while
the picker is open. Metadata and server responses have size limits, and network,
keyring and device confirmation waits are bounded.

## Validation

The package and `test-shell` run:

- `tests/home-assistant.py`: REST permissions, privacy and request bounds.
- `tests/home-assistant-live.py`: private HTTP/WebSocket fixtures, keyring failure
  and migration, preferences, light capabilities, device acknowledgements,
  reconnects and stdin EOF. Keyring calls use mocks, never the user's wallet.
- `tests/home-assistant-store.js`: concurrent UI requests, stale controls,
  credential lifetime, preference failures and deliberate keyboard input.
- `tests/home-assistant-panel.sh`: setup, expanded lights, picker and offline
  rendering on a private headless compositor with fixture data. Set
  `SEELE_HA_RENDER_DIR` to retain its PNG previews.

The main package also compiles production QML and runs `qmllint`. Build Notes
when changing the shared switch, which is also installed with that app.
