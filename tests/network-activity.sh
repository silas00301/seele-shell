#!/usr/bin/env bash
set -euo pipefail
umask 077
sources=${1:?shell source directory required}
fixture=${2:?Qt fixture required}
qt_import=${3:?Qt import directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/production/shared" "$work/runtime"
cp "$sources/NetworkActivityStore.qml" "$sources/NetworkActivityPanel.qml" "$work/production/"
shared="$sources/shared"
[[ -d "$shared" ]] || shared="$sources/../shared"
cp "$shared/"*.qml "$shared/"*.js "$work/production/shared/"
sed -i 's|import "../shared" as Shared|import "shared" as Shared|' "$work/production/NetworkActivityPanel.qml"
# Keep production theme tokens and painting; Quickshell's config IO and root
# require its executable and are unnecessary for offscreen panel interactions.
python3 - "$work/production/shared/Theme.qml" <<'PY'
import re, sys
from pathlib import Path
path = Path(sys.argv[1])
source = path.read_text().split('  FileView {')[0]
source = re.sub(r'import Quickshell.*\n', '', source).replace('ShellRoot {', 'Item {')
source = source.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ', '').replace('Qt.resolvedUrl("grain.png")', '""')
path.write_text(source + '}\n')
PY
# Transport stand-ins exercise the production store's Component ownership and
# actual QML signals without launching Quickshell. Native sampling has its own
# real-worker fixture; no worker policy is reimplemented here.
mkdir -p "$work/Quickshell/Io"
cat > "$work/Quickshell/qmldir" <<'QML'
module Quickshell
Scope 1.0 Scope.qml
QML
cat > "$work/Quickshell/Scope.qml" <<'QML'
import QtQuick
Item {}
QML
cat > "$work/Quickshell/Io/qmldir" <<'QML'
module Quickshell.Io
Process 1.0 Process.qml
SplitParser 1.0 SplitParser.qml
QML
cat > "$work/Quickshell/Io/Process.qml" <<'QML'
import QtQuick
QtObject {
  property bool running: false
  property var command
  property bool stdinEnabled
  property QtObject stdout
  signal exited(int code, int status)
  function write(value) {}
}
QML
cat > "$work/Quickshell/Io/SplitParser.qml" <<'QML'
import QtQuick
QtObject { signal read(string data) }
QML
cp "$fixture" "$work/tst_networkactivity.qml"
cd "$work"
XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$work" -import "$qt_import" -input "$work/tst_networkactivity.qml"
