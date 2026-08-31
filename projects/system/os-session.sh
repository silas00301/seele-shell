#!/usr/bin/env bash
set -euo pipefail

# A Pi session pointed at Seele itself: the user describes a change to their
# desktop in plain language, Pi implements it in the flake, and the session
# then rebuilds the system and offers to record the change.

repo=${SEELE_SHELL_REPO:-$HOME/seele}
workspace=${SEELE_SHELL_OS_WORKSPACE:-9}
pi=${SEELE_SHELL_PI:-pi}
nh=${SEELE_SHELL_NH:-nh}
ghostty=${SEELE_SHELL_GHOSTTY:-ghostty}
hyprctl=${SEELE_SHELL_HYPRCTL:-hyprctl}

briefing="You are running inside the Seele desktop shell's OS session, opened from the AI cockpit in ${repo}.

The user describes a change they want to their own desktop or system. Implement it in this repository, following AGENTS.md and the seele skill under .agents/skills/.

Do not activate the system and do not commit: when you finish implementing, say so and end your turn. This session then runs 'nh os switch' and offers to record the change with Jujutsu."

confirm() {
  local answer
  read -r -p "$1 [y/N] " answer </dev/tty || return 1
  [[ $answer == [yY]* ]]
}

repo_changes() {
  jj diff --summary 2>/dev/null || git -C "$repo" status --porcelain 2>/dev/null || true
}

commit_message() {
  local recent diff
  recent=$(jj log --no-graph -r 'latest(::@- & ~empty(), 8)' -T 'description.first_line() ++ "\n"' 2>/dev/null \
    || git -C "$repo" log -8 --format='%s' 2>/dev/null || true)
  diff=$(jj diff --stat 2>/dev/null || git -C "$repo" diff --stat 2>/dev/null || true)

  "$pi" -p --no-session --no-tools "Write the commit message for this change to the Seele Nix flake.

Match the style of the repository's recent subjects:
$recent

Rules: one line, imperative mood, no trailing period, no conventional-commit prefix, no quotes around it, at most 72 characters. Reply with the subject line and nothing else.

Changed files:
$diff" 2>/dev/null | tr -d '\r' | grep -v '^[[:space:]]*$' | head -n 1
}

session() {
  cd "$repo"

  printf '\033]0;Seele OS session\007'
  printf 'Seele OS session · %s\n' "$repo"
  printf 'Describe the change you want. The system rebuilds when you exit Pi.\n\n'

  seele-agent-run pi "$pi" --name "Seele OS session" --append-system-prompt "$briefing" || true

  if [[ -z $(repo_changes) ]]; then
    printf '\nNo changes in the working copy, so nothing to build.\n'
    read -r -p 'Press enter to close. ' _ </dev/tty || true
    return 0
  fi

  printf '\nRebuilding the system with nh os switch.\n'
  if ! "$nh" os switch; then
    printf '\nThe rebuild failed. The working copy is untouched, so you can reopen this session and keep going.\n'
    read -r -p 'Press enter to close. ' _ </dev/tty || true
    return 1
  fi

  printf '\nThe system is running the new configuration.\n'
  if confirm 'Record this change?'; then
    local message
    printf 'Writing a commit message…\n'
    message=$(commit_message)
    if [[ -z $message ]]; then
      printf 'Could not generate a message. Nothing was committed.\n'
    else
      printf '\n  %s\n\n' "$message"
      if confirm 'Commit with this message?'; then
        jj commit -m "$message"
        printf 'Committed.\n'
      else
        printf 'Nothing was committed.\n'
      fi
    fi
  fi

  read -r -p 'Press enter to close. ' _ </dev/tty || true
}

case ${1:-open} in
  open)
    "$hyprctl" dispatch workspace "$workspace" >/dev/null 2>&1 || true
    exec "$ghostty" --working-directory="$repo" --class=org.seele.os-session -e seele-os-session session
    ;;
  session) session ;;
  *)
    printf 'Usage: seele-os-session [open|session]\n' >&2
    exit 2
    ;;
esac
