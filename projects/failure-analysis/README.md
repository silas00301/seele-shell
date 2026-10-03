# Native failure analysis

Four Rust executables share one library: `seele-failure-report` collects and
privately presents a failed operation, `seele-rebuild` preserves `nh` progress and
status, `seele-rb` runs the reviewed rebuild workflow below, and
`seele-failure-generator` adds the systemd failure reporter drop-in.
The parent feature supplies executable paths, installs the existing Neovim viewer
and declares the notification/window/service integration. No Python or Pi runtime
is installed by this feature.

Collection uses the failed systemd invocation ID only. Immutable `MONITOR_*`
metadata takes precedence over a service that restarted before collection. Missing
identity never falls back to the unit's entire journal. At most three derivations
named in those messages receive `nix --offline log`; Nix is never installed or
fetched. Kernel warnings are requested only when invocation messages suggest a
kernel failure, and only within that invocation's timestamp window plus one
second at each end.

Reports cross from root to the desktop user through a bounded stdin JSON envelope
and `runuser ... env -i`, carrying only the known desktop environment. Raw reports
stay in the user's private runtime tree, with exclusive random names and mode
0600. Opening or appending validates the actual descriptor: regular file, matching
UID, mode 0600, no symlink and no hardlinks. Appends have a nonblocking lock and
size limit. Reports expire after 24 hours; the directory admits at most 1,024
retained reports. Output, stdin and JSON envelopes all have byte limits. Errors
never include arbitrary exception strings or captured environment values.

Notifications retain the existing actions, expiry and viewer. Doing nothing or
choosing **See error** performs no inference. **Analyze with AI** sends a redacted
copy to the shared native Codex broker, which owns model choice, the empty tool
set, schemas, retries, limits and cancellation. The sentence naming Pi now names
AI to reflect this backend change. No other presentation, transparency, motion or
window geometry changes. The broker sees a fixed safe activity label and at most
112 KiB of cleaned, redacted report tail. It never receives the original report,
host identity, repository path or source environment.

`seele-runtime::redact` owns common credential patterns, including incomplete PEM
blocks, quoted assignments, headers, command flags, URL credentials, JWTs and
common provider token prefixes. Failure analysis additionally removes known
secret environment values and local identities. Matching runs before truncation.
`seele-runtime::inference` owns submit/wait/release and cancellation cleanup for
all synchronous consumers. A bounded submit acknowledgement is shielded from
cancellation so its accepted job can still be cancelled by ID.

The rebuild wrapper inherits terminal input, merges stdout and stderr at the OS
pipe and forwards each byte unchanged, including carriage returns, ANSI progress
and UTF-8. Shared process ownership handles cancellation, terminal foreground
handoff, child reaping and a 24-hour deadline. Only the last 64 KiB is retained;
a failed notification cannot replace the original rebuild exit status. Successful
rebuilds create no report.

`seele-rb` (`rb` in Fish, SIL-25) is the check-build-diff-activate workflow.
It records the Jujutsu working copy with `jj log -r @`, which snapshots it and
prints its change and commit, and never stages, describes or rewrites anything.
It then runs `nix flake check --no-build --no-write-lock-file` and `nh os build
--diff never --out-link` into a private runtime directory, and shows `nvd diff`
against `/run/current-system`. A build identical to the running system stops
there. Activation hands `nh os switch` the built `/nix/store` path itself, which
nh treats as a store installable without evaluating anything, so the generation
activated is exactly the one diffed. `--dry-run` never activates, `--switch`
activates without asking, and the default asks on a terminal and activates only
on an explicit yes; without a terminal it never activates. Every step streams
unchanged. Success sends one transient notification, and a failed step stops
before activation and goes through the same private report and consent
notification as `seele-rebuild`, naming the step and command. The flake comes
from `--flake`, then `NH_OS_FLAKE`, then `NH_FLAKE`. Generation cleanup stays
with `nh clean`.

Validation from the submodule:

```sh
cargo test --manifest-path projects/failure-analysis/Cargo.toml
cargo clippy --manifest-path projects/failure-analysis/Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path projects/failure-analysis/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 projects/failure-analysis/tests/protocol.py target/debug/seele-failure-report
```

The production executable fixture supplies fake tools and a private broker. It
checks consent, redaction, local viewing, private files, invalid inputs, raw rebuild
bytes and exact exit status, and systemd mask/precedence handling. Rust tests also
exercise invocation selection, restart races, Nix/kernel bounds and unsafe file
rejection. Shared runtime tests cover secret syntax, interruption during broker
submission, bounded process I/O and cleanup. The fixtures never activate a host,
run a real rebuild or contact a model.
