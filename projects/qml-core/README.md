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

The notification panel can silence future toasts for one application from its
stack's bell button, in Current or History. Existing toasts, notification actions,
inbox entries and sender lifetimes are preserved; new and replaced messages still
enter the inbox. The same application key as stacking is used (desktop entry,
then application name); anonymous senders cannot be silenced. Critical messages
follow the existing DND policy and are also suppressed. Explicit pinning still
shows the selected notification. The selected bell resumes that app, and the
header's silence menu can resume every quiet app even after its last entry has
gone. Resuming never replays suppressed toasts and never changes global DND.
These choices survive QML reloads in memory, disappear when the shell exits, and
are bounded to 256 application keys of at most 512 bytes each.

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
are native, with one batched durable seen request when opening the panel. The
Audio panel's microphone test resolves its own devices, derives the card's whole
state and owns the microphone-use gate here: the meter reads zero whenever
nothing is being captured, clipping is reported from the worker's own sample
measurement rather than from the bar, a lost microphone or test output is named
instead of being replaced, and both test modes reach the same confirmation.

Focus timers keep their one retained state and absolute deadline in native policy.
Custom input accepts whole minutes from 1 through 240. The +5 action extends a
running or paused session only when the whole extension fits the four-hour total;
it preserves pause and completes an expired deadline before considering extension.
Idle and completed timers cannot be revived by extension. `tests/focus.js` and
native unit tests cover parsing, suspend, rounding and lifecycle bounds;
`tests/focus-timer.sh` instantiates the production panel and retained store in
Quickshell. `tests/focus-panel-offscreen.py` is an optional PySide6 render and
keyboard fixture using the real Rust policy CLI with substituted host objects.

The port inspector's proposed URL, refusal wording, row summary and bounded
action queue are native. A destination is accepted only as the kernel's own
rendering of an address, already bracketed when it is IPv6, and only `http` and
`https` are ever proposed, because a port number is proof of neither. Every
refusal has wording that does not read as a success.

`ListModels.js` is the single Qt adapter for matching stable row contracts in
Home Assistant, Transfers, Maintenance, AI Activity and Ports. The Rust planner receives
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

The calculator's bounded Pratt parser and dimension-checked conversion policy
also live here. Preview and commit share one evaluation path; commit projects a
maximum of 32 tape entries and the last finite answer. `CalculatorPanel.qml`
retains that state only while its Loader is alive. See
[`CALCULATOR.md`](../shell/CALCULATOR.md) for grammar, precision and privacy.
