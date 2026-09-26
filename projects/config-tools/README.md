# Native configuration tools

`seele-portable-config` owns refresh of generated config links. Directory
descriptors pin its private lock and manifest publication, malformed metadata
does not erase links, missing sources retain the previous generation, and user
files or replaced links are preserved. Traversal, retained metadata and lock
waiting are bounded. The materializer never changes existing directory modes.

`seele-launch MANIFEST [ARGUMENT ...]` combines that materialization with direct
application execution. Its version-1 JSON manifest declares an absolute program,
fixed arguments, additional PATH entries, environment values, and an optional
configuration source/destination. All declarations validate before configuration
publication. Compiled Nix wrappers pin the manifest. There
is no runtime shell evaluation or additional child between the launcher and the
application: `exec` retains the PID, signals, terminal and exit status.

Environment values and config destinations support `$NAME`, `${NAME}`,
`${NAME:-default}`, `${NAME-default}`, `${NAME:+alternate}` and
`${NAME+alternate}`, including nested defaults. Values read from the environment
are data: spaces, quotes, wildcard characters and command-looking text are never
reinterpreted. Undefined bare variables, command substitution, arithmetic,
assignment and unsupported shell operators fail closed. Backslash escapes for
the double-quoted shell characters remain available. Unlike a shell fragment,
literal quote characters are ordinary data and need no balancing.

The manifest is bounded to 1 MiB, with at most 256 arguments and PATH components
and 4096 exports. Each expansion and the aggregate declared environment have a
1 MiB limit; nesting is limited to 16 levels. Variable substitution preserves
non-UTF-8 Unix environment bytes. PATH is prefixed before exports, and exports
are evaluated in sorted name order, retaining the original Home Manager wrapper
ordering. Declare data substitutions in `home.sessionVariables`; command logic
belongs in a native executable rather than an environment string.

`seele-home-backup` delegates numbered backup semantics to the packaged native
GNU `mv`, using a fixed argument vector, an exact destination, and `--` before
the original filename. Existing directories and directory symlinks cannot redirect
the backup inside another directory.
`seele-portable-apps`, `seele-inputs`, and `seele-project-text` provide the
catalog, offline input projection, and safe project-text selection paths. They
share runtime file/process bounds and canonical text handling.

Run `cargo test -p seele-config-tools --locked` for native boundary and real exec
fixtures. `tests/{materialize,catalog,inputs,project_text}.py` retain compatibility
assertions through the native executables; Python is a development dependency
only. The launcher fixture uses a private temporary home and a synthetic target,
checks exact arguments/environment/PID and exit status, and never launches a
configured application or touches the user's home.

## Coordinated themes

`seele-theme list | current | set <id> | reset | init` reads the version-2
Home Manager catalog at `$XDG_CONFIG_HOME/seele-theme/catalog.json`. It accepts
safe catalog IDs, an explicit light/dark mode, exactly 16 six-digit RGB Base16
slots (`base00`–`base0F`), and a bounded generated Vicinae asset. The catalog pins desktop command paths; themes contain
no executable hooks. Listing is read-only and starts no desktop command.

The selected ID and the complete shell palette are stored in mode-0600
`$XDG_STATE_HOME/seele-theme/selection.json`. Private generation directories
contain Ghostty, Fish, tmux, GTK and Hyprland includes and a copy of the
Stylix-generated Vicinae TOML. Selection data includes the full Base16 palette
for Neovim and its projection into Seele’s shared color roles. A directory lock
serializes activation and switching; a complete generation is selected with an
atomic `current` symlink replacement before atomically publishing the shell
palette. A publication failure restores the old include target. A subsequent
`init` repairs an interrupted publication from the durable selection, refreshes
it from the current catalog and preserves the selected ID, including migration
from a version-1 saved selection. Invalid saved state
fails visibly; explicit `reset` replaces it with the declarative default.

Application reloads run after publication while still holding the lock. They
have bounded output and two-second deadlines, never restart services or elevate,
and report affected app names in the JSON `pending` array if they fail. A reload
failure leaves the chosen theme saved. The current default tmux server is
recolored if it exists, Ghostty's active desktop service uses systemd Reload,
Hyprland receives one Lua color update, and the desktop color preference and
Vicinae theme follow the selected mode and palette. Vicinae uses the stable
`seele-current` theme ID; its same-ID reload reads the newly published asset,
and Home Manager points both light and dark preferences at that ID. GTK apps may need reopening, Fish
updates on its next prompt, and non-service Ghostty instances need their own
Reload Configuration action. No wallpaper, font, application content, managed
config, or user account data is modified.

