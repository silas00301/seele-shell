# Vicinae integration

The shell package bundles this extension and Home Manager installs it in
Vicinae's extension directory. Every command is named after its TSX entry file
and carries its own search keywords, so `Seele`, a device, or a panel name all
reach it from the launcher's root search.

## Commands

- **Seele Caffeinate**: keep the machine awake, its displays on and its session
  unlocked until a selected task ends, a chosen duration passes, or the session
  is stopped. The searchable list shows the current session first, then the
  presets and whatever duration is typed into the search bar, then running
  builds and transfers, then other processes. Enter starts the highlighted
  choice; the current session's row offers Stop. The command composes no session
  text, parses no duration and keeps no time: `seele-control vicinae-caffeinate`
  returns display-ready rows, one projection of the live session, and the
  message for a refused start. A typed duration reaches native validation
  exactly as it was typed, because parsing it during rendering would mean a
  subprocess per keystroke. Stopping releases the same inhibitor the shell's
  coffee bar item releases, and never stops the tracked task. See
  [`projects/caffeinate/README.md`](../caffeinate/README.md) for the inhibition
  boundary, task identity and the protocol.

- **Seele Controls** (`seele.tsx`): the live view. Its first section carries the
  output and microphone level, Do Not Disturb, Wi-Fi, Bluetooth, Tailscale, and
  the privacy row, each showing its own state as a coloured tag rather than as
  prose. Batteries follow as one row per device. `Browse` pushes the windows,
  audio, keybinding, and generation views without leaving the command, and
  `Panels`, `AI`, and `Actions` group everything the shell can open.
- **Seele Windows and Workspaces** (`windows.tsx`): running windows in focus
  order, with an application icon, the focused window tagged, and a workspace
  filter in the search bar. Enter focuses the window, Ctrl+Enter focuses its
  workspace, Ctrl+W closes it, and Ctrl+Shift+W force quits after a destructive
  confirmation. Ctrl+R refreshes; an open view also polls every three seconds
  without flashing its loading indicator.
- **Seele Audio Devices** (`audio.tsx`): outputs and microphones, including
  available output profiles on inactive cards. A fresh snapshot resolves the
  selected node before switching, so a recycled PipeWire ID cannot select a
  different device after a reconnect. The current default is tagged `Default`,
  an output added to combined playback is tagged `Also playing`, and the section
  says how many outputs play together. Enter selects a device alone, Ctrl+Enter
  adds or removes it from combined playback, and the level actions act on the
  matching stream. At least one output stays selected.
- **Search Keybindings** (`keybindings.tsx`): the compositor's active bindings,
  grouped by modifier chord with the shortcut as the row's tag and the command
  it runs beneath its description. Enter closes the launcher and inputs the
  shortcut, Shift+Enter copies it, Ctrl+Shift+C copies the command, and Ctrl+R
  reloads.
- **Seele NixOS Generations** (`generations.tsx`): every generation retained by
  the system profile. The list shows its age, NixOS version, and kernel, and the
  running one is tagged. Enter opens a review whose metadata panel holds the
  build time, age, kernel, NixOS version, revision, specialisations, and state,
  with the `nvd` package diff against the captured immutable running closure as
  its content. A switch action appears only after that diff succeeds and
  requires a destructive confirmation. The picker deliberately contains no
  cleanup control; the existing `nh` policy owns retention.
- Direct commands open a panel and nothing else: **Seele Control Center**,
  **Seele Notifications**, **Seele Now Playing**, **Seele AI Cockpit**,
  **Seele Quick AI Prompt**, **Seele GitHub Inbox**, **Seele Home Assistant**,
  **Seele System Health**, **Seele Session Controls**, **Seele Notes**,
  **Seele Transfers**, and **Seele Screen Links and Codes**.

In Seele Controls, Enter toggles mute on either volume row, Ctrl+Up and
Ctrl+Down change that row's volume in five-percent steps, and the `Set Level`
submenu jumps to a fixed percentage. The shell enforces its existing output and
microphone limits and displays the OSD. Do Not Disturb toggles from the row, and
its `Quiet For` submenu starts the same 15-minute, 1-hour, and 4-hour quiet
periods the notification panel offers.

## Ownership

Live views subscribe to `seele-control watch-status`. Field patches merge into
the view and the worker stops when the view unmounts. Frames are bounded before
JSON decoding; only typed, known fields enter React state, which now also covers
the connection name and type, connectivity, Bluetooth device count, batteries,
and Tailscale state. Malformed streams stop their worker and refresh requests
coalesce until a reply. Polled views keep the loading indicator for the first
load and for an explicit refresh only. No device, notification, or window data is
saved by the extension. Pairing, remote access, and power actions keep using
their shell panels and existing prompts.

