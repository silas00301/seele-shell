# GitHub in Seele Shell

Click the GitHub mark or run `seele-shellctl control github` to see requested
reviews and your open pull requests. Each tab shows up to 20 recently updated
items, the total matching count, the latest commit's check rollup, and the
review decision. Missing check data is shown explicitly. A row opens its pull
request in the browser only when clicked or selected with Enter.

Use Up/Down or j/k to select a row, Tab to switch lists, R to refresh, Enter to
open the selected pull request, and Escape to close. Empty, loading, signed-out,
rate-limit, and network-error states remain readable. A failed refresh retains
the last successful data with a stale label; an expired login clears private
cached titles. The bar's review count is the last fetched value and does not
poll while the panel is closed.

## Account and privacy

The helper uses `gh api` with your existing GitHub CLI login. Sign in manually
with `gh auth login`; for an enterprise host, set `SEELE_GITHUB_HOST` in the
shell's environment and use `gh auth login --hostname HOST`. It does not run
login, request or print tokens, read credential files, or modify GitHub state.
The GraphQL request is a query, with no mutations. Opening a pull request is a
separate explicit browser action restricted to canonical HTTPS pull-request
URLs on the selected host.

Results live in QML memory and disappear on shell reload. No disk cache,
notification stream, or hardware-status polling is involved. Raw `gh` errors
are replaced with fixed messages so credentials and diagnostics never enter
the UI. Titles and repository names render as plain text.

## Refresh and validation

Opening the panel refreshes data if the last attempt is at least one minute
old. A timer refreshes once per minute while visible; rate-limit responses delay
automatic retries to five minutes. Manual refresh is limited to once per five
seconds and one request at a time. Each refresh makes two API requests: current
account identity, followed by one query for both PR lists. Each process has a
15-second deadline and a 256 KiB combined output limit. Oversized or timed-out
process groups are terminated. Shell shutdown or reload also kills and reaps the
active CLI process group, so it cannot outlive the shell worker.

`GitHubStore.qml` owns Qt refresh callbacks and live data. The shared Rust
`qml-core` owns display policy, and `runtime::github` owns the canonical URL gate
and bounded `seele-github-status` API projection. The package bundles native
binaries and `gh`; Python is used only by development fixtures. Focused tests
use a temporary fake `gh`, never an account:

```sh
cargo build -p seele-runtime -p seele-qml-core --locked
PYTHONDONTWRITEBYTECODE=1 python3 projects/runtime/tests/github.py target/debug/seele-github-status
node tests/github.js projects/shell/github.js projects/shell/GitHubStore.qml
```

They also run in `test-shell` and the shell package's install checks, alongside
`qmllint` for the QML store and panel.

The panel holds each request pending until its completion callback runs. A 35-second watchdog recovers startup failures; per-request tokens discard late output and callbacks after a timeout or subsequent refresh.
