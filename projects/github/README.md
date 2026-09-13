# GitHub in Seele Shell

The GitHub panel has Notifications, Reviews and My pull requests tabs.
`seele-shellctl control github` opens the same panel. Ctrl+Tab cycles tabs.
The existing PR lists retain their read-only, bounded `seele-github-status`
collector and one-minute refresh policy.

## Notification inbox

`seele-github-inbox` is a resident Rust worker in `projects/integrations/src/github/`.
It polls through the existing `gh` login every minute, whether the panel is open
or closed. Every page uses `all=false` and `participating=false`, so the inbox holds
unread threads only; there is no repository, reason, type or AI-priority filter.
GitHub keeps threads marked Done on the web in its `all=true` listing and exposes
no Done state, while web Done also marks a thread read and new activity makes it
unread again. Unread-only is therefore the one listing that never shows a thread
already Done on the web; a thread read there without Done leaves as well.
Each page becomes visible before
context collection or inference. Unknown types remain ordinary inbox entries.
A failed/incomplete refresh retains previously known entries and explicitly
marks the count incomplete; only a complete refresh reconciles absent items.

The worker requests original issue/PR bodies and all comments, PR reviews and CI
state, discussions/replies, release text, and commit messages/comments. Review
comments and check states use explicit GraphQL fields. It never requests PR files,
REST review-comment payloads containing `diff_hunk`, commit diffs, or comparisons.
Unsupported subjects expose their raw notification and an availability message.
Thread content is displayed as selectable plain text, including Markdown syntax;
remote images and markup are not loaded by an opened row.

Arrival starts background triage through `seele-runtime::inference`. The shared
broker owns model selection; this consumer has no Codex client or model setting.
The output schema requires summary, reason, attention, nextAction, changes, and
one exact priority: Immediate Action required, Action required soon, Action
required sometime, or Informational. No triage result changes GitHub state.
Pending/failed items remain usable. Priority sorts first, then update time and
thread ID. Earlier analysis stays visible when a revision produces a new result.
Small source stamps identify body, comment, label, state, review and CI changes.
Known notifications recheck source context every five minutes; updated notification
revisions queue immediately. R retries/rechecks the selected thread.

Only the first two priorities create desktop notifications. Their default action
opens that thread's row in the panel. Delivery is limited to sixteen concurrent waiters;
further alerts remain queued and are revalidated before delivery. Account/entry
revisions guard asynchronous results, and a private metadata ledger prevents
replaying identical alerts across shell restarts. Source text and AI output live
only in memory. At most sixteen inactive full threads are cached, alongside the
selected thread and current jobs; evicted content is fetched when reopened.

Each row carries its priority as a chip (Act now, Act soon, Sometime, FYI) or the
stage its analysis has reached. Up/Down or J/K moves between rows, and Enter or a
click unfolds the current row in place: its actions, the analysis sections, any
earlier analysis and the original thread grow inside the row, which Enter or
Escape folds again. O opens its exact GitHub subject, D marks Done, and R refreshes
the inbox or retries the open thread's triage. Tab reaches the open row's action
buttons. Opening a row makes no read-state write. Done is optimistic, uses the documented DELETE thread endpoint, and returns
to the last confirmed state on failure. Done revisions are reconciled on refresh;
new activity returns to the inbox. Browser handoff is always explicit.

## Public API boundary (SIL-40)

On 2026-09-13, the official REST OpenAPI and public GraphQL schemas expose no
saved-notification field, Save/Unsave, or operation to restore a Done thread.
Silas explicitly approved delivering the supported API scope first. Save and
post-Done Undo are therefore deferred and remain in the web inbox; the shell
never substitutes local-only state for a claimed GitHub write. The count covers
the complete unread API collection minus locally confirmed Done revisions. It
cannot promise parity for state/types GitHub does not expose: a thread read on the
web but not marked Done is indistinguishable from a Done one and leaves the inbox
too. GitHub's retention/access limits apply.

- [REST notifications](https://docs.github.com/en/rest/activity/notifications)
- [Official REST schema](https://github.com/github/rest-api-description/blob/main/descriptions/api.github.com/api.github.com.json)
- [Public GraphQL schema](https://docs.github.com/public/fpt/schema.docs.graphql)

## Account, limits and failures

Sign in manually with `gh auth login`. For enterprise, set `SEELE_GITHUB_HOST`
and sign in using that host. Notification access needs the appropriate classic
`notifications`/`repo` scopes; fine-grained tokens and GitHub App tokens are not
supported by GitHub's notification endpoints. The worker never starts login,
reads credential files or passes credentials to the broker/QML/logs. A changed
account clears the previous inbox and invalidates old results. Done verifies the
current identity again before writing.

API subprocesses have a 20-second/4-MiB bound. Pagination allows 200 pages of
50 notifications (10,000); hitting the bound reports an incomplete inbox rather
than claiming a complete count. Thread collections have a 2-MiB limit; broker
context has a 180-KiB limit. Oversized context leaves the raw notification usable
with an explicit error. Two background jobs and one independent selected-detail
fetch run concurrently. Automatic failures retry at most three times with bounded
backoff; a manual retry resets the budget. Rate limits pause automatic work for
five minutes. Errors are fixed messages, never raw CLI diagnostics.

The worker consumes bounded JSON lines on stdin: `refresh`, `select`, `back`,
`retry`, `done`, `open`, `inbox`, with an `id` where applicable. It emits `snapshot`
and `focus` events. QML owns Qt process lifecycle, focus, retained ListModels and
the drawing of thread identity, priority and analysis; Rust owns scheduling, state,
comparison and the original-thread presentation blocks. EOF/signals cancel
workers and their owned process groups. The only persisted data is account-scoped
thread IDs and revision fingerprints in mode-0600 files under the private
`$XDG_STATE_HOME/seele-github` directory.

## Validation

Use fixtures, never a real account or live desktop:

```sh
cargo test -p seele-integrations --all-features --locked
cargo clippy -p seele-integrations --all-targets --all-features --locked -- -D warnings
cargo build -p seele-integrations --bin seele-github-inbox --locked
python3 projects/integrations/tests/github.py target/debug/seele-github-inbox
```

The production-worker fixture tests multi-page loading, unknown types, raw/AI
failure fallback, full comments, retriage deltas, Done rollback, restart/count
reconciliation and notification thresholds/default actions. Rust tests cover
ordering, schema constraints, source stamps, stale/account results and URL gates.
`tests/github-inbox.sh` renders the actual panel in QtTest and checks keyboard
navigation, open-row actions and folding, and narrow/error states. These checks are wired into
the native/shell packages and `test-shell`; the complete shell compile check never
instantiates the desktop. Existing PR tests remain in `tests/github.js` and
`projects/runtime/tests/github.py`.
