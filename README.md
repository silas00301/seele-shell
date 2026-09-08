# Seele Shell

Seele is a Quickshell desktop shell packaged as a Nix flake. The flake exposes
the main shell plus separate Notes, greeter, lock-screen, and polkit packages.

## Layout

- `packages/core/`: shared Rust package definition and local upstream patches.
- `projects/shell/`: main Quickshell UI, agent integrations, and package definition.
- `projects/notes/`: standalone Notes and voice memo application.
- `projects/shared/`: theme, surface, typography, control, and waveform components shared by the shell and Notes.
- `projects/tools/`: Rust runtime for agent, audio, Bluetooth, clock, session, URI picking, and shell-control commands.
- `projects/greeter/`, `projects/lock/`, `projects/polkit/`: standalone shell surfaces and package definitions.
- `projects/vicinae/`: Vicinae extension source.
- `tests/`: package install checks and focused behavior tests.

Each directory under `projects/` owns one part of the desktop. The package
definition in `projects/shell/` combines the runtime tools. The greeter, lock,
and polkit packages use `packages/core/` directly and do not import the main
shell.

`projects/shared/Theme.qml` owns the design tokens the shell and Notes read
from — the type ramp, weights, tracking, spacing, control heights, elevation
fills, surface edges, and the two motion durations — with shared
components beside it. The shell retains thin inline aliases to those components. `CenteredGlyph.qml` keeps icon
ink centered inside fixed wells even when the font's advance width is uneven.
The greeter, lock, and polkit clients mirror the subset of those tokens they use
so all four read as one desktop.

## Status updates

`projects/shell/SystemState.qml` owns the shell's status fields. Apply full
snapshots and optimistic patches through `apply()` rather than replacing the
state object. Each field has its own notify signal, and unchanged JSON branches
keep their identity so unrelated updates do not rebuild device or notification
models. Add new status fields there with their startup defaults.

`tests/system-state.sh` checks update propagation, delegate reuse, and identical
rendered pixels for unchanged device data. It runs in `test-shell` and in the
shell package's install checks. Performance changes preserve the visual tokens,
rendering components, and animation timing.

`tests/shell-load.sh` compiles the complete QML configuration using the real
Quickshell runtime and a private headless Sway compositor. Unlike `qmllint`, it
catches invalid properties assigned through inline component aliases. It does
not instantiate the desktop or start its workers. Both `test-shell` and the
package install checks run it; a successful package build must pass this test.

Agent CPU sampling reads process names from the same `/proc/<pid>/stat` snapshot
as parent IDs and CPU ticks. It retains command-line discovery for harnesses
whose process name differs from their executable.

`seele-control watch-status` streams newline-delimited field patches. Persistent
D-Bus listeners trigger the existing read-only NetworkManager and BlueZ
probes only when those services change. One buffered `pw-dump -m` reader maintains
the PipeWire graph, and volume queries run only for relevant device changes or
explicit acknowledgements. Notification state belongs to the native QML store.
Ancillary state, including VPN clients, cameras, and agent activity, retains its
five-second refresh. Each listener subscribes before its startup query and
resnapshots after service or bus restarts. The PipeWire reader resets its graph
on reconnect and terminates with the controller.

The stream accepts `network`, `bluetooth`, `audio`, `aux`, and
`all` requests on stdin. Explicit requests return the requested fields even if
unchanged, so optimistic UI controls receive an acknowledgement. EOF stops the
controller. The one-shot `seele-control status` interface remains available.
`projects/tools/tests/live.rs` exercises the controller against a private D-Bus,
mock probes, and a PipeWire stream; `tests/status-patches.js` checks the QML
callback's handling of partial updates.

`seele-clock watch` retains timezone labels and seasonal search aliases in
memory. It emits an initial snapshot and handles `refresh` lines on stdin.
Every response recalculates live times, offsets, and pins. A changed TZDIR,
database tables/version, year, or locale invalidates the metadata cache. The
shell refreshes at minute boundaries and whenever the clock panel opens. Times,
ISO dates, and offsets come from the same snapshot. Local seconds tick only
while the clock is expanded; calendar models change only when the date changes.
The catalog merges `zone.tab` and `zone1970.tab`, adds aliases from `tzdata.zi`,
and normalizes saved pins. City and country names come from the database.

## Notes and voice memos

