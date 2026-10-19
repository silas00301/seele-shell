#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/shell" "$fixture/shared" "$fixture/Quickshell/Io"
source="${1:-$root/projects/shell}"
shared="${2:-$root/projects/shared}"
cp "$source/ShelfPanel.qml" "$fixture/shell/"
sed -i 's|import "shared" as Shared|import "../shared" as Shared|' "$fixture/shell/ShelfPanel.qml"
cp -r "$shared"/. "$fixture/shared/"
# Render with the real theme tokens, without loading a user theme or a ShellRoot.
python3 - "$fixture" <<'PY'
from pathlib import Path
import sys
root=Path(sys.argv[1]);source=(root/'shared/Theme.qml').read_text()
source=source[:source.index('  FileView {')]+'}\n'
source=source.replace('import Quickshell\n','').replace('import Quickshell.Io\n','').replace('ShellRoot {','QtObject {')
source=source.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ','')
(root/'shared/TestTheme.qml').write_text(source)
PY
cat > "$fixture/Quickshell/Io/qmldir" <<'EOF'
module Quickshell.Io
Process 1.0 Process.qml
StdioCollector 1.0 StdioCollector.qml
EOF
cat > "$fixture/Quickshell/Io/Process.qml" <<'EOF'
import QtQuick
QtObject { property var command: []; property bool running: false; property var stdout }
EOF
cat > "$fixture/Quickshell/Io/StdioCollector.qml" <<'EOF'
import QtQuick
QtObject { property string text: ""; signal streamFinished() }
EOF
cat > "$fixture/tst_shelf.qml" <<'EOF'
import QtQuick
import QtTest
import "shell"
import "shared" as Shared
Item {
  Shared.TestTheme { id: testTheme }
  width: 540; height: 640
  QtObject {
    id: state
    property var items: [
      {id:"1",name:"one.txt",kind:"file",uri:"file:///one.txt",path:"/one.txt",available:true,selected:true,image:false,bytes:10,caption:""},
      {id:"2",name:"two.txt",kind:"file",uri:"file:///two.txt",path:"/two.txt",available:true,selected:false,image:false,bytes:20,caption:""},
      {id:"3",name:"three.txt",kind:"file",uri:"file:///three.txt",path:"/three.txt",available:true,selected:false,image:false,bytes:30,caption:""}
    ]
    property string error: ""
    property var calls: []
    readonly property var selected: items.filter(function(item) {return item.selected && item.available})
    readonly property var paths: selected.map(function(item) {return item.path})
    readonly property string uris: selected.map(function(item) {return item.uri}).join("\r\n")
    function send(value) { calls = calls.concat([value]) }
  }
  ShelfPanel {
    id: shelf
    width: 500
    store: state
    theme: testTheme
  }
  SignalSpy { id: preview; target:shelf; signalName:"previewRequested" }
  SignalSpy { id: closed; target:shelf; signalName:"closeRequested" }
  TestCase {
    name:"ShelfPanel"; when:windowShown
    function initTestCase() { failOnWarning(/Binding loop/); failOnWarning(/Cannot specify/); shelf.forceActiveFocus() }
    function init() { shelf.forceActiveFocus(); shelf.cursor=0; state.calls=[]; preview.clear(); closed.clear() }
    function test_snapshotKeepsDelegates() {
      var original=findChild(shelf,"shelfRow-2")
      verify(original)
      var replacement=JSON.parse(JSON.stringify(state.items))
      replacement[1].selected=true
      state.items=replacement
      tryVerify(function(){return findChild(shelf,"shelfRow-2").entry.selected})
      verify(findChild(shelf,"shelfRow-2")===original,"polling and selection retain the row object")
    }
    function test_navigationAndSelection() {
      keyClick(Qt.Key_J);compare(shelf.cursor,1)
      keyClick(Qt.Key_Space);compare(state.calls[state.calls.length-1].id,"2")
      keyClick(Qt.Key_Return);compare(preview.count,1);compare(preview.signalArguments[0][0],"/two.txt")
      keyClick(Qt.Key_G,Qt.ShiftModifier);compare(shelf.cursor,2)
      keyClick(Qt.Key_G);compare(shelf.cursor,2)
      keyClick(Qt.Key_G);compare(shelf.cursor,0)
      keyClick(Qt.Key_Delete);compare(state.calls[state.calls.length-1].op,"remove")
      keyClick(Qt.Key_Escape);compare(closed.count,1)
    }
    function test_dragAndClipboardRemainExplicit() {
      var handle=findChild(shelf,"shelfDragHandle")
      verify(handle.enabled);compare(handle.Drag.mimeData["text/uri-list"],"file:///one.txt")
      var collect=findChild(shelf,"shelfClipboard")
      collect.forceActiveFocus();keyClick(Qt.Key_Return)
      compare(state.calls[state.calls.length-1].op,"clipboard")
      compare(preview.count,0,"button activation does not preview a row")
    }
  }
}
EOF
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software
export QML_IMPORT_PATH="$fixture${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}"
qmltestrunner -input "$fixture/tst_shelf.qml"
