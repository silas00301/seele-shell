# Fish command assistance

`seele-shell-ai` is the Rust implementation behind Seele's unchanged Fish Enter
binding. Leading `how` and `debug` requests produce text for the existing prompt
insertion path. All other Enter presses use Fish's normal execute action. No
suggestion is executed. Multiple reasonable suggestions still require a choice
in the same fzf picker, and destructive suggestions are inserted as comments
requiring deliberate uncommenting.

The crate separates private capture (`capture`), bounded local metadata
(`context`), strict generation/selection policy (`suggestions`), and CLI routing.
It uses the shared `seele-runtime` for peer-verified framed IPC, process ownership
and deadlines, secret redaction, and the Codex broker client lifecycle. Neither
Python, Pi nor Node is part of this package's runtime. Inference uses the active
Linux Codex broker's model selection, queue, retries and cancellation.

## Private capture

Only plain interactive Fish processes are wrapped; script and `-ic` invocations
retain their original semantics. The wrapper preserves login mode, shell nesting,
stdin/stdout and foreground terminal behavior. Its separate stderr PTY forwards
exact bytes and window-size changes. It ignores terminal interrupt/quit itself
so Fish receives them normally, forwards termination/hangup, owns and reaps Fish,
and cleans up the session even if Fish closes stderr before exiting.

The resident wrapper stores at most 64 KiB of current stderr plus one sanitized
failure record in memory. `begin`, `finish`, and explicit `debug` communicate over
a mode-0600 Unix socket in a unique mode-0700 runtime session directory. No command
output, failure record or diagnostic is written to disk. Successful commands keep
the last failure; a later failure replaces it. Two bounded control workers enforce
same-UID peers and absolute deadlines without blocking terminal output. Finish
briefly waits for already-written PTY bytes and checks the capture generation so
a stale completion cannot overwrite a newer command's capture.

This replaces the previous per-chunk file read, temporary write and fsync with a
bounded in-memory ring. State disappears when the owning Fish session exits.

## Explicit generation

Context consists of OS/kernel architecture, current directory, at most 32 visible
names, repository kind, a bounded sample of available command names, and boolean
or enumerated development-shell indicators. It never reads file contents, shell
history, environment secrets or project instructions. Sensitive filenames are
excluded. Four bounded workers inspect at most 64 PATH directories and 16,384
entries within a 150 ms scanning budget. Filesystem calls themselves remain subject
to kernel/filesystem stalls; this is not a hard realtime guarantee.

Only explicit `debug` reads the last failure. Secret syntax is redacted before
that command and its stderr reach the broker. Prompt/context data travels through
private sockets, never model argv. Strict response schema and local validation
reject extra fields, multiline commands, controls, format characters and oversized
responses. A conservative local guard overrides the model's destructive flag and
recognizes quoted, escaped and absolute destructive command names. It cannot prove
arbitrary shell semantics safe: review remains required for every insertion.

The picker retains configured fzf visual options, including generated theme colors,
while excluding inherited executable bindings, preview/default commands and option
files. Its fixed command inherits the foreground group and terminal stderr so
`/dev/tty` input and the visible picker continue to work. Shared process capture
still bounds output, reaps the direct
child and honors cancellation; this mode is used only for this trusted interactive
picker with no requested descendants.

## Validation

Run `cargo test --manifest-path projects/shell-ai/Cargo.toml` and
`cargo clippy --manifest-path projects/shell-ai/Cargo.toml --all-targets -- -D warnings`.
Tests cover bounded metadata without file contents, symlinked command availability,
control rejection, classification, in-memory failure lifecycle, exact PTY forwarding,
shutdown after stderr closes, private session cleanup, real broker socket exchanges,
explicit fzf selection and absence of command execution. A parent-checkout assertion
also verifies the unchanged Fish insertion/preexec/postexec wiring when available.
Native Nix and interactive host verification remain required before deployment.

Set `SEELE_TEST_FZF` to a real fzf executable to include the optional terminal
integration test. It allocates a controlling PTY, checks that fzf paints its prompt
and header, then selects the second result through actual terminal key input.
