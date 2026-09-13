# Native workspace

Seele's first-party runtime services and command helpers are built together by
the root Cargo workspace. Each executable has a thin entrypoint. Libraries own
reusable behavior; a service owns state only when several clients need the same
live state. Do not add a subprocess or socket between two functions merely to
share code.

The parent Seele flake consumes this repository through its pinned path input.
`packages/core/native.nix` is the common package builder. There is one root
`Cargo.lock`, one release profile, and one source fileset derived from the Cargo
workspace members. New crates must be workspace members before packaging. The
workspace declares Rust 1.97 as its minimum compiler version, matching the pinned
package sets; all crates inherit that baseline. Test newer toolchains without
introducing unsupported syntax or compiler-only lint attributes.

## Ownership

| Crate | Owns | Main consumers |
| --- | --- | --- |
| `runtime` | Private files, bounded framing and subprocesses, cancellation, inference protocol, Codex isolation, canonical GitHub URLs | All native services and command helpers |
| `broker` | Model selection, schema validation, bounded concurrency, retries, cancellation and supersession | Maintenance, Fish assistance, explicit application AI actions |
| `prompt` | Resident prompt state, consented context, capture lifecycle and resumable Codex turns | Shell prompt |
| `maintenance` | Sanitized findings, publisher ownership, snooze and repair policy | System Health |
| `failure-analysis` | Failed-invocation journal collection, local report presentation and explicit AI consent | Systemd reporter and rebuild wrapper |
| `shell-ai` | Fish context, private stderr ring, generation/debug requests and reviewed insertion | Fish Enter binding and history picker |
| `integrations` | Home Assistant connection state and transfers | QML stores and extension clients |
| `tools` | Desktop status, audio routing, Bluetooth, Notes storage, launchers, URI recognition and host adapters | Shell, Notes, lock, greeter, polkit and extensions |
| `config-tools` | Portable config materialization, direct launch, backup, catalog, input report and text projection | Parent flake helpers |
| `desktop-tools` | Screenshot consent/publication, terminal Spotify, Brave preferences and the existing Windows reboot service | Desktop bindings |
| `repo-tools` | Repository workflows, bounded Pi revision reads, packaged release updates and reviewed generation switching | Parent flake commands, Pi and Vicinae |
| `qml-core` | Pure display/editor policies, notification state and terminal footer layout | In-process `Seele.Core` Qt module and Node-API binding |
| `markdown-core` | Markdown block state and UTF-16 format spans | In-process `Seele.Markdown` Qt module |

Domain documentation lives beside its implementation. See
[`runtime/README.md`](../projects/runtime/README.md),
[`tools/README.md`](../projects/tools/README.md),
[`qml/README.md`](../projects/qml/README.md),
[`markdown-core/README.md`](../projects/markdown-core/README.md), and the
parent's broker, maintenance, failure-analysis and shell-ai protocol guides.

## Process and data boundaries

Use `runtime::process` for external programs. Specify a byte limit, deadline and
cancellation owner. Ordinary children belong to an owned process group and are
reaped. An explicitly daemonizing clipboard or GUI launcher uses the separate
successful-handoff path. Locks require the compositor's secure acknowledgement;
failure to receive it must never kill an already handed-off lock process.

Use the shared private-file and framing helpers instead of duplicating path,
permission, socket or JSON-line code. Requests and retained queues need separate
bounds: bounding one message does not bound the number of pending messages.
Publication must distinguish private state from ordinary vault content and
distinguish replacing owned state from creating a new file without overwriting
an external writer. Keep private text out of process arguments, diagnostics and
persistent state unless that component explicitly owns it.

The Codex broker is a private Unix-socket service because model policy and
concurrency must be shared across integrations. Payloads and results remain in
memory. A separate prompt controller owns its panel's resumable session. Both
use `runtime::codex` for the approved private configuration and authentication
refresh contract. No service may silently fall back to inherited instructions
or a different model when isolation configuration is unavailable.

## Qt and host bindings

`packages/core/quickshell.nix` owns the shared pinned Quickshell host for every
UI package and development fixture. Its local NetworkManager patch validates
wire modes before narrowing and returns `Unknown` for unrecognized values.
Patch the upstream `unwrapped` derivation and preserve its Qt wrapper; changing
only the outer wrapper does not rebuild the executable. The build runs
`tests/quickshell-network-mode.py` against the actual patched enum and function
bodies with undefined-behavior sanitization. Keep dependency fixes here rather
than adding separate overrides to individual applications.

