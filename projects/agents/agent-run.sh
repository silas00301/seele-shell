#!/usr/bin/env bash
set -euo pipefail

agent=${1:?agent required}
shift

state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/seele-shell/agents
mkdir -p "$state_dir"
state_file=$state_dir/$agent-heuristic-$$.json
started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Harnesses such as Claude Code do their work in helper processes, so measure
# CPU time across the whole subtree instead of only the launched command.
subtree_ticks() {
  awk -v target="$1" '
    {
      pid = $1
      rest = substr($0, index($0, ")") + 2)
      split(rest, fields, " ")
      parent[pid] = fields[2]
      ticks[pid] = fields[12] + fields[13]
      pids[++count] = pid
    }
    END {
      member[target] = 1
      changed = 1
      while (changed) {
        changed = 0
        for (i = 1; i <= count; i++) {
          if (!member[pids[i]] && member[parent[pids[i]]]) {
            member[pids[i]] = 1
            changed = 1
          }
        }
      }
      total = 0
      for (i = 1; i <= count; i++) {
        if (member[pids[i]]) total += ticks[pids[i]]
      }
      print total
    }
  ' /proc/[0-9]*/stat 2>/dev/null
}

write_state() {
  local status=$1
  local ended_at=${2:-}
  local exit_code=${3:-null}
  local tmp
  tmp=$(mktemp "$state_dir/.${agent}.XXXXXX")
  jq -nc \
    --arg agent "$agent" \
    --arg status "$status" \
    --arg source "heuristic" \
    --arg startedAt "$started_at" \
    --arg updatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg endedAt "$ended_at" \
    --argjson pid "$$" \
    --argjson exitCode "$exit_code" \
    '{agent:$agent,status:$status,source:$source,startedAt:$startedAt,updatedAt:$updatedAt,pid:$pid}
      + (if $endedAt == "" then {} else {endedAt:$endedAt,exitCode:$exitCode} end)' >"$tmp"
  mv "$tmp" "$state_file"
}

write_state working
"$@" &
child=$!
last_ticks=""
idle_seconds=0
current=working

while kill -0 "$child" 2>/dev/null; do
  ticks=$(subtree_ticks "$child" || true)
  if [[ -n $ticks && $ticks == "$last_ticks" ]]; then
    ((idle_seconds += 2))
  else
    idle_seconds=0
    last_ticks=$ticks
  fi

  next=working
  ((idle_seconds >= 20)) && next=input
  if [[ $next != "$current" ]]; then
    current=$next
    write_state "$current"
  fi
  sleep 2
 done

set +e
wait "$child"
exit_code=$?
set -e
write_state finished "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$exit_code"
exit "$exit_code"
