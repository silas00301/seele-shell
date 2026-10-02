#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail
umask 077
hook=${1:?agent hook required}
control=${2:?control required}
launcher=${3:?agent launcher required}
work=$(mktemp -d)
trap 'jobs -pr | xargs -r kill 2>/dev/null || true; rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/state" "$work/home"
export XDG_STATE_HOME="$work/state" HOME="$work/home"
export HOOK="$hook" CONTROL="$control"
cp "$BASH" "$work/cursor-agent"
cat >"$work/lifecycle.sh" <<'SH'
set -euo pipefail
key=$(printf 'conversation/one' | sha256sum | cut -d' ' -f1)
record="$XDG_STATE_HOME/seele-shell/agents/cursor-native-$key.json"
fire() { printf '%s' "$1" | "$HOOK" cursor host-event; }
fire '{"hook_event_name":"sessionStart","session_id":"conversation/one"}' >"$HOME/response"
[[ $(<"$HOME/response") == '{}' ]]
jq -e --argjson pid "$$" '.agent == "cursor" and .status == "input" and .source == "native" and .pid == $pid' "$record" >/dev/null
started=$(jq -r .startedAt "$record")
fire '{"hook_event_name":"beforeSubmitPrompt","conversation_id":"conversation/one","prompt":"PRIVATE PROMPT","user_email":"PRIVATE EMAIL","transcript_path":"PRIVATE PATH","attachments":[{"file_path":"PRIVATE FILE"}]}' >"$HOME/response"
[[ $(<"$HOME/response") == '{}' ]]
jq -e --arg started "$started" '.status == "working" and .startedAt == $started' "$record" >/dev/null
# A delayed fire-and-forget sessionStart cannot revert a submitted turn.
fire '{"hook_event_name":"sessionStart","session_id":"conversation/one"}' >/dev/null
jq -e '.status == "working"' "$record" >/dev/null
# All steps use conversation_id, whereas start/end also expose session_id.
fire '{"hook_event_name":"stop","conversation_id":"conversation/one","status":"completed"}' >/dev/null
jq -e '.status == "input"' "$record" >/dev/null
[[ $(stat -c %a "$record") == 600 ]]
[[ $(find "$(dirname "$record")" -name 'cursor-native-*.json' | wc -l) == 1 ]]
# Distinct identities that sanitize to the same string must not collide.
fire '{"hook_event_name":"beforeSubmitPrompt","conversation_id":"conversationone"}' >/dev/null
[[ $(find "$(dirname "$record")" -name 'cursor-native-*.json' | wc -l) == 2 ]]
status=$("$CONTROL" agent-status)
jq -e '.cursor.active and .cursor.source == "native" and .cursor.status == "input"' <<<"$status" >/dev/null
# Neither source content nor raw conversation identities are persisted.
if grep -R -E 'PRIVATE|conversation/one|conversationone' "$XDG_STATE_HOME/seele-shell/agents"; then exit 1; fi
before=$(sha256sum "$record")
fire '{"hook_event_name":"afterAgentThought","conversation_id":"conversation/one","text":"PRIVATE"}' >/dev/null
[[ $(sha256sum "$record") == "$before" ]]
fire '{"hook_event_name":"sessionEnd","session_id":"conversation/one"}' >/dev/null
[[ ! -e "$record" ]]
fire '{"hook_event_name":"sessionEnd","session_id":"conversationone"}' >/dev/null
[[ $(find "$XDG_STATE_HOME/seele-shell/agents" -name 'cursor-native-*.json' | wc -l) == 0 ]]
for payload in '{}' '{"hook_event_name":"stop","conversation_id":""}' '{"hook_event_name":"stop","conversation_id":42}'; do
  if fire "$payload"; then echo 'Invalid Cursor identity accepted' >&2; exit 1; fi
done
SH
"$work/cursor-agent" "$work/lifecycle.sh"
# The same native hooks in the desktop editor must resolve its process, but
# opening an idle editor must not create a CPU-inferred agent session.
cp "$BASH" "$work/cursor"
"$work/cursor" -c 'printf '\''{"hook_event_name":"beforeSubmitPrompt","conversation_id":"editor"}'\'' | "$HOOK" cursor host-event; sleep 30' >"$work/editor-output" &
editor=$!
for _ in {1..100}; do
  [[ -s "$work/editor-output" ]] && break
  sleep 0.02
done
key=$(printf editor | sha256sum | cut -d' ' -f1)
record="$work/state/seele-shell/agents/cursor-native-$key.json"
jq -e --argjson pid "$editor" '.pid == $pid and .status == "working"' "$record" >/dev/null
# Clicking the active indicator uses the same validated ancestor-window path.
export PATH="$work/bin:$PATH" EDITOR_PID="$editor"
# A successful focus dispatch is recorded without contacting the compositor.
printf '#!%s\nif [[ $1 == clients ]]; then printf '\''[{"pid":%%s,"address":"0xcafe"}]\\n'\'' "$EDITOR_PID"; else printf '\''%%s\\n'\'' "$*" >"$FOCUS_ARGS"; printf '\''ok\\n'\''; fi\n' "$BASH" >"$work/bin/hyprctl"
chmod +x "$work/bin/hyprctl"
export FOCUS_ARGS="$work/focus"
SEELE_CONTROL_NO_STATUS=1 "$control" agent-focus cursor
[[ $(<"$work/focus") == 'dispatch hl.dsp.focus({ window = "address:0xcafe" })' ]]
rm "$record"
"$control" agent-status | jq -e '(.cursor.active // false) == false' >/dev/null
kill "$editor"; wait "$editor" 2>/dev/null || true
# A CLI started outside Seele still participates in CPU fallback discovery.
"$work/cursor-agent" -c 'sleep 30; :' &
cli=$!
"$control" agent-status | jq -e '.cursor.active and .cursor.source == "cpu"' >/dev/null
kill "$cli"; wait "$cli" 2>/dev/null || true
# The launcher preserves explicit package selection, project cwd, and one prompt
# argument, including text that must never be evaluated by a shell.
printf '#!%s\nprintf '\''%%s\\0'\'' "$@" >"$LAUNCH_ARGS"\n' "$BASH" >"$work/bin/ghostty"
chmod +x "$work/bin/ghostty"
export LAUNCH_ARGS="$work/launch" SEELE_SHELL_GHOSTTY="$work/bin/ghostty" SEELE_SHELL_CURSOR='/managed/cursor-agent'
"$launcher" cursor 'explain $(touch never-created)'
mapfile -d '' -t args <"$work/launch"
[[ ${args[0]} == "--working-directory=$HOME" && ${args[4]} == cursor && ${args[5]} == /managed/cursor-agent && ${args[6]} == 'explain $(touch never-created)' ]]
[[ ! -e never-created ]]
printf 'Cursor lifecycle, ownership, focus, fallback and launch tests passed\n'
