# Caffeinate

`seele-caffeinate serve` is the user service behind the launcher's **Seele
Caffeinate** command and the shell's conditional coffee bar item. It owns one
session at a time: the inhibitor, the task whose end releases it, and the
snapshot both surfaces read. Neither UI owns the session, so closing the
launcher or the panel changes nothing.

## What is inhibited, and why only this

The session holds exactly one systemd-logind lock:
`org.freedesktop.login1.Manager.Inhibit("idle", "Seele Caffeinate", …, "block")`.
The returned descriptor *is* the inhibition; closing it is the release.

That single lock covers all three automatic actions the issue asks for on
`nerv`:

- **Automatic lock and display-off.** Hypridle's `general:ignore_systemd_inhibit`
  defaults to `0`, so it watches logind's `BlockInhibited` property and counts
  an `idle` lock. `onIdled` then returns before running any listener whose
  `ignore_inhibit` is unset, which is both of the parent's listeners — the
  1,800-second `loginctl lock-session` and the 1,860-second DPMS off. Releasing
  the lock restarts the listeners from scratch, so the normal policy resumes
  with a full timeout rather than firing immediately.
- **Automatic sleep.** logind's own `IdleAction` is governed by the same `idle`
  lock, so a configured idle suspend cannot run while a session is active.

Deliberately not used:

- **A `sleep` lock.** A `block` lock on `sleep` makes logind refuse
  `systemctl suspend`, which would break the explicit Suspend action the issue
  requires to keep working. Automatic sleep is an idle action and is already
  covered.
- **The Wayland idle-inhibit protocol** (`zwp_idle_inhibit_manager_v1`). It is
  per-surface and belongs to the window that wants it — the parent already uses
  it for mpv and fullscreen browsers. A session-wide feature needs a
  session-wide owner.
- **`org.freedesktop.ScreenSaver`.** Hypridle honours it too, but adding a
  second lock would mean two acquisitions, two releases and two ways to leak
  one.

One owner, one descriptor, one release point. Other applications' inhibitors
are untouched: logind tracks inhibitors independently and hypridle counts them,
so releasing this one never releases theirs. Explicit Lock and Suspend stay
available throughout, and Stop releases the inhibitor without locking or
suspending anything.

**Boundary.** This depends on hypridle keeping `ignore_systemd_inhibit = 0`.
If the parent ever sets it, the compositor half of the policy stops being
inhibited and only logind's `IdleAction` remains covered.

## Modes

- `manual` — until Stop.
- `duration` — until an absolute wall-clock deadline. The deadline is stored as
  an epoch second rather than a monotonic remainder, so an explicit suspend in
  the middle of a timed session neither extends nor shortens what was chosen.
- `task` — until the selected task ends, however it ends. Success, failure and
  cancellation are all "ended", and none of them produces a notification.

Expiry, Stop and a finished task all do the same thing: drop the session, which
closes the descriptor. Nothing locks or suspends as a completion action.

There is one active session. A new start acquires its inhibitor **before**
installing the session and dropping the replaced one, so a replacement opens no
gap and leaves no orphan; a failed acquisition leaves the existing session
exactly as it was and returns `inhibit-unavailable`.

Nothing is written to disk — no configuration, no history, no session record —
so an indefinite session cannot survive a reboot, and owner exit or logout
releases the lock by closing the process's descriptors.

## Task identity

The picker returns opaque keys, and starting revalidates the key against the
live system rather than trusting the listing.

- `process:<pid>:<start>` carries the Linux process start time from
  `/proc/<pid>/stat`. Starting opens a pidfd, then requires the pinned process's
  start time to equal the offered one, so a PID recycled between listing and
  starting is refused. The session follows that pidfd: it becomes readable
  exactly once the process terminates, so no rescan can be answered by an
  unrelated process wearing the same PID.
- `transfer:<id>` follows the transfer group id through
  `seele-transfers`' `snapshot` operation, not the lifetime of the long-running
  transfer service. The session ends when the group reaches a terminal state or
  leaves the listing.

The selected process is tracked as itself. A build is not inferred to have
finished because a launcher or one child exited — the session watches the
process that was chosen, and `nix build`, `nh os switch` or `cargo test` is
exactly that process. A command that cannot be mapped to a higher-level task is
labelled and tracked as the process it is.

