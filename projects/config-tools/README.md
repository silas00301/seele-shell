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

Two surfaces call this helper and neither owns anything it owns: the Vicinae
**Seele Themes** command and the shell's Themes panel (`ThemeStore.qml`,
`ThemePanel.qml`, with grouping, search and wording in `qml-core`'s
`themes.rs`). Both list only what they are showing and apply one theme at a
time. The shell panel switches as the reader moves between tiles, so a held
arrow key is coalesced: moves settle for a moment, only the last is sent, and a
choice made while a switch runs follows the moment it finishes. The shell panel does not treat its own `set` reply as the answer to which
theme is applied: it watches the published `selection.json` for that, so a
switch made from the launcher, from `seele-theme` directly or during activation
marks the same row. Their behavior is covered by `tests/vicinae-themes.cjs` and
`tests/themes.js`; parent-side integration is documented in Seele's
`docs/theme-switching.md`.
