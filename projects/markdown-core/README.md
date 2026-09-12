# Markdown source formatting

`seele-markdown-core` owns Notes' block grammar, inline precedence, claimed
intervals and UTF-16 source offsets. The small bridge in `projects/markdown`
retains the Qt APIs that own `QSyntaxHighlighter`, QML registration, theme colors
and undoable `QTextCursor` editing. The parser never rewrites note text.

Backtick runs are indexed once; claims use disjoint ordered intervals. The
remaining fixed patterns compile once through PCRE2, the same regex engine used
by Qt. The narrow native boundary accepts only validated Rust strings and
character-boundary offsets, avoiding repeated whole-subject UTF checks. Compiled
patterns are immutable; every match owns its own context and capture allocation.
There is no shared mutable Qt state or process-global regex match context.

Inline work stops above 128 Ki UTF-16 units per block, after one million regex
callouts, or after the cooperative 25 ms matching deadline on blocks larger
than 4 Ki units. Native match/depth/heap limits apply as well. A rejected block
retains structural formatting and editable source; incomplete inline formatting
is discarded. These are deliberate limits for pathological input, not changes
to the document bytes. Oversized blocks do not allocate the per-byte offset map.

Validation from the workspace root:

```sh
cargo test -p seele-markdown-core --locked
cargo clippy -p seele-markdown-core --all-targets --locked -- -D warnings
cargo bench -p seele-markdown-core --bench highlight --locked
nix build .#notes --no-link --no-write-lock-file
```

The QML module's CTest check compares every Qt character-format property and
block state against 32 reviewed fixtures from shell revision
`d16edb8851989948cb8a45b5b6be9934e6732dc1`. The migration additionally compared a
deterministic 3,017-document Unicode/malformed-input corpus with the original
compiled C++ renderer. The real Notes editor fixture checks caret movement,
typing, list continuation, selection, undo/redo, source preservation and pixels.
The C ABI returns owned span storage, freed exactly once by the Qt bridge.

The benchmark isolates parsing from Qt layout. It includes large unmatched
backticks, dense code/wiki spans and unclosed emphasis. Do not interpret its
numbers as complete editor or desktop frame latency.
