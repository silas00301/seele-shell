#!/usr/bin/env bash
set -euo pipefail
quickshell=${1:?quickshell executable required}
sources=${2:?shell source directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -m 700 "$work/runtime"
cp "$sources/FocusTimer.qml" "$sources/focus.js" "$work/"
cat > "$work/shell.qml" <<'QML'
import QtQuick
import Quickshell
ShellRoot {
  property int stage: 0
  property int completions: 0
  function check(condition, message) {
    if (!condition) { console.error("FOCUS_FAIL: " + message); Qt.quit() }
    return condition
  }
  FocusTimer { id: focusTimer; onCompleted: completions++ }
  Timer {
    interval: 300; repeat: true; running: true
    onTriggered: {
      if (stage === 0) {
        if (!check(focusTimer.initialized, "cold-start initialization")) return
        focusTimer.command("start", 1 / 60)
        if (!check(focusTimer.timerState.status === "running", "start")) return
        focusTimer.command("pause")
        if (!check(focusTimer.timerState.status === "paused", "pause")) return
      } else if (stage === 1) {
        if (!check(focusTimer.timerState.remaining === 1, "paused deadline")) return
        focusTimer.command("resume")
        if (!check(focusTimer.timerState.status === "running", "resume")) return
      } else if (stage === 7) {
        if (!check(focusTimer.timerState.status === "done" && focusTimer.label === "Done", "countdown completion")) return
        if (!check(completions === 1, "exactly one completion notification")) return
        focusTimer.command("cancel")
        if (!check(focusTimer.timerState.status === "idle", "cancel")) return
        console.log("FOCUS_PASS")
        Qt.quit()
      }
      stage++
    }
  }
}
QML
XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  timeout 10 "$quickshell" --no-color -p "$work" > "$work/log" 2>&1
if ! grep -q 'FOCUS_PASS' "$work/log" || grep -Eq 'FOCUS_FAIL|ReferenceError|TypeError' "$work/log"; then
  tail -40 "$work/log" >&2
  exit 1
fi
printf '%s\n' 'Focus timer cold start, pause/resume, countdown, completion, and cancel passed'
