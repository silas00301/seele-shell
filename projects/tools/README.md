# Native desktop tools

Every desktop command has a separate Rust binary. Thin `src/bin` entrypoints
share the `seele_tools` library; `seele-tools <command>` remains a compatibility
dispatcher. The URI and dictation workers retain their dedicated binaries.
There is no Python or JavaScript runtime in this crate. Qt/QML continues to own
presentation and animation.

## Shared boundaries

`seele-runtime` owns bounded subprocess I/O, process-group cancellation and
reaping, signal handling, private state publication, exclusive file publication,
JSON framing and output deadlines. Ordinary probes allow 20 seconds and 16 MiB
combined output; explicit interactive rebuilds keep terminal I/O and have a
24-hour deadline. Clipboard owners and desktop launchers may survive a successful
launcher exit; failed or cancelled launchers are terminated and reaped.

Desktop daemon records include executable identity and Linux process start time.
Signals use pidfds, so PID reuse cannot redirect a stop or force-quit action.
Legacy numeric records are accepted only for the exact expected executable and
same user. Log destinations reject symlinks, hardlinks and foreign owners. Logged device
daemons have an 8 MiB regular-file write limit so diagnostic floods cannot fill
the disk; exceeding it terminates the faulty daemon.

Mic sync and the live status feed share a bounded PipeWire JSON parser. Monitor
processes have owned cancellation and cleanup, queues have explicit capacities,
ALSA wakes coalesce, and fragmented UTF-8 remains intact. The retained graph is
limited to 4096 objects and 16 MiB; an ID index preserves stable row order while
avoiding repeated registry searches. Shutdown joins status producers before the
controller exits. Bluetooth pairing cleanup closes discoverability even after
SIGTERM or a D-Bus failure.

## Application launchers and lifecycle adapters

`seele-lock-run` preserves Quickshell's successful daemon handoff and returns
success only after the compositor reports `secure`. Launch has a ten-second
limit; the confirmation phase has one five-second deadline, including bounded
IPC probes. Timeout or cancellation after handoff leaves the lock process
running. A failed daemonizing parent is cleaned up by the shared supervisor.
An already secure lock is reused. `seele-greeter-run` supervises its foreground
Quickshell process, preserves its exit status and asks the private greeter
compositor to exit during ordinary completion or cancellation.

`seele-notes-run` asks an existing instance to open through bounded IPC and
otherwise replaces itself with Quickshell. Launchers share `SEELE_QUICKSHELL`
and `SEELE_CONFIG`; greeter also takes the packaged `SEELE_HYPRCTL` path.
`tests/launchers.rs` exercises daemon retention, secure acknowledgement, timeout,
cancellation, compositor cleanup and Notes exec behavior using synthetic
executables only.

Pi/OpenCode in-process host adapters call `seele-agent-hook`; they contain no
filesystem publication code. Native hooks accept only input/working/end events,
bound stdin to 1 MiB and two seconds, preserve valid start metadata, and publish
private records through the shared atomic writer. Status readers bound record
counts and bytes and reject symlink, hardlink and foreign-owner records. The
host-event parity and hostile-file tests live in `tests/harness-status.sh` at
the workspace root.

## Recognition

The warmed OCR pool has at most six single-threaded engines and a 128-job queue.
Input requests are limited to 64 KiB and eight queued messages. Superseded capture
sessions are bounded and their queued work observes cancellation. RGB allocations
share a 512 MiB process budget, with a 128 MiB limit per output; OCR engines and
working grayscale buffers have separate bounded lifetimes. Grim streams directly
into the retained RGB allocation, then the exact original PPM is written for Qt.

On x86-64, runtime CPU detection selects an AVX2/SSSE3 grayscale conversion; other
CPUs use scalar Rust. Both produce exactly `(77*R + 150*G + 29*B) >> 8`. Tests cover
all 16,777,216 RGB combinations, unaligned input and partial vector tails. No
build-machine CPU features are required.

`cargo bench -p seele-tools --bench grayscale` compares allocations and conversion
using the production implementations. One Linux x86-64 release run on an
AMD EPYC 9354P measured:

| RGB image | Scalar | Dispatched SIMD | Speedup |
| --- | ---: | ---: | ---: |
| 1920 × 640 | 0.870 ms | 0.226 ms | 3.85× |
| 3840 × 640 | 1.938 ms | 0.497 ms | 3.90× |
| 3840 × 2160 | 6.948 ms | 2.365 ms | 2.94× |

These are median conversion timings on the validation machine, not whole-picker
or whole-system speedup claims. Bilinear scaling and the original display pixels
are unchanged. The synthetic real-OCR fixture covers text, low-contrast browser
chrome, strip seams, QR/barcodes, multiple outputs and cancellation/cleanup.

