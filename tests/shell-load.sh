#!/usr/bin/env bash
set -euo pipefail
quickshell=${1:?quickshell executable required}
sources=${2:?shell source directory required}
sway=${3:?headless sway executable required}
work=$(mktemp -d)
compositor=
trap 'if [[ -n "$compositor" ]]; then kill "$compositor" 2>/dev/null || true; wait "$compositor" 2>/dev/null || true; fi; rm -rf "$work"' EXIT
mkdir -m 700 "$work/runtime" "$work/home"
export HOME="$work/home" XDG_CONFIG_HOME="$work/home" XDG_STATE_HOME="$work/home"
export XDG_RUNTIME_DIR="$work/runtime"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$work/no-session-bus"
unset DISPLAY WAYLAND_DISPLAY SWAYSOCK I3SOCK
printf '%s\n' 'xwayland disable' 'output * mode 800x600' > "$work/sway.conf"
WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1 \
  "$sway" --config "$work/sway.conf" > "$work/compositor.log" 2>&1 &
compositor=$!
for attempt in $(seq 1 100); do
  for socket in "$work/runtime"/wayland-*; do
    if [[ -S "$socket" ]]; then export WAYLAND_DISPLAY="$socket"; break; fi
  done
  [[ -z "${WAYLAND_DISPLAY:-}" ]] || break
  if ! kill -0 "$compositor" 2>/dev/null; then break; fi
  sleep 0.05
done
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  tail -40 "$work/compositor.log" >&2
  exit 1
fi
cp -r "$sources" "$work/production"
# Source checkouts use ../shared; installed packages carry shared/ inside them.
if [[ ! -d "$work/production/shared" && -d "$sources/../shared" ]]; then
  cp -r "$sources/../shared" "$work/shared"
fi
cat > "$work/shell.qml" <<'QML'
import QtQuick
import Quickshell
ShellRoot {
  property var production: null
  function check() {
    if (production.status === Component.Loading) return
    if (production.status === Component.Ready) console.log("SHELL_LOAD_PASS")
    else console.error("SHELL_LOAD_FAIL: " + production.errorString())
    Qt.quit()
  }
  Timer {
    interval: 1; running: true
    onTriggered: {
      // Compile every inline component with the real runtime imports. Do not
      // instantiate the desktop or start its workers, services, or file readers.
      production = Qt.createComponent("file://" + Quickshell.env("SEELE_TEST_SHELL"))
      production.statusChanged.connect(check)
      check()
    }
  }
}
QML
if ! SEELE_TEST_SHELL="$work/production/shell.qml" \
  QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software \
  timeout 15 "$quickshell" --no-color -p "$work" > "$work/log" 2>&1; then
  tail -40 "$work/log" >&2
  exit 1
fi
if ! grep -q 'SHELL_LOAD_PASS' "$work/log" || grep -Eq 'SHELL_LOAD_FAIL|ReferenceError|TypeError' "$work/log"; then
  tail -40 "$work/log" >&2
  exit 1
fi
printf '%s\n' 'Production shell compiles with the Quickshell runtime without starting the desktop'