The existing Rust `seele-control` owns generation/window snapshots, four-worker
profile resolution, package-diff formatting, device revalidation, keybinding
labels/modifiers/input policy and focus argv. Window closing and force quitting
reuse the desktop's own `application` endpoints, which validate the address and
own the compositor request, rather than repeating that policy here. The
extension requests those snapshots at its existing refresh points; rendering
starts no processes. Native queries use Nix-supplied paths or deliberate paths
through the running system, bounded output and deadlines, and process-group
cleanup. Hyprland dispatch uses Lua with validated numeric workspace IDs and
window addresses, and rejects Lua errors even when Hyprland exits successfully.
Failures show a toast without exposing subprocess output.

`ui.tsx` holds the shared presentation: the refresh action, the one shape every
unavailable source uses, level accessories, and the icon expressions for
applications, audio devices, and batteries. These are scalar expressions over
data the native endpoints already validated; they start no process and make no
authorization decision. Only icon names present in both the Raycast typings and
Vicinae's own icon set are used, because Vicinae resolves `@raycast/api` to its
own enum and an unknown name renders nothing.

The generation picker ignores `nixos-rebuild`'s profile-oriented `current`
field and compares canonical generation targets with `/run/current-system`.
Immediately before a switch it resolves the selected generation and running
system again, requiring the exact closures shown in the review.
Vicinae sends that validated integer plus both reviewed store basenames through
the running system's `run0` to the packaged `seele-switch-generation` helper.
After authentication, the root helper requires the fixed system paths still
resolve to both reviewed identities, closing changes during the authorization
dialog. It never executes a caller-supplied path. The root helper
advances `/nix/var/nix/profiles/system` with the running system's `nix-env`, and
executes the selected closure's `switch-to-configuration switch`. It never sets
`NIXOS_NO_CHECK`, accepts arbitrary paths, or performs garbage collection.

## Validation

Build and run package checks from the shell repository:

```sh
nix build .#default --no-link --no-write-lock-file
```

The package bundles every command declared in `package.json` and runs
`tests/vicinae.cjs` against the shared Rust projection and the host's focus
adapter. Native `tools/tests/vicinae.rs` runs the actual control binary against
private fake Hyprctl/nvd commands, covering immutable diff argv, projection,
invalid input and zero-exit Lua errors without contacting the desktop.
`tests/vicinae-views.cjs` renders the actual Seele Controls, windows, audio and
keybinding components against mocked host APIs: which rows a given status
produces, how state reads as tags and icons, the workspace filter and its
fallback when a workspace disappears, a declined force quit reaching no command,
and the exact argument vector every action sends. Rendering runs no command at
all. `tests/vicinae-runtime.cjs` checks focus handoff, error privacy, typed
field-patch merging, fragmented/oversized frames, quiet background polling, and
worker cleanup. `tests/vicinae-keybindings.cjs` tests the actual host callback
adapter against native policy; with a saved reference bundle it compares complete
sorted display rows and input argument vectors. Keybinding refresh performs one
native query; rendering only maps returned rows and retains host locale
collation. `tests/vicinae-generations.mjs` checks native generation-number
validation, active-closure detection, JSON normalization, locale-independent age
rendering, and safe diff rendering. Native tests bound filesystem resolution to
four workers and exercise cancellation before filesystem lookup.
`tests/vicinae-caffeinate.cjs` covers the Caffeinate component, duration handoff, task identity, refused starts and one in-flight Stop.
`tests/vicinae-generation-review.cjs` runs the actual React component against
controlled promises: failed/stale diffs cannot enable activation, confirmation
is deduplicated, and both reviewed identities reach the native helper. Native
`seele-repo-tools` tests exercise the privileged path using private fake closures;
none switches a real system. New command names must match their TSX entry files.

Headphone panels use an AirPods silhouette and label while AirPods are
connected, with priority over other supported headphones. Otherwise they use
an over-ear silhouette and the Headphones label. Seele Controls names its
headphone row after whatever is actually connected.

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

**Seele Transfers** opens the personal file-transfer panel. Its picker and drop
target feed the same service selection as `seele-transfers select <file>...`.

The host retains React section composition, locale collation, clipboard and
confirmation callbacks, and scalar control icon/text expressions. These rendering
expressions start no processes, retain no policy state and make no authorization
decisions. Native endpoints own snapshots, validation and action arguments.