## Notes storage

The Notes worker bounds requests before allocation, notes at 2 MiB, response
serialization at 32 MiB, listings at 4096 entries, recovery listings at 128 entries
and 16 MiB of stored JSON, and audio references at 256 per note. Capture-directory
summaries cache file identity, size, mtime and ctime. Recent or future timestamps
and clock reversal bypass reuse, avoiding same-tick changes with restored mtime.
Directory events invalidate matching names even when metadata agrees; overflow
and directory replacement invalidate the whole cache. Stable unchanged notes
are not reread and rehashed on every autosave/list. SHA-256 identifies content and recovery keys;
older recovery records remain readable and are cleared by recorded note path.

Vault paths must remain under the selected canonical root. Nested symlinks,
traversal, non-Markdown note paths and directories writable by other accounts are
rejected. Embedded recordings cannot escape the vault. A shared writable vault
must be moved to an appropriate user-owned directory before using this worker;
the worker does not change existing directory permissions.

New notes, conflict copies, trash/restore, migrated audio and finalized recordings
publish exclusively and retry numbered names; concurrent external files are never
overwritten by those operations. Existing-note autosave still uses optimistic
content comparison: another editor can change a file between comparison and
replacement, so this is not a cross-editor atomic compare-and-swap guarantee.
Conflict backups and private recovery drafts preserve the deliberate resolution
paths. Drafts, indices and settings use atomic durable publication.

Vault Markdown and finalized audio remain mode 0644, matching the existing
Obsidian content contract. Private state remains mode 0600. Recording uses the
shared subprocess supervisor, a one-hour PCM limit and a ten-second watchdog for missing PCM
data, preserves exact sample bytes, and finalizes recoverable audio after an
unexpected disconnect. SIGKILL, power loss or finalization I/O failure can leave a
private `.part` staging file; it is not automatically discarded.

## Validation

Run `cargo test -p seele-tools` and `cargo clippy -p seele-tools --all-targets -- -D
warnings`. Focused external fixtures are `tests/mic-sync.sh`,
`tests/control-actions.sh`, `tests/bluetooth-receiver.sh`, `tests/agent-state.sh`,
`tests/notes.py` and `tests/uri-picker.sh`. They use isolated fake desktop programs,
private temporary vaults and synthetic images. Python/Node in these fixtures are
development-only. Give temporary fixtures an ordinary private umask (077 or 022).

The local OCR validation used extracted Ubuntu ImageMagick 6, Tesseract English
data and Zint, with a development `magick`→`convert` shim and DejaVu Sans Mono.
That proves protocol/recognition behavior with synthetic fixtures; it does not
prove Maple NF font rendering or compositor-level visual parity.

## Bluetooth authorization

Pairing notifications carry a random nonce; a bounded native reader retrieves
only its matching private request. UI answers use stdin JSON, with a 4096-byte,
two-second input bound, and require the current nonce and a valid request-specific
code. The removed answer-in-argv interface must not be restored. The agent pins
the BlueZ bus owner, rejects other senders, bounds its private answer read and
observes shutdown and its pairing-window deadline. One authorization worker and
a two-message channel keep D-Bus callbacks responsive. Only the bus thread can
publish a request or grant trust; it drains queued cancellation and owner-change
messages before each worker completion. Cancel/Release invalidate the generation
and nonce, and losing the pinned BlueZ owner terminates the agent. Metadata reads
reuse one bounded D-Bus connection and target the pinned owner. The UI handlers
preserve supersession/dismissal without changing the dialog's visual items.
Real-device roaming and service authorization still need live-host validation.

Local security fixtures (no Bluetooth adapter or host D-Bus service):

```sh
cargo test -p seele-tools --test bluetooth
python3 tests/bluetooth-pairing.py target/debug/seele-control
node tests/bluetooth-pairing.js projects/shell/shell.qml
```

The Rust Bluetooth integration fixture needs `dbus-daemon` and a shell on PATH.
It runs the actual agent against an isolated fake BlueZ bus, checking sender
identity, one-worker admission, stale answers, queued cancellation before trust,
successful authorization, Release and bus-owner loss. Fixture subprocesses and
the private bus are owned and reaped; no host adapter is accessed.

The Pi/OpenCode status extensions only project their host callbacks. OpenCode's
busy/waiting sets, concurrent-session priority and event transitions belong to
`agents.rs`. The native hook retains bounded SHA-256 session identities in its
private metadata sidecar; it never receives permission contents or model text.
Startup/shutdown reset that sidecar, and stale process cleanup removes it.
