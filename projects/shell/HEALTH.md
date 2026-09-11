# Integration Health contract (version 1)

The integration owner declares `seele.health.providers.<id>` in Home Manager.
Set `enable`, `name`, `deadline` (milliseconds), `setup` (existing shell panel),
`actions`, optional `service` (managed user service), and `disruptive` actions.
Disabled entries are omitted; registration never depends on executables or
processes. Configured but silent providers remain visible as stale. GitHub and
Home Assistant registrations belong to the shell feature; Tailscale belongs to
its NixOS configuration. Hermes, backups and Codex broker owners use the same
contract when enabled; no placeholder rows are installed for absent integrations.

Publish on every state change and at a cadence below the configured deadline:

```
printf '%s\n' '{"state":"healthy","summary":"Connected","lastSuccess":1789160000000,"actions":["settings"]}' | seele-shellctl health-publish provider-id
```

The helper uses the installed shell's exact IPC path. `seele-shellctl health-status`
reads metadata; the `health` IPC target also exposes `snapshot()`. Arguments are
data, never shell source.
Integration publishers must construct summaries from fixed local descriptions,
not provider response bodies. Payloads have a 4096-character IPC limit, a
240-character summary and 1200-character optional sanitized detail. Do not send
credentials, private provider content, AI prompts/results or notifications.
`lastSuccess` is Unix milliseconds. The store stamps receipt time, derives stale
centrally, and replaces the previous payload. Nothing is written to disk and
there is no transition history. Config changes remove obsolete payloads.

Semantic states: `healthy`, `degraded`, `disconnected`, `setup-required`.
Valid actions: `retry`, `restart`, `reconnect`, `settings`, `diagnostics`. A
publication may offer only actions in its registration. `restart` executes only
`systemctl --user restart <registered-service>` as an argv array. Settings opens
the owner's existing panel. Diagnostics reveals only sanitized current detail.
Retry/reconnect require an owning integration callback in `handlers`; adapters
must complete the action with its token. External providers can expose managed
service restart/settings/diagnostics without any panel code. Disruptive actions
require a second explicit click; errors/progress stay on that provider's row.

GitHub publishes after its existing bounded refresh completes; registering
Health keeps that existing refresh scheduler active. Home Assistant heartbeats
come from its resident worker; Tailscale comes from the existing auxiliary
status feed. A hung worker cannot refresh its health by a UI-only timer.

`SystemHealthPanel` is generic. It sorts attention first, keeps healthy entries
compact, and offers a `maintenanceContent` Component extension for the separate
Maintenance tab. Health itself never produces notifications.
