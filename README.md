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
