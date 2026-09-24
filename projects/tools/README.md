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

## Caffeinate

`seele-caffeinate serve|request|watch` is the resident session service behind the
launcher command and the shell's conditional bar item. It holds one
systemd-logind `idle`/`block` inhibitor for the life of a session and nothing
else: the returned descriptor is the inhibition, so owner exit, logout and a
crash all release it, and no `sleep` lock is taken because that would refuse an
explicit suspend. Tracked processes use the same pidfd identity as the daemon
records above, so PID reuse cannot extend a session; tracked transfers follow the
transfer group id through the transfers socket, and a probe that cannot be read
ends the session after tolerating a service restart. Requests are bounded to
64 KiB behind a mode-0600 peer-verified socket with a private advisory lock, four
workers and a queue of eight. Nothing is persisted, so no session survives a
reboot. Duration parsing, labels and failure text belong to `qml-core`; see
[`projects/caffeinate/README.md`](../caffeinate/README.md) for the protocol,
the inhibition boundary and its fixtures.

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

## Port inspector

`seele-ports` is the resident, unprivileged worker behind the shell's Ports
panel. `seele-stop-listener` is the narrow privileged helper it reaches through
`run0`. Both link only `/proc` reading and `seele-runtime`, so neither carries
the D-Bus, image or recognition code of the rest of this crate.

Discovery parses `/proc/net/tcp` and `/proc/net/tcp6` and keeps only sockets in
the `0A` listening state, in the host network namespace. UDP, remote scanning
and other namespaces are out of scope. Ownership comes from mapping socket
inodes through `/proc/<pid>/fd`, with metadata from `comm`, `stat`, `status`,
`cgroup` and `cwd`; a project is the nearest `.jj`/`.git` root or build-manifest
directory above the working directory. Every root is injected, so the tests
build a synthetic process table instead of reading the host's. A field the
kernel will not show stays empty and is rendered as unknown; another user's
listener is listed with no owner rather than a guessed one. A host-side proxy is
named as a proxy instead of being presented as the application it forwards to.
Scans are bounded to 512 listeners, 4096 processes, 1024 descriptors per process
and 16 owners per socket. Listing reads `/proc` and nothing else: it starts no
system manager, contacts no listening service and raises no authentication
prompt.

A row's identity is its socket inode, so a rebound port is a different row. An
action carries a review token holding the whole decision — inode, binding, port,
target kind, unit, manager scope, PID, process start time and UID — and nothing
descriptive. Before acting, the worker rebuilds that token from a fresh scan and
compares tokens; it then acts on the freshly derived target rather than on
anything parsed out of the token, because privilege belongs to the current owner
and not to the review. The privileged helper repeats the same resolution after
authentication. A recycled PID has a different start time and a rebound port a
different inode, so a stale confirmation is refused rather than redirected. Both
process signal paths pin a pidfd before repeating the identity checks and signal
that handle; unsupported or exited handles fail closed. System service actions
also require every owner to belong to the system manager, not a same-named user unit.

Only a `.service` becomes a stop target, and only when every process holding the
socket belongs to it. A `.scope` and `user@<uid>.service` are excluded: stopping
either would end a session rather than a listener. A user unit is stopped through
the caller's own manager and needs no authentication; a system unit or a process
owned by another user goes through `run0`. Force is enforced in Rust, not in QML:
`stop` with mode `force` is refused as `not-escalatable` unless that exact token
already had a graceful attempt that left the listener bound. Force stays inside
the reviewed unit and never falls back to signalling a PID. Nothing is ever
disabled. Restart and `TriggeredBy` are read with `systemctl show` only while a
plan is being built, so a reactivating unit is disclosed rather than silently
worked around, and a refresh stays a pure `/proc` read.

The helper accepts a typed target — `identify`, `service` or `process` with
strictly parsed decimal and address arguments — and never a command, a path or
an argument list. It rejects a leading zero, a sign, surrounding whitespace and
any trailing character, and it refuses a unit name that is not a plain
`.service`. Each refusal has its own exit code, and every outcome is one JSON
line. The worker locates it beside its own canonicalized `current_exe()` rather
than through `PATH`, and requires a regular, executable file that is not group-
or world-writable, so nothing in the environment can substitute the program that
runs as root. `identify` is a separately authorized read that resolves owners and
signals nothing.

