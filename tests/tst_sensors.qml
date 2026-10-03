import QtQuick
import QtQuick.Controls
import QtTest
import "production" as Production
import "production/shared" as Shared

Item {
  width: 480
  height: 900
  Shared.Theme { id: theme }
  Production.SensorsStore { id: liveStore }
  QtObject {
    id: store
    property var snapshot: ({elapsed: 42, cadenceSeconds: 2, summary: "", attention: 0, limited: false, skipped: 0, deviceLimit: 64})
    property string error: ""
    property bool received: true
    property var rows: []
    property int resets: 0
    property int retries: 0
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
      Shared.PanelHeader { theme: theme; width: parent.width; glyph: "󰔏"; title: "Sensors"; detail: store.snapshot.summary || "Temperatures and fans on this machine" }
      Production.SensorsPanel { id: panel; theme: theme; store: store; width: parent.width }
    }
  }
  TestCase {
    name: "Sensors"
    when: windowShown
    // Rows in the worker's own shape; tools/tests/sensors.py pins that shape
    // against the real executable.
    function reading(id, label, value, state, status, ratio, limits, kind) {
      return {id: id, kind: kind || "temperature", label: label, value: value, peak: value, limits: limits || "", state: state, status: status, ratio: ratio}
    }
    function rows() {
      return [
        {id: "k10temp@/pci/0000:00:18.3", title: "CPU", detail: "k10temp · 0000:00:18.3", readings: [
          reading("temp1", "Tctl", "92 °C", "critical", "Critical", 1, "High 80 °C · critical 90 °C"),
          reading("temp3", "Tccd1", "61.5 °C", "normal", "", null)]},
        {id: "nct6798@/isa", title: "Mainboard", detail: "nct6798 · nct6775.656", readings: [
          reading("temp7", "Temperature 7", "—", "unavailable", "No reading", null),
          reading("fan1", "Fan 1", "0 RPM", "stopped", "Stopped", null, "", "fan"),
          reading("fan2", "CPU fan", "1180 RPM", "normal", "", null, "Minimum 300 RPM", "fan")]}
      ]
    }
    function init() {
      store.rows = rows()
      store.error = ""
      store.received = true
      store.resets = 0
      store.retries = 0
      store.snapshot = ({elapsed: 42, cadenceSeconds: 2, summary: "Hottest 92 °C · CPU Tctl", attention: 1, limited: false, skipped: 0, deviceLimit: 64})
      wait(20)
    }
    function test_states_meters_and_captions() {
      var hot = findChild(panel, "k10temp@/pci/0000:00:18.3/temp1")
      verify(hot !== null)
      compare(findChild(hot, "value").color, theme.red)
      var meter = findChild(hot, "meter")
      verify(meter.visible)
      compare(meter.ratio, 1)
      compare(findChild(hot, "status").text, "Critical")
      compare(findChild(hot, "detail").text, "Peak 92 °C  ·  High 80 °C · critical 90 °C")
      var calm = findChild(panel, "k10temp@/pci/0000:00:18.3/temp3")
      verify(!findChild(calm, "meter").visible)
      verify(!findChild(calm, "status").visible)
      compare(findChild(calm, "value").color, theme.text)
      compare(findChild(calm, "detail").text, "Peak 61.5 °C  ·  No limits reported")
      var fan = findChild(panel, "nct6798@/isa/fan1")
      compare(findChild(fan, "status").text, "Stopped")
      compare(findChild(fan, "value").color, theme.subtext)
      compare(findChild(fan, "detail").text, "Peak 0 RPM  ·  No minimum reported")
    }
    function test_reset_from_keyboard() {
      var reset = findChild(panel, "resetSensors")
      verify(reset.visible)
      reset.forceActiveFocus()
      keyClick(Qt.Key_Return)
      compare(store.resets, 1)
      keyClick(Qt.Key_Space)
      compare(store.resets, 2)
    }
    function test_empty_error_and_retry() {
      store.rows = []
      wait(10)
      verify(!findChild(panel, "resetSensors").visible)
      store.received = false
      wait(10)
      store.error = "Sensors stopped. Retry starts a new session."
      var retry = findChild(panel, "retrySensors")
      tryCompare(retry, "visible", true)
      // The banner lays out after it appears; click where it settled.
      waitForRendering(panel)
      mouseClick(retry)
      compare(store.retries, 1)
    }
    function test_owned_process_ignores_late_callbacks() {
      liveStore.panelOpen = true
      var old = liveStore.worker
      verify(old !== null)
      verify(old.running)
      compare(old.command[0], "seele-sensors")
      old.stdout.read(JSON.stringify({version: 1, rows: rows()}))
      compare(liveStore.rows.length, 2)
      liveStore.panelOpen = false
      compare(old.running, false)
      compare(liveStore.rows.length, 0)
      liveStore.panelOpen = true
      var fresh = liveStore.worker
      verify(fresh !== old)
      old.stdout.read(JSON.stringify({version: 1, rows: rows()}))
      old.exited(0, 0)
      compare(liveStore.rows.length, 0)
      compare(liveStore.error, "")
      fresh.stdout.read("not json")
      verify(liveStore.error.indexOf("stopped") >= 0)
      liveStore.panelOpen = false
      wait(20)
      compare(liveStore.worker, null)
    }
    function test_render() {
      wait(100)
      var image = grabImage(panel.parent.parent)
      verify(image.width > 400)
      image.save("sensors.png")
    }
  }
}