The visual tree, dimensions, materials, font tokens, transparency and animation
declarations remain in QML. `projects/shared/Theme.qml` owns material and motion
tokens; `Palette.js` is the single palette fallback and property-assignment
binding for all Qt clients. `Seele.Core` calls Rust in the same process; the
diagnostic `seele-qml-functions` executable is not used by the running UI. Results are
decoded once with a captured engine-local JSON parser and returned as `QJSValue`,
so nested arrays support ordinary JavaScript methods and prototype-like keys
remain own data. QVariant sequence wrappers are not interchangeable with arrays;
the actual Qt fixture checks this boundary, including notification results.
`projects/node/` exposes the same canonical C ABI through Node-API for Pi;
terminal width and theme callbacks remain in the host, while layout and
sanitation policy run in Rust without a render-time subprocess.

Keep QObject identity, signals, callbacks, Qt locale/date conversion and actual
property setters at the Qt boundary. Project plain data once when possible and
return indexes when a result must retain an existing object. Rust editor commands
use UTF-16 offsets and become one Qt edit block, preserving the undo stack.
`Seele.Markdown` leaves document mutation, font layout and painting to Qt while
Rust returns ordered format operations. Never reserialize a Markdown document
through a rich-text editor as a substitute for preserving its source.

Vicinae's React extension and the Pi/OpenCode extension APIs require host-language
callbacks. Keep those adapters small and move shared state, subprocesses,
publication and nontrivial policy to native endpoints. Avoid spawning a helper
from each render or animation frame. Development Python, Node, shell fixtures,
Nix expressions and upstream applications are separate from first-party runtime
services; removing their interpreters from the host globally is not this
workspace's package contract.

## Performance policy

Use measured bottlenecks. The release profile uses thin LTO and one codegen unit;
it does not bake in the build machine's CPU. Recognition dispatches SIMD only
after detecting the required CPU features and keeps a bit-exact scalar path.
OCR uses a bounded worker pool with one thread per engine to avoid nested
oversubscription. Independent context collection and I/O use bounded workers;
small state transitions stay synchronous.

Long-lived hardware listeners and metadata caches avoid repeated probes and
unchanged publication. Keep idle notification ticks native rather than moving
all notification text across an ABI every 250 ms. Bounds, cancellation and
latency under adversarial input matter alongside average throughput.

Reproducible benchmarks include:

```sh
cargo bench -p seele-tools --bench grayscale
cargo bench -p seele-markdown-core --bench highlight
```

Report the CPU, build profile, input sizes and what each timing includes. A
parser or pixel-conversion speedup is not a whole-shell speedup. Preserve golden
behavior and pixels before interpreting a faster result as an improvement.

## Validation

Inside the existing development environment:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo build --workspace --release --locked
```

Set `SEELE_TEST_CODEX` and `SEELE_TEST_FZF` to the packaged executables to enable
their actual-process gates. Codex fixtures must provide their own private
synthetic HOME and authentication, dead proxies outside their loopback endpoint,
and fake service data. Never point a fixture at a real credential directory.

`test-shell` and package install checks run native protocol fixtures, retained
JavaScript behavioral assertions through the same Rust functions, Qt editor and
pixel tests, and a full configuration compilation with real Quickshell on a
private headless compositor. Installed-layout checks load shared test adapters through explicit paths, so a
fixture cannot accidentally rely on sibling checkout files. The Markdown package
also compares semantic Qt
format properties and block states against pre-migration golden documents.

The parent repository additionally requires formatting, flake evaluation and
native-host builds documented in its `AGENTS.md`. If Nix or the matching host is
unavailable, record that boundary explicitly; standalone Cargo and Qt checks do
not replace Nix evaluation, closure inspection, compositor validation or live
hardware tests. Do not install Nix against the user's instruction and do not
activate a host as a side effect of validation. Unpublished submodule edits and
a parent gitlink that still names the baseline are not rebuild-ready inputs;
follow the repository's authorized publication and lock workflow separately.
