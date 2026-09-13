import QtQuick
import "production/shared/Native.js" as Bridge
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
      {entity_id:"fan.desk",name:"Fan",state:"on",unit:"",room:"Office",favorite:false,available:true,controllable:true,speed_control:true,percentage:50,percentage_step:25},
      {entity_id:"switch.coffee",name:"Coffee machine",state:"off",unit:"",room:"Kitchen",favorite:true,available:true,controllable:true},
      {entity_id:"binary_sensor.door",name:"Front door",state:"unavailable",unit:"",device_class:"door",room:"Hall",favorite:false,available:false,controllable:false}
    ]
    property var preferences: entities.map(e => ({entity_id:e.entity_id,name:"",room:"",favorite:e.favorite}))
    property var catalog: entities.concat([{entity_id:"switch.fan",name:"Fan",state:"off",unit:"",room:"Bedroom",favorite:false,available:true,controllable:true}])
    signal setupComplete()
    function groups() { return Bridge.call("home_assistant.project", [entities, preferences]).groups }
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
    implicitWidth: theme.homeAssistantWidth
    implicitHeight: panel.implicitHeight + theme.panelMargin * 2
    color: theme.base
    Rectangle {
      id: scene
      anchors.fill: parent
      color: theme.base
      Shared.PanelSurface { theme: theme }
      Shell.HomeAssistantPanel {
        id: panel
        theme: theme
        store: fixture
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: theme.panelMargin }
      }
    }
  }

  Timer {
    interval: 300
    running: true
    repeat: true
    onTriggered: {
      stop()
      if (theme.stage === 3) {
        panel.closed()
        if (fixture.catalogOpen) throw new Error("Catalog remains open after closing")
        panel.opened()
        if (!fixture.catalogOpen) throw new Error("Reopened picker did not resume discovery")
      }
      if (panel.implicitHeight <= 0 || panel.implicitHeight > panel.maximumHeight) throw new Error("Invalid panel height")
      scene.grabToImage(function(result) {
        var names = ["setup", "home", "lights", "devices", "offline", "empty", "small-screen"]
        if (!result.saveToFile(Quickshell.env("SEELE_HA_RENDER_DIR") + "/" + names[theme.stage] + ".png")) throw new Error("Could not render panel")
        theme.stage++
        if (theme.stage === 1) { fixture.configured = true; panel.expanded = "" }
        if (theme.stage === 2) panel.expanded = "light.desk"
        if (theme.stage === 3) { panel.page = "devices"; panel.editingId = "light.desk" }
        if (theme.stage === 4) { panel.page = "home"; panel.expanded = ""; fixture.connected = false }
        if (theme.stage === 5) { fixture.connected = true; fixture.entities = [] }
        if (theme.stage === 6) { panel.page = "devices"; panel.maximumHeight = 300 }
        if (theme.stage === 7) { console.log("HOME_ASSISTANT_PANEL_PASS"); Qt.quit() }
      })
      restart()
    }
  }
}
