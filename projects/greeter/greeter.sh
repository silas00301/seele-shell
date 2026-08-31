#!/usr/bin/env bash
set -u

if [[ "${1:-}" == "--help" ]]; then
  echo "Usage: seele-greeter"
  exit 0
fi

@QUICKSHELL@ -n -p @CONFIG@
status=$?

# greetd starts the real session as soon as Quickshell accepts it. Tear down
# the temporary compositor after the greeter exits so the new compositor can
# acquire the seat.
@HYPRCTL@ dispatch exit >/dev/null 2>&1 || true
exit "$status"
