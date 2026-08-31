#!/usr/bin/env bash
set -euo pipefail

socket=${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is not set}/yubikey-touch-detector.socket

while true; do
  if [[ -S $socket ]]; then
    while IFS= read -r -N 5 event; do
      printf '%s\n' "$event"
    done < <(socat -u "UNIX-CONNECT:$socket" - 2>/dev/null) || true
  fi
  sleep 1
done
