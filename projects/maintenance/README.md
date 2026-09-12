# Maintenance runtime

`seele-maintenance serve` and `seele-maintenance request` implement the existing
System Health JSON contract in Rust. The daemon accepts only its inherited
systemd Unix listening socket. Both endpoints check peer UID; the socket remains
a same-user contract rather than a boundary between one user's processes.

The crate separates the typed lifecycle (`model`), complete source probes
(`publishers`), scheduling/action/analysis policy (`service`), and socket/CLI
adapter. All process ownership, deadlines, socket framing and atomic state writes
come from `seele-runtime`; the daemon uses six independent source scheduler
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
all six publishers, source transaction rollback, inference/consent boundaries,
revision changes, submission/shutdown races, bounded action admission, and the
real daemon and client on a private inherited socket. Socket tests never start a
host service, invoke a desktop command or contact a model. The unchanged QML
store contract remains covered by `tests/maintenance.js` in the shell repository.

Explicit log viewing starts the fixed, validated Ghostty/journalctl command through
`systemd-run --user --collect --quiet`. The user service manager owns the window
so a long-running viewer outlives the bounded maintenance request.
