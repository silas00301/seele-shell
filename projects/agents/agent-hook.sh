#!/usr/bin/env bash
set -euo pipefail

# Publishes harness status for agents that expose lifecycle hooks instead of an
# extension API: Claude Code calls this from its settings hooks and Codex from
# its managed hooks. Pi and OpenCode publish the same records from plugins.

agent=${1:?agent required}
event=${2:?event required}

state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/seele-shell/agents

parent_of() {
  local stat rest
  [[ -r /proc/$1/stat ]] || return 1
  stat=$(</proc/$1/stat)
  rest=${stat##*') '}
  # shellcheck disable=SC2086
  set -- $rest
  [[ $2 =~ ^[0-9]+$ ]] || return 1
  printf '%s' "$2"
}

# Hooks run as short-lived children of the harness, so record the harness pid
# instead: the shell drops records whose process is gone.
owning_pid() {
  local pid=$PPID depth=0 comm
  while ((depth < 12)) && [[ -r /proc/$pid/comm ]]; do
    comm=$(</proc/$pid/comm)
    comm=${comm#.}
    comm=${comm%-wrapped}
    if [[ $comm == "$agent" ]]; then
      printf '%s' "$pid"
      return 0
    fi
    pid=$(parent_of "$pid") || break
    ((pid > 1)) || break
    depth=$((depth + 1))
  done
  printf '%s' "$PPID"
}

# Both harnesses describe the session on stdin, and name it the same way.
payload=""
[[ -t 0 ]] || payload=$(cat 2>/dev/null || true)
key=""
[[ -z $payload ]] || key=$(jq -r '.session_id // empty' <<<"$payload" 2>/dev/null || true)

pid=$(owning_pid)
key=${key//[^A-Za-z0-9_-]/}
[[ -n $key ]] || key=$pid

mkdir -p "$state_dir"
state_file=$state_dir/$agent-native-$key.json

if [[ $event == end ]]; then
  rm -f "$state_file"
  exit 0
fi

started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
if [[ -r $state_file ]]; then
  started_at=$(jq -r --arg fallback "$started_at" '.startedAt // $fallback' "$state_file" 2>/dev/null || printf '%s' "$started_at")
fi

tmp=$(mktemp "$state_dir/.${agent}.XXXXXX")
jq -nc \
  --arg agent "$agent" \
  --arg status "$event" \
  --arg startedAt "$started_at" \
  --arg updatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson pid "$pid" \
  '{agent:$agent,status:$status,source:"native",pid:$pid,startedAt:$startedAt,updatedAt:$updatedAt}' >"$tmp"
mv "$tmp" "$state_file"
