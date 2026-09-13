# Native repository helpers

Seven small Rust binaries share argument validation and the common runtime's
bounded process capture, terminal handoff, cancellation and child cleanup. The
parent flake supplies native wrappers and keeps its existing app names and
Jujutsu aliases. The helpers use the caller's existing Nix distribution; none
installs Nix or adds another distribution to PATH.

- `seele-check [--build]` preserves formatter, flake checks, native host evaluation
  and optional build order. It neither updates locks nor activates a host.
- `update-packaged` fetches bounded public CodexBar/T3 Code release metadata,
  validates versions, URLs and hashes before changing either source, and
  atomically replaces each complete pin file. Curl ignores user configuration
  and accepts HTTPS only. A concurrent source edit is rejected. The two files
  are independent atomic replacements, not a filesystem-wide transaction.
- `update-submodule [path]` retains the clean/publication checks and the
  narrowly scoped Git gitlink transaction required by Jujutsu's current submodule
  boundary, then imports that commit, advances the existing bookmark and refreshes
  transitive locks. Relative submodule paths cannot escape the repository; Git receives literal pathspecs. Running
  this publishing helper is a separate intentional action.
  `--pr` requires detached Git HEAD and leaves every bookmark unchanged; set and
  push the PR bookmark separately through Jujutsu. `--pr --keep-lock` also skips
  Nix after checking identical old/new submodule lock blobs and an unchanged
  tracked parent lock in both index and working tree. Review unchanged flake input
  declarations independently before using it: this option does not evaluate Nix
  or prove the package graph builds.
- `jj-flip` resolves both stable change IDs before changing either revision.
- `jj-pr submit|checkout [number-or-url]` retains GitHub's interactive PR creation
  and Gum selection. It validates API branch/repository metadata, quotes Jujutsu
  revision literals, uses exact fetch patterns and removes temporary fork remotes
  on success, failure or cancellation. Picker output is captured while terminal
  input and stderr remain attached to the existing interactive terminal.

Run from the shell root:

```sh
cargo test -p seele-repo-tools
cargo build -p seele-repo-tools
PYTHONDONTWRITEBYTECODE=1 python3 projects/repo-tools/tests/check.py target/debug/seele-check
PYTHONDONTWRITEBYTECODE=1 python3 projects/repo-tools/tests/protocol.py target/debug
PYTHONDONTWRITEBYTECODE=1 python3 projects/repo-tools/tests/submodule.py target/debug/update-submodule
```

Fixtures run the real native binaries against fake Git/Jujutsu/GitHub/Gum/Curl/Nix
commands. They cover command ordering, exit behavior, native-host dispatch,
branch quoting, fork cleanup, dirty/publication gates and invalid release metadata
without repository publication, network access, model requests or activation.
The submodule fixture also uses real Git and Jujutsu in private temporary
repositories with a local origin. It verifies that PR mode preserves every
bookmark and unrelated staged, unstaged and untracked changes, rejects changed
parent locks before mutation, and commits only the gitlink without invoking Nix.

`seele-switch-generation` is the narrow privileged NixOS helper used by the
Vicinae generation picker. It accepts a positive decimal generation and the
reviewed target and running store basenames. These strictly validated identities
are comparison values, never caller-selected executable paths. After `run0`
authentication, it resolves the generation from the fixed system profile and
requires both closures still match the review. It uses the running closure's
`nix-env`, revalidates the profile and running closure after switching, and
invokes the exact validated activation executable with a clean environment.
Native fixtures exercise argument, permission, failed-switch, authorization-delay
and profile/running-closure races without activating a machine. The picker owns
the successful package-diff review and explicit confirmation.

`seele-pi-jj ABSOLUTE_JJ detect|revision` supplies Pi's asynchronous footer
callbacks with bounded Jujutsu metadata from the inherited working directory.
It uses fixed root/log arguments, two-second/64-KiB limits per child, and shared
process-group cleanup; failures emit no repository output. `tests/pi.rs` covers
exact argv, non-repository fallback, flood/timeout limits and error privacy.
