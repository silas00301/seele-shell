# Linear capture drafts

`seele-linear-capture [path|connect|disconnect]` is a Linux native explicit
wizard, not a resident scraper. `linear.rs` owns wallet access, draft collection,
selected metadata, exact Seele issue lookup, project/team selection, final
review, fixed GraphQL requests, signed PUT and issue/comment creation. Shared
integration process management owns bounded native dialogs, cancellation and
clipboard handoff. Secrets and draft text remain off command arguments.

PNG and MP4 inputs are opened without final symlinks, validated as owned regular
single-link files, bounded to 128 MiB, and copied to memory with modification
checks before review. Remote errors are static. GraphQL responses are bounded,
redirects refused, and only known HTTPS storage origins accepted. API keys are
never attached to a storage PUT. No retry or external action occurs before
final Upload to Linear; read-only destination lookup follows drafting.

Tests use a private loopback HTTP origin compiled only into tests, synthetic
keys and fake native dialogs/wallet. Production has no endpoint override. Run:

```sh
cargo test -p seele-integrations --lib linear::
python3 projects/integrations/tests/linear.py target/debug/seele-linear-capture
```

The screenshot helper's explicit `linear` mode annotates and saves first, then
hands one published path to this wizard. The parent owns its shortcut and
wrapper dependencies. Existing recordings enter through the file picker; no
folder is watched. See parent `docs/linear-capture.md` for setup, retention and
remaining live acceptance boundaries.
