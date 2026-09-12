# Native integration services

This crate builds two independent Rust executables over one shared library:

- `seele-home-assistant status|set ENTITY on|off|watch` owns connection metadata,
  Secret Service credentials, HTTP/WebSocket I/O, the entity projection and
  per-device confirmed controls.
- `seele-transfers serve|request|select FILE...|watch` owns the private desktop
  transfer service and its provider-neutral protocol. The Taildrop adapter is
  the only module that knows Tailscale LocalAPI routes or progress fields.

The QML components, palettes, layout, animation curves and material/transparency
remain unchanged. The binary protocols retain the existing JSON field names,
command arguments and notification actions. Python is used only by local
black-box test fixtures; neither executable starts a Python interpreter.

## Boundaries and execution

Each executable has two Tokio reactor threads. Streaming file I/O, WebSocket
receives and independent transfers run concurrently without allocating a thread
per request. Disk transfer buffers are 256 KiB, HTTP/WebSocket messages are capped
at 2 MiB, and UI input is bounded before allocation. Home Assistant keeps only
an allowlist of state attributes, at most 16,384 entities, 32 selected devices
and 32 queued service calls. Transfer limits are eight active jobs, 32 socket
clients, 16 notification action waiters, 256 files per selection, 4,096 history
groups and 8 MiB of persisted metadata. Reaching a limit returns a typed error;
failed history is never silently dropped to make room.

`home_assistant/connection.rs` owns authentication, subscriptions, heartbeat and
reconnect backoff. `home_assistant/live.rs` is the single owner of preferences,
UI requests and pending changes. Transport messages carry a generation so a
replaced connection cannot alter its successor. A service success is not a
device acknowledgement: completion also requires the reported device state.
Only selected entity events, catalog updates and the 20-second health heartbeat
publish display snapshots.

`transfers/model.rs` owns metadata; `service.rs` owns lifecycle and explicit
file actions; `provider.rs` streams the local daemon protocol; `files.rs` owns
file identity and descriptor-relative publication; `mod.rs` owns bounded Unix
IPC. Source device/inode/size/timestamps guard retries against changed files.
These identities are metadata, not content hashes, and remain service-side.

Private configuration and history use the shared `seele-runtime` durable atomic
writer. Subprocesses use its bounded process-group owner, with an async bridge
that also cancels when its future is dropped. The stdin and stdout adapters use reactor
readiness rather than uncancellable background I/O threads. Unchanged history
is not rewritten, and unchanged transfer snapshots are suppressed between
20-second heartbeats. EOF, SIGINT and
SIGTERM shut down connection workers and reap keyring/desktop/notification
helpers. No native machine activation is performed by this crate.

Transfer destinations must be owned by the current user and not writable by
other users or groups. A receipt is streamed into a private random temporary
file, synced, then published with an exclusive hard link to a numbered name.
The directory is synced before the delivered path is journaled and before
acknowledging the daemon. Graceful cancellation removes the private incomplete
file. An abrupt SIGKILL or power loss can leave a private `.seele-*.part` file;
the service does not guess which pre-existing files are safe to delete.

## Validation

```sh
cargo test --locked --manifest-path projects/integrations/Cargo.toml
cargo clippy --locked --manifest-path projects/integrations/Cargo.toml --all-targets -- -D warnings
cargo build --locked --manifest-path projects/integrations/Cargo.toml
python3 projects/integrations/tests/home_assistant.py target/debug/seele-home-assistant
python3 projects/integrations/tests/transfers.py target/debug/seele-transfers
```

The Home Assistant fixture needs development-only `aiohttp`; the transfer
fixture uses Python's standard library. Both suites invoke the real Rust
executables and use temporary keyring helpers, loopback HTTP/WebSockets or a
private Unix HTTP daemon. They never open a user's connection or wallet, send a
real Taildrop transfer, or activate the desktop. Visual validation remains in
the existing QML production-function and headless-render tests.
