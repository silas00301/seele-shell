# Shared Codex inference

`seele-codex` uses the mode-0600 `%t/seele-codex.sock` user socket. Systemd owns
that socket across broker restarts. `serve` accepts only that inherited socket
and verifies each client's uid. It exits after five idle minutes, and systemd
restarts failures. Any same-user integration may submit; no registration or
consumer credentials are needed. The `nerv` profile installs it.

The CLI reads one JSON value on stdin and writes one JSON value on stdout:

- `seele-codex request`: low-level asynchronous protocol.
- `seele-codex call`: submit, wait for the same job lifecycle, then release nonfailed work.

A request example:

```json
{"op":"submit","request":{"consumer":"github","label":"Notification triage","prompt":"Classify the supplied notification.","context":{"title":"Example"},"input":{"version":"1","schema":{"type":"object","required":["title"]}},"output":{"version":"1","schema":{"type":"object","properties":{"priority":{"type":"string"}},"required":["priority"],"additionalProperties":false}},"class":"background","item":"notification-1","revision":1}}
```

`call` takes the inner `request` object. The only scheduling classes are
`interactive` and `background`. Input/output schemas use JSON Schema 2020-12;
inline schemas only (references and retrieval are disabled). Each schema has a
consumer-owned version, a 64 KiB/4,096-node/depth-32 bound and a compiled validator
reused across attempts. Patterns use a linear-time regex engine: lookaround and
backreferences are rejected. Each schema permits at most 32 patterns, with 64 KiB
compilation and DFA budgets per pattern. Context and results are limited to
16,384 nodes and depth 64. Numeric literals have at most 128 characters and
exponents in -308..308, preventing small JSON inputs from causing unbounded
arbitrary-precision arithmetic allocations.
Message size is 256 KiB,
queue capacity is 128 retained jobs, and concurrency defaults to two (1–8).
Consumers provide a deliberately safe label, never a label derived from private
content. Model selection belongs exclusively to `seele.codexBroker.model`.

Every reply carries `ok` and an `epoch`. Submit returns `job.id`. All subsequent
job operations must include that id and epoch. `broker_restarted` means the
consumer must resubmit from its own durable source; broker jobs are memory-only.

`configuration` accepts only `op` and returns `{ok, epoch, model}` without
creating or listing jobs. The prompt panel queries it on explicit Send and pins
that model for its current session.

Operations are `list`, `status`, `wait`, `cancel`, `retry`, `next`, and `release`.
`list` needs no epoch and returns metadata only, in running/retrying, actual
queue, then terminal order. `status` and `wait` expose a result only after output
validation succeeds. The states are queued, running, retrying, succeeded, failed,
cancelled, and superseded. Typed failures include invalid_input, capacity,
superseded, invalid_state, unknown_job, broker_restarted, broker_unavailable,
model_failure, runtime_failure, isolation_failure, and authentication_unavailable.

Same-consumer item revisions are monotonic integers. New revisions supersede
queued and active older jobs; obsolete results are never delivered. Monotonic
revision watermarks survive release for this epoch. The table holds at most
8,192 distinct consumer/item pairs and then rejects new pairs with `capacity`
until the next broker epoch; it never evicts a watermark to accept an obsolete
revision. Disconnecting
a submitter does not cancel work. Cancel also stops schema retries. Transient
failures retry twice with bounded backoff; invalid output regenerates at most eight times; the eighth failure ends with
`invalid_output`. Isolation and authentication failures end immediately without retry. These
resource limits replace the previous unbounded regeneration loop. Retry reuses a failed request unchanged. Next promotes
one queued job without interrupting active work or changing its scheduling class.
Release discards a terminal request and result. A bounded metadata-only tail
keeps successful, cancelled, and superseded confirmations visible for five seconds;
dismissing a failure removes it immediately. Consumers must release completed
jobs to recover capacity; failures remain available for an explicit retry.

Codex owns authentication. Each attempt gets private empty HOME/CODEX_HOME
roots and delegates only an existing private, same-user, single-link `auth.json`
through a symlink. Codex's file-storage refresh writes through that link to its
existing store; Seele never copies credential bytes. Missing, unsafe, and keyring-only
credentials fail closed with `authentication_unavailable`; log in using Codex's
file credential store before using these integrations. The source contract and
synthetic refresh fixtures must be reviewed when updating Codex.

Each attempt runs `codex exec --ignore-user-config
--ignore-rules --ephemeral --sandbox read-only` from an empty private runtime
workspace. The complete installed feature set is disabled, except the flag that
skips host skill discovery; image tools, web search, and project instructions
are also disabled. The output schema lives in an immutable sealed anonymous memfd inherited only by
its owning attempt. Feature discovery uses its own empty HOME/CODEX_HOME. Prompts travel
on stdin, and JSON events/results remain in memory. Logs are disabled and the
private runtime tree is removed after every attempt, including cancellation.
No integration environment variables are inherited. The broker never logs
exception text or payloads. Operational timing, model, attempts, outcome and
usage are available through metadata.

The Tokio reactor has two I/O threads, a bounded blocking pool and one model
worker per configured slot. Schema compilation and validation run off the I/O
reactor; the scheduler mutex never covers model or validation work. Up to 256
clients may connect, with ten-second frame/write deadlines and strict framing.
The 512 KiB reply budget covers validated results and the bounded metadata tail.
All clients and the activated listening socket are verified as same-user.
The shared `seele-runtime` library owns deadline/cancellation-aware process groups,
concurrent pipe I/O and authenticated Unix-socket RPC. Health authentication output
goes directly to `/dev/null`, never into a captured buffer or health payload.

Validation, from the submodule:

```sh
cargo test --manifest-path projects/broker/Cargo.toml
cargo clippy --manifest-path projects/broker/Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path projects/broker/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 projects/broker/tests/protocol.py target/debug/seele-codex
PYTHONDONTWRITEBYTECODE=1 python3 projects/broker/tests/codex_loopback.py target/debug/seele-codex
```

Python is only a fixture driver; both installed executables are native Rust.
The protocol fixture drives production socket activation and real subprocesses,
verifies environment isolation, immutable schemas, cleanup, cancellation, limits
and health IPC without using an account or model. The loopback check uses the
real packaged Codex with a local fake model endpoint and proves its tool list is
empty and hostile global instructions stay outside the request. Exit 77 identifies an installed Codex missing mandatory isolation flags;
package checks require a supported Codex rather than weakening those flags.
Run that check whenever the packaged Codex changes. No fixture performs remote
inference. QML, rendering, transparency and animation are unchanged.