Run `seele-notes`, choose **Seele Notes** in the application launcher, or use
`seele-shellctl notes`. The flake exposes `packages.<system>.notes`. This is a
separate desktop application: it can run without Seele Shell, and repeated
launches reopen its existing window. It reads the same `seele-shell/theme.json`
and imports the same QML materials and tokens.

Notes support editable titles and text, full-text search, autosave, and
recoverable Trash. Ctrl+N creates a note, Ctrl+F searches, Ctrl+S retries a save,
Ctrl+Enter starts/stops a voice memo, and Ctrl+W closes the window. Closing the
window saves text and finishes recording. The idle process stays available for
reopening; it does not keep the microphone open.

`seele-notes-store watch` exchanges JSON lines on stdin/stdout. Notes live in
`$XDG_DATA_HOME/seele-shell/notes` (default `~/.local/share/seele-shell/notes`),
with 0700 directories and 0600 atomic JSON/WAV files. A note's directory contains
`note.json` and its voice memos. Trash is reversible and never deletes audio.
Text stays out of process arguments. Save acknowledgements carry request IDs so
an older response cannot replace newer edits. Failed saves retain the draft
and block a pending move to Trash.

Recording uses the default PipeWire/PulseAudio microphone through `parecord`.
The worker owns and reaps the child, streams real levels, and finalizes a mono
16 kHz PCM WAV on Stop, stdin EOF, or SIGTERM. Recording is capped at one hour;
a locked recording note cannot be trashed. `.part` files are excluded from the
library until a complete WAV is saved. Playback supports pause and seeking and
is isolated from the editor if QtMultimedia cannot load. No cloud service,
transcription, or telemetry is involved.

`tests/notes.py` checks the production storage and recorder with a synthetic
microphone, including permissions, restore, failure, EOF, and signal cleanup.
`tests/notes-store.js` tests the QML save/reconnect callbacks and
`tests/notes.js` covers search and draft merging. Build with
`nix build .#notes --no-link --no-write-lock-file`.

## Dictation waveform

`DictationState.qml` follows `voxtype status --follow --format json`. The
bottom overlay stays on the output where dictation began, accepts no pointer
or keyboard input, and shows Listening or Transcribing. Its waveform consumes
Voxtype 0.7's `voxtype/audio.sock` native frames through the Rust
`seele-dictation-levels` bridge. It drains the 100 Hz feed and sends 20 Hz peaks
to the shared waveform; no second microphone capture or persistent audio file
is created. Socket reconnects, malformed samples, and EOF cleanup are covered
by `tests/dictation.py`. Voxtype's own OSD remains disabled.

## Notifications

Quickshell owns the notification service; disable Mako when running the shell.
Notifications stack by app and expand in place. The panel has Current and
History views, manual DND, and explicit actions. Ordinary toasts last 30 seconds
and pause while hovered; critical and pinned toasts remain visible. A toast's
close button hides it without dismissing the inbox entry.

Verification codes can be copied without dismissal. Notification search,
title/message copying, and timed DND are intentionally absent. Notification
text stays in memory, including history across QML reloads.
`tests/notifications.js` covers lifecycle and grouping;
`tests/notification-server.sh` checks the native service on a private bus.

## Screen links

Run `seele-shellctl uris` (Super + Ctrl + S on nerv) to freeze every output and
highlight visible URIs with globally unique numbers. Type a number to open it
with `xdg-open`, or click its highlight or badge. Escape dismisses the picker;
Backspace edits a number. When a number also prefixes another one, Enter opens
the exact match: `1` + Enter selects 1 when 10 also exists. Numbers are stable
as OCR results arrive. Enter or a click can open an already numbered link
while other regions are still being recognized. Automatic number selection
waits until the complete number set is known.
QR codes and barcodes share the same numbering as ordinary text links. Each
code shows its decoded text below it, or above it when there is no room below.
Selecting a code opens its URI, or copies its exact text when it has no URI.
Ctrl + number copies any selection, including ordinary links. Ctrl stays in
effect for the whole number even if released between digits; Enter still
confirms an ambiguous number. Ctrl + click also copies a selection.
If a scan finds no links or codes, it releases the frozen screens and keyboard
immediately. A click-through result card remains for five seconds. Capture
failures and timeouts follow the same dismissal behavior.

