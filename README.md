# Seele Shell

Seele is a Quickshell desktop shell packaged as a Nix flake. The flake exposes
the main shell plus separate greeter, lock-screen, and polkit packages.

## Layout

- `packages/core/`: shared Rust package definition and local upstream patches.
- `projects/shell/`: main Quickshell UI, agent integrations, and package definition.
- `projects/tools/`: Rust runtime for agent, audio, Bluetooth, clock, session, and shell-control commands.
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