A task whose state cannot be read is not evidence that it is still running. A
transfer probe that fails is tolerated for two more passes — enough for a
service restart — and then ends the session rather than leaving the machine
awake for something nobody can observe.

## Rows

`tasks` lists transfers first, then recognized builds, then other processes
owned by this user, each group newest first. A row carries its command, its
project where that is reliable, and its PID where it is a process; a
service-owned transfer carries a name instead, built from its direction, file
count and device.

The command is the program plus at most two sub-command-shaped arguments —
`nix build`, `nh os switch`, `cargo test`. Remaining arguments are not shown:
they carry paths, prompts and occasionally credentials, and none of them makes
the row easier to recognize.

A build's project is the name of the nearest ancestor of its working directory
containing `.git` or `.jj`. Anything shallower than a version-control root
would be a guess, so an unreadable or unmarked directory simply has no project,
and only builds are resolved at all.

Recognized builds are an explicit allowlist in `projects/tools/src/caffeinate.rs`:
whole-purpose build commands, plus multi-purpose tools paired with the
sub-commands that make them a build. Extending it is a one-line change; getting
it wrong only costs a row its `Build` chip and its project, never its tracking.

## Protocol

`seele-caffeinate request` reads one bounded JSON request from stdin and reaches
the mode-0600 `$XDG_RUNTIME_DIR/seele-caffeinate.sock`. Both peers verify the
current UID, and a private advisory lock prevents a second service instance from
replacing a live socket and the session behind it. Requests are bounded to
64 KiB and replies to 4 MiB, with four workers and a queue of eight.

Operations are `snapshot`, `tasks`, `start` and `stop`. `start` takes
`mode: "manual" | "duration" | "task"`, plus `duration` (a string such as `45m`,
`1h30` or `1:30`) or `task` (a key from `tasks`). Replies are `{"ok":true, …}`
or `{"ok":false,"error":"<code>"}` with sanitized codes:
`service-unavailable`, `inhibit-unavailable`, `task-unavailable`,
`invalid-duration`, `duration-out-of-range`, `no-session`, `invalid-request`.

`seele-caffeinate watch` polls `snapshot` and emits a line only when the session
changed, with a 20-second heartbeat for broken-pipe detection. A timed session
therefore ticks once a second and an untimed one is silent.

## Presentation

`projects/qml-core/src/caffeinate.rs` owns duration parsing, its bounds
(one minute to twenty-four hours), the remaining-time and elapsed labels, the
mode headline, the bar text and the failure messages. The shell store and the
native `seele-control vicinae-caffeinate` endpoint both read that one
projection, so the bar, the panel and the launcher never describe one session
two ways. Neither QML nor React computes a remaining time, parses a typed
duration or chooses a message.

The launcher hands a typed duration to the service verbatim and shows whatever
the service says about it. Parsing on every keystroke would mean a subprocess on
every keystroke, which the extension's rendering contract does not allow.

The shell's surface is a readout: the bar item appears only while a session is
active, and the panel shows the session and Stop. Starting a session belongs to
the launcher.

## Validation

```sh
cargo test --locked -p seele-tools -p seele-qml-core
cargo clippy --locked -p seele-tools -p seele-qml-core --all-targets --all-features -- -D warnings
node tests/caffeinate.js projects/shell/CaffeinateStore.qml \
  projects/shell/CaffeinatePanel.qml projects/shell/shell.qml
```

The Rust tests cover the timed deadline, a manual session's Stop, a real child
process ending its own session, a rejected start time standing in for PID reuse,
selection-key parsing, the command label's argument exclusion, duration bounds
and unknown operations. They never acquire a real inhibitor: a fixture session
carries no descriptor, which is what a released one is.

`tests/caffeinate.js` runs the production store methods and the shared native
projection, then checks the conditional bar item, the shared store and the
panel's Stop in the real `shell.qml`. `tests/vicinae-caffeinate.cjs` drives the
actual launcher component against controlled native replies: duration text
reaches the service unparsed, a task starts from its key, builds and transfers
precede other processes, a refused start only shows its native message, and two
rapid Stops make one call.

Actual inhibition still needs a configured desktop. On `nerv`, start each mode
and confirm that the 1,800-second lock and 1,860-second display-off do not fire,
that `loginctl list-inhibitors` shows exactly one `idle`/`block` lock named
Seele Caffeinate, that `Super + L` and an explicit Suspend still work, and that
releasing the session restores the normal idle policy.