The `query` request accepts `text` and optional `scope` (`all`, `loopback`, or
`network`; omitted/unrecognized values mean `all`). Snapshots echo the normalized
scope beside the parsed query and preserve an explicit HTTP/HTTPS scheme.
Loopback includes IPv4, IPv6 and IPv4-mapped IPv6 loopback addresses; Network
includes wildcard and specific non-loopback bindings. These describe the bind
address only, not firewall policy or reachability. Scope and text are an
intersection; `total` still counts all discovered listeners. Missing owner
metadata stays unknown in every view. Filtering changes no review token or
native action/revalidation policy. The synthetic `tests/ports.rs` fixture
covers scope/query intersections, unchanged action identity and unknown ownership;
its model tests include mapped IPv6 loopback and both wildcard families.
`tests/ports.js` exercises late replies and confirmation dismissal, while
`tests/ports-panel.js` renders the production panel and checks pointer/keyboard
filter selection, reset, and valid list selection after filtering. The shell
package and development checks both run that Qt fixture.

Changing either filter dismisses the current confirmation immediately, removes
old rows until the matching snapshot arrives, and rejects late query/plan replies.
The empty state resets both filters, while row scheme choices survive filtering.

Nothing is written to disk. Closing the panel clears rows, selections,
escalations and the identities an authentication paid for. `qml-core`'s `ports`
functions own the proposed URL, the failure wording, the row summary and the
bounded action queue, so an address is never assembled by string concatenation
in QML.

## Microphone test

`seele-mic-test` is resident for exactly as long as the Audio panel is open and
reads line-delimited JSON requests on stdin, answering with `mode`, `level` and
`users` events. It owns two shapes of one question: a five-second sample, held
in memory and played back once when it is complete, and a live monitor that
accumulates nothing. Nothing is played into the output while a sample is being
captured.

Capture and playback are `parecord` and `pacat` streams that name their device
explicitly, so the test never changes a default sink or moves another
application's stream. Both streams disable reconnection and movement, so losing
the selected device ends the test instead of selecting another device. The live monitor hands audio to its playback child through
a private non-blocking pipe sized to about eighty-five milliseconds; an output
that stalls costs one buffer of dropped audio rather than latency the user then
hears for the rest of the session. Writes preserve complete PCM frames even
when capture splits a sample or playback stalls. Stop and EOF are serviced on
every supervisor poll, including when capture emits no bytes. Levels and clipping are measured from the
captured samples, with clipping decided on linear values and held for 1.5
seconds, so a compressed meter cannot hide it.

Device identity is resolved when a test starts, not when the panel opened.
Monitor sources are refused as inputs because playing one back into its own
output is the feedback this test exists to avoid. Microphone use comes from
`pactl list source-outputs`, excluding corked streams, monitor readers and the
test's own named streams; a server that cannot be asked reports that limitation
rather than an empty list. The report is republished while a test runs, because
a warning does not make the microphone exclusive. Start remains disabled until
the first report arrives; unavailable detection is shown as a limitation.

The sample never reaches disk, and no state outlives the process: closing the
panel terminates the worker, which ends the capture stream, the playback stream
and the retained sample together.

## Validation

Run `cargo test -p seele-tools` and `cargo clippy -p seele-tools --all-targets -- -D
warnings`. `tests/ports.rs` exercises the port inspector against a synthetic
`/proc` in a private temporary directory, recording the system manager, signal
and authentication calls instead of performing them; the panel's own store is
covered by `tests/ports.js` at the workspace root. Focused external fixtures are
`tests/mic-sync.sh`, `tests/mic-test.sh`, `tests/control-actions.sh`, `tests/bluetooth-receiver.sh`,
`tests/agent-state.sh`, `tests/notes.py` and `tests/uri-picker.sh`. They use isolated fake desktop programs,
private temporary vaults, a private PipeWire instance with synthetic audio, and
synthetic images; no fixture opens the user's real microphone or outputs. Python/Node in these fixtures are
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
