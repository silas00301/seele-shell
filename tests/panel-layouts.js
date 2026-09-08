// Render the production fragments, not a second copy of their layout.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {spawnSync} = require('node:child_process');
const source = path.resolve(process.argv[2]);
const qmlImport = process.argv[3];
const shell = fs.readFileSync(path.join(source, 'shell.qml'), 'utf8');
function block(start, end) {
  const first = shell.indexOf(start);
  const last = shell.indexOf(end, first);
  if (first < 0 || last < first) throw Error(`Missing production fragment: ${start}`);
  return shell.slice(first, last);
}
const focus = block('        Column {\n          id: focusContent', '\n      }\n    }\n  }\n\n  // Calendar');
const clock = block('                Item {\n                  id: zoneTimeLabels', '\n                Shared.ActionButton');
const addresses = block('              SectionRule {\n                width: parent.width\n                label: "IP ADDRESSES"', '\n              Text {\n                width: parent.width\n                visible: networkWindow.addresses.length');
const work = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-panel-tests-'));
try {
  const shared = fs.existsSync(path.join(source, 'shared')) ? path.join(source, 'shared') : path.resolve(source, '../shared');
  fs.cpSync(shared, path.join(work, 'shared'), {recursive: true});
  // qmltestrunner cannot load Quickshell's executable-only plugins. Keep the
  // visual tokens and timer commands, omitting only config IO and reload storage.
  const themePath = path.join(work, 'shared/Theme.qml');
  const theme = fs.readFileSync(themePath, 'utf8').split('  FileView {')[0]
    .replace(/import Quickshell.*\n/g, '').replace('ShellRoot {', 'Item {')
    .replace(/Quickshell.env\("SEELE_SHELL_WALLPAPER"\) \|\| /, '');
  fs.writeFileSync(themePath, theme + '}\n');
  fs.copyFileSync(path.join(source, 'focus.js'), path.join(work, 'focus.js'));
  fs.writeFileSync(path.join(work, 'tst_panels.qml'), `
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtTest
import "focus.js" as Focus
import "shared" as Shared
TestCase {
  id: testCase
  name: "PanelLayouts"
  when: windowShown
  visible: true
  width: 480; height: 800
  Shared.Theme { id: root; function closeOverlays() {} }
  component PanelHeader: Shared.PanelHeader { theme: root }
  component CardEdge: Shared.CardEdge { theme: root }
  component SectionLabel: Shared.SectionLabel { theme: root }
  component SeeleListView: Shared.SeeleListView { theme: root }
  component SlimScrollBar: Shared.SlimScrollBar { theme: root }
  component NotificationButton: Shared.ActionButton {
    theme: root
    property alias label: button.text
    property string successLabel
    property string controlAction
    property string value
    id: button
  }
  component SectionRule: Shared.SectionRule { theme: root }
  component SegmentWell: Shared.SegmentWell { theme: root }
  component Segment: Shared.Segment { theme: root }
  component MeterBar: Shared.MeterBar { theme: root }
  component IconButton: Shared.IconButton { theme: root }
  component HoverWash: Shared.HoverWash { theme: root }
  IconButton { id: notificationClose; x: 400; y: 480; hoverTint: root.dangerColor }
  component HoverTip: QtObject { property var mouse; property bool inOverlay; property string text }
  QtObject {
    id: focusTimer
    property var timerState: Focus.initial()
    readonly property string label: Focus.label(timerState.remaining)
    function command(action, minutes) { timerState = Focus.update(timerState, action, Date.now(), minutes) }
  }
  Item { id: focusHost; width: 350; height: 350; ${focus} }
  Item {
    id: timezoneRow
    y: 380; width: 100; height: root.notificationRowHeight
    property var modelData: ({time: "19:12", day: "2026-09-08", id: "Europe/Berlin", offset: 7200})
    property bool pinned: false
    property int index: 0
    RowLayout { anchors.fill: parent; ${clock} }
  }
  QtObject {
    id: networkWindow
    property bool addressesExpanded: false
    property var addresses: Array.from({length: 12}, function(_, i) {
      return {label: "IPv4", value: "192.0.2." + (i + 1), detail: "Fixture interface"}
    })
  }
  QtObject { id: controlProcess; property bool running: false }
  Column { id: addressHost; y: 480; width: 350; spacing: root.spaceSmall; ${addresses} }
  QtObject { id: clockWindow; property bool copyPending: false; function copyClockTimestamp(offset, label) {} }
  QtObject { id: timezoneList; property int currentIndex: 0 }
  function findText(item, text) {
    if (item.text === text) return item
    for (var child of item.children || []) { var found = findText(child, text); if (found) return found }
    return null
  }
  function clickText(text) {
    var label = findText(focusHost, text)
    verify(label !== null, text + " exists")
    mouseClick(label, label.width / 2, label.height / 2)
    wait(20)
  }
  function test_notificationButtonTint() {
    var wash = notificationClose.children[0]
    compare(wash.tint, root.dangerColor)
    notificationClose.hovered = true
    wait(root.durationFast + 30)
    compare(wash.color, root.dangerColor)
    notificationClose.hovered = false
    wait(root.durationFast + 30)
    compare(wash.color.a, 0)
    compare(wash.color.r, root.dangerColor.r)
  }
  function test_addressesStayCompact() {
    compare(addressList.height, 0)
    compare(addressList.visible, false)
    var label = findText(addressHost, "IP ADDRESSES")
    verify(label !== null)
    mouseClick(label, label.width / 2, label.height / 2)
    tryCompare(networkWindow, "addressesExpanded", true)
    tryCompare(addressList, "height", root.controlHeight * 4)
    compare(addressList.count, 12)
    verify(addressList.contentHeight > addressList.height)
    addressHost.children[0].forceActiveFocus()
    keyClick(Qt.Key_Space)
    tryCompare(addressList, "height", 0)
    compare(addressList.visible, false)
  }
  function test_focusControls() {
    clickText("50 min")
    compare(focusTimer.timerState.status, "running")
    compare(focusTimer.timerState.duration, 3000)
    clickText("Pause")
    compare(focusTimer.timerState.status, "paused")
    clickText("Resume")
    compare(focusTimer.timerState.status, "running")
    clickText("Cancel")
    compare(focusTimer.timerState.status, "idle")
  }
  function test_focusKeyboard() {
    focusContent.forceActiveFocus()
    keyClick(Qt.Key_3)
    compare(focusTimer.timerState.status, "running")
    compare(focusTimer.timerState.duration, 300)
    keyClick(Qt.Key_Space)
    compare(focusTimer.timerState.status, "paused")
    keyClick(Qt.Key_Space)
    compare(focusTimer.timerState.status, "running")
    keyClick(Qt.Key_Delete)
    compare(focusTimer.timerState.status, "idle")
  }
  function test_clockLabelsDoNotOverlap() {
    wait(30)
    var time = findText(timezoneRow, "19:12")
    var date = findText(timezoneRow, "2026-09-08")
    verify(time !== null && date !== null)
    var timePos = time.mapToItem(timezoneRow, 0, 0)
    var datePos = date.mapToItem(timezoneRow, 0, 0)
    verify(datePos.y >= timePos.y + time.height, "date overlaps time: " + datePos.y + " < " + (timePos.y + time.height))
    verify(time.width > 0 && date.width > 0)
  }
}
`);
  const result = spawnSync('qmltestrunner', ['-import', qmlImport, '-input', work], {
    stdio: 'inherit', env: {...process.env, QT_QPA_PLATFORM: 'offscreen', QT_QUICK_BACKEND: 'software'},
  });
  process.exitCode = result.status ?? 1;
} finally {
  fs.rmSync(work, {recursive: true, force: true});
}
