# Prompt controller

`seele-ai-prompt` owns the existing QML prompt's context access, Codex turn
lifecycle, clipboard copy and validated window insertion. QML retains layout,
animations, transparency, input bindings, source confirmation and preview UI.
The resident Rust process reads and writes the existing newline-delimited JSON
protocol. Four workers and bounded queues keep subprocess waits away from its
message loop; shared runtime code enforces subprocess groups, output limits,
cancellation and transport deadlines.

Opening the panel invokes no model and reads no clipboard, selection or screen.
Context is collected only through explicit preview requests. Clipboard and
selection require the matching permission at submission. Screenshots are private
runtime files tied to the pinned output; a new preview invalidates the preceding
value before collection, and a failure cannot submit old context. A terminal
class alone is insufficient for directory lookup: the PID must belong to this
user and identify a supported terminal executable. Process discovery is bounded.

Each model turn runs in a private empty workspace with the shared Codex
feature-disable policy and read-only sandbox. The broker supplies the configured
model on explicit Send; follow-ups keep that model for the panel session. Follow-ups resume a validated session UUID only within
one open panel. Closing, reopening and termination invalidate outstanding work,
remove captures, terminate active subprocess groups and delete the session,
including a session first announced by a subsequently cancelled turn. Cleanup
uses bounded retries; an unavailable Codex deletion command can still prevent
remote session deletion, so no stronger deletion guarantee is claimed.

Copy and insert jobs snapshot the answer and window identity when accepted and
check that generation before performing an action. Insertion revalidates the
original Hyprland client and focused window and rejects terminal controls.
Clipboard copy explicitly allows wl-copy's successful background owner to remain
alive; cancellation and failed parent processes still receive group cleanup.

`cargo test -p seele-prompt` exercises real controller processes with local fake
Codex, compositor and clipboard commands. Tests cover opening without collection,
explicit source assembly, failed preview invalidation, session resume, copy and
insertion input, screenshot cleanup, and close/termination during the first turn.
No test contacts a model account or alters the real desktop. Native compositor
and visual comparison remain necessary to validate actual host rendering.

The optional real-CLI gate runs with
`SEELE_TEST_CODEX=/absolute/path/to/codex cargo test -p seele-prompt --test controller real_codex_first_and_resumed_image_turns_have_no_tools -- --exact`
at the workspace root. It creates only synthetic file authentication, uses private
HOME/CODEX_HOME and an unauthenticated loopback provider, and guards other network
routes with a closed loopback proxy. On Codex 0.153.4 and the broker's configured
`gpt-5.6-luna`, first and resumed image turns expose an empty tool set and exclude
hostile original instructions/configuration; the synthetic MCP process never
starts. Model or CLI changes require this gate again.

Inference uses a private HOME and CODEX_HOME for each open panel. Only the existing
owned, mode0600, regular, single-link auth.json is delegated; credentials are not
copied into the private context or read by the controller. Codex refreshes that
same existing file through its symlink, matching the public 0.153.4
[FileAuthStorage implementation](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/login/src/auth/storage.rs).
The context, logs and local session files disappear after closing and bounded
worker cleanup. Shared runtime tests verify synthetic refresh and unsafe-file
rejection. Keyring-only authentication currently fails closed with an actionable
account diagnostic; it is not silently replaced by inherited user configuration.
