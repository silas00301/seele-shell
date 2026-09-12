# Shared runtime

## Codex isolation

`codex::Toolless` centralizes discovery, feature disabling, no-tools policy,
configuration flags and the authentication environment. `codex::Context` owns an
empty private HOME/CODEX_HOME and logs for one broker attempt or resumable prompt
panel. It delegates only the existing private owned regular auth.json through a
symlink. The runtime validates metadata, never reads credential bytes, and rejects
symlinks, hardlinks, FIFOs, devices, foreign owners and broad permissions.

This closes an installed-CLI gap proven with hostile synthetic global AGENTS.md:
`--ignore-user-config` and a zero project instruction budget still allowed that
file into the actual request. The
[0.153.4 global instruction loader](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/codex-home/src/instructions/mod.rs)
is independent of project instruction limits. The private home removes that input.

The existing file-auth refresh contract is preserved: Codex 0.153.4
[FileAuthStorage::save](https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/login/src/auth/storage.rs)
opens with truncate/write/create, following the symlink into the original store.
Synthetic tests exercise that exact operation and verify no credential copy or
source removal. Upstream auth storage changes require reviewing this contract.
Keyring-only auth is unsupported and fails closed; no user home fallback occurs.

`inference::configured_model` reads the broker's strict configuration operation
without creating work. Prompt pins that model across panel turns. Model selection
is part of the no-tools contract: the real Codex 0.153.4 gate proves an empty tool
set with configured gpt-5.6-luna; another tested older model exposed the user-input
tool despite feature disabling. Re-run both broker and prompt actual-wire gates
when the configured model or Codex version changes. Fixtures use synthetic private
authentication and loopback HTTP, never a real account.
