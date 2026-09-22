import QtQuick
import QtQuick.Controls
import QtTest
import "production" as Production
import "production/shared" as Shared

Item {
  width: 520
  height: 640
  Shared.Theme { id: theme }
  Production.NetworkActivityStore { id: liveStore }
  QtObject {
    id: store
    property var snapshot: ({elapsed: 42, limited: false, interfaceLimit: 256, historyCapacity: 60})
    property string error: ""
    property bool received: true
    property var rows: []
    property string selectedId: ""
    property int resets: 0
    property int retries: 0
    readonly property int selectedIndex: rows.findIndex(function(row) { return row.id === selectedId })
    readonly property var selected: selectedIndex < 0 ? null : rows[selectedIndex]
    function select(index) { selectedId = rows[index].id }
    function reset() { resets++ }
    function retry() { retries++ }
  }
  Rectangle { anchors.fill: parent; color: theme.base }
  Shared.PanelSurface {
    theme: theme
    anchors { left: parent.left; right: parent.right; top: parent.top; margins: theme.panelMargin }
    height: column.implicitHeight + theme.panelMargin * 2
    Column {
      id: column
      anchors { left: parent.left; right: parent.right; top: parent.top; margins: theme.panelMargin }
      spacing: theme.panelSpacing
      Shared.PanelHeader { theme: theme; width: parent.width; glyph: "󰛳"; title: "Network activity"; detail: "Live traffic on this machine" }
      Production.NetworkActivityPanel { id: panel; theme: theme; store: store; width: parent.width }
    }
  }
  TestCase {
    name: "NetworkActivity"
    when: windowShown
    function sample(id, name, state) {
      var rx = [], tx = []
      for (var i = 0; i < 60; i++) {
        rx.push(i > 24 && i < 29 ? null : 250000 + Math.sin(i * 0.4) * 90000 + (i > 40 ? 270000 : 0))
        tx.push(40000 + Math.sin(i * 0.3) * 24000)
      }
      return {id:id,name:name,state:state,status:"Live · sampled every second",rxRate:520000,txRate:49000,rxLabel:"507.8 KiB/s",txLabel:"47.9 KiB/s",rxTotal:"21.4 MiB",txTotal:"2.0 MiB",incomplete:false,rx:rx,tx:tx,maximum:1048576,scaleLabel:"1.0 MiB/s"}
    }
    function init() {
      store.rows = [sample("2:1", "ethernet0", "Up"), sample("3:2", "wifi0", "Down")]
      store.selectedId = "2:1"
      store.error = ""
      store.resets = 0
      panel.forceActiveFocus()
      wait(30)
    }
    function test_keyboard_interface_and_reset() {
      var selector = findChild(panel, "interfaceSelector")
      verify(selector.activeFocus)
      compare(selector.displayText, "ethernet0")
      keyClick(Qt.Key_Down)
      compare(store.selectedId, "3:2")
      compare(selector.displayText, "wifi0")
      var reset = findChild(panel, "resetActivity")
      reset.forceActiveFocus()
      keyClick(Qt.Key_Return)
      compare(store.resets, 1)
      keyClick(Qt.Key_Space)
      compare(store.resets, 2)
    }
    function test_popup_bounded_and_pointer_selection() {
      var selector = findChild(panel, "interfaceSelector")
      var many = []
      for (var i = 0; i < 30; i++) many.push(sample(String(i), "interface" + i, "Unknown"))
      store.rows = many
      store.selectedId = "0"
      mouseClick(selector)
      tryCompare(selector.popup, "visible", true)
      verify(selector.popup.height <= theme.controlHeight * 6)
      keyClick(Qt.Key_Down)
      keyClick(Qt.Key_Return)
      compare(store.selectedId, "1")
      tryCompare(selector.popup, "visible", false)
    }
    function test_disappearance_never_switches_silently() {
      store.rows = [sample("3:2", "wifi0", "Down")]
      wait(10)
      compare(panel.entry, null)
      compare(store.selectedId, "2:1")
      compare(findChild(panel, "interfaceSelector").displayText, "Choose an interface")
      store.select(0)
      compare(panel.entry.name, "wifi0")
    }
    function test_owned_process_ignores_late_callbacks() {
      liveStore.panelOpen = true
      var old = liveStore.worker
      verify(old !== null)
      verify(old.running)
      old.stdout.read(JSON.stringify({version:1, rows:[sample("old", "old0", "Up")]}))
      compare(liveStore.rows.length, 1)
      liveStore.panelOpen = false
      compare(old.running, false)
      liveStore.panelOpen = true
      var fresh = liveStore.worker
      verify(fresh !== old)
      old.stdout.read(JSON.stringify({version:1, rows:[sample("stale", "stale0", "Up")]}))
      old.exited(0, 0)
      compare(liveStore.rows.length, 0)
      compare(liveStore.error, "")
      fresh.stdout.read(JSON.stringify({version:1, rows:[sample("fresh", "new0", "Up")]}))
      compare(liveStore.selectedId, "fresh")
      liveStore.panelOpen = false
      compare(fresh.running, false)
      wait(20)
      compare(liveStore.worker, null)
    }
    function test_render() {
      wait(100)
      var image = grabImage(panel.parent.parent)
      verify(image.width > 400)
      verify(image.height > 400)
      image.save("network-activity.png")
      var chart = findChild(panel, "trafficChart")
      compare(chart.series.length, 2)
      compare(chart.series[0].values[26], null)
      compare(chart.maximum, 1048576)
    }
  }
}
