# Vicinae integration

The shell package bundles this extension and Home Manager installs it in
Vicinae's extension directory. Search for `Seele` in Vicinae to find:

- **Seele Controls**: live output and microphone levels, mute and volume
  actions, Do Not Disturb, privacy activity, and the shell's panels.
- **Seele Windows and Workspaces**: running windows ordered by focus history,
  searchable by title, application, and workspace. Enter focuses the selected
  window or workspace. Ctrl+R refreshes; an open view also refreshes every
  three seconds.
- **Seele Audio Devices**: outputs and microphones, including available output
  profiles on inactive cards. Enter selects a device. A fresh snapshot resolves
  its node before switching so a recycled PipeWire ID cannot select a different
  device after a reconnect. Ctrl+Enter adds an output to current playback or
  removes it; at least one output stays selected. Enter returns to that device
  alone. The shell Audio panel offers the same selection through its Multiple
  outputs switch.
- **Search Keybindings**: the compositor's active bindings. Enter closes the
  launcher and inputs the shortcut. Shift+Enter copies it; Ctrl+R reloads it.
- **Seele Notes and Voice Memos**: open the separate Notes desktop app.
- **Seele Screen Links and Codes**, **Seele Control Center**, **Seele AI
  Cockpit**, and **Seele Session Controls**: direct commands for root search,
  aliases, favorites, and Vicinae shortcuts.

In Seele Controls, Enter toggles mute on either volume row. Ctrl+Up and
Ctrl+Down change that row's volume in five-percent steps. The shell enforces
its existing output and microphone limits and displays the OSD.

Live controls subscribe to `seele-control watch-status`. Field patches merge
into the view and the worker stops when the view unmounts. No device,
notification, or window data is saved by the extension. Pairing, remote access,
and power actions keep using their shell panels and existing prompts.

All subprocesses use paths supplied by Nix, argument arrays, and bounded
execution. Hyprland dispatch uses Lua with validated numeric workspace IDs and
window addresses. Failures show a toast without exposing subprocess output.

Build and run package checks from the shell repository:

```sh
nix build .#default --no-link --no-write-lock-file
```

The package bundles every command declared in `package.json` and runs
`tests/vicinae.cjs` for window selection, Lua targeting, and audio-profile
arguments. `tests/vicinae-runtime.cjs` checks focus handoff, error privacy,
field-patch merging, and worker cleanup. New command names must match their
TSX entry files.

Headphone panels use an AirPods silhouette and label while AirPods are
connected, with priority over other supported headphones. Otherwise they use
an over-ear silhouette and the Headphones label.

Simultaneous playback uses PipeWire's `module-combine-sink` through the packaged
`pactl` client. It mirrors active outputs and requests latency compensation.
Activate an inactive card profile before adding its output. Hardware profiles
remain exclusive, and wireless devices may still have different audible delay.
The combined output lasts for the audio-server session. Returning to one output
removes Seele's combined device. Existing streams on the old default follow the
selection; applications explicitly routed elsewhere keep their output.

`tests/audio-routing.sh` verifies real routing, failures, and cleanup using a
private PipeWire server and null sinks. `tests/headphones-icon.sh` checks that
the two rendered silhouettes stay separate.
