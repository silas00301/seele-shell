import QtQuick
import QtTest

TestCase {
  id: testCase
  name: "SystemState"
  when: windowShown
  width: 320
  height: 100

  property var state: null
  property var legacyData: ({ audioDevices: [] })
  Component { id: stateFactory; SystemState {} }

  QtObject {
    id: legacy
    readonly property var audioModel: testCase.legacyData.audioDevices.filter(function(device) { return device.kind === "output" })
  }
  SignalSpy { id: legacyChanges; target: legacy; signalName: "audioModelChanged" }

  // The shell filters device lists in bindings like this. A replaced shared
  // status object used to rebuild this model even for a volume-only update.
  Rectangle {
    id: panel
    width: 320
    height: 100
    color: "#1e1e2e"
    readonly property var audioModel: testCase.state
      ? testCase.state.audioDevices.filter(function(device) { return device.kind === "output" }) : []

    Column {
      Repeater {
        id: devices
        model: panel.audioModel
        delegate: Text {
          required property var modelData
          text: modelData.name
          color: "#cdd6f4"
        }
      }
    }
  }

  SignalSpy { id: audioChanges; target: panel; signalName: "audioModelChanged" }
  SignalSpy { id: notificationChanges; target: testCase.state; signalName: "notificationsChanged" }
  SignalSpy { id: volumeChanges; target: testCase.state; signalName: "volumeChanged" }

  function snapshot() {
    return {
      volume: 60,
      muted: false,
      audioDevices: [
        { id: 1, kind: "output", name: "Speakers", default: true },
        { id: 2, kind: "input", name: "Microphone", default: true }
      ],
      notifications: { count: 1, items: [{ id: 4, summary: "Message", time: 100 }], history: [] },
      headphones: { connected: true, name: "Headphones", battery: null },
      agentStates: { pi: { active: true, status: "working" } }
    }
  }

  function init() {
    state = createTemporaryObject(stateFactory, testCase)
    verify(state !== null)
    state.apply(snapshot())
    legacyData = snapshot()
    legacyChanges.clear()
    audioChanges.clear()
    notificationChanges.clear()
    volumeChanges.clear()
  }

  function cleanup() {
    state = null
  }

  function test_identical_polls_do_not_notify_or_rebuild_delegates() {
    var first = devices.itemAt(0)
    verify(first !== null)
    var list = state.audioDevices
    for (var i = 0; i < 1000; i++) {
      legacyData = snapshot()
      state.apply(snapshot())
    }
    compare(legacyChanges.count, 1000)
    compare(audioChanges.count, 0)
    compare(notificationChanges.count, 0)
    compare(volumeChanges.count, 0)
    verify(state.audioDevices === list)
    verify(devices.itemAt(0) === first)
  }

  function test_volume_updates_do_not_touch_other_models() {
    var first = devices.itemAt(0)
    for (var i = 0; i < 100; i++) {
      var next = snapshot()
      next.volume = i
      state.apply(next)
    }
    compare(state.volume, 99)
    compare(volumeChanges.count, 100)
    compare(audioChanges.count, 0)
    compare(notificationChanges.count, 0)
    verify(devices.itemAt(0) === first)
  }

  function test_changed_devices_update_the_visible_model() {
    var next = snapshot()
    next.audioDevices[0].name = "Headphones"
    state.apply(next)
    compare(audioChanges.count, 1)
    compare(devices.itemAt(0).text, "Headphones")
    compare(notificationChanges.count, 0)
  }

  function test_history_changes_preserve_current_notification_items() {
    var current = state.notifications.items
    state.apply({ notifications: { count: 1, items: [{ time: 100, summary: "Message", id: 4 }], history: [{ id: 3 }] } })
    compare(notificationChanges.count, 1)
    verify(state.notifications.items === current)
    compare(state.notifications.history[0].id, 3)
  }

  function test_partial_updates_preserve_other_fields_and_allow_reconciliation() {
    var audio = state.audioDevices
    state.apply({ volume: 75, microphoneMuted: true, bluetoothScanning: true })
    compare(state.volume, 75)
    compare(state.microphoneMuted, true)
    compare(state.bluetoothScanning, true)
    verify(state.audioDevices === audio)
    state.apply({ volume: 60, microphoneMuted: false, bluetoothScanning: false })
    compare(state.volume, 60)
    compare(state.microphoneMuted, false)
    compare(state.bluetoothScanning, false)
    compare(audioChanges.count, 0)
  }

  function test_removal_array_order_and_null_values_are_not_lost() {
    state.apply({ headphones: { connected: false }, audioDevices: [] })
    compare(state.headphones, { connected: false })
    compare(devices.count, 0)
    compare(state.retain([1, 2], [2, 1]), [2, 1])
    compare(state.retain({ battery: 50 }, { battery: null }), { battery: null })
    compare(state.retain([], {}), {})
    compare(state.retain({}, []), [])
    var unusual = JSON.parse('{"__proto__":{"name":"ordinary JSON"}}')
    var retained = state.retain({}, unusual)
    verify(Object.prototype.hasOwnProperty.call(retained, "__proto__"))
    compare(JSON.stringify(retained), JSON.stringify(unusual))
  }

  function test_input_objects_are_not_mutated() {
    var next = snapshot()
    var text = JSON.stringify(next)
    state.apply(next)
    compare(JSON.stringify(next), text)
    verify(next.audioDevices !== state.audioDevices)
  }

  function test_unchanged_and_unrelated_updates_render_identically() {
    waitForRendering(panel)
    var before = grabImage(panel)
    state.apply(snapshot())
    state.apply({ volume: 75, muted: true })
    wait(0)
    verify(before.equals(grabImage(panel)), "unchanged device UI must keep the same pixels")
  }
}
