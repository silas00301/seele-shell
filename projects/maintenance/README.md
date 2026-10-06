# Maintenance runtime

`seele-maintenance serve` and `seele-maintenance request` implement the existing
System Health JSON contract in Rust. The daemon accepts only its inherited
systemd Unix listening socket. Both endpoints check peer UID; the socket remains
a same-user contract rather than a boundary between one user's processes.

The crate separates the typed lifecycle (`model`), complete source probes
(`publishers`), scheduling/action/analysis policy (`service`), and socket/CLI
adapter. All process ownership, deadlines, socket framing and atomic state writes
come from `seele-runtime`; the daemon uses seven independent source scheduler
threads, eight request workers with eight queued connections, and at most eight
concurrent explicit actions. Source rechecks coalesce. No probe, notification,
repair process or model wait runs while holding the state lock. Unchanged source
snapshots preserve revisions and avoid filesystem writes and repeated alerts.

Publishers own ongoing conditions. A complete validated source snapshot commits
atomically; one invalid or unavailable probe preserves the entire previous
snapshot. Disruptive actions require explicit confirmation and a current finding
revision, and are restricted to registered IDs and fixed argv. AI analysis starts
only on explicit request. It submits curated metadata and optional bounded
in-memory diagnostics to the Codex broker, validates the response shape and
registered repair IDs, and never runs its proposals. Changed findings mark older
analysis stale. Shutdown recovers an accepted job ID even if it races submission,
then cancels and releases the job; failed broker jobs remain in Activity.

State is a mode-0600 typed metadata projection in a private directory. Diagnostic
and model payloads are separate memory-only maps. Existing Python-era metadata
and fingerprints load without migration or repeat notifications. Unknown saved
fields are discarded, IDs are reconstructed, persisted links/secrets and Unicode
direction controls are sanitized, and resolved rows and action outcomes expire
after seven days. State reads reject symlinks, nonregular files, another UID and
nonprivate modes. Shared atomic writes use exclusive unpredictable temporary
files, descriptor-relative replacement, and file/directory durability barriers.

Requests and subprocess output are limited to 256 KiB, diagnostics to 16 KiB,
source snapshots to 512 records, and total records to 4,096. Persisted and public
snapshot bytes are independently bounded to 16 MiB. Socket operations enforce
absolute deadlines, including clients that send one byte at a time. Config and
lock files are bounded before parsing. Configured filesystem probes can still be
subject to a kernel-level filesystem stall; no performance or security absolute
is claimed for a malfunctioning kernel, filesystem or external command.

The public source policy and actions remain documented in the parent
`modules/packages/_maintenance/README.md`. Nix/host/compositor validation is a
separate requirement; this migration does not activate services or install Nix.

Run `cargo test --manifest-path projects/maintenance/Cargo.toml` and
`cargo clippy --manifest-path projects/maintenance/Cargo.toml --all-targets -- -D warnings`.
Tests drive the typed lifecycle, malformed publication and persisted state,
all seven publishers, source transaction rollback, inference/consent boundaries,
revision changes, submission/shutdown races, bounded action admission, and the
real daemon and client on a private inherited socket. Socket tests never start a
host service, invoke a desktop command or contact a model. The unchanged QML
store contract remains covered by `tests/maintenance.js` in the shell repository.

## Restart required

The `restart` source compares `/run/booted-system` with `/run/current-system`
on every scheduled check, so it runs once the session starts after a boot and
again within the configured interval (60 seconds by default) of any activation:
`nixos-rebuild switch`, `nh os switch`, the Vicinae generation picker, a
rollback, or `switch-to-configuration` run by hand. It needs no privilege,
because both links and every file it reads are world-readable, and so no root
publisher or system-to-user channel exists. While the two generations differ in
a part that only a boot applies, it publishes one finding under key
`booted-system`; once they agree again, after a restart into the current
generation or a rollback to the booted one, the next check resolves it.

`src/restart.rs` compares only the parts `switch-to-configuration` cannot
replace in the running system:

- `kernel`: the image the boot loader starts, named by the module tree's
  `lib/modules/<release>` ("Linux 6.12.8 → 6.12.10") or as a rebuild.
- `kernel-modules`: the running kernel loads only modules built for it, so a
  new out-of-tree driver is named by the store packages the tree links to,
  without the kernel version its store name carries. NVIDIA's modules are
  `nvidia-open` (on `nerv`) or `nvidia-kernel-modules`, installed under
  `updates/` ("nvidia-open 570.153.02 → 575.64").
- `initrd`: early boot and CPU microcode. A kernel change always rebuilds it,
  so it is named only when the kernel stayed.
- `kernel-params`: named by parameter, never by value, since a command line can
  carry device identifiers.
- `firmware`: activation points the firmware loader at the new files, but a
  driver that already loaded its firmware keeps it until it probes again.
  Packages are named without their compression or firmware-output suffix
  ("linux-firmware 20250808 → 20250911", NVIDIA's GSP firmware as
  "nvidia-x11 570.153.02 → 575.64").
- `systemd`: the switch re-executes PID 1 and the user managers, but NixOS
  marks `systemd-logind` `restartIfChanged = false`, so the login manager keeps
  the booted build.
- The bus binary of the declared `dbus-implementation`: NixOS only reloads the
  system bus, because restarting it would end the session.
- `switch-inhibitors`: what NixOS modules declare a switch must not change in
  place, compared over the keys both generations declare, as upstream does.

Everything else a switch restarts, reloads or re-executes is left out.
`init-interface-version` is left out too: an incompatible init makes
`switch-to-configuration` refuse before activation, so `/run/current-system`
never reaches it.

The finding is `eventually`: it counts toward the bar's System Health mark and
never sends a notification, and recurring checks of an unchanged pair neither
advance its revision nor write state. Its one registered action, `open-power`,
is not disruptive and runs only the fixed `seele-shellctl power`, which opens
the shell's Power panel without closing one already shown. Nothing here
restarts anything; that stays a choice made in the Power panel. A missing or
unreadable generation, or a malformed generated file, is a probe failure that
keeps the previous finding. `tests/restart.rs` builds fixture generations in a
fake store and drives the probe, deduplication, the action and resolution after
reboot and rollback through the real service.

A generation activated with `nixos-rebuild test` is current without being the
boot default, so the restart this finding asks for boots the default instead;
one staged with `nixos-rebuild boot` is not current yet, so nothing is reported
until it is.

Explicit log viewing starts the fixed, validated Ghostty/journalctl command through
`systemd-run --user --collect --quiet`. The user service manager owns the window
so a long-running viewer outlives the bounded maintenance request.

## Individual backup versions

`restic_files.rs` and `seele-backup-files` form a narrow read-only boundary for
exact regular-file paths under one configured home. Credential references,
restic executable, host and allowed home come from the parent package wrapper.
Inherited environment is cleared; tagged host snapshots and the exact file are
revalidated before bounded extraction. No root-side restore write is available.

`seele-restore-file` is an unprivileged native-dialog client, with Quick Look
preview, size/SHA-256 and bounded text comparison, and exclusive mode-0600 copy
publication. It retains previews under a private session runtime directory until
exit. The fixed helper is invoked with run0 pipe I/O and a service lifetime
limit. No Vicinae command or persistent content cache is added.

`tests/restic_files.py` runs the actual backend with a disposable encrypted
restic repository. `tests/restore_ui.py` uses fake authentication/dialogs and the
real file operations. The parent guide `docs/backup-files.md` owns setup,
credential grammar, bounds and live acceptance requirements.
