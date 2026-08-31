#!/usr/bin/env bash
set -euo pipefail

pi_extension=${1:?Pi extension required}
opencode_extension=${2:?OpenCode extension required}
control=${3:?control script required}
hook=${4:?agent hook script required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

esbuild "$pi_extension" --bundle --platform=node --format=cjs \
  --external:@earendil-works/pi-coding-agent --outfile="$work/pi-extension.cjs" >/dev/null
cat >"$work/pi-test.cjs" <<'JS'
const fs = require("node:fs");
const path = require("node:path");
const handlers = new Map();
const extension = require(process.argv[2]).default;
extension({ on(name, handler) { handlers.set(name, handler); } });
const stateFile = path.join(process.env.XDG_STATE_HOME, "seele-shell", "agents", `pi-native-${process.pid}.json`);
const read = () => JSON.parse(fs.readFileSync(stateFile, "utf8"));

(async () => {
  await handlers.get("session_start")({}, {});
  if (read().status !== "input" || read().source !== "native") {
    throw new Error("session_start must report native input state");
  }
  await handlers.get("agent_start")({}, {});
  if (read().status !== "working") throw new Error("agent_start must report working");
  await handlers.get("agent_settled")({}, {});
  if (read().status !== "input") throw new Error("agent_settled must report input");
  await handlers.get("session_shutdown")({}, {});
  if (fs.existsSync(stateFile)) throw new Error("session_shutdown must remove state");
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
JS
XDG_STATE_HOME="$work/state" node "$work/pi-test.cjs" "$work/pi-extension.cjs"

esbuild "$opencode_extension" --bundle --platform=node --format=cjs \
  --external:@opencode-ai/plugin --outfile="$work/opencode-extension.cjs" >/dev/null
cat >"$work/opencode-test.cjs" <<'JS'
const fs = require("node:fs");
const path = require("node:path");
const plugin = require(process.argv[2]).SeeleShellStatus;
const stateFile = path.join(process.env.XDG_STATE_HOME, "seele-shell", "agents", `opencode-native-${process.pid}.json`);
const read = () => JSON.parse(fs.readFileSync(stateFile, "utf8"));

(async () => {
  const hooks = await plugin({});
  if (read().status !== "input" || read().source !== "native") {
    throw new Error("OpenCode startup must report native input state");
  }
  await hooks.event({ event: { type: "session.status", properties: { sessionID: "one", status: { type: "busy" } } } });
  if (read().status !== "working") throw new Error("OpenCode busy session must report working");
  await hooks.event({ event: { type: "permission.asked", properties: { sessionID: "one" } } });
  if (read().status !== "input") throw new Error("OpenCode permission must report input");
  await hooks.event({ event: { type: "permission.replied", properties: { sessionID: "one" } } });
  if (read().status !== "working") throw new Error("OpenCode permission reply must restore working");
  await hooks.event({ event: { type: "session.status", properties: { sessionID: "one", status: { type: "idle" } } } });
  if (read().status !== "input") throw new Error("OpenCode idle session must report input");
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
JS
XDG_STATE_HOME="$work/state" node "$work/opencode-test.cjs" "$work/opencode-extension.cjs"

mkdir -p "$work/state/seele-shell/agents"
cat >"$work/state/seele-shell/agents/pi-native-$$.json" <<JSON
{"agent":"pi","status":"working","pid":$$,"source":"native"}
JSON
cat >"$work/state/seele-shell/agents/pi-zheuristic-$$.json" <<JSON
{"agent":"pi","status":"input","pid":$$,"source":"heuristic"}
JSON
sed '/^case "${1:-status}" in/,$d' "$control" >"$work/functions.sh"
result=$(XDG_STATE_HOME="$work/state" bash -c 'source "$1"; agent_states' _ "$work/functions.sh")
actual=$(jq -r '.pi.status' <<<"$result")
source=$(jq -r '.pi.source' <<<"$result")
[[ $actual == working && $source == native ]] || {
  printf 'native state lost to heuristic fallback: status=%s source=%s\n' "$actual" "$source" >&2
  exit 1
}

cat >"$work/state/seele-shell/agents/pi-native-waiting-$$.json" <<JSON
{"agent":"pi","status":"input","pid":$$,"source":"native"}
JSON
result=$(XDG_STATE_HOME="$work/state" bash -c 'source "$1"; agent_states' _ "$work/functions.sh")
actual=$(jq -r '.pi.status' <<<"$result")
[[ $actual == input ]] || {
  printf 'concurrent input state did not take precedence: status=%s\n' "$actual" >&2
  exit 1
}

rm -f "$work/state/seele-shell/agents/"*.json

# Claude Code publishes through its settings hooks and names the session itself.
printf '{"session_id":"abc-123","hook_event_name":"UserPromptSubmit"}' |
  XDG_STATE_HOME="$work/state" bash "$hook" claude working
record=$work/state/seele-shell/agents/claude-native-abc-123.json
[[ -f $record ]] || {
  printf 'Claude hook did not publish a session record\n' >&2
  exit 1
}
jq -e '.agent == "claude" and .status == "working" and .source == "native" and (.pid | type) == "number"' "$record" >/dev/null

started=$(jq -r '.startedAt' "$record")
printf '{"session_id":"abc-123"}' | XDG_STATE_HOME="$work/state" bash "$hook" claude input
jq -e --arg started "$started" '.status == "input" and .startedAt == $started' "$record" >/dev/null || {
  printf 'Claude hook lost the session start time across events\n' >&2
  exit 1
}

printf '{"session_id":"abc-123"}' | XDG_STATE_HOME="$work/state" bash "$hook" claude end
[[ ! -f $record ]] || {
  printf 'Claude session end did not remove its record\n' >&2
  exit 1
}

# Codex reports the same lifecycle through its managed hooks.
printf '{"session_id":"t-9","hook_event_name":"UserPromptSubmit"}' |
  XDG_STATE_HOME="$work/state" bash "$hook" codex working
jq -e '.agent == "codex" and .status == "working" and .source == "native"' \
  "$work/state/seele-shell/agents/codex-native-t-9.json" >/dev/null || {
  printf 'Codex hook did not publish a working record\n' >&2
  exit 1
}
printf '{"session_id":"t-9"}' | XDG_STATE_HOME="$work/state" bash "$hook" codex end
[[ ! -f "$work/state/seele-shell/agents/codex-native-t-9.json" ]] || {
  printf 'Codex session end did not remove its record\n' >&2
  exit 1
}

# A hook fired outside any session still names its record after the harness.
XDG_STATE_HOME="$work/state" bash "$hook" codex input </dev/null
[[ $(find "$work/state/seele-shell/agents" -name 'codex-native-*.json' | wc -l) == 1 ]] || {
  printf 'a hook without a session id did not publish a record\n' >&2
  exit 1
}

rm -f "$work/state/seele-shell/agents/"*.json

# Records outlive their process only long enough to report a finished run.
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
stale=$(date -u -d '-1 hour' +%Y-%m-%dT%H:%M:%SZ)
cat >"$work/state/seele-shell/agents/pi-heuristic-old.json" <<JSON
{"agent":"pi","status":"finished","source":"heuristic","pid":4194303,"startedAt":"$stale","updatedAt":"$stale","endedAt":"$stale"}
JSON
cat >"$work/state/seele-shell/agents/codex-heuristic-recent.json" <<JSON
{"agent":"codex","status":"finished","source":"heuristic","pid":4194303,"startedAt":"$now","updatedAt":"$now","endedAt":"$now"}
JSON
result=$(XDG_STATE_HOME="$work/state" bash -c 'source "$1"; agent_states' _ "$work/functions.sh")
[[ $(jq -r '.codex.status' <<<"$result") == finished && $(jq -r '.codex.active' <<<"$result") == false ]] || {
  printf 'a run that just finished must stay visible: %s\n' "$result" >&2
  exit 1
}
[[ $(jq -r 'has("pi")' <<<"$result") == false ]] || {
  printf 'an abandoned record kept reporting a status: %s\n' "$result" >&2
  exit 1
}
[[ ! -f "$work/state/seele-shell/agents/pi-heuristic-old.json" ]] || {
  printf 'an abandoned record was not cleaned up\n' >&2
  exit 1
}

rm -f "$work/state/seele-shell/agents/"*.json

# Harnesses without any integration are read from their CPU usage instead.
cp "$(command -v bash)" "$work/codex"
"$work/codex" -c 'sleep 30; :' &
harness=$!
trap 'rm -rf "$work"; kill "$harness" 2>/dev/null || true' EXIT

result=$(XDG_STATE_HOME="$work/state" bash -c 'source "$1"; agent_states' _ "$work/functions.sh")
[[ $(jq -r '.codex.active' <<<"$result") == true ]] || {
  printf 'a running harness was not detected: %s\n' "$result" >&2
  exit 1
}

sample=$work/state/seele-shell/agents/.cpu-sample.json
jq --argjson now "$(date +%s)" 'to_entries | map(.value.at = ($now - 25)) | from_entries' "$sample" >"$sample.aged"
mv "$sample.aged" "$sample"
result=$(XDG_STATE_HOME="$work/state" bash -c 'source "$1"; agent_states' _ "$work/functions.sh")
[[ $(jq -r '.codex.status' <<<"$result") == input ]] || {
  printf 'a quiet harness was not reported as waiting: %s\n' "$result" >&2
  exit 1
}

# A harness that reports for itself is never second-guessed by its CPU usage.
cat >"$work/state/seele-shell/agents/codex-native-t-1.json" <<JSON
{"agent":"codex","status":"working","source":"native","pid":$harness,"startedAt":"$now","updatedAt":"$(date -u +%Y-%m-%dT%H:%M:%SZ)"}
JSON
result=$(XDG_STATE_HOME="$work/state" bash -c 'source "$1"; agent_states' _ "$work/functions.sh")
[[ $(jq -r '.codex.status' <<<"$result") == working ]] || {
  printf 'a hook record was overruled by a quiet CPU sample: %s\n' "$result" >&2
  exit 1
}

printf 'harness status checks passed\n'
