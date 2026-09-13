import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared
import "production/shared/Native.js" as Bridge

TestCase {
  id: testCase
  name: "HomeAssistantInteraction"
  when: windowShown
  visible: true
  width: 460
  height: 760
  Shared.Theme { id: theme }
  readonly property var initial: [
    {entity_id:"light.desk",name:"Desk light",state:"on",room:"Office",favorite:true,available:true,controllable:true,dimmable:true,temperature:true,brightness:65,kelvin:3200,min_kelvin:2200,max_kelvin:6500},
    {entity_id:"sensor.temperature",name:"Temperature",state:"21.4",unit:"°C",room:"Office",favorite:true,available:true,controllable:false},
    {entity_id:"sensor.humidity",name:"Humidity",state:"46",unit:"%",room:"Office",favorite:false,available:true,controllable:false,device_class:"humidity"},
    {entity_id:"fan.desk",name:"Fan",state:"on",room:"Office",favorite:false,available:true,controllable:true,speed_control:true,percentage:50,percentage_step:25}
  ]
  QtObject {
    id: fixture
    property bool configured: true
    property bool connected: true
    property bool ready: true
    property bool busy: false
    property bool settingsPending: false
    property bool catalogOpen: false
    property string url: "https://home.example"
    property string error: ""
    property string summary: ""
    property var pending: ({})
    property var entities: []
    property var preferences: []
    property var catalog: []
    property var writes: []
    signal setupComplete()
    function groups() { return Bridge.call("home_assistant.project", [entities, preferences]).groups }
    function preference(id) { return preferences.find(item => item.entity_id === id) || null }
    function discover(open) { catalogOpen = open }
    function refresh() {}
    function setup(url, token) { writes = writes.concat([{url:url,token:token}]) }
    function select(id) {}
    function save(entries, summary) { writes = writes.concat([{entries:entries,summary:summary}]) }
    function edit(id, field, value) {}
    function move(id, offset) {}
    function moveTarget(id, offset) { return -1 }
    function setValue(entity, desired) {
      if (!connected || settingsPending || pending[entity.entity_id]) return
      writes = writes.concat([{id:entity.entity_id,desired:desired}])
      var next = Object.assign({}, pending)
      next[entity.entity_id] = desired
      pending = next
    }
    function setState(entity, desired) { setValue(entity, {state:desired}) }
  }
  Shell.HomeAssistantPanel {
    id: panel
    theme: theme
    store: fixture
    width: theme.homeAssistantWidth - theme.panelMargin * 2
  }
  function child(name) {
    var item = findChild(panel, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function init() {
    failOnWarning(/.?/)
    fixture.entities = JSON.parse(JSON.stringify(initial))
    fixture.preferences = fixture.entities.map(e => ({entity_id:e.entity_id,name:"",room:"",favorite:e.favorite}))
    fixture.catalog = fixture.entities.concat([{entity_id:"switch.extra",name:"Extra switch",room:"Bedroom",state:"off",available:true,controllable:true}])
    fixture.connected = true
    fixture.configured = true
    fixture.error = ""
    fixture.busy = false
    fixture.settingsPending = false
    fixture.pending = ({})
    fixture.writes = []
    panel.page = "home"
    panel.expanded = ""
    panel.editingId = ""
    panel.selectedOnly = true
    panel.maximumHeight = theme.homeAssistantMaximumHeight
    child("deviceSearch").clear()
    panel.opened()
    wait(theme.durationNormal + 20)
  }
  function cleanup() { panel.closed() }

  function test_readouts_and_rows_keep_identity_during_live_updates() {
    var reading = child("reading:sensor.temperature")
    var control = child("control:light.desk")
    panel.expanded = "light.desk"
    wait(theme.durationNormal + 20)
    var slider = findChild(control, "Brightness")
    slider.forceActiveFocus()
    var next = JSON.parse(JSON.stringify(fixture.entities))
    next[1].state = "22.1"
    fixture.entities = next
    wait(30)
    compare(child("reading:sensor.temperature"), reading)
    compare(child("control:light.desk"), control)
    verify(slider.activeFocus)
    compare(reading.payload.state_label, "22.1")
    compare(fixture.writes.length, 0)
  }
  function test_pointer_opens_controls_without_switching_power() {
    var control = child("control:light.desk")
    mouseClick(control, theme.controlHeight + theme.spaceLarge, theme.rowHeight / 2)
    compare(panel.expanded, "light.desk")
    compare(fixture.writes.length, 0)
    keyClick(Qt.Key_Escape)
    compare(panel.expanded, "")
  }
  function test_keyboard_power_and_pending_intent() {
    var power = child("power:light.desk")
    power.forceActiveFocus()
    keyClick(Qt.Key_Space)
    compare(fixture.writes.length, 1)
    compare(fixture.writes[0].desired.state, "off")
    compare(child("control:light.desk").isOn, false)
    verify(!power.enabled)
    keyClick(Qt.Key_Space)
    compare(fixture.writes.length, 1)
    fixture.pending = ({})
    power.forceActiveFocus()
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(fixture.writes.length, 1)
    keyClick(Qt.Key_Return)
    compare(fixture.writes.length, 2)
  }
  function test_slider_commits_once_and_keeps_requested_value() {
    panel.expanded = "light.desk"
    wait(theme.durationNormal + 20)
    var slider = findChild(child("control:light.desk"), "Brightness")
    slider.forceActiveFocus()
    keyClick(Qt.Key_Right)
    compare(fixture.writes.length, 1)
    compare(fixture.writes[0].desired.brightness, 66)
    compare(slider.value, 66)
    verify(!slider.enabled)
    fixture.entities = JSON.parse(JSON.stringify(fixture.entities))
    compare(slider.value, 66)
  }
  function test_drag_ignores_live_values_until_release() {
    panel.expanded = "light.desk"
    wait(theme.durationNormal + 20)
    var slider = findChild(child("control:light.desk"), "Brightness")
    mousePress(slider, slider.width * 0.65, slider.height / 2)
    mouseMove(slider, slider.width * 0.8, slider.height / 2)
    verify(slider.pressed)
    var chosen = slider.value
    var next = JSON.parse(JSON.stringify(fixture.entities))
    next[0].brightness = 10
    fixture.entities = next
    compare(slider.value, chosen)
    compare(fixture.writes.length, 0)
    mouseRelease(slider, slider.width * 0.8, slider.height / 2)
    compare(fixture.writes.length, 1)
    compare(fixture.writes[0].desired.brightness, Math.round(chosen))
    compare(slider.value, Math.round(chosen))
  }
  function test_offline_controls_and_readouts() {
    fixture.connected = false
    wait(20)
    verify(!child("power:light.desk").enabled)
    panel.expanded = "light.desk"
    wait(theme.durationNormal + 20)
    verify(!findChild(child("control:light.desk"), "Brightness").enabled)
    compare(child("reading:sensor.temperature").payload.state_label, "21.4")
    compare(fixture.writes.length, 0)
  }
  function test_picker_focus_search_and_atomic_details() {
    panel.page = "devices"
    wait(30)
    var search = child("deviceSearch")
    verify(search.activeFocus)
    verify(fixture.catalogOpen)
    search.text = "no matching room"
    wait(20)
    search.clear()
    wait(30)
    panel.editingId = "light.desk"
    wait(theme.durationNormal + 20)
    var device = child("device:light.desk")
    var fields = []
    function collect(item) {
      if (item.Accessible.name === "Display name" || item.Accessible.name === "Room") fields.push(item)
      for (var i = 0; i < item.children.length; i++) collect(item.children[i])
    }
    collect(device)
    compare(fields.length, 2)
    fields[0].text = "Work light"
    fields[1].text = "Study"
    compare(fixture.writes.length, 0)
    child("save:light.desk").clicked()
    compare(fixture.writes.length, 1)
    compare(fixture.writes[0].entries[0].name, "Work light")
    compare(fixture.writes[0].entries[0].room, "Study")
    panel.closed()
    verify(!fixture.catalogOpen)
    panel.opened()
    verify(fixture.catalogOpen)
  }
  function test_token_clears_on_submit_and_close() {
    fixture.configured = false
    panel.opened()
    wait(20)
    var token = child("accessToken")
    token.text = "synthetic-fixture-only"
    child("connectHome").clicked()
    compare(fixture.writes.length, 1)
    compare(token.text, "")
    token.text = "synthetic-fixture-only"
    panel.closed()
    compare(token.text, "")
  }
  function test_short_output_and_long_names_are_bounded() {
    var many = []
    for (var i = 0; i < 32; i++) many.push({entity_id:"sensor." + i,name:"A very long sensor name in a very long room " + i,state:"123456789012345678901234567890",unit:" kWh",available:i % 2 === 0,room:"A room name that must never extend outside this narrow panel",favorite:false})
    fixture.entities = many
    panel.maximumHeight = 320
    wait(30)
    verify(panel.implicitHeight <= 320)
    var last = child("reading:sensor.31")
    verify(last.width <= panel.width / 2)
    verify(last.parent.parent.width <= panel.width)
    fixture.entities = []
    wait(30)
    verify(panel.implicitHeight > theme.rowHeight)
    verify(panel.implicitHeight <= 320)
  }
}
