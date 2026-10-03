#!/usr/bin/env bash
set -euo pipefail

sources=${1:?usage: focus-exit-cue.sh SOURCES QT_IMPORT}
qt_import=${2:?usage: focus-exit-cue.sh SOURCES QT_IMPORT}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/shots"
cp "$sources/FocusExitCue.qml" "$sources/FocusExitRim.qml" "$work/"
sed "s|__SHOT_DIR__|$work/shots|g" "$(dirname "${BASH_SOURCE[0]}")/tst_focusexit.qml" > "$work/tst_focusexit.qml"

set +e
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner \
  -import "$qt_import" \
  -input "$work/tst_focusexit.qml"
status=$?
set -e
if [[ -d /opt/cursor/artifacts ]]; then
  cp "$work/shots/"*.png /opt/cursor/artifacts/ 2>/dev/null || true
fi
test "$status" -eq 0
test -s "$work/shots/focus-exit-peak.png"
test -s "$work/shots/focus-exit-dismissed.png"
