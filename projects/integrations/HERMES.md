# Hermes Desktop and nerv

SIL-24 uses the official Hermes **Desktop Light** application as the interface
for the existing server agent. It does not install or launch a second Python
agent. `nerv` enables `seele.hermes`; other hosts are unchanged.

The package pins NousResearch/hermes-agent commit
`10c6188de188871f64a88dd95bc6b262adb0c307`, uses its official Nix Desktop
builder and npm lock, and adds small asserted framework bridges. Desktop's
connection settings own the gateway URL, sign-in and credentials. The initial
URL is `http://hermes:9119`; a saved connection wins. Electron uses the system
Secret Service wallet, refuses `basic_text` and plaintext saves, and fails
rather than writing credentials without an unlocked wallet. Credentials never
belong in Nix options, environment overrides or this repository.

Desktop reads only the shared published `seele-shell/theme.json` palette. Its
semantic tokens project the shared type/shape/interaction ramp from installed
`Theme.qml`, and theme changes repaint without restarting. The Shell panel
uses the existing shared rule, card, edge and action components.

## Server prerequisite

A default `hermes gateway run` container alone does **not** expose Desktop's
HTTP/WebSocket dashboard. Enable the dashboard in the existing official Docker
container with `HERMES_DASHBOARD=1`, `HERMES_DASHBOARD_HOST=0.0.0.0` and
`HERMES_DASHBOARD_PORT=9119`. Publish container port 9119 **on the host's
Tailscale address**, or use a private Tailscale Serve endpoint. Do not publish
it on every interface. This repository does not change the remote container.

The current official dashboard requires an authentication provider on a
non-loopback bind. Configure the server's bundled password provider using
`HERMES_DASHBOARD_BASIC_AUTH_USERNAME` and `HERMES_DASHBOARD_BASIC_AUTH_PASSWORD`
in your server secret mechanism, or its OAuth provider. Enter credentials
through Desktop's Remote connection settings; follow its advertised sign-in
flow. No credentials are needed or requested in a PR. A Tailscale Serve HTTPS
URL can be entered there or set as the non-secret `gatewayUrl` default.

Sources: [official Desktop guide](https://hermes-agent.nousresearch.com/docs/user-guide/desktop),
[Docker guide](https://hermes-agent.nousresearch.com/docs/docker), and the
[pinned container dashboard launcher](https://github.com/NousResearch/hermes-agent/blob/10c6188de188871f64a88dd95bc6b262adb0c307/docker/s6-rc.d/dashboard/run).

## Shell lifecycle

The bar opens the connection and approval panel; its Open Hermes Desktop action
opens the actual application. Vicinae's Hermes command opens the same panel.
Idle, listening, thinking and speaking come from Desktop's real transport,
active-work, wake-word and playback stores. The active session is an opaque
SHA-256 identity. A missing heartbeat for 15 seconds becomes disconnected.
No prompt, title, transcript, screenshot or audio reaches the Shell service.
Wake word and voice remain explicit Desktop features; Seele enables neither.

## nerv tools

`seele-hermes` serves stateless Streamable HTTP MCP at
`http://nerv:8766/mcp`. It binds only nerv's actual Tailscale IPv4 address,
authenticates the real TCP peer through `tailscale whois` on every request,
requires the configured `peer` (`hermes` by default), and rejects browser
Origins. Prefer the full tailnet node DNS name when naming the peer. Grant a
Tailscale ACL from Hermes to nerv on that port. Incoming SSH is unnecessary.

In the **server's** Hermes MCP settings, add:

```yaml
mcp_servers:
  seele:
    url: http://nerv:8766/mcp
```

Restart/reload the server agent's MCP configuration using its normal workflow.
See [official MCP configuration](https://hermes-agent.nousresearch.com/docs/features/mcp).

Tools read the configured flake's Jujutsu revision, host platform and NixOS
version, retained generations and immutable package diffs, and selected service
status/journal lines. Logs are bounded and redacted. `services` is an explicit
allowlist (initially `nix-daemon.service`); normal journal permissions apply.
There is no arbitrary command, arbitrary path or desktop-context endpoint.

Rebuild requests are disabled by default. Set `seele.hermes.allowRebuild = true`
locally to let the server **request** a fixed `nerv` rebuild. Each request pins
the checkout's current Jujutsu commit, expires in 120 seconds, and appears in
the local panel. Deny discards it. Approve consumes it once, rechecks the current
revision, and opens `seele-rebuild os switch git+file://…?rev=<approved>&submodules=1#nerv`
in Ghostty. The immutable parent commit and its committed submodule pins own
the build, so subsequent checkout or submodule edits cannot change its source.
Normal system authentication still applies. MCP exposes no approval operation,
never accepts a command or target, and cannot authenticate or approve itself.
The terminal owns build output and completion; a successful handoff is not a
successful rebuild. Git-backed fetching needs the committed child revisions to be available; a
fetch failure fails the rebuild rather than falling back to the mutable checkout.

## Capability discovery and approval outcomes

`capabilities` reports the service allowlist, whether rebuild requests are
configured, the required local approval, and connection metadata. It exposes
no desktop or audio content. `rebuild_status` accepts only a request UUID and
reports pending, denied, expired, consumed, stale or handed-off outcomes. These
are at most 32 memory-only records, retired after five minutes; restart discards
them. Unknown/retired requests never imply success, and `handed-off` still means
only that the rebuild terminal opened. It never claims activation succeeded.

## Options and checks

`seele.hermes` offers `enable`, `gatewayUrl`, `peer`, `port`, `flake`, `services`
and `allowRebuild`. All are non-secret. The service belongs to the graphical
session, holds only metadata in memory, and retries after Tailscale/listener
failure. Closing Desktop expires its lifecycle; stopping the user service
removes its private socket and pending approvals.

Focused Rust tests cover lifecycle expiry, strict metadata and tool arguments,
allowlists, MCP initialization, tailnet identity, HTTP bounds and the separate
approval boundary. The CLI fixture uses private sockets and fake dependencies;
it never contacts the server or switches the host. A release still requires
Nix package/host checks and a real Desktop sign-in and tailnet MCP query on nerv.
