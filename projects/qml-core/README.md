# Native QML policy

This Rust library owns the shell's pure data algorithms. `Seele.Core` is a small
Qt plugin that validates and converts primitive arguments and returns values
through one bounded C ABI. `projects/shared/Native.js` supplies the Qt binding.
The matching files beside QML scenes project QObject properties and forward
calls; they contain no second implementation of the policy. The C++ bridge validates
incoming Qt values before expansion, then decodes Rust's bounded JSON response
once through the engine's captured built-in JSON parser. It returns engine-owned
`QJSValue` arrays and objects, preserving `.filter()`, `.map()`, identity and own
`__proto__` data properties. Ordinary QVariant sequence wrappers fail those
JavaScript contracts; the real Qt regression covers them explicitly.

Notification state uses a separate opaque Rust object owned by a Qt QObject.
The Qt engine releases it with the JavaScript wrapper. There is no process-wide
state registry, socket, worker process, or event log. Timer calls contain only a
timestamp; the Rust object returns compact effects and DND properties. Text is
exported only for a published view or the memory-only reload snapshot. The Qt
adapter retains actual notification objects and invokes advertised actions,
dismissal, expiration and scene callbacks after the Rust mutation completes.

Notification admission limits one entry to 256 KiB of conservative escaped JSON
weight and all retained entries to 4 MiB. At most 4,096 current entries and 100
history entries are retained. An oversized notification is dismissed; under a
flood, old history is released before the oldest current notifications. These
limits do not change ordinary toast, pin, action, stack, replacement or timed
DND behavior. Reload metadata is projected to known scalar fields before copying.
Notification text, verification codes and history never reach disk.

Health's Rust policy validates private metadata and typed actions, derives stale
state, and groups priority rows. Its thin Qt wrapper uses `localeCompare` for
actual Qt locale collation, retaining registration order for equal labels.
GitHub's UI and native producer share one strict pull-request URL validator.
Notification verification, images, markup and action ordering share the same
native functions used by the state machine.

Media normalization, mirrored-player selection, playback-rate choices, volume
limits and bounded keyboard seeking share Rust policy. The Qt adapters take
small snapshots appropriate to each query and return the original selected
player object. The real Quickshell media fixture verifies that enum singleton
values are projected without sending a QObject through the ABI. Home Assistant's room/favorite projection and preference ordering
run when source data changes; catalog merging uses indexed deduplication.
Transfers' progress projection, strict local URL decoding and action admission
are native, with one batched durable seen request when opening the panel.

`ListModels.js` is the single Qt adapter for matching stable row contracts in
Home Assistant, Transfers, Maintenance and AI Activity. The Rust planner receives
only IDs and computes the original insert/move sequence in O(n log n); actual
role values, unchanged payloads and delegate identity stay with Qt. System status
passes a known-field/type schema before Qt retains unchanged branches. Agent,
capacity, Bluetooth and battery presentation use projections tied to their
source properties; palette colors and scene geometry remain Qt bindings.

Tests use `seele-qml-functions`, a fixture CLI running the same functions. Its
bounded notification replay mode exists only for synchronous Node fixtures;
production Qt never replays or stores events. The existing health, GitHub and
notification assertions still exercise the production wrappers and QML methods.
`tests/tst_nativefunctions.qml` exercises the actual plugin, while Rust tests
cover strict ABI envelopes, allocation/free, isolated state objects, restoration,
callback ordering, flood bounds and object cleanup.

A local release benchmark is available with:

```sh
cargo test -p seele-qml-core --release state_snapshot_cost -- --ignored --nocapture
```

It reports measurements rather than asserting machine-dependent timing. On the
validation machine, a 100-entry timer roundtrip fell from 674 microseconds for
full snapshots to 2 microseconds with the resident state; 1,000 entries fell from
6.39 milliseconds to 4.2 microseconds. These measurements include Rust JSON
input/output and exclude Qt conversion and rendering. They are not whole-shell
performance claims.
