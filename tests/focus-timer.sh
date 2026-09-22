#!/usr/bin/env bash
set -euo pipefail
quickshell=${1:?quickshell executable required}
sources=${2:?shell source directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -m 700 "$work/runtime"
cp "$sources/FocusTimer.qml" "$sources/FocusPanel.qml" "$sources/focus.js" "$work/"
source "$(dirname "${BASH_SOURCE[0]}")/qml-fixture.sh"
copy_qml_shared "$sources" "$work"
cat > "$work/shell.qml" <<'QML'
import QtQuick
import Quickshell
import "shared" as Shared
ShellRoot {
  property int stage: 0
  property int completions: 0
  function check(condition, message) {
    if (!condition) { console.error("FOCUS_FAIL: " + message); Qt.quit() }
    return condition
  }
  Shared.Theme { id: theme }
  FocusTimer { id: focusTimer; onCompleted: completions++ }
  FocusPanel { id: panel; width: 318; theme: theme; timer: focusTimer }
  function find(item, name) {
    if (item.objectName === name) return item
    for (var child of item.children) { var found = find(child, name); if (found) return found }
    return null
  }
  Timer {
    interval: 300; repeat: true; running: true
    onTriggered: {
      if (stage === 0) {
        if (!check(focusTimer.initialized, "cold-start initialization")) return
        var input = find(panel, "focusCustomMinutes")
        var start = find(panel, "focusCustomStart")
        var extend = find(panel, "focusExtend")
        input.text = "241"
        if (!check(!start.enabled, "invalid custom duration is disabled")) return
        input.text = "37"
        if (!check(start.enabled, "valid custom duration is enabled")) return
        start.clicked()
        if (!check(focusTimer.timerState.duration === 2220, "production custom start")) return
        extend.clicked()
        if (!check(focusTimer.timerState.duration === 2520, "production extension")) return
        focusTimer.command("pause")
        extend.clicked()
        if (!check(focusTimer.timerState.status === "paused" && focusTimer.timerState.duration === 2820, "paused extension")) return
        focusTimer.command("custom", "240")
        if (!check(!extend.enabled, "maximum extension disabled")) return
        focusTimer.command("cancel")
        if (!check(!extend.enabled, "idle extension disabled")) return
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
        focusTimer.command("extend")
        if (!check(completions === 1 && focusTimer.timerState.status === "done", "completed extension cannot restart")) return
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
if ! XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  timeout 10 "$quickshell" --no-color -p "$work" > "$work/log" 2>&1; then
  tail -40 "$work/log" >&2
  exit 1
fi
if ! grep -q 'FOCUS_PASS' "$work/log" || grep -Eq 'FOCUS_FAIL|ReferenceError|TypeError' "$work/log"; then
  tail -40 "$work/log" >&2
  exit 1
fi
printf '%s\n' 'Focus timer cold start, pause/resume, countdown, completion, and cancel passed'
