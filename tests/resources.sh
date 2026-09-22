#!/usr/bin/env bash
set -euo pipefail
umask 077
shell=${1:?shell source directory required}
fixture=${2:?Qt fixture required}
qt_import=${3:?Qt QML import directory required}
native_import=${4:?native QML import directory required}
quickshell_import=${5:?Quickshell QML import directory required}
lifecycle=${6:?lifecycle Qt fixture required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/runtime" "$work/Quickshell/Io"
cp "$shell/ResourcesPanel.qml" "$shell/ResourcesState.qml" "$shell/ResourcesStore.qml" "$work/"
cp -r "$shell/shared" "$work/shared"
sed -i 's|../shared|shared|g' "$work/ResourcesPanel.qml" "$work/ResourcesState.qml"
python3 - "$work" <<'PY'
from pathlib import Path
import sys
root=Path(sys.argv[1]);source=(root/'shared/Theme.qml').read_text()
source=source[:source.index('  FileView {')]+'}\n'
source=source.replace('import Quickshell\n','').replace('import Quickshell.Io\n','').replace('ShellRoot {','QtObject {')
source=source.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ','')
(root/'shared/TestTheme.qml').write_text(source)
PY
cp "$fixture" "$work/tst_resources.qml"
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -import "$native_import" -input "$work/tst_resources.qml"
cp "$lifecycle" "$work/tst_resources_lifecycle.qml"
cat > "$work/Quickshell/Io/qmldir" <<'QML'
module Quickshell.Io
Process 1.0 Process.qml
SplitParser 1.0 SplitParser.qml
StdioCollector 1.0 StdioCollector.qml
QML
cat > "$work/Quickshell/Io/Process.qml" <<'QML'
import QtQuick
QtObject {
  property bool running: false
  property var command
  property bool stdinEnabled
  property QtObject stdout
  property QtObject stderr
  signal started()
  signal exited(int code, int status)
  function write(value) {}
}
QML
cat > "$work/Quickshell/Io/SplitParser.qml" <<'QML'
import QtQuick
QtObject { signal read(string data) }
QML
cat > "$work/Quickshell/Io/StdioCollector.qml" <<'QML'
import QtQuick
QtObject {}
QML
PATH="$shell/../../bin:$PATH" XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$work" -import "$qt_import" -import "$native_import" -input "$work/tst_resources_lifecycle.qml"
