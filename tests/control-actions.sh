#!/usr/bin/env bash
set -euo pipefail
control=${1:?control executable required}
work=$(mktemp -d)
trap 'jobs -pr | xargs -r kill 2>/dev/null || true; rm -rf "$work"' EXIT
mkdir -p "$work/bin"
export MOCK_ACTIONS="$work/actions"
export SEELE_CONTROL_NO_STATUS=1
export SEELE_LOCK="$work/bin/lock"
cat >"$SEELE_LOCK" <<'SH'
#!/usr/bin/env bash
printf 'lock\n' >>"$MOCK_ACTIONS"
exit "${MOCK_LOCK_EXIT:-0}"
SH
cat >"$work/bin/systemctl" <<'SH'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >>"$MOCK_ACTIONS"
exit "${MOCK_SYSTEMCTL_EXIT:-0}"
SH
cat >"$work/bin/hyprctl" <<'SH'
#!/usr/bin/env bash
if [[ "$*" == "clients -j" ]]; then
  printf '[{"address":"0xabc","pid":%s}]\n' "${MOCK_APP_PID:-0}"
else
  printf 'hyprctl %s\n' "$*" >>"$MOCK_ACTIONS"
fi
SH
for mock in "$work/bin/"*; do
  sed -i "1s|.*|#!$BASH|" "$mock"
done
chmod +x "$work/bin/"*
export PATH="$work/bin:$PATH"

: >"$MOCK_ACTIONS"
"$control" application quit 0xAbC
test "$(cat "$MOCK_ACTIONS")" = "hyprctl dispatch closewindow address:0xabc"
: >"$MOCK_ACTIONS"
if "$control" application quit not-an-address; then
  echo "invalid application address reported success" >&2
  exit 1
fi
test ! -s "$MOCK_ACTIONS"
sleep 30 &
app_pid=$!
export MOCK_APP_PID=$app_pid
"$control" application force-quit abc
wait "$app_pid" 2>/dev/null || true
if kill -0 "$app_pid" 2>/dev/null; then
  echo "force quit left the application running" >&2
  exit 1
fi
unset MOCK_APP_PID

for command in lock lock-suspend; do
  : >"$MOCK_ACTIONS"
  if MOCK_LOCK_EXIT=1 "$control" "$command"; then
    echo "failed lock reported success" >&2
    exit 1
  fi
  test "$(cat "$MOCK_ACTIONS")" = lock
done
: >"$MOCK_ACTIONS"
"$control" lock-suspend
test "$(cat "$MOCK_ACTIONS")" = $'lock\nsystemctl suspend'
if MOCK_SYSTEMCTL_EXIT=1 "$control" lock-suspend; then
  echo "failed suspend reported success" >&2
  exit 1
fi

for name in wpctl tailscale protonvpn bluetoothctl timeout; do
  cat >"$work/bin/$name" <<'SH'
#!/usr/bin/env bash
printf '%s %s\n' "${0##*/}" "$*" >>"$MOCK_ACTIONS"
exit 1
SH
  sed -i "1s|.*|#!$BASH|" "$work/bin/$name"
  chmod +x "$work/bin/$name"
done
for action in 'volume up' 'microphone mute' 'tailscale up' 'proton-vpn connect' 'audio-device 3' 'bluetooth-pair-worker AA:BB:CC:DD:EE:FF'; do
  : >"$MOCK_ACTIONS"
  read -r -a args <<<"$action"
  if "$control" "${args[@]}"; then
    echo "failed action reported success: $action" >&2
    exit 1
  fi
  test "$(wc -l <"$MOCK_ACTIONS")" -eq 1
done
: >"$MOCK_ACTIONS"
if "$control" audio-device; then exit 1; fi
test ! -s "$MOCK_ACTIONS"
