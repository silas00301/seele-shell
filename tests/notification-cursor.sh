#!/usr/bin/env bash
set -euo pipefail
sources=${1:?shell source directory required}
shift
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/shared"
cp "$sources/notifications.js" "$work/"
shared="$sources/shared"
[[ -d "$shared" ]] || shared="$sources/../shared"
cp "$shared/Native.js" "$work/shared/"
sed -i 's|../shared/Native.js|shared/Native.js|' "$work/notifications.js"
node - "$sources/shell.qml" "$work/tst_cursor.qml" <<'JS'
const fs=require('node:fs');
const source=fs.readFileSync(process.argv[2],'utf8');
const start=source.indexOf('  component NotificationList:');
const end=source.indexOf('  // Notification popups',start);
if(start<0 || end<start) throw Error('production notification list missing');
fs.writeFileSync(process.argv[3],`import QtQuick
import QtTest
import "notifications.js" as Notifications
Item {
  id: root
  width: 320; height: 120
  property var systemData: ({notifications:{items:[],history:[]}})
  property int spaceMedium: 8
  property int spaceSmall: 6
  property int spaceTight: 4
  property int durationNormal: 0
  property var calls: []
  function notificationPopupEntries() { return [] }
  function notificationActionable(entry) { return false }
  function activateNotification(id) { calls.push(id) }
  function dismissNotification(id) { calls.push(id) }
  function runControl(action, value, extra) { calls.push(action) }
  QtObject { id: notificationStore; property var controller: ({group:function(){},pin:function(){}}) }
  component SeeleListView: ListView {}
  // Only the card's geometry and input bindings are substituted. The production
  // list, native arrays, reconciliation bindings and scroll methods run intact.
  component NotificationCard: Rectangle {
    property var entry: ({})
    property string group: ""
    property bool history: false
    property bool popup: false
    property bool alwaysUnfolded: false
    property bool keyboardSelected: false
    property bool groupLead: false
    property bool stacked: false
    property bool collapsible: false
    property int count: 1
    property int depth: 0
    signal toggled()
    height: 60
  }
${source.slice(start,end)}
  NotificationList { id: list; anchors.fill: parent }
  TestCase {
    name: "NotificationCursor"
    when: windowShown
    function initTestCase() { failOnWarning(/Binding loop/) }
    function test_identity_folding_and_scrolling() {
      root.systemData={notifications:{items:[{id:1,app_name:"Chat"},{id:2,app_name:"Chat"},{id:3,app_name:"Chat"},{id:4,app_name:"Mail"}],history:[]}}
      tryCompare(list,"count",2)
      compare(list.cursor.id,"")
      list.reconcileCursor(1)
      compare(list.cursor.id,"1")
      list.toggleGroup("app:chat")
      tryVerify(function(){return list.model[0].expanded})
      list.reconcileCursor(1)
      compare(list.cursor.id,"2")
      list.reconcileCursor(1)
      compare(list.cursor.id,"3")
      tryVerify(function(){return list.contentY>0})
      list.toggleGroup("app:chat")
      tryCompare(list,"cursorId","1")
      list.reconcileCursor(1)
      compare(list.cursor.id,"4")
      root.systemData={notifications:{items:[{id:1,app_name:"Chat"},{id:5,app_name:"Mail"}],history:[]}}
      tryCompare(list,"cursorId","5")
      list.resetCursor()
      compare(list.cursor.id,"")
    }
  }
}
`);
JS
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner -input "$work" "$@"