The overlay uses the shell's existing palette, Maple typography, surface tokens,
header, edges and grain. It has no opening animation or full-screen blur pass.
Each output displays its own capture; normalized OCR coordinates also handle
fractional scaling, mixed resolutions, negative monitor positions and rotated
outputs. Output removal or size changes dismiss the picker.

`UriPicker.qml` owns the UI lifecycle, numeric input and generation checks.
The resident `seele-uri-worker` is a separate Rust binary, so OCR never blocks
the shell's render thread. It preloads up to six independent, single-threaded
Tesseract engines. Grim captures outputs concurrently as uncompressed PPM;
Qt displays those same files without a PNG encode/decode round trip. OCR runs
in overlapping horizontal strips, interleaved across outputs. A Rust grayscale
pass normalizes dark backgrounds and enlarges each strip by 1.5× with bilinear
interpolation, improving small-text recognition without changing the displayed
capture. Tesseract uses local adaptive thresholds to retain dim address-bar text
beside bright icons and borders. Each strip owns
only links whose center lies in its core region, preventing duplicate numbers
at seams. Results stream into a retained QML ListModel. Idle workers block on
channels and retain models, while image allocations are released after OCR.
The same bounded worker pool runs ZBar on each whole output, so code detection
does not depend on OCR strip boundaries. Decoded payloads retain punctuation
and line breaks; they do not go through OCR's prose cleanup.

All added runtime dependencies come from official nixpkgs: Grim, ZBar and
Tesseract 5 with its English data. There are no added flake inputs, Rust crates, downloads
at runtime, or OCR services. Captures live in a private runtime directory and
are removed on dismissal, errors, EOF, or graceful worker termination.

Recognition supports explicit hierarchical URIs (including custom handlers),
`mailto:`, `tel:`, `sms:`, `magnet:`, `geo:`, `news:`, `urn:`, bare domains
(opened as HTTPS), and email addresses. It preserves paths, queries, fragments
and balanced punctuation, and joins tightly spaced OCR tokens around URI
punctuation. It deliberately does not guess replacements for misread characters
or reconstruct visibly truncated links. Bare domains and email hosts require
a public TLD from the ICANN section of nixpkgs' public suffix list, embedded
at build time. Code attributes such as `determinate.url` are ignored. Quoted
assignment values keep their URI while surrounding quotes and semicolons are
removed; ordinary sentence spacing after a period is preserved.
As with any OCR, very small text,
complex backgrounds and links wrapped across lines may not be recognized.

`tests/uri-picker.sh` runs real OCR against generated dark-screen fixtures on
two simulated outputs, including 16px text with query punctuation and a link
crossing a strip boundary, quoted Nix assignments, and sentence boundaries. A
separate dim address-bar fixture checks local contrast. The tests verify
QR URI and Unicode text payloads, Code 128 barcodes alongside ordinary OCR,
capture identity, file permissions, numbering, cancellation during capture and
OCR, failure cleanup, and shutdown. `tests/uri-picker.js` covers numeric prefix
selection, copy actions, and badge and caption placement at screen edges. For a local OCR timing sample,
`seele-uri-worker --image /path/to/frame.ppm` emits the same JSON events,
including `captureMs` and total `elapsedMs`; this excludes compositor and
rendering latency and is not a desktop latency benchmark.

## Build and test

```sh
nix build .#default
nix build .#greeter
nix build .#lock
nix build .#polkit
```

The package install checks run during each build. Enter the development shell
with `nix develop`. It includes the Nix, QML, Node, Python, and Rust tools used
by this repository. `RUST_SRC_PATH` points rust-analyzer at the standard
library sources.

The shell provides these commands:

```sh
check       # evaluate the flake
build-all   # build every package
test-shell  # run focused tests and syntax checks
```

## Focus timer

Right-click the menu-bar clock, or run `seele-shellctl control focus`, to open
25-minute and 50-minute focus sessions or a five-minute break. The countdown
appears beside the clock while active; pause/resume and cancel stay in its panel.
Keys 1/2/3 start presets, Space pauses/resumes (or starts 25 minutes), Delete
cancels, and Escape closes the panel. Presets explicitly replace a running timer.

The deadline includes time spent suspended. A completed session sends one desktop
notification and retains its Done indicator until dismissed or restarted. It does
not change Do Not Disturb or start another session automatically. State survives
QML reloads in memory, but is never written to disk and resets on shell exit.
`tests/focus.js` checks the production deadline state machine, pause/resume,
completion, invalid input, clock rollback, and reload restoration.
