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
- **Seele NixOS Generations**: every generation retained by the system profile,
  with its build time, kernel, NixOS version, and running state. Enter opens a
  review whose build time comes first and whose package changes are calculated
  against the captured immutable running closure with `nvd`. A switch action appears after that
  diff succeeds and requires a destructive confirmation. The picker deliberately
  contains no cleanup control; the existing `nh` policy owns retention.
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
into the view and the worker stops when the view unmounts. Frames are bounded
before JSON decoding; only typed, known fields enter React state. Malformed
streams stop their worker and refresh requests coalesce until a reply. No device,
notification, or window data is saved by the extension. Pairing, remote access,
and power actions keep using their shell panels and existing prompts.

The existing Rust `seele-control` owns generation/window snapshots, four-worker
profile resolution, package-diff formatting, device revalidation, keybinding
labels/modifier/input policy and focus argv.
The extension requests those snapshots at its existing refresh points; rendering
starts no processes. Native queries use Nix-supplied paths or deliberate paths
through the running system, bounded output and deadlines, and process-group
cleanup. Hyprland dispatch uses Lua with validated numeric workspace IDs and
window addresses, and rejects Lua errors even when Hyprland exits successfully. Failures show a toast
without exposing subprocess output.

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

Build and run package checks from the shell repository:

```sh
nix build .#default --no-link --no-write-lock-file
```

The package bundles every command declared in `package.json` and runs
`tests/vicinae.cjs` against the shared Rust projection and the host's focus
adapter. Native `tools/tests/vicinae.rs` runs the actual control binary against
private fake Hyprctl/nvd commands, covering immutable diff argv, projection,
invalid input and zero-exit Lua errors without contacting the desktop. `tests/vicinae-runtime.cjs` checks focus handoff, error privacy,
typed field-patch merging, fragmented/oversized frames, and worker cleanup.
`tests/vicinae-keybindings.cjs` tests the actual host callback adapter against
native policy; with a saved reference bundle it compares complete sorted display
rows and input argument vectors. Keybinding refresh performs one native query;
rendering only maps returned rows and retains host locale collation. `tests/vicinae-generations.mjs` checks
native generation-number validation, active-closure detection, JSON normalization,
and safe diff rendering. Native tests bound filesystem resolution to four workers
and exercise cancellation before filesystem lookup.
`tests/vicinae-generation-review.cjs` runs the actual React component against
controlled promises: failed/stale diffs cannot enable activation, confirmation
is deduplicated, and both reviewed identities reach the native helper. Native
`seele-repo-tools` tests exercise the privileged path using private fake closures;
none switches a real system. New command names must match their TSX entry files.

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

**Seele Transfers** opens the personal file-transfer panel. Its picker and drop
target feed the same service selection as `seele-transfers select <file>...`.

The host retains React section composition, locale collation, clipboard and
confirmation callbacks, and scalar control icon/text expressions. These rendering
expressions start no processes, retain no policy state and make no authorization
decisions. Native endpoints own snapshots, validation and action arguments.
