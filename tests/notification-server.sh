#!/usr/bin/env bash
set -euo pipefail
# A private bus and a windowless shell keep this away from the user's desktop.
if [[ ${SEELE_NOTIFICATION_TEST_BUS:-0} != 1 ]]; then
  bus_config=$(mktemp)
  trap 'rm -f "$bus_config"' EXIT
  cat > "$bus_config" <<'CONF'
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>
CONF
  dbus-run-session --config-file="$bus_config" -- env SEELE_NOTIFICATION_TEST_BUS=1 bash "$0" "$@"
  exit
fi
quickshell=${1:?quickshell executable required}
shellctl=${2:?unwrapped shellctl executable required}
sources=${3:?shell source directory required}
work=$(mktemp -d)
shell_pid=
trap 'if [[ -n "$shell_pid" ]]; then kill "$shell_pid" 2>/dev/null || true; wait "$shell_pid" 2>/dev/null || true; fi; rm -rf "$work"' EXIT
mkdir -p "$work/config" "$work/runtime"
chmod 700 "$work/runtime"
export XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software
export LC_ALL=C.UTF-8
export SEELE_SHELL_PATH="$work/config"
export PATH="$(dirname "$quickshell"):$PATH"
cp "$sources/NotificationStore.qml" "$sources/notifications.js" "$work/config/"
# Exercise the production IPC handlers as well as the production state module.
node - "$sources/shell.qml" "$work/config/shell.qml" <<'JS'
const fs = require('node:fs');
const source = fs.readFileSync(process.argv[2], 'utf8');
const start = source.indexOf('    function notificationStatus():');
const end = source.indexOf('    function ping():', start);
if (start < 0 || end < start) throw Error('notification IPC handlers missing');
fs.writeFileSync(process.argv[3], `import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
  NotificationStore { id: notificationStore }
  IpcHandler {
    target: "seele-shell"
${source.slice(start,end)}
  }
}
`);
JS
"$quickshell" -n -p "$work/config" >"$work/log" 2>&1 &
shell_pid=$!
state() { "$shellctl" notification-status; }
wait_state() {
  for attempt in $(seq 1 100); do
    if state 2>/dev/null | jq -e "$1" >/dev/null; then return; fi
    sleep 0.05
  done
  cat "$work/log" >&2
  state >&2 || true
  echo "notification condition failed: $1" >&2
  exit 1
}
notify() {
  gdbus call --session --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications --method org.freedesktop.Notifications.Notify \
    -- Fixture "$1" '' "$2" "$3" "['default', 'Open', 'reply', 'Reply']" "$4" "$5" \
    | sed -n 's/(uint32 \([0-9]*\),)/\1/p'
}
wait_state '.notifications.count == 0'
resident=$(notify 0 'Resident' 'Reply here' "{'resident': <true>, 'value': <int32 25>}" -1)
wait_state ".notifications.items[0].id == $resident and .notifications.items[0].progress == 25"
"$shellctl" notification invoke "$resident" reply
wait_state '.notifications.count == 1'
notify "$resident" 'Updated' 'Still here' "{'resident': <true>, 'value': <int32 70>}" 0 >/dev/null
wait_state '.notifications.items[0].summary == "Updated" and .notifications.items[0].timeout == 0 and .notifications.items[0].progress == 70'
"$shellctl" notification retire "$resident"
wait_state '.notifications.count == 1 and (.notifications.popups | length) == 0'
"$shellctl" notification dismiss "$resident"
wait_state '.notifications.count == 0 and (.notifications.history | length) == 1'
if "$shellctl" notification invoke "$resident" reply; then exit 1; fi
transient=$(notify 0 'Transient' 'Skip history' "{'transient': <true>}" 500)
wait_state '.notifications.count == 0 and (.notifications.popups | length) == 0'
wait_state '(.notifications.history | length) == 1'
ordinary=$(notify 0 'Ordinary' 'Open app' '{}' -1)
wait_state '.notifications.count == 1'
"$shellctl" notification invoke "$ordinary" default
wait_state '.notifications.count == 0 and (.notifications.history | length) == 2'
"$shellctl" notification clear-history
wait_state '(.notifications.history | length) == 0'
"$shellctl" notification dnd
notify 0 'Quiet' 'Stay in inbox' '{}' -1 >/dev/null
wait_state '.dnd == true and .notifications.count == 1 and (.notifications.popups | length) == 0'
"$shellctl" notification dnd
wait_state '.dnd == false and (.notifications.popups | length) == 0'
if grep -Ei 'ReferenceError|TypeError|Cannot assign|is not a function' "$work/log"; then exit 1; fi
