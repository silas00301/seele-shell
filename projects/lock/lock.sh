#!/usr/bin/env bash
set -euo pipefail

quickshell=@QUICKSHELL@
config=@CONFIG@

usage() {
  cat <<'EOF'
Usage: seele-lock [--status]

Lock the current Wayland session and wait until the compositor confirms that
every output is secure.

Options:
  --status  Print securing, secure, or unlocked
  -h        Show this help
EOF
}

status() {
  "$quickshell" ipc -p "$config" call seele-lock status 2>/dev/null
}

case "${1:-}" in
  --status)
    status || printf 'unlocked\n'
    exit 0
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  ""|--immediate)
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

if passwd_entry=$(getent passwd "${USER:-}" 2>/dev/null); then
  IFS=: read -r _ _ _ _ display_name _ _ <<<"$passwd_entry"
  display_name=${display_name%%,*}
  if [[ -n $display_name ]]; then
    export SEELE_LOCK_NAME=$display_name
  fi
fi

# Quickshell keys duplicate detection to the configuration path. Starting the
# same locker again is harmless, and the readiness loop below observes the one
# that already owns ext-session-lock-v1.
#
# An instance that failed to take the lock is not harmless, though: `-n` would
# exit against it without locking anything, and the readiness loop would then
# poll a leftover that is never going to secure. Reap that one first. Only ever
# while it reports `unlocked` -- killing a client that holds the lock leaves the
# compositor locked with nothing left alive to unlock it.
if [[ $(status || true) == unlocked ]]; then
  "$quickshell" kill -p "$config" >/dev/null 2>&1 || true
fi

"$quickshell" -n -d -p "$config"

for _ in {1..100}; do
  state=$(status || true)
  if [[ $state == secure ]]; then
    exit 0
  fi
  sleep 0.05
done

printf 'seele-lock: compositor did not confirm a secure lock\n' >&2
exit 1
