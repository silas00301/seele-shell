# Native desktop helpers

`seele-screenshot` and `spt-st` replace the parent flake's runtime Bash helpers.
They share bounded subprocess management, file-descriptor inheritance, private
filesystem checks and cancellation with `seele-runtime`. The parent supplies
native executable wrappers with the feature's exact dependencies; Python is used
only by isolated fixtures.

## Screenshots

On Linux, `seele-screenshot capture|annotate|upload` retains the same frozen
Hyprpicker frame, Slurp window/monitor/region picker, Satty annotation, Zenity
upload consent, clipboard behavior and notifications. The parent keybindings and
all native dialog text remain unchanged.

Monitor and client snapshots are collected concurrently. Geometry accounts for
output scaling, rotation, visible workspaces and pinned windows; tiny clicks
resolve to the smallest containing suggested rectangle. Invalid or excessive
geometry is rejected before capture. The freeze process must remain alive until
Grim has finished and is terminated on every completion or cancellation.

Intermediate PNGs stay in a private runtime directory. Completed images are
published atomically as mode-0600 timestamped files in `Pictures/Screenshots`,
with exclusive numbered names on collisions. Cancelling annotation creates no
public placeholder. The destination must be user-owned and not writable by
other users. The helper validates opened image descriptors rather than trusting
later path lookups, rejects symlinks and hardlinks, and bounds image size.

Upload still requires the native consent dialog naming the public third-party
host 0x0.st and its 24-hour secret link. Curl receives the original validated
private descriptor, so replacement of the public screenshot path during consent
cannot upload another file. Its own configuration is disabled, redirects are
not followed, TLS and HTTPS are enforced, and both duration and response size
are bounded. Decline, failure, or invalid links retain and copy the local image.
Clipboard processes may retain their background ownership after successful
handoff; failed or cancelled handoffs are cleaned up.

Run from the shell root:

```sh
cargo test -p seele-desktop-tools
cargo build -p seele-desktop-tools
PYTHONDONTWRITEBYTECODE=1 python3 projects/desktop-tools/tests/screenshot.py target/debug/seele-screenshot
```

The fixture uses fake desktop programs and performs no uploads. It covers
capture, clicks, annotation cancellation, exclusive names and modes, exact
consent, descriptor-only upload, public-path replacement, copy fallback,
clipboard daemon survival, geometry bounds and process cleanup. Native compositor
rendering still requires the live Hyprland/Slurp/Satty stack; the fixture does not
establish pixel identity by itself.

## Spotify status

`spt-st` parses one bounded `spotify_player get key playback` JSON response in
Rust and emits the same `song · artist` label when playing, or an empty line when
paused/unavailable. Untrusted terminal controls and directional formatting are
removed. This binary is portable across the parent flake's Linux/Darwin systems;
no Spotify credentials are read by the helper.

## Brave Qt preferences

`set-brave-qt-theme` edits only the three existing Chromium theme fields before
Brave starts. A `SingletonLock`, including a dangling symlink, prevents edits.
Root/profile directories and files are opened without following final symlinks;
file ownership, link count, permissions, type and size are checked. Profiles are
pinned by directory descriptors, and file identity/content timestamps and the
browser lock are rechecked before atomic publication. Invalid or concurrently
changed preferences stay untouched. Successful files are private mode 0600.
There is no `find`, `jq` or temporary-name shell logic at runtime. Fixtures use
only temporary browser profiles and verify data preservation and idempotence.

## Existing Windows boot command

`reboot-windows` still requests the existing `reboot-windows.service`; that
service's `reboot-windows-service` executable selects the first exact Windows
Boot Manager entry, sets BootNext and requests the same nonblocking reboot.
The host's service and Polkit authorization are unchanged. Packaged compiled
wrappers supply fixed root-owned `efibootmgr` and `systemctl` paths; the helper
clears inherited environment, validates targets, bounds output and each command
to five seconds, and stops before later actions on failure. The service requires
root. No rollback races another firmware writer: as before, failure after a
successful BootNext write may leave that selection for the next boot. Native
fixtures exercise parsing and execution order using fake scripts only; they do
not touch firmware, invoke real systemctl, or reboot anything.
