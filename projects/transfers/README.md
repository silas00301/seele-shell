# Personal transfers

`seele-transfers serve` is the user service behind the Transfers Control Center
module, temporary bar status and panel. `select <file>...` stages original paths
in memory and opens the panel; picking an available device consumes that
selection once. The file picker, drop target, file-manager entries and Vicinae
all reach this same flow. Directories and non-local URLs are rejected.

The user service receives automatically into `SEELE_TRANSFERS_DOWNLOADS`, set to
the Home Manager XDG Downloads directory on `nerv`. Neither the service nor its
file-manager entries are enabled on `asuka`. The existing Tailscale operator
permission gives the desktop user access to the daemon; no root helper or new
protocol is added. iPhone and iPad use Tailscale's own share sheet and Files flow.

## Provider contract

The QML client knows only `version: 1` transfer groups, files, targets and
capabilities. `TaildropProvider` alone owns the Unix HTTP LocalAPI paths and
normalizes daemon events. A replacement provider implements:

- `targets()`: currently available eligible personal `{id, name}` devices;
- `send(target, original_path, cancellation_control)`: completion or typed failure;
- `events(callback, stop)`: normalized outgoing `{target,name,bytes,finished}` and
  incoming `{name,size,bytes}` progress;
- `waiting()`, `receive(name, downloads, control, progress)`, `forget(name)`:
  acknowledge a received file only after its download is durably saved;
- `capabilities`: supported send, receive, resume and cancellation behavior.

The Taildrop adapter intersects `file-targets` with the logged-in user's owner
ID and requires `Online == true`, including again before each send/retry. It
streams each file independently to the daemon's `file-put` endpoint, never to
Seele infrastructure. Retry uses the provider's native resumption when offered
by the receiving platform; otherwise the affected original file restarts. The
service attempts each file at most three times, preserving successful files.
Manual Retry keeps the group and target. Source changes, missing files and
unavailable targets are typed failures, never deferred offline queues.

Progress comes from the daemon's IPN bus, not bytes written to its local socket.
Old finished outgoing events cannot inflate a new transfer's progress. Incoming
network progress is followed by local receipt progress. Receipts use atomic,
mode-0600 exclusive numbered reservations in Downloads, so existing files and
symlinks never get overwritten. Only an interrupted receipt's own incomplete
file is removed. Completed files are never deleted by cancellation or history
cleanup. Move reserves a collision-safe name; Trash uses `gio trash`.

## Protocol and state

`seele-transfers request` reads one bounded JSON request from stdin and reaches
the mode-0600 `$XDG_RUNTIME_DIR/seele-transfers.sock`. Operations are `snapshot`,
`select` (paths), `send` (target), `cancel`, `retry`, `seen`, `dismiss`, `focus`
(group id), and `open`, `reveal`, `move`, `trash` (group id and file index; Move
also supplies a destination directory). Errors are typed, sanitized codes.
`watch` streams snapshots for the shell. The service owns the shared selection,
so repeated clicks cannot start another group after it has been consumed.

History is mode-0600 metadata in `$XDG_STATE_HOME/seele-transfers/history.json`:
filenames, known locations, device labels, direction, sizes, lifecycle and
outcomes. Original paths enable retries without copying contents into a queue.
No file bytes, previews, hashes, indexes or download links are stored. Resolved
entries expire after seven days; active and unresolved failed groups stay.
Clearing history removes metadata only. A service restart turns unfinished
jobs into an explicit retryable interruption instead of silently sending again.

`seele-shellctl transfers` opens the panel idempotently, so simultaneous
notification focus and desktop activation cannot toggle it closed. Explicit
notification activations carry a revision so the same group can be revealed again.

Notifications are incoming completion or any transfer failure, with one Open
Transfers action that focuses the group. Successful outgoing sends remain
quiet. Opening the panel marks incoming groups seen; the bar stays only for
active jobs, unseen incoming files and unresolved failures.

## Taildrop boundaries

The upstream Linux LocalAPI currently does **not** expose a sender identity in
`PartialFile` or `WaitingFile`, nor an operation to cancel an incoming transfer
that the daemon is still receiving from another device. These capabilities are
reported false. Incoming source labels therefore say “Personal device”; Cancel
during that stage instructs the user to stop it on the sender. Local receipt
and outgoing sends can be cancelled. This is a limitation of v1 against SIL-45's
full acceptance criteria, not a claim of unavailable provider functionality.

The APIs are isolated but not guaranteed stable upstream. Their source contracts:
[Taildrop LocalAPI client](https://github.com/tailscale/tailscale/blob/main/client/local/local.go),
[IPN progress types](https://github.com/tailscale/tailscale/blob/main/ipn/backend.go),
and [CLI send implementation](https://github.com/tailscale/tailscale/blob/main/cmd/tailscale/cli/file.go).

## Validation

```
PYTHONDONTWRITEBYTECODE=1 python3 tests/transfers.py projects/transfers/transfers.py
node tests/transfers.js projects/shell/TransfersStore.qml projects/shell/TransfersPanel.qml projects/shell/shell.qml
```

The Python suite includes a real Unix HTTP fake daemon and checks original bytes,
own-user/online eligibility, numbered collisions, file permissions, retry bounds,
cancellation, history and desktop actions. JavaScript executes the production
QML methods for selection, repeated activation, stable rows and notification
focus. The package runs both suites plus the existing production shell compile.
A real `nerv`/iOS Taildrop transfer and native QML/file-dialog validation remain
necessary on a configured desktop.