Run `python3 projects/config-tools/tests/themes.py target/debug/seele-theme`
after building this crate. The package runs the same fixture against the
installed executable. It covers first use, read-only listing, preservation,
concurrent switching/activation, invalid palettes, missing generated assets,
legacy state migration, rollback, symlink boundaries
and exact reload arguments against fake desktop tools.

### Light, dark and the schedule

The desktop keeps two presets, one for light mode and one for dark, and a mode
that picks between them; any preset may fill either slot. They live beside the
selection in mode-0600 `preferences.json`, and every command below changes
them under the same directory lock as publication, so a picker, the scheduler
and activation never interleave. A preset is published only when the applied
one actually changes. Before there were preferences, the one saved selection
becomes the slot of its own mode and the other slot starts at that mode's
default: the default itself, else its family's variant of that mode (Latte
beside Mocha), else the catalog's first preset of that mode.

- `set <id>` fills the slot of the mode on screen and always republishes, which
  is what choosing the applied theme again asks for.
- `pick <id>` fills the slot of the preset's own mode and switches to that
  mode, so a light preset brings light mode with it and the other slot keeps
  its preset.
- `slot <dark|light> <id>` fills one slot, publishing only if its mode is on
  screen; `mode <dark|light>` switches mode; `restore <mode> <dark> <light>`
  puts back all three at once, which is how a picker cancels.
- `auto off | sun | schedule <light HH:MM> <dark HH:MM>` sets the schedule.
  Turning it on puts the desktop where the schedule says now; off keeps the
  times for next time.
- `tick` runs one step of the schedule, and `follow` runs it as a loop.

The schedule is edge-triggered. It records the last boundary it acted on, and a
step changes the mode only when a newer boundary has passed, so a mode chosen by
hand holds until the next sunrise, sunset or fixed time, and a boundary missed
while the machine slept is caught up at the next step. `follow` wakes at the next
boundary and at least once a minute, so a resumed machine, a changed clock and
new settings are seen promptly.

`appearance.rs` computes the boundaries. Fixed times go through the system's
own local time via `libc`, as `seele-clock` does, so a daylight-saving day still
switches at the time on the clock. Sunrise and sunset use the algorithm behind
NOAA's solar calculator, whose results match published tables to within a
couple of minutes. A polar summer or winter has no boundary and holds light or
dark. The place is never asked for and never stored: it is the system
timezone's reference city from the tz database's `zone1970.tab` (from `TZ`, else
`/etc/localtime`), which says nothing a timezone does not already say. A zone
without a city, such as `Etc/UTC`, cannot follow the sun, and the command
refuses rather than guessing. `list` and every reply describe the slots, the
mode, the schedule, today's sunrise and sunset, and the next boundary, for a
picker to draw.

`src/appearance.rs` tests the schedule against a fixed-offset calendar, and
the sun against independently published times for Berlin across the year, New
York, Sydney and Tromsø's polar day and night.
`python3 projects/config-tools/tests/appearance.py target/debug/seele-theme`
drives the real CLI with a fixed clock and a synthetic tz table through
migration, both slots, the mode, restore, both schedules, a hand-chosen mode
holding until its boundary, catch-up after a gap, and `follow` applying a
boundary on its own.

### Pickers

Three surfaces call this helper and none owns anything it owns: the Vicinae
**Seele Themes** command, the shell's floating Themes switcher, and the Control
Center's Themes panel (`ThemeStore.qml`, `ThemePanel.qml` and
`ThemeSettingsPanel.qml`, with ordering, movement and the schedule's sentence in
`qml-core`'s `themes.rs`). The launcher applies the theme for the mode on screen
with `set`.

The switcher is a carousel of every preset, never filtered, and each step sends
`pick`, so it needs no mode control of its own; Escape cancels with `restore`.
Moves are coalesced: a held arrow key settles for a moment, only the last move
is sent, one request runs at a time, and a newer request of the same kind
replaces one still waiting. The Control Center panel holds the rest: Light and
Dark (`auto off`, then `mode`), Auto with sunrise and sunset or two times
(`auto`), and `slot` for a mode that should wear a preset of the other kind.
Its tile's knob shows Light, Dark or Auto and steps to the next.

No surface treats a reply as the answer to which theme is applied: the store
watches the published `selection.json` and `preferences.json` and reads the
helper again whenever either changes while nothing of its own is pending, so a
switch made from the launcher, by the schedule or during activation reaches the
tile and an open picker alike. Their behavior is covered by
`tests/vicinae-themes.cjs`, `tests/themes.js` and `tests/tst_themes.qml`;
parent-side integration is documented in Seele's `docs/theme-switching.md`.
