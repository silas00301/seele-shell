# Seele Shell

Seele is a Quickshell desktop shell packaged as a Nix flake. The flake exposes
the main shell plus separate greeter, lock-screen, and polkit packages.

## Layout

- `packages/core/`: shared Rust package definition and local upstream patches.
- `projects/shell/`: main Quickshell UI, agent integrations, and package definition.
- `projects/tools/`: Rust runtime for agent, audio, Bluetooth, clock, session, URI picking, and shell-control commands.
- `projects/greeter/`, `projects/lock/`, `projects/polkit/`: standalone shell surfaces and package definitions.
- `projects/vicinae/`: Vicinae extension source.
- `tests/`: package install checks and focused behavior tests.

Each directory under `projects/` owns one part of the desktop. The package
definition in `projects/shell/` combines the runtime tools. The greeter, lock,
and polkit packages use `packages/core/` directly and do not import the main
shell.

`projects/shell/shell.qml` opens with the design tokens every surface reads
from — the type ramp, weights, tracking, spacing, control heights, elevation
fills, surface edges, and the two motion durations — followed by the shared
components those surfaces are assembled out of. `CenteredGlyph.qml` keeps icon
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

Agent CPU sampling reads process names from the same `/proc/<pid>/stat` snapshot
as parent IDs and CPU ticks. It retains command-line discovery for harnesses
whose process name differs from their executable.

`seele-control watch-status` streams newline-delimited field patches. Persistent
D-Bus listeners trigger the existing read-only NetworkManager, BlueZ, and mako
probes only when those services change. One buffered `pw-dump -m` reader maintains
the PipeWire graph, and volume queries run only for relevant device changes or
explicit acknowledgements. History aging uses cached notification lists.
Ancillary state, including VPN clients, cameras, and agent activity, retains its
five-second refresh. Each listener subscribes before its startup query and
resnapshots after service or bus restarts. The PipeWire reader resets its graph
on reconnect and terminates with the controller.

The stream accepts `network`, `bluetooth`, `notifications`, `audio`, `aux`, and
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
shell keeps its 30-second clock refresh and restarts either worker if it exits.

## Screen links

Run `seele-shellctl uris` (Super + Ctrl + S on nerv) to freeze every output and
highlight visible URIs with globally unique numbers. Type a number to open it
with `xdg-open`, or click its highlight or badge. Escape dismisses the picker;
Backspace edits a number. When a number also prefixes another one, Enter opens
the exact match: `1` + Enter selects 1 when 10 also exists. Numbers are stable
as OCR results arrive. Enter or a click can open an already numbered link
while other regions are still being recognized. Automatic number selection
waits until the complete number set is known.

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
capture. Each strip owns
only links whose center lies in its core region, preventing duplicate numbers
at seams. Results stream into a retained QML ListModel. Idle workers block on
channels and retain models, while image allocations are released after OCR.

All added runtime dependencies come from official nixpkgs: Grim and Tesseract 5
with its English data. There are no added flake inputs, Rust crates, downloads
at runtime, or OCR services. Captures live in a private runtime directory and
are removed on dismissal, errors, EOF, or graceful worker termination.

Recognition supports explicit hierarchical URIs (including custom handlers),
`mailto:`, `tel:`, `sms:`, `magnet:`, `geo:`, `news:`, `urn:`, bare domains
(opened as HTTPS), and email addresses. It preserves paths, queries, fragments
and balanced punctuation, and joins tightly spaced OCR tokens around URI
punctuation. It deliberately does not guess replacements for misread characters
or reconstruct visibly truncated links. As with any OCR, very small text,
complex backgrounds and links wrapped across lines may not be recognized.

`tests/uri-picker.sh` runs real OCR against generated dark-screen fixtures on
two simulated outputs, including 16px text with query punctuation and a link
crossing a strip boundary. It verifies
capture identity, file permissions, numbering, cancellation during capture and
OCR, failure cleanup, and shutdown. `tests/uri-picker.js` covers numeric prefix
selection and badge placement at screen edges. For a local OCR timing sample,
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
