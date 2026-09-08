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
if [[ ! -d "$work/production/shared" ]]; then
  cp -r "$sources/../shared" "$work/shared"
fi
if [[ ! -d "$work/production/shared" ]]; then
  cp -r "$sources/../shared" "$work/production/shared"
fi
cp "${4:?Home Assistant panel fixture required}" "$work/shell.qml"
export SEELE_HA_RENDER_DIR="${SEELE_HA_RENDER_DIR:-$work}"
mkdir -p "$SEELE_HA_RENDER_DIR"
if ! SEELE_TEST_SHELL="$work/production/shell.qml" \
  QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software \
  timeout 15 "$quickshell" --no-color -p "$work" > "$work/log" 2>&1; then
  tail -40 "$work/log" >&2
  exit 1
fi
if ! grep -q 'HOME_ASSISTANT_PANEL_PASS' "$work/log" || grep -Eq 'Error|Cannot assign|Binding loop|Unable to assign' "$work/log"; then
  tail -40 "$work/log" >&2
  exit 1
fi
printf '%s\n' 'Home Assistant setup, light controls, entity picker and offline state render on a private compositor'
