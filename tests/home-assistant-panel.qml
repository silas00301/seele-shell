import QtQuick
import Quickshell
import "production/shared" as Shared
import "production" as Shell

Shared.Theme {
  id: theme
  property int stage: 0
  QtObject {
    id: fixture
    property bool configured: false
    property bool connected: true
    property bool ready: true
    property bool busy: false
    property bool settingsPending: false
    property bool catalogOpen: false
    property string url: "https://home.example"
    property string error: ""
    property string summary: "sensor.temperature"
    property var pending: ({})
    property var entities: [
      {entity_id:"light.desk",name:"Desk light",state:"on",unit:"",room:"Office",favorite:true,available:true,controllable:true,dimmable:true,temperature:true,brightness:65,kelvin:3200,min_kelvin:2200,max_kelvin:6500},
      {entity_id:"sensor.temperature",name:"Temperature",state:"21.4",unit:"°C",room:"Office",favorite:false,available:true,controllable:false},
      {entity_id:"sensor.humidity",name:"Humidity",state:"46",unit:"%",room:"Office",favorite:false,available:true,controllable:false,device_class:"humidity"},
      {entity_id:"fan.desk",name:"Fan",state:"on",unit:"",room:"Office",favorite:false,available:true,controllable:true,speed_control:true,percentage:50,percentage_step:25}
    ]
    property var preferences: [
      {entity_id:"light.desk",name:"Desk light",room:"",favorite:true},
      {entity_id:"sensor.temperature",name:"",room:"",favorite:false}
    ]
    property var catalog: entities.concat([{entity_id:"switch.fan",name:"Fan",state:"off",unit:"",room:"Bedroom",favorite:false,available:true,controllable:true}])
    signal setupComplete()
    function rows() { return [{heading:"Favorites"}, entities[0], {heading:"Office",detail:"21.4°C · 46%"}, entities[3]] }
    function preference(id) { return preferences.find(item => item.entity_id === id) || null }
    function discover(open) { catalogOpen = open }
    function refresh() {}
    function setup(url, token) {}
    function select(id) {}
    function save(entries, summary) {}
    function edit(id, field, value) {}
    function move(id, offset) {}
    function moveTarget(id, offset) { return -1 }
    function setValue(entity, desired) {}
    function setState(entity, desired) {}
  }
  PanelWindow {
    id: window
    visible: true
    implicitWidth: 440
    implicitHeight: panel.implicitHeight + theme.panelMargin * 2
    color: theme.base
    Shell.HomeAssistantPanel {
      id: panel
      theme: theme
      store: fixture
      anchors { left: parent.left; right: parent.right; top: parent.top; margins: theme.panelMargin }
    }
  }
  Timer {
    interval: 300
    running: true
    repeat: true
    onTriggered: {
      stop()
      if (theme.stage === 2) {
        panel.closed()
        if (fixture.catalogOpen) throw new Error("Catalog remains open after closing")
        panel.opened()
        if (!fixture.catalogOpen) throw new Error("Reopened picker did not resume discovery")
      }
      if (panel.implicitHeight <= 0 || panel.implicitHeight > 900) throw new Error("Invalid panel height")
      panel.grabToImage(function(result) {
        var names = ["setup", "lights", "devices", "offline"]
        if (!result.saveToFile(Quickshell.env("SEELE_HA_RENDER_DIR") + "/" + names[theme.stage] + ".png")) throw new Error("Could not render panel")
        theme.stage++
        if (theme.stage === 1) { fixture.configured = true; panel.expanded = "fan.desk" }
        if (theme.stage === 2) { panel.page = "devices"; panel.editingId = "light.desk" }
        if (theme.stage === 3) { panel.page = "home"; fixture.connected = false }
        if (theme.stage === 4) { console.log("HOME_ASSISTANT_PANEL_PASS"); Qt.quit() }
      })
      restart()
    }
  }
}
