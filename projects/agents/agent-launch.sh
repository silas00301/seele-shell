#!/usr/bin/env bash
set -euo pipefail

agent=${1:-pi}
shift || true
prompt=${*:-}

active_pid=$(hyprctl activewindow -j 2>/dev/null | jq -r '.pid // empty')
cwd=$HOME
if [[ $active_pid =~ ^[0-9]+$ && -d /proc/$active_pid/cwd ]]; then
  candidate=$(readlink -f "/proc/$active_pid/cwd" 2>/dev/null || true)
  [[ -d $candidate && -r $candidate ]] && cwd=$candidate
fi

case "$agent" in
  pi)
    command=("${SEELE_SHELL_PI:-pi}")
    [[ -z $prompt ]] || command+=("$prompt")
    ;;
  opencode)
    command=("${SEELE_SHELL_OPENCODE:-opencode}" "$cwd")
    [[ -z $prompt ]] || command+=(--prompt "$prompt")
    ;;
  codex)
    command=("${SEELE_SHELL_CODEX:-codex}")
    [[ -z $prompt ]] || command+=("$prompt")
    ;;
  claude)
    command=("${SEELE_SHELL_CLAUDE:-claude}")
    [[ -z $prompt ]] || command+=("$prompt")
    ;;
  *)
    printf 'Unknown agent: %s\n' "$agent" >&2
    exit 2
    ;;
esac

exec "${SEELE_SHELL_GHOSTTY:-ghostty}" --working-directory="$cwd" --class=org.seele.agent -e seele-agent-run "$agent" "${command[@]}"
