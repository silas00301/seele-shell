//@ pragma UseQApplication

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.Mpris
import Quickshell.Services.SystemTray
import Quickshell.Wayland
import Quickshell.Widgets
import "media.js" as Media
import "time.js" as Time

ShellRoot {
  id: root

  // Seele's native desktop shell.
  property color base: "#1e1e2e"
  property color mantle: "#181825"
  property color surface: "#313244"
  property color overlay: "#6c7086"
  property color text: "#cdd6f4"
  property color subtext: "#a6adc8"
  property color accent: "#b4befe"
  property color red: "#f38ba8"
  property color green: "#a6e3a1"
  property color yellow: "#f9e2af"
  property string fontFamily: "Maple Mono NF CN"
  // iOS-style privacy indicator colours, deliberately outside the theme palette.
  property color iosOrange: "#ff9f0a"
  property color iosGreen: "#30d158"
  property color iosRed: "#ff453a"
  property string wallpaper: Quickshell.env("SEELE_SHELL_WALLPAPER") || "/etc/wallpaper/wallpaper.jpg"

  // Shared shape and surface tokens. Hyprland rounds windows at 8px, so every
  // panel, button, and bar entry rounds the same way, and each hover, press,
  // and selection tint is defined once instead of per widget.
  readonly property int radius: 8
  readonly property int radiusSmall: 6
  readonly property int barHeight: 30
  readonly property int barItemHeight: 22
  readonly property int barSpacing: 2
  readonly property int barPadding: 4
  readonly property int panelGap: 5
  // One spring for every scrollable. Qt's default overshoot drifts long enough
  // to read as lag rather than as feedback, so the flick decelerates hard and
  // the rebound is short.
  readonly property int scrollRebound: 130
  readonly property int scrollDeceleration: 9000
  readonly property int scrollFlickVelocity: 2200
  readonly property int osdGap: 16
  readonly property int panelMargin: 16
  readonly property int panelSpacing: 10
  readonly property int scrollGutter: 8
  readonly property int scrollInset: 4
  readonly property int panelHeaderHeight: 28
  // Text needs more contrast than decorative borders and inactive glyphs.
  readonly property color mutedText: subtext
  // Textured chrome. Surfaces stay translucent so the compositor's blur
  // shows through, a quiet vertical wash gives them depth, and a fixed grain
  // film keeps a large panel from reading as flat plastic.
  readonly property string grain: "grain.png"
  readonly property real grainOpacity: 0.05
  readonly property color panelColor: alpha(base, 0.86)
  readonly property color panelBorder: alpha(accent, 0.65)
  readonly property color hoverColor: alpha(accent, 0.18)
  readonly property color pressColor: alpha(accent, 0.42)
  readonly property color selectedColor: alpha(accent, 0.24)
  readonly property color activeTint: alpha(accent, 0.14)
  readonly property color fillColor: alpha(accent, 0.45)
  readonly property color fillDanger: alpha(red, 0.45)
  readonly property color successColor: alpha(green, 0.25)
  readonly property color dangerTint: alpha(red, 0.14)
  readonly property color dangerColor: alpha(red, 0.28)
  readonly property color dangerPress: alpha(red, 0.48)

  property bool agentsOpen: false
  // Panels stay on the screen they were opened from. Tracking Hyprland's
  // focused monitor instead would move an open panel to another output the
  // moment the pointer crossed a screen edge.
  property string overlayScreen: ""
  property string osdScreen: ""
  property string notificationPopupScreen: ""
  property string controlPanel: ""
  property var mediaPanelPlayer: null
  // A module being dragged between the Control Center and the menu bar.
  // `dragKind` is "add" when it came from the panel and "remove" when it was
  // pulled off the bar; `dragOverBar` is the live drop decision.
  property string dragModule: ""
  property string dragKind: ""
  property bool dragOverBar: false
  // Set while a draggable bar entry is held. The bar opens its input region
  // down the screen on the press rather than once the drag is recognised,
  // because that region change takes a round trip and the pointer would
  // otherwise leave it — and lose the grab — before it applied.
  property string barPressModule: ""
  property bool trayMenuOpen: false
  property bool trayPinned: false
  readonly property bool trayExpanded: trayPinned
  property int volumeDrag: -1
  property int microphoneDrag: -1
  readonly property int outputVolumeMaximum: 150
  property string cameraPreviewDevice: ""
  property bool agentUsageOpen: false
  property bool agentModelsOpen: false
  property string agentMetricPeriod: "day"
  property bool notificationHistoryOpen: false
  // Mako no longer expires anything, so the popup owns its own lifetime. The
  // notification itself stays current in the panel either way: retiring a
  // popup hides a toast, it does not dismiss what raised it.
  readonly property int notificationPopupSeconds: 10
  property double notificationNow: 0
  property var notificationPopupRetired: ({})
  // A toast is a place to notice something and the panel is a place to read it,
  // so the panel shows a notification whole and only the toast keeps it to one
  // line until asked.
  property var notificationUnfolded: ({})
  property var clockData: ({ pinned: [], zones: [] })
  property var activeTrayItem: null
  property bool osdOpen: false
  property string osdKind: "volume"
  property bool headphonesOsdConnected: false
  property string headphonesOsdName: "Headphones"
  property string headphonesOsdKind: "airpods"
  property var yubikeyTouchSources: ({})
  property bool yubikeyTouchRequired: false
  property bool polkitPrompting: false
  property bool lockPrompting: false
  property bool statusInitialized: false
  property bool statusRefreshQueued: false
  property bool bluetoothStatusRefreshQueued: false
  property string pendingControlAction: ""
  property string pendingControlValue: ""
  property string pendingControlExtra: ""
  property string completedControlAction: ""
  property string completedControlValue: ""
  property string completedControlExtra: ""
  property string failedControlAction: ""
  property string failedControlValue: ""
  property string failedControlExtra: ""
  property int windowsCountdown: -1
  property var agentData: ({
    subscriptions: [],
    local: { today: {}, daily: [], periods: {}, models: [], totalTokens: 0, totalCost: 0 },
    launchers: []
  })
  readonly property var agentMetricData: {
    var periods = (agentData.local || {}).periods || {}
    return periods[agentMetricPeriod] || { totalTokens: 0, totalCost: 0, models: [] }
  }
  property var systemData: ({
    volume: 0,
    muted: false,
    microphoneVolume: 0,
    microphoneMuted: false,
    microphoneActive: false,
    connection: "Disconnected",
    connectionType: "",
    connectivity: "unknown",
    wifiEnabled: false,
    wifiAvailable: false,
    ipAddress: "",
    gateway: "",
    tailscale: { available: false, backend: "Unavailable", connected: false, needsLogin: false, name: "", ip: "", tailnet: "", peers: 0, onlinePeers: 0 },
    protonVpn: { available: false, connected: false, connection: "" },
    sshServer: { available: false, mode: "off", tailscaleAvailable: false, sshAvailable: false },
    bluetoothAvailable: false,
    bluetoothPowered: false,
    bluetoothConnected: 0,
    bluetoothScanning: false,
    bluetoothReceiver: false,
    bluetoothDiscoverable: false,
    bluetoothDevices: [],
    headphones: { connected: false, name: "", kind: "", battery: null, controls: false, noiseMode: "" },
    voxtypeStatus: "unavailable",
    cameraDevices: [],
    cameraDevice: "",
    cameraActive: false,
    screenRecording: false,
    audioDevices: [],
    batteries: [],
    trayHidden: [],
    barModules: ({}),
    airpodsEarDetection: true,
    agentStates: {},
    notifications: { count: 0, items: [], history: [] },
    dnd: false
  })
  property bool agentRefreshing: false
  property string bluetoothBusy: ""
  property string bluetoothAction: ""
  property int bluetoothScanIntent: -1
  property int bluetoothScanQueued: -1
  readonly property bool bluetoothScanActive: bluetoothScanIntent >= 0 ? bluetoothScanIntent === 1 : !!systemData.bluetoothScanning
  // Starting the bridge takes about a second, and the shared status poll can
  // land inside that window still reporting the old value. Holding the
  // requested state until the reported one agrees keeps the switch from
  // snapping back and forth on its own, the way the scan intent does.
  property int bluetoothReceiverIntent: -1
  readonly property bool bluetoothReceiverActive: bluetoothReceiverIntent >= 0 ? bluetoothReceiverIntent === 1 : !!systemData.bluetoothReceiver
  property string bluetoothForget: ""
  property var pairingRequest: ({})
  property string pairingScreen: ""
  readonly property bool pairingPrompting: !!(pairingRequest && pairingRequest.token)
  property string agentError: ""
  property var speedtestData: ({ ping: -1, jitter: -1, download: -1, upload: -1, server: "" })
  property string speedtestError: ""
  property string speedtestPhase: ""
  property bool speedtestReceived: false
  property date now: new Date()

  function alpha(color, opacity) {
    return Qt.rgba(color.r, color.g, color.b, opacity)
  }

  function focusedScreen(screen) {
    return !Hyprland.focusedMonitor || Hyprland.focusedMonitor.name === screen.name
  }

  function currentScreen() {
    return Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : ""
  }

  function pinnedScreen(pin, screen) {
    if (!screen) return false
    return pin === "" ? focusedScreen(screen) : pin === screen.name
  }

  function panelHere(panel, screen) {
    return controlPanel === panel && pinnedScreen(overlayScreen, screen)
  }

  function agentsHere(screen) {
    return agentsOpen && pinnedScreen(overlayScreen, screen)
  }

  function closeTrayMenu() {
    trayMenuOpen = false
    activeTrayItem = null
  }

  function closeOverlays() {
    cancelModuleDrag()
    agentsOpen = false
    controlPanel = ""
    mediaPanelPlayer = null
    overlayScreen = ""
    closeTrayMenu()
    windowsCountdown = -1
    windowsTimer.stop()
    bluetoothForget = ""
    notificationHistoryOpen = false
    bluetoothForgetTimer.stop()
  }

  function toggleLauncher(mode) {
    closeOverlays()
    Quickshell.execDetached(["seele-control", "launcher-toggle"])
  }

  function toggleAgents(screen) {
    var shouldOpen = !agentsOpen
    closeOverlays()
    agentsOpen = shouldOpen
    if (!agentsOpen) return
    overlayScreen = screen || currentScreen()
    if (!agentData.generatedAt || agentError !== "") refreshAgents()
  }

  function toggleControl(panel, screen) {
    var shouldOpen = controlPanel !== panel
    closeOverlays()
    controlPanel = shouldOpen ? panel : ""
    if (controlPanel === "") return
    overlayScreen = screen || currentScreen()
    refreshStatus()
  }

  function toggleMedia(player, screen) {
    var samePlayer = root.controlPanel === "media" && root.mediaPanelPlayer === player
    var shouldOpen = !!player && !(samePlayer && root.overlayScreen === (screen || root.currentScreen()))
    root.closeOverlays()
    if (!shouldOpen) return
    root.mediaPanelPlayer = player
    root.controlPanel = "media"
    root.overlayScreen = screen || root.currentScreen()
  }

  function toggleControls() {
    toggleControl("system")
  }

  function refreshAgents() {
    if (!agentProcess.running) {
      agentRefreshing = true
      agentError = ""
      agentProcess.running = true
    }
  }

  function agentMetricPeriodLabel() {
    return {
      day: "day",
      week: "7 days",
      month: "30 days",
      all: "all time"
    }[agentMetricPeriod] || "day"
  }

  function refreshStatus() {
    if (statusProcess.running) root.statusRefreshQueued = true
    else statusProcess.running = true
  }

  function refreshBluetoothStatus() {
    if (bluetoothStatusProcess.running) root.bluetoothStatusRefreshQueued = true
    else bluetoothStatusProcess.running = true
  }

  function parseAgentData(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      if (!parsed || !parsed.subscriptions) throw new Error("missing subscription data")
      agentData = parsed
      agentError = ""
    } catch (error) {
      agentError = String(error)
    }
  }

  function parseSystemData(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      if (parsed) {
        var nextHeadphones = parsed.headphones || ({})
        var currentHeadphones = root.systemData.headphones || ({})
        if (root.statusInitialized && !!nextHeadphones.connected !== !!currentHeadphones.connected) {
          root.headphonesOsdConnected = !!nextHeadphones.connected
          root.headphonesOsdName = String(nextHeadphones.name || currentHeadphones.name || "Headphones")
          root.headphonesOsdKind = String(nextHeadphones.kind || currentHeadphones.kind || "airpods")
          root.showTimedOsd("airpods")
        }
        var previousNotifications = (root.systemData.notifications || {}).items || []
        var nextNotifications = (parsed.notifications || {}).items || []
        var previousIds = {}
        for (var i = 0; i < previousNotifications.length; i++) previousIds[String(previousNotifications[i].id)] = true
        for (var j = 0; j < nextNotifications.length; j++) {
          if (!previousIds[String(nextNotifications[j].id)]) {
            root.notificationPopupScreen = root.currentScreen()
            break
          }
        }
        // Stamped before the assignment below, so the popup bindings never see
        // fresh items against a clock from the last tick.
        root.notificationNow = Date.now() / 1000
        systemData = parsed
        root.pruneNotificationPopups(parsed.notifications)
        root.statusInitialized = true
        if (root.volumeDrag >= 0 && Number(parsed.volume) === root.volumeDrag) root.volumeDrag = -1
        if (root.microphoneDrag >= 0 && Number(parsed.microphoneVolume) === root.microphoneDrag) root.microphoneDrag = -1
        root.reconcileBluetoothScanIntent(!!parsed.bluetoothScanning)
        root.reconcileBluetoothReceiverIntent(!!parsed.bluetoothReceiver)
      }
    } catch (error) {
      console.warn("seele-shell/status", error)
    }
  }

  function parseBluetoothData(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      if (!parsed) return
      root.patchSystemData({
        bluetoothAvailable: !!parsed.available,
        bluetoothPowered: !!parsed.powered,
        bluetoothScanning: !!parsed.scanning,
        bluetoothReceiver: !!parsed.receiver,
        bluetoothDiscoverable: !!parsed.discoverable,
        bluetoothConnected: Number(parsed.connected || 0),
        bluetoothDevices: parsed.devices || []
      })
      root.reconcileBluetoothScanIntent(!!parsed.scanning)
      root.reconcileBluetoothReceiverIntent(!!parsed.receiver)
    } catch (error) {
      console.warn("seele-shell/bluetooth-status", error)
    }
  }

  function reconcileBluetoothScanIntent(scanning) {
    if (root.bluetoothScanIntent >= 0 && scanning === (root.bluetoothScanIntent === 1)) root.bluetoothScanIntent = -1
    if (!scanning) bluetoothScanTimer.stop()
  }

  function reconcileBluetoothReceiverIntent(receiver) {
    if (root.bluetoothReceiverIntent >= 0 && receiver === (root.bluetoothReceiverIntent === 1)) root.bluetoothReceiverIntent = -1
  }

  function formatTokens(value) {
    var count = Number(value || 0)
    if (count >= 1000000000) return (count / 1000000000).toFixed(1) + "B"
    if (count >= 1000000) return (count / 1000000).toFixed(1) + "M"
    if (count >= 1000) return (count / 1000).toFixed(0) + "K"
    return String(Math.round(count))
  }

  function resetText(value) {
    if (!value) return ""
    var reset = new Date(value)
    var delta = reset.getTime() - now.getTime()
    if (!(delta > 0)) return "now"
    var minutes = Math.floor(delta / 60000)
    var hours = Math.floor(minutes / 60)
    var days = Math.floor(hours / 24)
    if (days > 0) return days + "d " + (hours % 24) + "h"
    if (hours > 0) return hours + "h " + (minutes % 60) + "m"
    return Math.max(1, minutes) + "m"
  }

  function workspaceIds(screen) {
    var ids = []
    var values = Hyprland.workspaces.values || []
    for (var i = 0; i < values.length; i++) {
      var workspace = values[i]
      var onScreen = workspace.monitor && screen && workspace.monitor.name === screen.name
      var occupied = workspace.toplevels && workspace.toplevels.values.length > 0
      if (workspace.id > 0 && onScreen && (workspace.active || occupied)) ids.push(workspace.id)
    }
    ids.sort(function(a, b) { return a - b })
    return ids
  }

  function workspaceActive(id, screen) {
    var values = Hyprland.workspaces.values || []
    for (var i = 0; i < values.length; i++) {
      var workspace = values[i]
      if (workspace.id === id && workspace.active && (!workspace.monitor || !screen || workspace.monitor.name === screen.name)) return true
    }
    return false
  }

  function workspaceOccupied(id) {
    var values = Hyprland.workspaces.values || []
    for (var i = 0; i < values.length; i++) {
      if (values[i].id === id) return values[i].toplevels.values.length > 0
    }
    return false
  }

  function activateWorkspace(id) {
    var values = Hyprland.workspaces.values || []
    for (var i = 0; i < values.length; i++) {
      if (values[i].id === id) {
        values[i].activate()
        return
      }
    }
  }

  function activeWindow(screen) {
    var monitors = Hyprland.monitors.values || []
    for (var i = 0; i < monitors.length; i++) {
      var monitor = monitors[i]
      if (screen && monitor.name === screen.name && monitor.activeWorkspace) {
        var windows = monitor.activeWorkspace.toplevels.values || []
        if (monitor.focused && Hyprland.activeToplevel && windowTitle(Hyprland.activeToplevel) !== "") return Hyprland.activeToplevel
        for (var j = 0; j < windows.length; j++) {
          if (windowTitle(windows[j]) !== "") return windows[j]
        }
        return null
      }
    }
    return null
  }

  function windowTitle(window) {
    if (!window) return ""
    var ipc = window.lastIpcObject || {}
    return String(ipc.title || window.title || "")
  }

  function windowClasses(window) {
    if (!window) return []
    var ipc = window.lastIpcObject || {}
    var candidates = [ipc.class, ipc.initialClass, window.appId]
    var classes = []
    for (var i = 0; i < candidates.length; i++) {
      var appId = String(candidates[i] || "").trim()
      if (appId !== "") classes.push(appId)
    }
    return classes
  }

  function windowIcon(window) {
    if (DesktopEntries.applications.values.length === 0) return ""
    var classes = root.windowClasses(window)
    for (var i = 0; i < classes.length; i++) {
      var entry = DesktopEntries.heuristicLookup(classes[i])
      if (entry && entry.icon) return Quickshell.iconPath(String(entry.icon))
    }
    return ""
  }

  // Window titles follow the document, not the application: Spotify names the
  // playing track and browsers name the page. Always label the entry with the app.
  function windowAppName(window) {
    var classes = root.windowClasses(window)
    for (var i = 0; i < classes.length; i++) {
      var entry = DesktopEntries.heuristicLookup(classes[i])
      if (entry && String(entry.name || "") !== "") return String(entry.name)
    }
    if (classes.length === 0) return ""
    var fallback = classes[0].split(".").pop().replace(/[-_]+/g, " ").trim()
    return fallback === "" ? "" : fallback.charAt(0).toUpperCase() + fallback.slice(1)
  }

  function windowLabel(window) {
    return root.windowAppName(window) || root.windowTitle(window)
  }

  function spotifyPlayer() {
    return spotifyMediaSlot.player
  }

  function devicePlayer() {
    return deviceMediaSlot.player
  }

  function mediaLabel(player) {
    return Media.label(player)
  }

  function mediaTitle(player) {
    return Media.title(player)
  }

  function mediaSubtitle(player) {
    return Media.subtitle(player)
  }

  function formatMediaTime(seconds) {
    seconds = Math.max(0, Math.floor(Number(seconds) || 0))
    var minutes = Math.floor(seconds / 60)
    var remainder = seconds % 60
    return minutes + ":" + (remainder < 10 ? "0" : "") + remainder
  }

  function mediaTimelineAvailable(player) {
    return Media.timelineAvailable(player)
  }

  function mediaIsLive(player) {
    return Media.liveStream(player)
  }

  // The bar keeps Spotify and the device player in separate entries, but the
  // Control Center carries one now-playing module, so it needs a single player.
  function nowPlayingPlayer() {
    return Media.activePlayer(Mpris.players.values || [])
  }

  function agentStatus(id) {
    var states = systemData.agentStates || {}
    return states[id] ? String(states[id].status || "idle") : "idle"
  }

  function bluetoothDevices() {
    return root.systemData.bluetoothPowered ? (root.systemData.bluetoothDevices || []) : []
  }

  // Devices playing into this machine. They stay in the one device list with
  // everything else and are only counted here, for what the receiver row says
  // about itself.
  function bluetoothSources() {
    return root.bluetoothDevices().filter(function(device) { return device.source && device.connected })
  }

  function bluetoothReceiverDetail() {
    // Discoverability belongs to the search row, which owns that window; this
    // row only reports what is actually playing here.
    var sources = root.bluetoothSources()
    var streaming = sources.filter(function(device) { return device.streaming }).length
    if (streaming > 0) return streaming + " device" + (streaming === 1 ? "" : "s") + " streaming"
    if (sources.length > 0) return sources.length + " device" + (sources.length === 1 ? "" : "s") + " connected"
    if (!root.bluetoothReceiverActive) return "Play a phone through this PC"
    return "Waiting for a paired device"
  }

  function bluetoothIcon(device) {
    var icon = String(device && device.icon || "")
    var name = String(device && device.name || "").toLowerCase()
    if (icon.indexOf("headset") >= 0 || icon.indexOf("headphone") >= 0 || /airpod|buds|headphone|headset|beats|wh-|wf-/.test(name)) return "󰋋"
    if (icon.indexOf("speaker") >= 0 || icon === "audio-card" || /speaker|soundcore|boom|jbl|sonos/.test(name)) return "󰓃"
    if (icon === "input-keyboard" || /keyboard|keychron|k[0-9]+ /.test(name)) return "󰌌"
    if (icon === "input-mouse" || /mouse|mx master/.test(name)) return "󰍽"
    if (icon === "input-gaming" || /controller|gamepad|dualsense|xbox/.test(name)) return "󰊴"
    if (icon === "phone" || /phone|pixel|galaxy|iphone/.test(name)) return "󰄜"
    if (icon === "computer" || /macbook|thinkpad|laptop/.test(name)) return "󰌢"
    if (icon === "video-display" || /\[tv\]|fernseher|television/.test(name)) return "󰔂"
    if (icon === "printer") return "󰐪"
    if (/watch|band/.test(name)) return "󰖐"
    return "󰂱"
  }

  function bluetoothDetail(device) {
    if (!device) return ""
    if (root.bluetoothForget === device.address) return "Tap again to forget"
    if (root.bluetoothBusy === device.address) {
      if (root.bluetoothAction === "trust") return "Updating autoconnect…"
      if (root.bluetoothAction === "forget") return "Forgetting…"
      return device.connected ? "Disconnecting…" : device.paired ? "Connecting…" : "Pairing…"
    }
    var suffix = device.trusted ? " · auto" : ""
    if (device.streaming) return "Streaming here" + suffix
    if (device.connected) return "Connected" + suffix
    if (device.paired) return "Paired" + suffix
    return "Available"
  }

  function bluetoothSignal(device) {
    if (!device || device.connected) return ""
    if (device.battery !== null && device.battery !== undefined) return device.battery + "%"
    if (device.rssi === null || device.rssi === undefined) return ""
    if (device.rssi >= -60) return "󰤨"
    if (device.rssi >= -75) return "󰤥"
    return "󰤟"
  }

  function toggleBluetoothDevice(device) {
    if (!device || !device.address) return
    root.bluetoothForget = ""
    bluetoothForgetTimer.stop()
    root.runBluetooth(device.connected ? "disconnect" : "connect", device.address)
  }

  function runBluetooth(command, value) {
    if (bluetoothProcess.running) return false
    root.bluetoothAction = String(command)
    if (command !== "scan" && command !== "toggle" && command !== "receiver" && command !== "pairing") root.bluetoothBusy = String(value || "")
    bluetoothProcess.command = ["seele-control", "bluetooth", String(command), String(value || "")]
    bluetoothProcess.running = true
    return true
  }

  function setBluetoothScanning(active) {
    root.bluetoothScanIntent = active ? 1 : 0
    if (active) bluetoothScanTimer.restart()
    else bluetoothScanTimer.stop()
    if (bluetoothProcess.running) {
      root.bluetoothScanQueued = active ? 1 : 0
      return
    }
    root.runBluetooth("scan", active ? "on" : "off")
  }

  function toggleBluetoothPower() {
    var powered = !root.systemData.bluetoothPowered
    if (!root.runBluetooth("toggle", "")) return
    root.patchSystemData({ bluetoothPowered: powered })
    if (!powered) {
      root.bluetoothScanIntent = 0
      root.bluetoothReceiverIntent = 0
      bluetoothScanTimer.stop()
    }
  }

  function toggleBluetoothReceiver() {
    var enabled = !root.bluetoothReceiverActive
    if (!root.runBluetooth("receiver", "toggle")) return
    root.bluetoothReceiverIntent = enabled ? 1 : 0
  }

  function setBluetoothPairing(payload) {
    try {
      var parsed = JSON.parse(String(payload || ""))
      if (!parsed || !parsed.token) return
      // Pin the prompt to the output that is focused when the request lands,
      // the way every other surface here pins itself at open time.
      root.pairingScreen = root.currentScreen()
      root.pairingRequest = parsed
    } catch (error) {
      console.warn("seele-shell/bluetooth-pairing", error)
    }
  }

  function clearBluetoothPairing() {
    root.pairingRequest = ({})
    root.pairingScreen = ""
  }

  function answerBluetoothPairing(verdict, value) {
    var token = String((root.pairingRequest || {}).token || "")
    if (token === "") return
    Quickshell.execDetached(["seele-control", "bluetooth-pairing-answer", token, String(verdict), String(value || "")])
    root.clearBluetoothPairing()
  }

  // The models that make this end type the code rather than compare one.
  function pairingWantsCode() {
    var kind = String((root.pairingRequest || {}).kind || "")
    return kind === "passkey" || kind === "pincode"
  }

  function pairingCode() {
    var code = String((root.pairingRequest || {}).passkey || "")
    // Grouped the way a phone shows it, so the two are compared at a glance.
    return code.length === 6 ? code.slice(0, 3) + " " + code.slice(3) : code
  }

  function forgetBluetoothDevice(device) {
    if (!device || !device.address) return
    if (root.bluetoothForget !== device.address) {
      root.bluetoothForget = device.address
      bluetoothForgetTimer.restart()
      return
    }
    root.bluetoothForget = ""
    bluetoothForgetTimer.stop()
    root.runBluetooth("forget", device.address)
  }

  function trayHiddenIds() {
    return root.systemData.trayHidden || []
  }

  function trayItemHidden(item) {
    return !!item && root.trayHiddenIds().indexOf(String(item.id)) >= 0
  }

  function trayItems() {
    var items = SystemTray.items.values || []
    var result = []
    for (var i = 0; i < items.length; i++) {
      if (root.trayExpanded || !root.trayItemHidden(items[i])) result.push(items[i])
    }
    return result
  }

  function trayHiddenCount() {
    var items = SystemTray.items.values || []
    var count = 0
    for (var i = 0; i < items.length; i++) {
      if (root.trayItemHidden(items[i])) count++
    }
    return count
  }

  function trayItemNamed(name) {
    var wanted = String(name || "").toLowerCase()
    var items = SystemTray.items.values || []
    for (var i = 0; i < items.length; i++) {
      var identity = (String(items[i].id || "") + " " + String(items[i].title || "")).toLowerCase()
      if (identity.indexOf(wanted) >= 0) return items[i]
    }
    return null
  }

  function openTrayItemMenu(item, screen) {
    if (!item) return false
    if (item.menu) {
      var sameMenu = root.trayMenuOpen && root.activeTrayItem === item
      root.closeOverlays()
      if (!sameMenu) {
        root.activeTrayItem = item
        root.trayMenuOpen = true
        root.overlayScreen = screen || root.currentScreen()
      }
    } else {
      root.closeOverlays()
      Quickshell.execDetached(["seele-control", "tray-menu", item.id])
    }
    return true
  }

  function toggleTrayItemHidden(item) {
    if (!item) return
    var id = String(item.id)
    if (!root.runControl("tray", "toggle", id)) return
    var hidden = root.trayHiddenIds().slice()
    var index = hidden.indexOf(id)
    if (index >= 0) hidden.splice(index, 1)
    else hidden.push(id)
    root.patchSystemData({ trayHidden: hidden })
  }

  function setAudioDevice(id, profile) {
    id = String(id)
    // A profile entry carries the card to switch on rather than a sink node to
    // default to, so the profile index has to travel with the id.
    if (!root.runControl("audio-device", id, profile)) return
    var devices = root.systemData.audioDevices || []
    var kind = ""
    for (var i = 0; i < devices.length; i++) if (String(devices[i].id) === id) kind = devices[i].kind
    var updated = []
    for (var j = 0; j < devices.length; j++) {
      var device = {}
      for (var key in devices[j]) device[key] = devices[j][key]
      if (device.kind === kind) device.default = String(device.id) === id
      updated.push(device)
    }
    root.patchSystemData({ audioDevices: updated })
  }

  function openCameraPreview(device) {
    cameraPreviewLaunchTimer.device = String(device || "")
    root.controlPanel = ""
    cameraPreviewLaunchTimer.restart()
  }

  function openCameraSettings(device) {
    cameraSettingsLaunchTimer.device = String(device || "")
    root.controlPanel = ""
    cameraSettingsLaunchTimer.restart()
  }


  // A notification is worth clicking only when it carries an action to invoke.
  function notificationActionable(entry) {
    var actions = entry && entry.actions
    if (!actions) return false
    for (var key in actions) return true
    return false
  }

  function activateNotification(id) {
    id = String(id)
    if (!root.runControl("notifications", "invoke", id)) return
    // Invoking an action closes the notification, so drop it locally rather
    // than waiting for the next status read to notice.
    var notifications = root.systemData.notifications || { count: 0, items: [], history: [] }
    var items = []
    for (var i = 0; i < (notifications.items || []).length; i++) {
      if (String(notifications.items[i].id) !== id) items.push(notifications.items[i])
    }
    root.patchSystemData({ notifications: { count: items.length, items: items, history: notifications.history || [] } })
    root.closeOverlays()
  }
  function notificationPopupEntries() {
    var items = (root.systemData.notifications || {}).items || []
    var live = []
    for (var i = 0; i < items.length; i++) {
      if (root.notificationPopupRetired[String(items[i].id)]) continue
      if (root.notificationNow - Number(items[i].time || 0) >= root.notificationPopupSeconds) continue
      live.push(items[i])
    }
    return live
  }

  function toggleNotificationUnfolded(id) {
    var key = String(id)
    var unfolded = {}
    for (var other in root.notificationUnfolded) unfolded[other] = root.notificationUnfolded[other]
    if (unfolded[key]) delete unfolded[key]
    else unfolded[key] = true
    root.notificationUnfolded = unfolded
  }

  function retireNotificationPopup(id) {
    var retired = {}
    for (var key in root.notificationPopupRetired) retired[key] = true
    retired[String(id)] = true
    root.notificationPopupRetired = retired
  }

  // Only ids that are still current need remembering; anything dismissed or
  // invoked has left the panel and would otherwise accumulate here forever.
  function pruneNotificationPopups(notifications) {
    var items = (notifications || {}).items || []
    var present = {}
    for (var i = 0; i < items.length; i++) present[String(items[i].id)] = true
    var retired = {}
    var dropped = false
    for (var key in root.notificationPopupRetired) {
      if (present[key]) retired[key] = true
      else dropped = true
    }
    if (dropped) root.notificationPopupRetired = retired
  }

  function dismissNotification(id) {
    id = String(id)
    if (!root.runControl("notifications", "dismiss", id)) return
    var notifications = root.systemData.notifications || { count: 0, items: [], history: [] }
    var items = []
    for (var i = 0; i < (notifications.items || []).length; i++) {
      if (String(notifications.items[i].id) !== id) items.push(notifications.items[i])
    }
    root.patchSystemData({ notifications: { count: items.length, items: items, history: notifications.history || [] } })
  }

  function clearNotifications() {
    root.runControl("notifications", "clear")
  }

  function batteryEntries() {
    return root.systemData.batteries || []
  }

  function batteryPrimary() {
    var entries = root.batteryEntries()
    var system = null
    var lowest = null
    for (var i = 0; i < entries.length; i++) {
      if (entries[i].kind === "system" && !system) system = entries[i]
      if (!lowest || Number(entries[i].percent) < Number(lowest.percent)) lowest = entries[i]
    }
    return system || lowest
  }

  function batteryCharging(entry) {
    return !!entry && String(entry.status || "").toLowerCase() === "charging"
  }

  function batteryIcon(entry) {
    if (!entry) return "󰂑"
    if (root.batteryCharging(entry)) return "󰂄"
    var percent = Number(entry.percent || 0)
    if (percent >= 80) return "󰁹"
    if (percent >= 55) return "󰂀"
    if (percent >= 30) return "󰁾"
    if (percent >= 15) return "󰁻"
    return "󰂃"
  }

  function batteryColor(entry) {
    if (root.batteryCharging(entry)) return root.green
    var percent = Number(entry && entry.percent || 0)
    if (percent <= 15) return root.red
    if (percent <= 30) return root.yellow
    return root.text
  }

  function headphonesBatteryText() {
    var headphones = root.systemData.headphones || ({})
    if (headphones.kind === "nothing" && headphones.battery !== null && headphones.battery !== undefined) {
      return Number(headphones.battery) + "%"
    }
    var entries = root.batteryEntries()
    var values = []
    for (var i = 0; i < entries.length; i++) {
      if (String(entries[i].name || "").toLowerCase().indexOf("airpods") >= 0) {
        var component = String(entries[i].name).replace(/^AirPods\s*/i, "") || "battery"
        values.push(component + " " + Number(entries[i].percent) + "%")
      }
    }
    return values.join(" · ")
  }

  function privateNetworkActive() {
    return !!(root.systemData.tailscale && root.systemData.tailscale.connected)
      || !!(root.systemData.protonVpn && root.systemData.protonVpn.connected)
  }

  function tailscaleDetail() {
    var state = root.systemData.tailscale || {}
    if (!state.available || state.backend === "Unavailable") return "Service unavailable"
    if (state.needsLogin) return "Sign in required"
    if (!state.connected) return "Disconnected"
    var identity = state.tailnet || state.ip || state.name || "Connected"
    return identity + " · " + Number(state.onlinePeers || 0) + "/" + Number(state.peers || 0) + " peers online"
  }

  function protonVpnDetail() {
    var state = root.systemData.protonVpn || {}
    if (!state.available) return "Client unavailable"
    return state.connected ? (state.connection || "Connected") : "Disconnected · fastest server on connect"
  }


  // Menu bar modules -----------------------------------------------------------
  // Each Control Center module can also carry a menu bar entry. Only a choice
  // the user actually made is stored, so a module nobody has moved keeps the
  // placement it shipped with.
  function moduleGlyph(id) {
    var glyphs = {
      network: "󰖩",
      vpn: "󰒃",
      bluetooth: "󰂯",
      camera: "󰄁",
      airpods: "󰋋",
      audio: "󰕾",
      media: "󰎆"
    }
    return glyphs[String(id)] || "󰘮"
  }

  function moduleLabel(id) {
    var labels = {
      network: "Network",
      vpn: "VPN",
      bluetooth: "Bluetooth",
      camera: "Camera",
      airpods: "Headphones",
      audio: "Sound",
      media: "Now Playing"
    }
    return labels[String(id)] || String(id)
  }

  // VPN is the one module that had no menu bar entry before the Control Center
  // existed; everything else keeps the entry it already had.
  function barModuleDefault(id) {
    return String(id) !== "vpn"
  }

  function barModulePinned(id) {
    var modules = root.systemData.barModules || ({})
    var value = modules[String(id)]
    return value === undefined || value === null ? root.barModuleDefault(id) : !!value
  }

  function setBarModulePinned(id, pinned) {
    id = String(id)
    if (root.barModulePinned(id) === !!pinned) return
    if (!root.runControl("bar", pinned ? "show" : "hide", id)) return
    var modules = {}
    var current = root.systemData.barModules || ({})
    for (var key in current) modules[key] = current[key]
    modules[id] = !!pinned
    root.patchSystemData({ barModules: modules })
  }

  function beginModuleDrag(id, kind) {
    root.dragModule = String(id)
    root.dragKind = String(kind)
    // A module pulled off the bar starts over it; one dragged out of the panel
    // has to reach the bar before it counts as dropped there.
    root.dragOverBar = String(kind) === "remove"
  }

  function updateModuleDrag(overBar) {
    if (root.dragModule !== "") root.dragOverBar = !!overBar
  }

  function endModuleDrag() {
    if (root.dragModule === "") return
    var id = root.dragModule
    var pinned = root.dragOverBar
    root.cancelModuleDrag()
    root.setBarModulePinned(id, pinned)
  }

  function cancelModuleDrag() {
    root.dragModule = ""
    root.dragKind = ""
    root.dragOverBar = false
  }
  // The VPN module owns one private network: whichever client is already
  // connected, and otherwise the first one that could connect.
  function privateNetworkTarget() {
    var tailscale = root.systemData.tailscale || {}
    var proton = root.systemData.protonVpn || {}
    if (tailscale.connected) return "tailscale"
    if (proton.connected) return "proton-vpn"
    if (tailscale.available && tailscale.backend !== "Unavailable") return "tailscale"
    if (proton.available) return "proton-vpn"
    return ""
  }

  function privateNetworkAction() {
    var target = root.privateNetworkTarget()
    if (target === "tailscale") {
      var tailscale = root.systemData.tailscale || {}
      return tailscale.connected ? "down" : tailscale.needsLogin ? "login" : "up"
    }
    if (target === "proton-vpn") return (root.systemData.protonVpn || {}).connected ? "disconnect" : "connect"
    return ""
  }

  function privateNetworkDetail() {
    var tailscale = root.systemData.tailscale || {}
    var proton = root.systemData.protonVpn || {}
    var names = []
    if (tailscale.connected) names.push("Tailscale")
    if (proton.connected) names.push(proton.connection || "Proton VPN")
    if (names.length > 0) return names.join(" · ")
    if (root.privateNetworkTarget() === "") return "Unavailable"
    return tailscale.needsLogin ? "Sign in required" : "Off"
  }

  function privateNetworkBusy() {
    var target = root.privateNetworkTarget()
    return target !== "" && root.controlBusy(target, root.privateNetworkAction())
  }

  function togglePrivateNetwork() {
    var target = root.privateNetworkTarget()
    if (target !== "") root.runControl(target, root.privateNetworkAction())
  }

  function cameraDetail() {
    if (root.systemData.cameraActive) return "In use"
    var devices = root.systemData.cameraDevices || []
    if (devices.length === 0) return "No camera"
    return String(devices[0].name || "Ready")
  }

  function previewCamera() {
    var devices = root.systemData.cameraDevices || []
    for (var i = 0; i < devices.length; i++) {
      if (String(devices[i].device || "") === root.cameraPreviewDevice) return devices[i]
    }
    for (var j = 0; j < devices.length; j++) {
      if (String(devices[j].device || "") === String(root.systemData.cameraDevice || "")) return devices[j]
    }
    return devices.length > 0 ? devices[0] : null
  }

  function headphonesDetail() {
    var headphones = root.systemData.headphones || ({})
    if (!headphones.connected) return "Not connected"
    return root.headphonesBatteryText() || "Connected"
  }

  function startSpeedtest() {
    if (speedtestProcess.running) return
    root.speedtestError = ""
    root.speedtestPhase = "selecting"
    root.speedtestReceived = false
    root.speedtestData = { ping: -1, jitter: -1, download: -1, upload: -1, server: "" }
    speedtestProcess.running = true
  }

  function patchSpeedtestData(patch) {
    var next = {}
    for (var key in root.speedtestData) next[key] = root.speedtestData[key]
    for (var field in patch) next[field] = patch[field]
    root.speedtestData = next
  }

  function handleSpeedtestEvent(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      if (parsed.phase) {
        root.speedtestPhase = String(parsed.phase)
        var patch = {}
        if (parsed.ping !== undefined) patch.ping = Number(parsed.ping)
        if (parsed.jitter !== undefined) patch.jitter = Number(parsed.jitter)
        if (parsed.download !== undefined) patch.download = Number(parsed.download)
        if (parsed.upload !== undefined) patch.upload = Number(parsed.upload)
        root.patchSpeedtestData(patch)
      } else {
        root.parseSpeedtestData(output)
      }
    } catch (error) {
      root.speedtestError = "Speed test failed"
    }
  }

  function parseSpeedtestData(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      var ping = Number(parsed.ping)
      var download = Number(parsed.download)
      var upload = Number(parsed.upload)
      if (isNaN(ping) || isNaN(download) || isNaN(upload)) throw new Error("missing speed values")
      root.speedtestData = {
        ping: ping,
        jitter: Number(parsed.jitter || 0),
        download: download,
        upload: upload,
        server: String(parsed.server || "Ookla Speedtest")
      }
      root.speedtestReceived = true
      root.speedtestError = ""
      root.speedtestPhase = ""
    } catch (error) {
      root.speedtestReceived = false
      root.speedtestError = "Speed test failed"
      root.speedtestPhase = ""
    }
  }

  function speedtestPingText() {
    if (root.speedtestError !== "") return "Failed"
    var ping = Number(root.speedtestData.ping)
    if (ping >= 0) return ping.toFixed(1) + " ms"
    if (root.speedtestPhase === "selecting") return "Locating…"
    if (root.speedtestPhase === "ping") return "Measuring…"
    return "—"
  }

  function speedtestScale() {
    var maximum = Math.max(Number(root.speedtestData.download || 0), Number(root.speedtestData.upload || 0))
    if (maximum <= 100) return 100
    if (maximum <= 250) return 250
    if (maximum <= 500) return 500
    if (maximum <= 1000) return 1000
    return Math.ceil(maximum / 1000) * 1000
  }

  function speedtestValue(value) {
    value = Number(value)
    if (value < 0 || isNaN(value)) return "—"
    return value.toFixed(value >= 100 ? 0 : 1) + " Mbps"
  }

  function audioDevices(kind) {
    var devices = root.systemData.audioDevices || []
    var result = []
    for (var i = 0; i < devices.length; i++) {
      if (devices[i].kind === kind) result.push(devices[i])
    }
    return result
  }

  function activeAgents() {
    var launchers = root.agentData.launchers || []
    var states = root.systemData.agentStates || {}
    var names = { pi: "Pi", opencode: "OpenCode", codex: "Codex", claude: "Claude Code" }
    var ids = ["pi", "opencode", "codex", "claude"]
    var result = []
    for (var i = 0; i < launchers.length; i++) names[launchers[i].id] = launchers[i].name
    for (var id in states) if (ids.indexOf(id) < 0) ids.push(id)
    for (var j = 0; j < ids.length; j++) {
      var state = states[ids[j]]
      if (state && state.active) result.push({ id: ids[j], name: names[ids[j]] || ids[j], status: String(state.status || "running") })
    }
    return result
  }

  function agentBadge(id) {
    var badges = { pi: "PI", opencode: "OC", codex: "CX", claude: "CC" }
    return badges[id] || String(id).substring(0, 2).toUpperCase()
  }

  function agentColor(status) {
    if (status === "input") return root.yellow
    if (status === "working") return root.accent
    if (status === "finished") return root.green
    return root.subtext
  }

  function subscriptionSummary() {
    var subscriptions = root.agentData.subscriptions || []
    if (subscriptions.length === 0) return "No subscriptions"
    var names = []
    for (var i = 0; i < subscriptions.length; i++) names.push(subscriptions[i].name)
    return names.join(" · ")
  }

  function patchSystemData(patch) {
    var next = {}
    for (var key in root.systemData) next[key] = root.systemData[key]
    for (var field in patch) next[field] = patch[field]
    root.systemData = next
  }

  function agoText(value) {
    var seconds = Math.max(0, Math.floor(root.now.getTime() / 1000) - Number(value || 0))
    if (seconds < 60) return "just now"
    var minutes = Math.floor(seconds / 60)
    if (minutes < 60) return minutes + "m ago"
    var hours = Math.floor(minutes / 60)
    if (hours < 24) return hours + "h ago"
    return Math.floor(hours / 24) + "d ago"
  }

  QsMenuOpener {
    id: trayMenuOpener
    menu: root.activeTrayItem ? root.activeTrayItem.menu : null
  }

  function showTimedOsd(kind) {
    if (root.yubikeyTouchRequired) return
    root.osdKind = kind
    root.osdScreen = root.currentScreen()
    root.osdOpen = true
    osdTimer.restart()
  }

  function handleYubikeyEvent(value) {
    var event = String(value || "").trim()
    if (!/^(GPG|U2F|MAC)_[01]$/.test(event)) return
    var source = event.substring(0, 3)
    var next = {}
    for (var key in root.yubikeyTouchSources) next[key] = root.yubikeyTouchSources[key]
    if (event.endsWith("_1")) next[source] = true
    else delete next[source]
    root.yubikeyTouchSources = next

    var required = false
    for (var active in next) required = required || !!next[active]
    root.yubikeyTouchRequired = required
    // Seele Polkit and Seele Lock already draw the touch request. Both publish
    // their state, so the desktop OSD stands down while either one owns it.
    if (required && !root.polkitPrompting && !root.lockPrompting) {
      osdTimer.stop()
      root.osdKind = "yubikey"
      root.osdScreen = root.currentScreen()
      root.osdOpen = true
    } else if (root.osdKind === "yubikey") {
      root.osdOpen = false
    }
  }

  function startOsSession() {
    closeOverlays()
    Quickshell.execDetached(["seele-os-session"])
  }

  function runAgent(id, prompt) {
    agentsOpen = false
    var args = ["seele-agent", id || "pi"]
    if (String(prompt || "").trim() !== "") args.push(String(prompt).trim())
    Quickshell.execDetached(args)
  }

  function controlArgument(value) {
    return value === undefined || value === null ? "" : String(value)
  }

  function controlBusy(action, value, extra) {
    return root.pendingControlAction === String(action)
      && root.pendingControlValue === root.controlArgument(value)
      && root.pendingControlExtra === root.controlArgument(extra)
  }

  function controlCompleted(action, value, extra) {
    return root.completedControlAction === String(action)
      && root.completedControlValue === root.controlArgument(value)
      && root.completedControlExtra === root.controlArgument(extra)
  }

  function controlFailed(action, value, extra) {
    return root.failedControlAction === String(action)
      && root.failedControlValue === root.controlArgument(value)
      && root.failedControlExtra === root.controlArgument(extra)
  }

  function runControl(action, value, extra) {
    if (controlProcess.running) return false
    var controlValue = root.controlArgument(value)
    var controlExtra = root.controlArgument(extra)
    var args = ["seele-control", action]
    if (controlValue !== "") args.push(controlValue)
    if (controlExtra !== "") args.push(controlExtra)
    root.pendingControlAction = String(action)
    root.pendingControlValue = controlValue
    root.pendingControlExtra = controlExtra
    root.completedControlAction = ""
    root.failedControlAction = ""
    controlProcess.command = args
    controlProcess.running = true
    if (action === "volume") root.showTimedOsd("volume")
    return true
  }

  function audioWheelSteps(wheel) {
    var angle = Number(wheel.angleDelta.y)
    if (angle !== 0) return (angle > 0 ? 1 : -1) * Math.max(1, Math.round(Math.abs(angle) / 120))
    var pixels = Number(wheel.pixelDelta.y)
    return pixels === 0 ? 0 : pixels > 0 ? 1 : -1
  }

  function adjustAudioFromWheel(wheel, microphone) {
    var steps = root.audioWheelSteps(wheel)
    if (steps === 0) return
    var dragged = microphone ? root.microphoneDrag : root.volumeDrag
    var reported = Number(microphone ? root.systemData.microphoneVolume : root.systemData.volume)
    var current = dragged >= 0 ? dragged : isNaN(reported) ? 0 : reported
    var maximum = microphone ? 100 : root.outputVolumeMaximum
    var adjusted = Math.max(0, Math.min(maximum, Math.round(current + steps * 5)))
    if (microphone) {
      root.microphoneDrag = adjusted
      if (!microphoneDragTimer.running) microphoneDragTimer.start()
    } else {
      root.volumeDrag = adjusted
      if (!volumeDragTimer.running) volumeDragTimer.start()
    }
    wheel.accepted = true
  }

  function toggleWindowsReboot() {
    if (windowsCountdown >= 0) {
      windowsCountdown = -1
      windowsTimer.stop()
    } else {
      windowsCountdown = 10
      windowsTimer.restart()
    }
  }

  function subscriptionLimit(id) {
    var subscriptions = root.agentData.subscriptions || []
    var wanted = String(id).toLowerCase()
    var result = null
    for (var i = 0; i < subscriptions.length; i++) {
      var subscriptionId = String(subscriptions[i].id || "").toLowerCase()
      var subscriptionName = String(subscriptions[i].name || "").toLowerCase()
      if (subscriptionId !== wanted && subscriptionName.indexOf(wanted) < 0) continue
      var limits = subscriptions[i].limits || []
      for (var j = 0; j < limits.length; j++) {
        if (!result || Number(limits[j].usedPercent) > Number(result.usedPercent)) result = limits[j]
      }
    }
    return result
  }

  function freePercent(limit) {
    return limit ? Math.max(0, 100 - Math.round(Number(limit.usedPercent || 0))) : 100
  }

  function menuBarCapacity(id) {
    var limit = root.subscriptionLimit(id)
    return limit ? root.freePercent(limit) : -1
  }

  function refreshClock() {
    if (!clockProcess.running) clockProcess.running = true
  }

  function parseClockData(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      if (parsed && parsed.zones) root.clockData = parsed
    } catch (error) {
      console.warn("seele-shell/clock", error)
    }
  }

  function clockZone(id) {
    var zones = root.clockData.zones || []
    for (var i = 0; i < zones.length; i++) if (zones[i].id === id) return zones[i]
    return null
  }

  function timezonePinned(id) {
    return (root.clockData.pinned || []).indexOf(id) >= 0
  }

  function filteredTimezones(query) {
    return Time.orderZones(root.clockData.zones || [], root.clockData.pinned || [], query)
  }

  function pinTimezone(id) {
    if (clockActionProcess.running) return
    clockActionProcess.command = root.timezonePinned(id) ? ["seele-clock", "unpin", id] : ["seele-clock", "pin", id]
    clockActionProcess.running = true
  }

  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/seele-shell/theme.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      try {
        var theme = JSON.parse(text())
        root.base = theme.base || root.base
        root.mantle = theme.mantle || root.mantle
        root.surface = theme.surface || root.surface
        root.overlay = theme.overlay || root.overlay
        root.text = theme.text || root.text
        root.subtext = theme.subtext || root.subtext
        root.accent = theme.accent || root.accent
        root.red = theme.red || root.red
        root.green = theme.green || root.green
        root.yellow = theme.yellow || root.yellow
        root.fontFamily = theme.fontFamily || root.fontFamily
      } catch (error) {
        console.warn("seele-shell/theme", error)
      }
    }
  }

  Process {
    id: agentProcess
    command: ["seele-agent-state"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.parseAgentData(text)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (String(text).trim() !== "") root.agentError = String(text).trim()
    }
    onExited: root.agentRefreshing = false
  }

  Process {
    id: clockProcess
    command: ["seele-clock", "list"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.parseClockData(text)
    }
  }

  Process {
    id: clockActionProcess
    onExited: root.refreshClock()
  }

  Process {
    id: statusProcess
    command: ["seele-control", "status"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.parseSystemData(text)
    }
    onExited: {
      if (root.statusRefreshQueued) {
        root.statusRefreshQueued = false
        Qt.callLater(function() { statusProcess.running = true })
      }
    }
  }

  // Mako owns the history and timeout. Watching its D-Bus traffic makes the
  // shell-native popup appear and disappear without waiting for the 5s status
  // refresh.
  Process {
    command: [
      "busctl",
      "--user",
      "--match=type='method_call',interface='org.freedesktop.Notifications',member='Notify'",
      "--match=type='signal',interface='org.freedesktop.Notifications',member='NotificationClosed'",
      "monitor"
    ]
    running: true
    stdout: SplitParser {
      onRead: root.refreshStatus()
    }
  }

  Process {
    id: controlProcess
    environment: ({ SEELE_CONTROL_NO_STATUS: "1" })
    onExited: function(exitCode) {
      if (exitCode === 0) {
        root.completedControlAction = root.pendingControlAction
        root.completedControlValue = root.pendingControlValue
        root.completedControlExtra = root.pendingControlExtra
      } else {
        root.failedControlAction = root.pendingControlAction
        root.failedControlValue = root.pendingControlValue
        root.failedControlExtra = root.pendingControlExtra
        if (root.pendingControlAction === "volume" && String(root.volumeDrag) === root.pendingControlValue) root.volumeDrag = -1
        if (root.pendingControlAction === "microphone" && String(root.microphoneDrag) === root.pendingControlValue) root.microphoneDrag = -1
      }
      root.pendingControlAction = ""
      root.pendingControlValue = ""
      root.pendingControlExtra = ""
      controlFeedbackTimer.restart()
      root.refreshStatus()
    }
  }

  Process {
    id: bluetoothStatusProcess
    command: ["seele-control", "bluetooth-status"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.parseBluetoothData(text)
    }
    onExited: {
      if (root.bluetoothStatusRefreshQueued) {
        root.bluetoothStatusRefreshQueued = false
        Qt.callLater(function() { bluetoothStatusProcess.running = true })
      }
    }
  }

  Process {
    id: speedtestProcess
    command: ["seele-control", "speedtest"]
    stdout: SplitParser {
      onRead: data => root.handleSpeedtestEvent(data)
    }
    onExited: function(exitCode) {
      Qt.callLater(function() {
        if (exitCode !== 0 || !root.speedtestReceived) {
          root.speedtestPhase = ""
          root.speedtestError = "Speed test failed"
        }
      })
    }
  }

  Process {
    id: yubikeyWatchProcess
    command: ["seele-yubikey-watch"]
    running: true
    stdout: SplitParser {
      onRead: data => root.handleYubikeyEvent(data)
    }
  }

  // Seele Polkit publishes whether its dialog is prompting. Both agents watch
  // the same touch detector, so without this the OSD and the dialog would ask
  // for the same touch twice, with the OSD stranded behind a fullscreen layer.
  FileView {
    path: Quickshell.env("XDG_RUNTIME_DIR") + "/seele-polkit.state"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      root.polkitPrompting = text().trim() === "1"
      if (root.polkitPrompting && root.osdKind === "yubikey") root.osdOpen = false
    }
  }

  FileView {
    path: Quickshell.env("XDG_RUNTIME_DIR") + "/seele-lock.state"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      root.lockPrompting = text().trim() === "1"
      if (root.lockPrompting && root.osdKind === "yubikey") root.osdOpen = false
    }
  }

  Process {
    id: bluetoothProcess
    environment: ({ SEELE_CONTROL_NO_STATUS: "1" })
    onExited: {
      root.bluetoothBusy = ""
      root.bluetoothAction = ""
      root.refreshBluetoothStatus()
      if (root.bluetoothScanQueued >= 0) {
        var scan = root.bluetoothScanQueued === 1
        root.bluetoothScanQueued = -1
        Qt.callLater(function() { root.runBluetooth("scan", scan ? "on" : "off") })
      }
    }
  }

  MediaSlot {
    id: spotifyMediaSlot

    playing: Media.spotifyPlayer(Mpris.players.values || [])
  }

  MediaSlot {
    id: deviceMediaSlot

    playing: Media.devicePlayer(Mpris.players.values || [])
  }

  Timer {
    interval: 60 * 1000
    repeat: true
    running: true
    triggeredOnStart: true
    onTriggered: root.refreshAgents()
  }

  Timer {
    interval: 5000
    repeat: true
    running: true
    triggeredOnStart: true
    onTriggered: root.refreshStatus()
  }

  Timer {
    interval: 30000
    repeat: true
    running: true
    triggeredOnStart: true
    onTriggered: {
      root.now = new Date()
      root.refreshClock()
    }
  }

  // `now` moves in half-minute steps, which cannot retire a ten second popup.
  // This runs only while something is on screen to retire.
  Timer {
    interval: 500
    repeat: true
    running: root.notificationPopupEntries().length > 0
    triggeredOnStart: true
    onTriggered: root.notificationNow = Date.now() / 1000
  }

  Timer {
    interval: 1000
    repeat: true
    running: root.controlPanel === "clock"
    triggeredOnStart: true
    onTriggered: root.now = new Date()
  }

  Timer {
    id: controlFeedbackTimer
    interval: 1200
    onTriggered: {
      root.completedControlAction = ""
      root.completedControlValue = ""
      root.completedControlExtra = ""
      root.failedControlAction = ""
      root.failedControlValue = ""
      root.failedControlExtra = ""
    }
  }

  Timer {
    id: cameraPreviewLaunchTimer

    property string device: ""

    interval: 400
    onTriggered: root.runControl("camera-preview", device)
  }

  Timer {
    id: cameraSettingsLaunchTimer

    property string device: ""

    interval: 400
    onTriggered: root.runControl("camera-settings", device)
  }

  Timer {
    id: bluetoothScanTimer
    interval: 120000
    onTriggered: root.setBluetoothScanning(false)
  }

  // A search needs the fastest cadence, but the receiver row reports live state
  // too — what is connected and whether it is actually streaming — and the
  // shared five-second status poll is too slow to read as live.
  Timer {
    interval: root.bluetoothScanActive ? 500 : 1500
    repeat: true
    running: root.controlPanel === "bluetooth"
      && (root.bluetoothScanActive || root.bluetoothReceiverActive || root.bluetoothSources().length > 0)
    triggeredOnStart: true
    onTriggered: root.refreshBluetoothStatus()
  }

  Timer {
    id: bluetoothForgetTimer
    interval: 4000
    onTriggered: root.bluetoothForget = ""
  }

  Timer {
    id: osdTimer
    interval: root.osdKind === "airpods" ? 3200 : 1400
    onTriggered: if (root.osdKind !== "yubikey") root.osdOpen = false
  }

  Timer {
    id: windowsTimer
    interval: 1000
    repeat: true
    onTriggered: {
      if (root.windowsCountdown <= 1) {
        stop()
        root.windowsCountdown = -1
        root.controlPanel = ""
        root.runControl("reboot-windows")
      } else {
        root.windowsCountdown--
      }
    }
  }

  IpcHandler {
    target: "seele-shell"
    function ping(): string { return "ok" }
    function toggleLauncher(mode: string): void { root.toggleLauncher(mode) }
    function toggleAgents(): void { root.toggleAgents() }
    function toggleControls(): void { root.toggleControls() }
    function toggleControl(panel: string): void { root.toggleControl(panel) }
    function launchAgent(id: string, prompt: string): void { root.runAgent(id, prompt) }
    function refreshAgents(): void { root.refreshAgents() }
    function updateStatus(json: string): void { root.parseSystemData(json) }
    function refreshStatus(): void { root.refreshStatus() }
    function showVolume(): void { root.showTimedOsd("volume") }
    function showMicrophone(muted: string): void {
      // A caller that already knows the new mute passes it, so the OSD does
      // not wait on a fresh reading of the whole system to say one thing.
      if (muted !== "") root.patchSystemData({ microphoneMuted: muted === "muted" })
      root.showTimedOsd("microphone")
    }
    function bluetoothPairingRequest(request: string): void { root.setBluetoothPairing(request) }
    function bluetoothPairingDismiss(): void { root.clearBluetoothPairing() }
    function close(): void { root.closeOverlays() }
  }

  // Scrollables differ in what they hold, never in how they move. Both carry
  // the same spring, so a list and a free-form panel rebound identically.
  component SeeleListView: ListView {
    boundsBehavior: Flickable.DragAndOvershootBounds
    flickDeceleration: root.scrollDeceleration
    maximumFlickVelocity: root.scrollFlickVelocity
    rebound: Transition {
      NumberAnimation { properties: "x,y"; duration: root.scrollRebound; easing.type: Easing.OutCubic }
    }
  }

  component SeeleFlickable: Flickable {
    boundsBehavior: Flickable.DragAndOvershootBounds
    flickDeceleration: root.scrollDeceleration
    maximumFlickVelocity: root.scrollFlickVelocity
    rebound: Transition {
      NumberAnimation { properties: "x,y"; duration: root.scrollRebound; easing.type: Easing.OutCubic }
    }
  }

  component RefreshGlyph: Item {
    id: refreshGlyph

    property bool spinning: false
    property color color: root.accent
    property alias font: idleRefresh.font
    onColorChanged: activitySpinner.requestPaint()

    Text {
      id: idleRefresh

      visible: !refreshGlyph.spinning
      anchors.fill: parent
      text: "󰑐"
      color: refreshGlyph.color
      font.family: root.fontFamily
      font.pixelSize: 16
      horizontalAlignment: Text.AlignHCenter
      verticalAlignment: Text.AlignVCenter
    }

    Canvas {
      id: activitySpinner

      visible: refreshGlyph.spinning
      anchors.centerIn: parent
      width: Math.min(parent.width, parent.height, idleRefresh.font.pixelSize)
      height: width
      antialiasing: true
      transformOrigin: Item.Center
      onVisibleChanged: if (visible) requestPaint()
      onWidthChanged: requestPaint()
      onPaint: {
        var context = getContext("2d")
        context.clearRect(0, 0, width, height)
        context.beginPath()
        context.lineWidth = Math.max(1.5, width * 0.14)
        context.lineCap = "round"
        context.strokeStyle = refreshGlyph.color
        context.arc(width / 2, height / 2, Math.max(1, width / 2 - context.lineWidth), -Math.PI / 2, Math.PI)
        context.stroke()
      }

      NumberAnimation on rotation {
        from: 0
        to: 360
        duration: 720
        loops: Animation.Infinite
        running: activitySpinner.visible
      }
    }
  }

  component ControlSwitch: Rectangle {
    id: control

    property bool checked: false
    property bool busy: false
    signal toggled()

    implicitWidth: 40
    implicitHeight: 22
    opacity: enabled ? 1 : 0.42
    radius: height / 2
    color: switchMouse.pressed ? root.alpha(control.checked ? root.accent : root.text, 0.72) : control.checked ? root.accent : root.alpha(root.overlay, 0.4)
    border.width: 1
    border.color: control.busy ? root.accent : switchMouse.pressed ? root.text : switchMouse.containsMouse ? root.accent : "transparent"

    Behavior on color { ColorAnimation { duration: 80 } }

    Rectangle {
      visible: !control.busy
      width: parent.height - 6
      height: width
      radius: width / 2
      y: 3
      x: control.checked ? control.width - width - 3 : 3
      color: control.checked ? root.base : root.text

      Behavior on x { NumberAnimation { duration: 80; easing.type: Easing.OutCubic } }
    }

    RefreshGlyph {
      visible: control.busy
      anchors.centerIn: parent
      width: 16
      height: 16
      spinning: visible
      color: control.checked ? root.base : root.text
      font.pixelSize: 11
    }

    MouseArea { id: switchMouse; anchors.fill: parent; enabled: control.enabled && !control.busy; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: control.toggled() }
  }

  component HeadphonesIcon: Item {
    id: headphonesIcon

    property color tint: root.text
    property string kind: "airpods"

    implicitWidth: 16
    implicitHeight: 16

    Repeater {
      visible: headphonesIcon.kind === "airpods"
      model: [0, 1]
      Item {
        required property int modelData
        width: 6
        height: headphonesIcon.height
        x: modelData === 0 ? 1 : headphonesIcon.width - width - 1
        Rectangle { width: 6; height: 6; radius: 3; y: 2; color: headphonesIcon.tint }
        Rectangle { width: 2.4; height: 7; radius: 1.2; x: 1.8; y: 7.5; color: headphonesIcon.tint }
      }
    }

    Text {
      visible: headphonesIcon.kind !== "airpods"
      anchors.centerIn: parent
      text: "󰋋"
      color: headphonesIcon.tint
      font.family: root.fontFamily
      font.pixelSize: Math.min(parent.width, parent.height)
    }
  }

  // Depth wash, drawn under a surface's content and inside its border.
  component SurfaceWash: Rectangle {
    anchors.fill: parent
    anchors.margins: 1
    color: "transparent"

    gradient: Gradient {
      GradientStop { position: 0.0; color: root.alpha(root.text, 0.06) }
      GradientStop { position: 0.55; color: "transparent" }
      GradientStop { position: 1.0; color: root.alpha(root.mantle, 0.45) }
    }
  }

  // Grain film and edge highlight, drawn over a surface's content so the
  // texture is even across the panel and the cards inside it. Neither layer
  // accepts input, so everything underneath stays clickable. A tiled image
  // cannot follow a rounded corner, so `inset` pulls the film inside the arc:
  // anything past radius * (1 - 1 / sqrt(2)) stays within the surface.
  component SurfaceGrain: Item {
    id: grainLayer

    property real inset: 0

    anchors.fill: parent
    z: 1

    Image {
      anchors.fill: parent
      anchors.margins: grainLayer.inset
      source: root.grain
      fillMode: Image.Tile
      opacity: root.grainOpacity
      smooth: false
    }

    Rectangle {
      anchors.top: parent.top
      anchors.topMargin: 1
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.leftMargin: grainLayer.inset * 3
      anchors.rightMargin: grainLayer.inset * 3
      height: 1
      color: root.alpha(root.text, 0.1)
    }
  }

  component HoverTip: PopupWindow {
    id: hoverTip

    property var mouse: null
    property string text: ""

    visible: mouse !== null && mouse.containsMouse && text !== ""
      && root.controlPanel === "" && !root.agentsOpen && !root.trayMenuOpen
    implicitWidth: hoverTipLabel.implicitWidth + 20
    implicitHeight: 26
    color: "transparent"
    grabFocus: false

    onTextChanged: if (visible) Qt.callLater(hoverTip.reposition)

    anchor {
      window: hoverTip.mouse ? hoverTip.mouse.QsWindow.window : null
      adjustment: PopupAdjustment.Slide
      gravity: Edges.Bottom | Edges.Right

      onAnchoring: {
        if (!hoverTip.mouse) return
        var position = hoverTip.mouse.QsWindow.contentItem.mapFromItem(
          hoverTip.mouse,
          hoverTip.mouse.width / 2 - hoverTip.width / 2,
          hoverTip.mouse.height + 5
        )
        anchor.rect.x = position.x
        anchor.rect.y = position.y
      }
    }

    Rectangle {
      anchors.fill: parent
      radius: root.radiusSmall
      color: root.panelColor
      border.color: root.panelBorder
      border.width: 1

      Text {
        id: hoverTipLabel
        anchors.centerIn: parent
        text: hoverTip.text
        color: root.text
        font.family: root.fontFamily
        font.pixelSize: 10
      }
    }
  }

  // Qt cannot round an Image or a live video surface, so the source is drawn
  // through a mask instead. The source item must hide itself; the effect draws
  // it in its place.
  component RoundedSource: Item {
    id: roundedSource

    property Item source: null
    property real radius: root.radius

    Item {
      id: roundedMask

      anchors.fill: parent
      layer.enabled: true
      visible: false

      Rectangle {
        anchors.fill: parent
        radius: roundedSource.radius
        color: "black"
      }
    }

    MultiEffect {
      anchors.fill: parent
      source: roundedSource.source
      maskEnabled: true
      maskSource: roundedMask
    }
  }

  // Every panel introduces itself with a glyph, the way the AI cockpit does.
  component PanelGlyph: Text {
    anchors.verticalCenter: parent.verticalCenter
    width: 32
    color: root.accent
    font.family: root.fontFamily
    font.pixelSize: 20
    horizontalAlignment: Text.AlignLeft
    verticalAlignment: Text.AlignVCenter
    transform: Translate { x: 2 }
  }

  component SpeedGauge: Rectangle {
    id: speedGauge

    property string label: ""
    property string icon: ""
    property real value: -1
    property real maximum: 100
    property color tint: root.accent
    property bool active: false
    readonly property real ratio: Math.max(0, Math.min(1, value / Math.max(1, maximum)))
    property real displayedRatio: ratio

    radius: root.radius
    color: root.mantle

    Behavior on displayedRatio {
      NumberAnimation { duration: 520; easing.type: Easing.OutCubic }
    }

    onDisplayedRatioChanged: gaugeCanvas.requestPaint()
    onTintChanged: gaugeCanvas.requestPaint()

    Text {
      anchors.top: parent.top; anchors.topMargin: 8; anchors.horizontalCenter: parent.horizontalCenter
      text: speedGauge.icon + "  " + speedGauge.label
      color: speedGauge.tint
      font.family: root.fontFamily
      font.pixelSize: 8
      font.bold: true
    }

    Canvas {
      id: gaugeCanvas

      anchors.fill: parent
      onWidthChanged: requestPaint()
      onHeightChanged: requestPaint()
      onPaint: {
        var context = getContext("2d")
        context.clearRect(0, 0, width, height)
        var centerX = width / 2
        var radius = Math.min(width * 0.36, height * 0.42)
        var centerY = height - radius * 0.55 - 8
        var start = Math.PI * 0.82
        var end = Math.PI * 2.18
        var sweep = end - start

        context.lineCap = "round"
        context.lineWidth = 6
        context.strokeStyle = root.alpha(root.overlay, 0.28)
        context.beginPath()
        context.arc(centerX, centerY, radius, start, end, false)
        context.stroke()

        if (speedGauge.displayedRatio > 0) {
          context.strokeStyle = speedGauge.tint
          context.beginPath()
          context.arc(centerX, centerY, radius, start, start + sweep * speedGauge.displayedRatio, false)
          context.stroke()
        }

        context.lineCap = "butt"
        context.lineWidth = 1
        context.strokeStyle = root.alpha(root.text, 0.34)
        for (var tick = 0; tick <= 10; tick++) {
          var tickAngle = start + sweep * tick / 10
          var tickInner = radius - (tick % 5 === 0 ? 9 : 6)
          var tickOuter = radius - 2
          context.beginPath()
          context.moveTo(centerX + Math.cos(tickAngle) * tickInner, centerY + Math.sin(tickAngle) * tickInner)
          context.lineTo(centerX + Math.cos(tickAngle) * tickOuter, centerY + Math.sin(tickAngle) * tickOuter)
          context.stroke()
        }

        var needleAngle = start + sweep * speedGauge.displayedRatio
        context.lineCap = "round"
        context.lineWidth = 2
        context.strokeStyle = speedGauge.tint
        context.beginPath()
        context.moveTo(centerX, centerY)
        context.lineTo(centerX + Math.cos(needleAngle) * (radius - 12), centerY + Math.sin(needleAngle) * (radius - 12))
        context.stroke()
        context.fillStyle = speedGauge.tint
        context.beginPath()
        context.arc(centerX, centerY, 3.5, 0, Math.PI * 2, false)
        context.fill()
      }
    }

    Text {
      anchors.horizontalCenter: parent.horizontalCenter
      anchors.bottom: parent.bottom; anchors.bottomMargin: 12
      text: speedGauge.active && speedGauge.value < 0 ? "Measuring…" : speedGauge.value < 0 ? "" : root.speedtestValue(speedGauge.value)
      color: speedGauge.tint
      font.family: root.fontFamily
      font.pixelSize: 11
      font.bold: true
    }
  }

  // Shared chrome for every floating panel, so panels differ only in what
  // they hold, never in how they are framed.
  component PanelSurface: Rectangle {
    id: panelSurface

    readonly property bool hovered: panelHover.hovered

    anchors.fill: parent
    radius: root.radius
    color: root.panelColor
    border.color: root.panelBorder
    border.width: 1
    antialiasing: true

    SurfaceWash { radius: root.radius - 1 }
    SurfaceGrain { inset: 3 }
    HoverHandler { id: panelHover }
  }

  // Scroll indicators are hairlines rather than the platform's full-width
  // bars, and panels reserve `scrollGutter` for them so a bar never sits on
  // top of the content's own edge.
  component SlimScrollBar: ScrollBar {
    id: scrollBar

    required property bool popupHovered

    policy: ScrollBar.AsNeeded
    implicitWidth: root.scrollGutter
    padding: 2
    opacity: popupHovered || pressed ? 1 : 0
    enabled: popupHovered || pressed
    visible: policy !== ScrollBar.AlwaysOff && (policy === ScrollBar.AlwaysOn || size < 1)
    // An attached indicator is a sibling of the view's content, so without
    // this it renders behind the rows it belongs to.
    z: 2
    // A list of every timezone would otherwise grind the handle down to a few
    // pixels.
    minimumSize: height > 0 ? Math.min(0.5, 36 / height) : 0

    background: Item {}
    contentItem: Rectangle {
      implicitWidth: root.scrollGutter - 4
      radius: width / 2
      opacity: 1
      color: root.alpha(root.overlay, scrollBar.pressed ? 1 : 0.75)
    }
  }

  // Every bar entry is a rounded pill on the same radius as windows, buttons,
  // and panels, and takes its hover, press, and open state from here so the
  // whole strip reacts identically. The entry itself spans the bar's full
  // height while only the pill is inset, so a pointer thrown at the top of the
  // screen still lands on the entry under it.
  component BarItem: Item {
    id: barItem

    property bool hovered: false
    property bool active: false
    // Set on an entry that can be dragged off the bar. The entry has to stay
    // mapped for the whole gesture — hiding it to preview the removal would
    // destroy the item holding the pointer grab and cancel the drag — so it
    // fades instead once the pointer is past the bar.
    property string module: ""

    opacity: barItem.module !== "" && root.dragModule === barItem.module && !root.dragOverBar ? 0.4 : 1

    Behavior on opacity { NumberAnimation { duration: 110 } }

    anchors.verticalCenter: parent.verticalCenter
    height: root.barHeight

    Rectangle {
      anchors.fill: parent
      anchors.topMargin: root.barPadding
      anchors.bottomMargin: root.barPadding
      anchors.leftMargin: root.barSpacing / 2
      anchors.rightMargin: root.barSpacing / 2
      radius: root.radius
      color: parent.active ? root.selectedColor : parent.hovered ? root.hoverColor : "transparent"

      Behavior on color { ColorAnimation { duration: 110 } }
    }
  }

  // A bar label whose baseline stays put no matter what the text contains.
  // A single glyph the primary font lacks — a heart in a track title, say —
  // pulls in a fallback whose taller line box moves an auto-sized, centred
  // Text off the line every neighbouring entry sits on. The invisible
  // reference pins the baseline to the one the primary font would have
  // produced, so the drift cannot depend on the words.
  component BarLabel: Item {
    id: barLabel

    property alias text: barLabelText.text
    property color color: root.text
    property int maximumWidth: 175

    implicitWidth: Math.min(barLabel.maximumWidth, barLabelText.implicitWidth)
    implicitHeight: root.barHeight

    Text {
      id: barLabelReference

      visible: false
      anchors.verticalCenter: parent.verticalCenter
      text: "M"
      font.family: root.fontFamily
      font.pixelSize: 10
    }

    Text {
      id: barLabelText

      anchors.left: parent.left
      anchors.right: parent.right
      anchors.baseline: barLabelReference.baseline
      elide: Text.ElideRight
      color: barLabel.color
      font.family: root.fontFamily
      font.pixelSize: 10
    }
  }

  // Pointer handling for a Control Center module that can be dragged onto the
  // menu bar. A press that never moves is still an ordinary click; past the
  // threshold it becomes a drag, and Wayland's implicit pointer grab keeps the
  // motion arriving after the pointer has left this surface, which is what lets
  // a panel track a drop onto a different layer surface at all. The panel sits
  // `panelGap` below the bar, so a negative enough y is the whole hit test.
  component ModuleDragArea: MouseArea {
    id: moduleDrag

    property string module: ""
    property real originX: 0
    property real originY: 0
    property bool dragging: false
    signal activated(var mouse)

    anchors.fill: parent
    hoverEnabled: true
    preventStealing: true
    cursorShape: Qt.PointingHandCursor

    onPressed: function(mouse) {
      moduleDrag.originX = mouse.x
      moduleDrag.originY = mouse.y
      moduleDrag.dragging = false
    }

    onPositionChanged: function(mouse) {
      if (!moduleDrag.pressed || moduleDrag.module === "") return
      if (!moduleDrag.dragging) {
        if (Math.abs(mouse.x - moduleDrag.originX) < 6 && Math.abs(mouse.y - moduleDrag.originY) < 6) return
        moduleDrag.dragging = true
        root.beginModuleDrag(moduleDrag.module, "add")
      }
      // The panel surface starts at the top of the screen, so scene coordinates
      // are screen coordinates and the bar is simply the first `barHeight` rows.
      root.updateModuleDrag(moduleDrag.mapToItem(null, mouse.x, mouse.y).y < root.barHeight)
    }

    onReleased: function(mouse) {
      if (moduleDrag.dragging) root.endModuleDrag()
      else moduleDrag.activated(mouse)
      moduleDrag.dragging = false
    }

    onCanceled: {
      if (moduleDrag.dragging) root.cancelModuleDrag()
      moduleDrag.dragging = false
    }
  }

  // Menu bar pointer handling for a module that can be dragged out of the bar.
  // The panel opens on release rather than on press, because opening it on the
  // press put the panel — and the click-away catcher it activates — directly in
  // the path of the drag that follows, where they take the pointer before the
  // gesture can travel. A modifier would have avoided the conflict, but a layer
  // surface without keyboard focus never receives modifier state, so
  // `mouse.modifiers` is always empty up here.
  component BarModuleArea: MouseArea {
    id: barModuleArea

    property string module: ""
    property real originY: 0
    property bool dragging: false
    signal activated(var mouse)

    anchors.fill: parent
    hoverEnabled: true
    preventStealing: true

    onPressed: function(mouse) {
      barModuleArea.originY = mouse.y
      barModuleArea.dragging = false
      // Opening the bar's input region here rather than once the drag is
      // recognised: the region change costs a round trip the pointer would
      // otherwise outrun on its way down the screen.
      root.barPressModule = barModuleArea.module
    }

    onPositionChanged: function(mouse) {
      if (!barModuleArea.pressed || barModuleArea.module === "") return
      if (!barModuleArea.dragging) {
        if (mouse.y - barModuleArea.originY < 8) return
        barModuleArea.dragging = true
        root.beginModuleDrag(barModuleArea.module, "remove")
      }
      root.updateModuleDrag(mouse.y >= 0 && mouse.y < root.barHeight)
    }

    onReleased: function(mouse) {
      if (barModuleArea.dragging) root.endModuleDrag()
      else if (mouse.y >= 0 && mouse.y < root.barHeight) barModuleArea.activated(mouse)
      barModuleArea.dragging = false
      root.barPressModule = ""
    }

    onCanceled: {
      if (barModuleArea.dragging) root.cancelModuleDrag()
      barModuleArea.dragging = false
      root.barPressModule = ""
    }
  }

  // The dedicated Audio panel uses one horizontal level row for either output
  // or microphone, with the same batching and mute behavior as the compact
  // Control Center controls below.
  component AudioLevelRow: Row {
    id: audioLevelRow

    property bool microphone: false
    readonly property int shown: audioLevelRow.microphone
      ? (root.microphoneDrag >= 0 ? root.microphoneDrag : Number(root.systemData.microphoneVolume))
      : (root.volumeDrag >= 0 ? root.volumeDrag : Number(root.systemData.volume))
    readonly property int maximum: audioLevelRow.microphone ? 100 : root.outputVolumeMaximum
    readonly property bool muted: audioLevelRow.microphone ? !!root.systemData.microphoneMuted : !!root.systemData.muted

    spacing: 8

    Rectangle {
      width: audioLevelRow.width - 52
      height: 44
      radius: root.radius
      color: root.surface
      clip: true

      Rectangle {
        width: parent.width * Math.max(0, Math.min(1, audioLevelRow.shown / audioLevelRow.maximum))
        radius: parent.radius
        height: parent.height
        color: audioLevelRow.muted ? root.fillDanger : root.fillColor
      }

      Rectangle {
        visible: !audioLevelRow.microphone
        x: parent.width * 100 / audioLevelRow.maximum
        width: 1
        height: parent.height
        color: root.alpha(root.text, 0.25)
      }

      Row {
        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12
        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: parent.width - 46
          text: audioLevelRow.microphone
            ? (audioLevelRow.muted ? "󰍭  Microphone muted" : root.systemData.microphoneActive ? "󰍬  Microphone in use" : "󰍬  Microphone")
            : (audioLevelRow.muted ? "󰝟  Output muted" : "󰕾  Output")
          color: root.text
          font.family: root.fontFamily
          font.pixelSize: 11
          font.bold: true
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: 46
          text: audioLevelRow.shown + "%"
          color: root.subtext
          font.family: root.fontFamily
          font.pixelSize: 11
          horizontalAlignment: Text.AlignRight
        }
      }

      MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        function valueAt(x) { return Math.max(0, Math.min(audioLevelRow.maximum, Math.round(x / width * audioLevelRow.maximum))) }
        onPressed: function(mouse) {
          if (audioLevelRow.microphone) {
            root.microphoneDrag = valueAt(mouse.x)
            microphoneDragTimer.restart()
          } else {
            root.volumeDrag = valueAt(mouse.x)
            volumeDragTimer.restart()
          }
        }
        onPositionChanged: function(mouse) {
          if (!pressed) return
          if (audioLevelRow.microphone) {
            root.microphoneDrag = valueAt(mouse.x)
            if (!microphoneDragTimer.running) microphoneDragTimer.restart()
          } else {
            root.volumeDrag = valueAt(mouse.x)
            if (!volumeDragTimer.running) volumeDragTimer.restart()
          }
        }
        onReleased: function(mouse) {
          if (audioLevelRow.microphone) {
            root.microphoneDrag = valueAt(mouse.x)
            microphoneDragTimer.stop()
            root.runControl("microphone", String(root.microphoneDrag))
          } else {
            root.volumeDrag = valueAt(mouse.x)
            volumeDragTimer.stop()
            root.runControl("volume", String(root.volumeDrag))
          }
        }
        onWheel: function(wheel) { root.adjustAudioFromWheel(wheel, audioLevelRow.microphone) }
      }
    }

    Rectangle {
      width: 44
      height: 44
      radius: root.radius
      color: audioMuteMouse.pressed ? root.pressColor : audioLevelRow.muted ? root.dangerColor : audioMuteMouse.containsMouse ? root.hoverColor : root.surface

      Text {
        anchors.centerIn: parent
        text: audioLevelRow.microphone ? (audioLevelRow.muted ? "󰍭" : "󰍬") : (audioLevelRow.muted ? "󰝟" : "󰕾")
        color: audioLevelRow.muted ? root.red : root.text
        font.family: root.fontFamily
        font.pixelSize: 15
      }

      ModuleDragArea {
        id: audioMuteMouse
        onActivated: {
          if (audioLevelRow.microphone) {
            if (root.runControl("microphone", "mute")) root.patchSystemData({ microphoneMuted: !root.systemData.microphoneMuted })
          } else {
            if (root.runControl("volume", "mute")) root.patchSystemData({ muted: !root.systemData.muted })
          }
        }
      }

      HoverTip {
        mouse: audioMuteMouse
        text: audioLevelRow.microphone
          ? (audioLevelRow.muted ? "Unmute microphone" : "Mute microphone")
          : (audioLevelRow.muted ? "Unmute output" : "Mute output")
      }
    }
  }

  // One compact horizontal level inside the shared Control Center Audio card.
  // The mute button sits inside the track instead of consuming another column.
  component ControlLevel: Rectangle {
    id: controlLevel

    property bool microphone: false
    readonly property int shown: controlLevel.microphone
      ? (root.microphoneDrag >= 0 ? root.microphoneDrag : Number(root.systemData.microphoneVolume))
      : (root.volumeDrag >= 0 ? root.volumeDrag : Number(root.systemData.volume))
    readonly property int maximum: controlLevel.microphone ? 100 : root.outputVolumeMaximum
    readonly property bool muted: controlLevel.microphone ? !!root.systemData.microphoneMuted : !!root.systemData.muted
    readonly property real fillRatio: Math.max(0, Math.min(1, controlLevel.shown / controlLevel.maximum))

    radius: root.radius
    color: root.alpha(root.base, 0.52)
    clip: true

    Rectangle {
      width: parent.width * controlLevel.fillRatio
      height: parent.height
      radius: parent.radius
      color: controlLevel.muted ? root.fillDanger : root.fillColor
    }

    Rectangle {
      visible: !controlLevel.microphone
      x: parent.width * 100 / controlLevel.maximum
      width: 1
      height: parent.height
      color: root.alpha(root.text, 0.25)
    }

    Text {
      anchors.right: parent.right
      anchors.rightMargin: 10
      anchors.verticalCenter: parent.verticalCenter
      text: controlLevel.shown + "%"
      color: root.text
      font.family: root.fontFamily
      font.pixelSize: 10
      font.bold: true
    }

    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      function valueAt(x) { return Math.max(0, Math.min(controlLevel.maximum, Math.round(x / width * controlLevel.maximum))) }
      function updateValue(value) {
        if (controlLevel.microphone) {
          root.microphoneDrag = value
          microphoneDragTimer.restart()
        } else {
          root.volumeDrag = value
          volumeDragTimer.restart()
        }
      }
      onPressed: function(mouse) { updateValue(valueAt(mouse.x)) }
      onPositionChanged: function(mouse) {
        if (pressed) updateValue(valueAt(mouse.x))
      }
      onReleased: function(mouse) {
        var value = valueAt(mouse.x)
        if (controlLevel.microphone) {
          root.microphoneDrag = value
          microphoneDragTimer.stop()
          root.runControl("microphone", String(value))
        } else {
          root.volumeDrag = value
          volumeDragTimer.stop()
          root.runControl("volume", String(value))
        }
      }
      onWheel: function(wheel) { root.adjustAudioFromWheel(wheel, controlLevel.microphone) }
    }

    Rectangle {
      z: 2
      anchors.left: parent.left
      anchors.leftMargin: 6
      anchors.verticalCenter: parent.verticalCenter
      width: 30
      height: 30
      radius: width / 2
      color: controlLevelMuteMouse.pressed ? root.pressColor : controlLevel.muted ? root.dangerColor : controlLevelMuteMouse.containsMouse ? root.hoverColor : root.alpha(root.base, 0.72)

      Text {
        anchors.centerIn: parent
        text: controlLevel.microphone ? (controlLevel.muted ? "󰍭" : "󰍬") : (controlLevel.muted ? "󰝟" : "󰕾")
        color: controlLevel.muted ? root.red : root.text
        font.family: root.fontFamily
        font.pixelSize: 15
      }

      MouseArea {
        id: controlLevelMuteMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
          if (controlLevel.microphone) {
            if (root.runControl("microphone", "mute")) root.patchSystemData({ microphoneMuted: !root.systemData.microphoneMuted })
          } else {
            if (root.runControl("volume", "mute")) root.patchSystemData({ muted: !root.systemData.muted })
          }
        }
      }

      HoverTip {
        mouse: controlLevelMuteMouse
        text: controlLevel.microphone
          ? (controlLevel.muted ? "Unmute microphone" : "Mute microphone")
          : (controlLevel.muted ? "Unmute output" : "Mute output")
      }
    }
  }

  // One radio in the Control Center's connectivity card. The round knob owns
  // the radio itself and the rest of the row hands off to the panel that owns
  // the devices behind it, the way macOS expands a module in place.
  component ConnectivityRow: Item {
    id: connectivityRow

    property string icon: ""
    property string label: ""
    property string detail: ""
    property string module: ""
    property bool active: false
    property bool busy: false
    property bool toggleEnabled: true
    signal toggled()
    signal opened()

    height: 49
    opacity: connectivityRow.module !== "" && root.dragModule === connectivityRow.module ? 0.45 : 1

    Rectangle {
      id: connectivityKnob

      anchors.verticalCenter: parent.verticalCenter
      width: 30
      height: 30
      radius: width / 2
      opacity: connectivityRow.toggleEnabled ? 1 : 0.42
      color: connectivityKnobMouse.pressed ? root.pressColor : connectivityRow.active ? root.accent : connectivityKnobMouse.containsMouse ? root.hoverColor : root.alpha(root.overlay, 0.35)

      Text {
        visible: !connectivityRow.busy
        anchors.centerIn: parent
        text: connectivityRow.icon
        color: connectivityRow.active ? root.base : root.text
        font.family: root.fontFamily
        font.pixelSize: 15
      }

      RefreshGlyph {
        visible: connectivityRow.busy
        anchors.centerIn: parent
        width: 16
        height: 16
        spinning: visible
        color: connectivityRow.active ? root.base : root.text
        font.pixelSize: 11
      }

      MouseArea {
        id: connectivityKnobMouse
        anchors.fill: parent
        enabled: connectivityRow.toggleEnabled && !connectivityRow.busy
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: connectivityRow.toggled()
      }
    }

    Rectangle {
      anchors.left: connectivityKnob.right
      anchors.leftMargin: 4
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      height: 38
      radius: root.radius
      color: connectivityLabelMouse.pressed ? root.pressColor : connectivityLabelMouse.containsMouse ? root.hoverColor : "transparent"

      Column {
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: parent.left
        anchors.leftMargin: 8
        anchors.right: parent.right
        anchors.rightMargin: 22
        spacing: 1

        Text { width: parent.width; text: connectivityRow.label; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
        Text { width: parent.width; text: connectivityRow.detail; elide: Text.ElideRight; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 9 }
      }

      // The hand-off is only worth advertising under the pointer; the row is
      // quiet otherwise.
      Text {
        visible: connectivityLabelMouse.containsMouse
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: "󰅂"
        color: root.overlay
        font.family: root.fontFamily
        font.pixelSize: 12
      }

      ModuleDragArea {
        id: connectivityLabelMouse
        module: connectivityRow.module
        onActivated: connectivityRow.opened()
      }
    }
  }

  // A Control Center module tile. The glyph is a component slot because each
  // supported headphone family has its own silhouette.
  component ControlTile: Rectangle {
    id: controlTile

    property Component glyph: null
    property string label: ""
    property string detail: ""
    property string module: ""
    property bool active: false
    property bool compact: false
    signal activated()

    radius: root.radius
    opacity: controlTile.module !== "" && root.dragModule === controlTile.module ? 0.45 : 1
    color: controlTileMouse.pressed ? root.pressColor : controlTile.active ? root.activeTint : controlTileMouse.containsMouse ? root.hoverColor : root.surface

    Row {
      anchors.fill: parent
      anchors.leftMargin: controlTile.compact ? 8 : 10
      anchors.rightMargin: controlTile.compact ? 8 : 10
      spacing: controlTile.compact ? 5 : 9

      Item {
        width: controlTile.compact ? 18 : 22
        height: parent.height
        Loader { anchors.centerIn: parent; sourceComponent: controlTile.glyph }
      }

      Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - (controlTile.compact ? 23 : 31)
        spacing: 2

        Text { width: parent.width; text: controlTile.label; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: controlTile.compact ? 10 : 11; font.bold: true }
        Text { visible: !controlTile.compact; width: parent.width; text: controlTile.detail; elide: Text.ElideRight; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 9 }
      }
    }

    ModuleDragArea {
      id: controlTileMouse
      module: controlTile.module
      onActivated: controlTile.activated()
    }
  }

  component ControlCenterGrid: Item {
    id: controlGrid

    property string screenName: ""
    readonly property real gap: 8
    readonly property real cellSize: (width - gap * 3) / 4
    readonly property real mediaSize: cellSize * 2 + gap
    readonly property real mediaHeight: 148
    readonly property real controlSpacing: 4
    readonly property real audioPadding: 12
    readonly property real audioSliderHeight: 46
    readonly property real controlsHeight: audioSliderHeight * 2 + audioPadding * 3
    readonly property real smallTileHeight: 55
    readonly property real controlsY: mediaHeight + gap
    readonly property real devicesY: controlsY + controlsHeight + gap

    height: devicesY + smallTileHeight

    Rectangle {
      id: controlCenterMedia

      readonly property var player: root.nowPlayingPlayer()
      width: parent.width
      height: controlGrid.mediaHeight
      radius: root.radius
      color: root.surface
      opacity: root.dragModule === "media" ? 0.45 : 1

      ModuleDragArea {
        module: "media"
        cursorShape: Qt.ArrowCursor
        onActivated: root.toggleMedia(controlCenterMedia.player, controlGrid.screenName)
      }

      Item {
        id: controlCenterMediaArtFrame

        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.margins: 12
        width: height

        Image {
          id: controlCenterMediaArt

          anchors.fill: parent
          visible: false
          source: controlCenterMedia.player ? String(controlCenterMedia.player.trackArtUrl || "") : ""
          fillMode: Image.PreserveAspectCrop
          sourceSize.width: width * 3
          sourceSize.height: height * 3
          smooth: true
          mipmap: true
          asynchronous: true
          cache: true
        }

        RoundedSource {
          anchors.fill: parent
          source: controlCenterMediaArt
          radius: root.radiusSmall
          visible: controlCenterMediaArt.status === Image.Ready
        }

        Rectangle {
          anchors.fill: parent
          visible: controlCenterMediaArt.status !== Image.Ready
          radius: root.radiusSmall
          color: root.mantle
          Text {
            anchors.centerIn: parent
            text: "󰎆"
            color: controlCenterMedia.player ? root.accent : root.overlay
            font.family: root.fontFamily
            font.pixelSize: 28
          }
        }
      }

      Item {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.left: controlCenterMediaArtFrame.right
        anchors.right: parent.right
        anchors.topMargin: 12
        anchors.bottomMargin: 9
        anchors.leftMargin: 12
        anchors.rightMargin: 12

        Text {
          id: controlCenterMediaTitle

          anchors.top: parent.top
          anchors.left: parent.left
          anchors.right: parent.right
          text: controlCenterMedia.player ? (root.mediaTitle(controlCenterMedia.player) || "Unknown track") : "Not Playing"
          elide: Text.ElideRight
          color: root.text
          font.family: root.fontFamily
          font.pixelSize: 12
          font.bold: true
        }

        Text {
          visible: text !== ""
          anchors.top: controlCenterMediaTitle.bottom
          anchors.topMargin: 2
          anchors.left: parent.left
          anchors.right: parent.right
          text: controlCenterMedia.player ? root.mediaSubtitle(controlCenterMedia.player) : ""
          elide: Text.ElideRight
          color: root.subtext
          font.family: root.fontFamily
          font.pixelSize: 9
        }

        Row {
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.verticalCenter: parent.verticalCenter
          anchors.verticalCenterOffset: 4
          spacing: 9

          MediaButton {
            flat: true
            icon: "󰒮"
            enabled: !!controlCenterMedia.player && controlCenterMedia.player.canGoPrevious
            onActivated: controlCenterMedia.player.previous()
          }
          MediaButton {
            flat: true
            icon: controlCenterMedia.player && controlCenterMedia.player.isPlaying ? "󰏤" : "󰐊"
            primary: true
            enabled: !!controlCenterMedia.player
            onActivated: controlCenterMedia.player.togglePlaying()
          }
          MediaButton {
            flat: true
            icon: "󰒭"
            enabled: !!controlCenterMedia.player && controlCenterMedia.player.canGoNext
            onActivated: controlCenterMedia.player.next()
          }
        }

        MediaTimeline {
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.bottom: parent.bottom
          player: controlCenterMedia.player
        }
      }
    }

    Rectangle {
      y: controlGrid.controlsY
      width: controlGrid.mediaSize
      height: controlGrid.controlsHeight
      radius: root.radius
      color: root.surface

      Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        spacing: controlGrid.controlSpacing

        ConnectivityRow {
          width: parent.width
          height: 40
          module: "network"
          icon: root.systemData.connection === "Disconnected" ? "󰖪" : root.systemData.connectionType.indexOf("wireless") >= 0 ? "󰖩" : "󰈀"
          label: root.systemData.wifiAvailable ? "Wi-Fi" : "Network"
          detail: root.systemData.wifiAvailable && !root.systemData.wifiEnabled ? "Off" : (root.systemData.connection || "Disconnected")
          active: root.systemData.wifiAvailable ? root.systemData.wifiEnabled : root.systemData.connection !== "Disconnected"
          toggleEnabled: root.systemData.wifiAvailable
          busy: root.controlBusy("wifi", "toggle")
          onToggled: if (root.runControl("wifi", "toggle")) root.patchSystemData({ wifiEnabled: !root.systemData.wifiEnabled })
          onOpened: root.toggleControl("network", controlGrid.screenName)
        }

        ConnectivityRow {
          width: parent.width
          height: 40
          module: "bluetooth"
          visible: root.systemData.bluetoothAvailable
          icon: root.systemData.bluetoothPowered ? "󰂯" : "󰂲"
          label: "Bluetooth"
          detail: !root.systemData.bluetoothAvailable ? "Unavailable"
            : !root.systemData.bluetoothPowered ? "Off"
            : root.systemData.bluetoothConnected + " connected"
          active: root.systemData.bluetoothPowered
          toggleEnabled: root.systemData.bluetoothAvailable
          busy: bluetoothProcess.running && root.bluetoothAction === "toggle"
          onToggled: root.toggleBluetoothPower()
          onOpened: root.toggleControl("bluetooth", controlGrid.screenName)
        }

        ConnectivityRow {
          width: parent.width
          height: 40
          module: "vpn"
          icon: "󰒃"
          label: "VPN"
          detail: root.privateNetworkDetail()
          active: root.privateNetworkActive()
          toggleEnabled: root.privateNetworkTarget() !== ""
          busy: root.privateNetworkBusy()
          onToggled: root.togglePrivateNetwork()
          onOpened: root.toggleControl("vpn", controlGrid.screenName)
        }
      }
    }

    Rectangle {
      x: controlGrid.mediaSize + controlGrid.gap
      y: controlGrid.controlsY
      width: controlGrid.mediaSize
      height: controlGrid.controlsHeight
      radius: root.radius
      color: controlCenterAudioMouse.pressed ? root.pressColor : controlCenterAudioMouse.containsMouse ? root.hoverColor : root.surface
      opacity: root.dragModule === "audio" ? 0.45 : 1

      ModuleDragArea {
        id: controlCenterAudioMouse
        module: "audio"
        onActivated: root.toggleControl("audio", controlGrid.screenName)
      }

      Column {
        z: 2
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: controlGrid.audioPadding

        ControlLevel {
          width: parent.width
          height: controlGrid.audioSliderHeight
        }

        ControlLevel {
          width: parent.width
          height: controlGrid.audioSliderHeight
          microphone: true
        }
      }
    }

    ControlTile {
      y: controlGrid.devicesY
      width: (root.systemData.headphones || {}).connected ? controlGrid.mediaSize : controlGrid.width
      height: controlGrid.smallTileHeight
      module: "camera"
      label: "Camera"
      detail: root.cameraDetail()
      active: root.systemData.cameraActive
      glyph: Text {
        text: root.systemData.cameraActive ? "󰄀" : "󰄁"
        color: root.systemData.cameraActive ? root.red : root.accent
        font.family: root.fontFamily
        font.pixelSize: 14
      }
      onActivated: root.toggleControl("camera", controlGrid.screenName)
    }

    ControlTile {
      visible: !!(root.systemData.headphones || {}).connected
      x: controlGrid.mediaSize + controlGrid.gap
      y: controlGrid.devicesY
      width: controlGrid.mediaSize
      height: controlGrid.smallTileHeight
      module: "airpods"
      label: (root.systemData.headphones || {}).name || "Headphones"
      detail: root.headphonesDetail()
      active: true
      glyph: HeadphonesIcon { kind: String((root.systemData.headphones || {}).kind || "airpods"); tint: root.accent }
      onActivated: root.toggleControl("airpods", controlGrid.screenName)
    }
  }

  component NotificationList: SeeleListView {
    id: notificationList

    property bool history: false
    property bool popup: false
    // Everywhere except a toast, a notification is shown in full without being
    // asked: the panel is where you go to read what you missed.
    readonly property bool alwaysUnfolded: !notificationList.popup

    spacing: 6
    clip: true
    model: notificationList.history ? (root.systemData.notifications.history || [])
      : notificationList.popup ? root.notificationPopupEntries()
      : (root.systemData.notifications.items || [])

    delegate: Rectangle {
      id: notificationEntry

      required property var modelData
      readonly property bool actionable: !notificationList.history && root.notificationActionable(modelData)
      readonly property bool unfolded: notificationList.alwaysUnfolded || !!root.notificationUnfolded[String(modelData.id)]
      // A single elided line reports its full width, which is the only way to
      // know there is more to show without measuring the text twice.
      readonly property bool truncated: notificationBody.implicitWidth > notificationBody.width
      // Only a toast folds, and only when there is something folded away.
      readonly property bool unfoldable: !notificationList.alwaysUnfolded && (truncated || unfolded)
      width: ListView.view.width
      height: Math.max(60, notificationText.implicitHeight + 18)
      radius: root.radius
      color: notificationEntry.actionable && notificationOpenMouse.pressed ? root.pressColor
        : notificationEntry.actionable && notificationOpenMouse.containsMouse ? root.hoverColor
        : root.surface

      MouseArea {
        id: notificationOpenMouse
        anchors.fill: parent
        enabled: notificationEntry.actionable
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.activateNotification(notificationEntry.modelData.id)
      }

      Column {
        id: notificationText
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 9
        spacing: 3
        Row {
          width: parent.width
          Text { width: parent.width - (notificationEntry.unfoldable ? 110 : 84); text: modelData.summary || modelData.app_name || "Notification"; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
          Text { width: 58; text: root.agoText(modelData.time); color: root.overlay; font.family: root.fontFamily; font.pixelSize: 9; horizontalAlignment: Text.AlignRight }
          Rectangle {
            visible: notificationEntry.unfoldable
            width: visible ? 26 : 0
            height: 20
            radius: root.radiusSmall
            color: notificationUnfoldMouse.pressed ? root.pressColor : notificationUnfoldMouse.containsMouse ? root.hoverColor : "transparent"
            Text {
              anchors.centerIn: parent
              text: notificationEntry.unfolded ? "󰅃" : "󰅀"
              color: notificationUnfoldMouse.containsMouse ? root.accent : root.subtext
              font.family: root.fontFamily
              font.pixelSize: 10
            }
            MouseArea {
              id: notificationUnfoldMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.toggleNotificationUnfolded(notificationEntry.modelData.id)
            }
            HoverTip { mouse: notificationUnfoldMouse; text: notificationEntry.unfolded ? "Show less" : "Show the whole notification" }
          }
          Rectangle {
            readonly property bool busy: !notificationList.popup && root.controlBusy("notifications", "dismiss", String(notificationEntry.modelData.id))
            visible: !notificationList.history
            width: visible ? 26 : 0
            height: 20
            radius: root.radiusSmall
            color: notificationDismissMouse.pressed ? root.dangerPress : busy ? root.selectedColor : notificationDismissMouse.containsMouse ? root.dangerColor : "transparent"
            Text { visible: !parent.busy; anchors.centerIn: parent; text: "󰅖"; color: notificationDismissMouse.containsMouse ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
            RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 14; height: 14; spinning: visible; font.pixelSize: 10 }
            MouseArea {
              id: notificationDismissMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              // On a popup this closes the toast and leaves the notification
              // in the panel; in the panel it is the actual dismissal.
              onClicked: notificationList.popup
                ? root.retireNotificationPopup(notificationEntry.modelData.id)
                : root.dismissNotification(notificationEntry.modelData.id)
            }
            HoverTip { mouse: notificationDismissMouse; text: notificationList.popup ? "Hide · stays in notifications" : "Dismiss" }
          }
        }
        Text {
          id: notificationBody
          width: parent.width
          text: modelData.body || modelData.app_name || ""
          color: root.subtext
          font.family: root.fontFamily
          font.pixelSize: 9
          wrapMode: notificationEntry.unfolded ? Text.WordWrap : Text.NoWrap
          elide: notificationEntry.unfolded ? Text.ElideNone : Text.ElideRight
          // Bounded, so one pathological notification cannot take the panel.
          maximumLineCount: notificationEntry.unfolded ? 8 : 1
        }
      }


    }
  }

  // Notification popups -------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: notificationPopupWindow

      required property var modelData
      readonly property var entries: root.notificationPopupEntries()
      screen: modelData
      visible: !root.systemData.dnd
        && root.controlPanel !== "notifications"
        && entries.length > 0
        && root.pinnedScreen(root.notificationPopupScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 400
      // Rows size themselves to whatever is unfolded, so the surface follows
      // the list's own content rather than a fixed row height.
      implicitHeight: Math.min(430, Math.max(66, notificationPopupList.contentHeight)) + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-notifications"

      PanelSurface {
        NotificationList {
          id: notificationPopupList
          popup: true
          anchors.fill: parent
          anchors.margins: root.panelMargin
        }
      }
    }
  }

  // A menu bar media entry outlives the track it is showing. A pause is usually
  // a short interruption, so the slot keeps the player it last saw playing for
  // a minute rather than collapsing mid-track and reflowing every entry beside
  // it. The held player is checked against the bus on every read, because a
  // client that quits takes its player object with it.
  component MediaSlot: QtObject {
    id: mediaSlot

    property var playing: null
    property var held: null
    property Timer hold: Timer {
      interval: 60000
      onTriggered: mediaSlot.held = null
    }

    readonly property var player: mediaSlot.playing || Media.presentPlayer(Mpris.players.values || [], mediaSlot.held)

    onPlayingChanged: {
      if (mediaSlot.playing) {
        mediaSlot.held = mediaSlot.playing
        mediaSlot.hold.stop()
      } else if (mediaSlot.held) {
        mediaSlot.hold.restart()
      }
    }
  }

  // Artwork for a menu bar media entry. Playback state belongs to the entry as
  // much as the track does, so a held player dims its artwork behind a pause
  // glyph, and one without artwork shows that glyph in place of its icon.
  component BarMediaArt: Item {
    id: barMediaArt

    property var player: null
    property string icon: ""
    property color iconColor: root.accent
    readonly property bool paused: !!barMediaArt.player && !barMediaArt.player.isPlaying
    readonly property bool hasArt: barMediaArtImage.status === Image.Ready

    width: 16
    height: 16

    Image {
      id: barMediaArtImage

      anchors.fill: parent
      visible: false
      source: barMediaArt.player ? String(barMediaArt.player.trackArtUrl || "") : ""
      fillMode: Image.PreserveAspectCrop
      sourceSize.width: width * 4
      sourceSize.height: height * 4
      smooth: true
      mipmap: true
      asynchronous: true
      cache: true
    }

    RoundedSource {
      anchors.fill: parent
      source: barMediaArtImage
      radius: root.radiusSmall
      visible: barMediaArt.hasArt
      opacity: barMediaArt.paused ? 0.4 : 1
    }

    Text {
      anchors.centerIn: parent
      visible: !barMediaArt.hasArt && !barMediaArt.paused
      text: barMediaArt.icon
      color: barMediaArt.iconColor
      font.family: root.fontFamily
      font.pixelSize: 13
    }

    Text {
      anchors.centerIn: parent
      visible: barMediaArt.paused
      text: "󰏤"
      color: barMediaArt.hasArt ? root.text : root.mutedText
      font.family: root.fontFamily
      font.pixelSize: barMediaArt.hasArt ? 11 : 13
    }
  }

  component MediaButton: Rectangle {
    id: mediaButton

    property string icon: ""
    property bool primary: false
    property bool flat: false
    signal activated()

    width: mediaButton.flat ? (mediaButton.primary ? 38 : 34) : mediaButton.primary ? 34 : 28
    height: mediaButton.flat ? 32 : 28
    radius: root.radius
    opacity: mediaButton.enabled ? 1 : 0.35
    color: mediaButtonMouse.pressed ? root.pressColor : mediaButtonMouse.containsMouse ? root.hoverColor : mediaButton.flat ? "transparent" : mediaButton.primary ? root.alpha(root.accent, 0.22) : root.mantle

    Text {
      anchors.centerIn: parent
      text: mediaButton.icon
      color: root.text
      font.family: root.fontFamily
      font.pixelSize: mediaButton.flat ? (mediaButton.primary ? 22 : 18) : mediaButton.primary ? 15 : 13
    }

    MouseArea { id: mediaButtonMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: mediaButton.activated() }
  }

  component MediaTimeline: Column {
    id: mediaTimeline

    property var player: null
    property real draggedPosition: -1
    property int tick: 0
    readonly property bool available: root.mediaTimelineAvailable(mediaTimeline.player)
    readonly property bool live: root.mediaIsLive(mediaTimeline.player)
    readonly property real length: mediaTimeline.available && !mediaTimeline.live ? Number(mediaTimeline.player.length) : 0
    readonly property real reportedPosition: {
      var refresh = mediaTimeline.tick
      return mediaTimeline.player ? Number(mediaTimeline.player.position) : 0
    }
    readonly property real shownPosition: mediaTimeline.draggedPosition >= 0
      ? mediaTimeline.draggedPosition
      : Math.max(0, Math.min(mediaTimeline.length, mediaTimeline.reportedPosition))

    visible: available
    spacing: 3

    Rectangle {
      width: parent.width
      height: 16
      color: "transparent"

      Rectangle {
        visible: !mediaTimeline.live
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 5
        radius: height / 2
        color: root.alpha(root.overlay, 0.4)

        Rectangle {
          width: parent.width * (mediaTimeline.length > 0 ? mediaTimeline.shownPosition / mediaTimeline.length : 0)
          height: parent.height
          radius: parent.radius
          color: root.accent
        }
      }

      Rectangle {
        visible: !mediaTimeline.live && mediaTimeline.draggedPosition >= 0
        x: Math.max(0, Math.min(parent.width - width, parent.width * (mediaTimeline.length > 0 ? mediaTimeline.shownPosition / mediaTimeline.length : 0) - width / 2))
        anchors.verticalCenter: parent.verticalCenter
        width: 10
        height: 10
        radius: width / 2
        color: timelineMouse.pressed ? root.text : root.accent
      }

      MouseArea {
        id: timelineMouse

        anchors.fill: parent
        enabled: !mediaTimeline.live
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        function positionAt(x) { return Math.max(0, Math.min(mediaTimeline.length, x / width * mediaTimeline.length)) }
        onPressed: function(mouse) { mediaTimeline.draggedPosition = positionAt(mouse.x) }
        onPositionChanged: function(mouse) {
          if (pressed) mediaTimeline.draggedPosition = positionAt(mouse.x)
        }
        onReleased: function(mouse) {
          var position = positionAt(mouse.x)
          mediaTimeline.draggedPosition = -1
          if (mediaTimeline.player) mediaTimeline.player.position = position
          mediaTimeline.tick++
        }
        onCanceled: mediaTimeline.draggedPosition = -1
      }

      Text {
        id: liveTimelineLabel
        visible: mediaTimeline.live
        anchors.centerIn: parent
        text: "LIVE"
        color: root.overlay
        font.family: root.fontFamily
        font.pixelSize: 9
        font.bold: true
      }

      Rectangle {
        visible: mediaTimeline.live
        anchors.left: parent.left
        anchors.right: liveTimelineLabel.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        height: 5
        radius: height / 2
        color: root.overlay
      }

      Rectangle {
        visible: mediaTimeline.live
        anchors.left: liveTimelineLabel.right
        anchors.leftMargin: 8
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 5
        radius: height / 2
        color: root.overlay
      }
    }

    Row {
      visible: !mediaTimeline.live
      width: parent.width
      Text { width: parent.width / 2; text: root.formatMediaTime(mediaTimeline.shownPosition); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 9 }
      Text { width: parent.width / 2; text: root.formatMediaTime(mediaTimeline.length); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 9; horizontalAlignment: Text.AlignRight }
    }

    Timer {
      interval: 1000
      repeat: true
      running: mediaTimeline.visible && !mediaTimeline.live && !!mediaTimeline.player && mediaTimeline.player.isPlaying && mediaTimeline.draggedPosition < 0
      onTriggered: mediaTimeline.tick++
    }
  }

  Timer {
    id: volumeDragTimer
    interval: 32
    onTriggered: {
      if (root.volumeDrag < 0) return
      if (!root.runControl("volume", String(root.volumeDrag))) restart()
    }
  }

  Timer {
    id: microphoneDragTimer
    interval: 32
    onTriggered: {
      if (root.microphoneDrag < 0) return
      if (!root.runControl("microphone", String(root.microphoneDrag))) restart()
    }
  }

  // Wallpaper ----------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      anchors { top: true; bottom: true; left: true; right: true }
      exclusionMode: ExclusionMode.Ignore
      WlrLayershell.layer: WlrLayer.Background
      WlrLayershell.namespace: "seele-shell-background"
      color: root.base

      Image {
        anchors.fill: parent
        source: "file://" + root.wallpaper
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
      }
    }
  }

  // Bar ----------------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: barWindow

      // Like the Control Center panel, the bar keeps a surface larger than the
      // strip it draws, so a module dragged off it stays inside the surface that
      // was pressed and the gesture is not cut short. The reserved space stays
      // pinned to the visible strip, and everything below it takes input only
      // while a module is being pulled out.
      readonly property bool dragging: root.dragKind === "remove" || root.barPressModule !== ""

      required property var modelData
      screen: modelData
      anchors { top: true; left: true; right: true }
      implicitHeight: modelData.height
      exclusionMode: ExclusionMode.Normal
      exclusiveZone: root.barHeight
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Top
      WlrLayershell.namespace: "seele-shell-bar"
      mask: Region {
        width: barWindow.width
        height: barWindow.dragging ? barWindow.height : root.barHeight
      }

      Rectangle {
        id: barSurface
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: root.barHeight
        color: root.alpha(root.mantle, 0.9)

        SurfaceWash {}

        Row {
          anchors.left: parent.left
          anchors.top: parent.top
          anchors.bottom: parent.bottom
          spacing: 0

          BarItem {
            width: 30
            hovered: menuMouse.containsMouse
            Image {
              anchors.centerIn: parent
              width: 20
              height: 20
              source: "seele.svg"
              fillMode: Image.PreserveAspectFit
              smooth: true
              mipmap: true
            }
            MouseArea {
              id: menuMouse
              anchors.fill: parent
              hoverEnabled: true
              onClicked: root.toggleLauncher("apps")
            }
            HoverTip { mouse: menuMouse; text: "Applications" }
          }

          Repeater {
            model: root.workspaceIds(barWindow.modelData)
            Item {
              required property int modelData
              readonly property bool active: root.workspaceActive(modelData, barWindow.modelData)
              readonly property bool occupied: root.workspaceOccupied(modelData)
              width: active ? 44 : 22
              height: parent.height

              Behavior on width {
                NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
              }

              Rectangle {
                id: workspacePill
                width: parent.active ? 42 : 20
                height: root.barItemHeight
                anchors.centerIn: parent
                radius: root.radius
                color: parent.active ? root.accent : workspaceMouse.containsMouse ? root.alpha(root.accent, 0.55) : parent.occupied ? root.alpha(root.subtext, 0.65) : root.alpha(root.subtext, 0.3)

                Behavior on width {
                  NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
                }

                Behavior on color {
                  ColorAnimation { duration: 140 }
                }

                Text {
                  anchors.centerIn: parent
                  text: String(parent.parent.modelData)
                  color: root.base
                  font.family: root.fontFamily
                  font.pixelSize: 10
                  font.bold: parent.parent.active
                }
              }

              MouseArea {
                id: workspaceMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.activateWorkspace(parent.modelData)
              }
              HoverTip { mouse: workspaceMouse; text: "Workspace " + modelData }
            }
          }

          BarItem {
            readonly property var window: root.activeWindow(barWindow.modelData)
            visible: window !== null && root.windowLabel(window) !== ""
            width: Math.min(230, activeWindowRow.implicitWidth + 14)
            hovered: activeWindowMouse.containsMouse
            Row {
              id: activeWindowRow
              anchors.centerIn: parent
              height: parent.height
              spacing: 6
              IconImage {
                visible: source !== ""
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: 15; implicitHeight: 15
                source: root.windowIcon(parent.parent.window)
              }
              BarLabel {
                anchors.verticalCenter: parent.verticalCenter
                height: parent.height
                maximumWidth: 190
                color: root.subtext
                text: root.windowLabel(parent.parent.window)
              }
            }
            MouseArea { id: activeWindowMouse; anchors.fill: parent; hoverEnabled: true }
            HoverTip { mouse: activeWindowMouse; text: root.windowTitle(activeWindowMouse.parent.window) }
          }

          BarItem {
            width: 30
            hovered: voxtypeMouse.containsMouse
            Text {
              anchors.centerIn: parent
              text: root.systemData.voxtypeStatus === "recording" ? "󰍬" : root.systemData.voxtypeStatus === "transcribing" ? "󰔟" : "󰍭"
              color: root.systemData.voxtypeStatus === "recording" ? root.red : root.systemData.voxtypeStatus === "transcribing" ? root.yellow : root.subtext
              font.family: root.fontFamily
              font.pixelSize: 14
            }
            MouseArea { id: voxtypeMouse; anchors.fill: parent; hoverEnabled: true; onClicked: root.runControl("voxtype") }
            HoverTip { mouse: voxtypeMouse; text: "Voxtype: " + root.systemData.voxtypeStatus }
          }
        }

        Row {
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.top: parent.top
          anchors.bottom: parent.bottom
          spacing: 0

          BarItem {
            width: localClock.implicitWidth + 14
            hovered: clockMouse.containsMouse
            active: root.panelHere("clock", barWindow.modelData)
            Text {
              id: localClock
              anchors.centerIn: parent
              text: Qt.formatDateTime(root.now, "HH:mm")
              color: root.text
              font.family: root.fontFamily
              font.pixelSize: 12
              font.bold: true
            }
            MouseArea { id: clockMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("clock", barWindow.modelData.name) }
            HoverTip { mouse: clockMouse; text: "Time zones" }
          }

          BarItem {
            width: localDate.implicitWidth + 14
            hovered: dateMouse.containsMouse
            active: root.panelHere("calendar", barWindow.modelData)
            Text {
              id: localDate
              anchors.centerIn: parent
              text: Qt.formatDateTime(root.now, "yyyy-MM-dd")
              color: root.subtext
              font.family: root.fontFamily
              font.pixelSize: 12
            }
            MouseArea { id: dateMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("calendar", barWindow.modelData.name) }
            HoverTip { mouse: dateMouse; text: "Calendar" }
          }

          BarItem {
            visible: root.systemData.microphoneMuted
            width: 30
            hovered: microphoneMutedIndicator.containsMouse
            Rectangle {
              anchors.centerIn: parent
              width: 26; height: 18; radius: root.radiusSmall
              color: root.alpha(root.overlay, 0.35)
              Text {
                anchors.centerIn: parent
                text: "󰍭"
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: 12
              }
            }
            MouseArea {
              id: microphoneMutedIndicator
              anchors.fill: parent
              hoverEnabled: true
              onClicked: {
                root.patchSystemData({ microphoneMuted: false })
                root.runControl("microphone", "mute")
              }
            }
            HoverTip { mouse: microphoneMutedIndicator; text: "Microphone muted · click to unmute" }
          }

          BarItem {
            visible: root.systemData.microphoneActive && !root.systemData.microphoneMuted
            width: 20
            hovered: microphoneActiveIndicator.containsMouse
            Rectangle {
              anchors.centerIn: parent
              width: 9; height: 9; radius: 4.5
              color: root.iosOrange
            }
            MouseArea {
              id: microphoneActiveIndicator
              anchors.fill: parent
              hoverEnabled: true
              onClicked: {
                root.patchSystemData({ microphoneMuted: true })
                root.runControl("microphone", "mute")
              }
            }
            HoverTip { mouse: microphoneActiveIndicator; text: "Microphone in use · click to mute" }
          }

          BarItem {
            visible: root.systemData.cameraActive
            width: 20
            hovered: cameraActiveIndicator.containsMouse
            Rectangle {
              anchors.centerIn: parent
              width: 9; height: 9; radius: 4.5
              color: root.iosGreen
            }
            MouseArea { id: cameraActiveIndicator; anchors.fill: parent; hoverEnabled: true; onPressed: root.toggleControl("camera", barWindow.modelData.name) }
            HoverTip { mouse: cameraActiveIndicator; text: "Camera in use" }
          }

          BarItem {
            visible: root.systemData.screenRecording
            width: 20
            hovered: screenRecordingIndicator.containsMouse
            Rectangle {
              anchors.centerIn: parent
              width: 9; height: 9; radius: 4.5
              color: root.iosRed
            }
            MouseArea { id: screenRecordingIndicator; anchors.fill: parent; hoverEnabled: true }
            HoverTip { mouse: screenRecordingIndicator; text: "Screen is being recorded" }
          }
        }

        Row {
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.bottom: parent.bottom
          spacing: 0

          BarItem {
            id: deviceMediaItem

            readonly property var player: root.devicePlayer()
            module: "media"
            visible: player !== null && root.barModulePinned("media")
            width: visible ? Math.min(210, deviceMediaRow.implicitWidth + 14) : 0
            hovered: deviceMediaMouse.containsMouse
            active: root.panelHere("media", barWindow.modelData) && root.mediaPanelPlayer === player
            Row {
              id: deviceMediaRow
              anchors.centerIn: parent
              height: parent.height
              spacing: 5
              BarMediaArt {
                anchors.verticalCenter: parent.verticalCenter
                player: deviceMediaItem.player
                icon: "󰎆"
                iconColor: root.accent
              }
              BarLabel {
                anchors.verticalCenter: parent.verticalCenter
                height: parent.height
                maximumWidth: 175
                text: root.mediaLabel(parent.parent.player)
              }
            }
            BarModuleArea { id: deviceMediaMouse; module: "media"; onActivated: root.toggleMedia(parent.player, barWindow.modelData.name) }
            HoverTip { mouse: deviceMediaMouse; text: root.mediaLabel(deviceMediaItem.player) }
          }

          BarItem {
            id: spotifyMediaItem

            readonly property var player: root.spotifyPlayer()
            module: "media"
            visible: player !== null && root.barModulePinned("media")
            width: visible ? Math.min(210, spotifyMediaRow.implicitWidth + 14) : 0
            hovered: spotifyMediaMouse.containsMouse
            active: root.panelHere("media", barWindow.modelData) && root.mediaPanelPlayer === player
            Row {
              id: spotifyMediaRow
              anchors.centerIn: parent
              height: parent.height
              spacing: 5
              BarMediaArt {
                anchors.verticalCenter: parent.verticalCenter
                player: spotifyMediaItem.player
                icon: "󰓇"
                iconColor: root.green
              }
              BarLabel {
                anchors.verticalCenter: parent.verticalCenter
                height: parent.height
                maximumWidth: 175
                text: root.mediaLabel(parent.parent.player)
              }
            }
            BarModuleArea { id: spotifyMediaMouse; module: "media"; onActivated: root.toggleMedia(parent.player, barWindow.modelData.name) }
            HoverTip { mouse: spotifyMediaMouse; text: root.mediaLabel(spotifyMediaItem.player) }
          }

          // The bar is anchored to its right edge, so the tray grows leftward
          // and the expander has to be the group's rightmost item. Placed ahead
          // of the icons it reveals, every click slid it out from under the
          // pointer by exactly the width it had just added, and the next click
          // landed on whichever icon had taken its place -- so the arrow opened
          // the tray but could never close it again.
          Row {
            anchors.verticalCenter: parent.verticalCenter
            height: root.barHeight
            spacing: 0

          Repeater {
            model: root.trayItems()
            BarItem {
              required property var modelData
              width: 30
              hovered: trayMouse.containsMouse
              opacity: root.trayItemHidden(modelData) ? 0.45 : 1
              IconImage {
                anchors.centerIn: parent
                implicitWidth: 16; implicitHeight: 16
                source: parent.modelData.icon
              }
              MouseArea {
                id: trayMouse
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                preventStealing: true
                function openContextMenu() {
                  root.openTrayItemMenu(parent.modelData, barWindow.modelData.name)
                }
                onClicked: function(mouse) {
                  if (mouse.button === Qt.MiddleButton) root.toggleTrayItemHidden(parent.modelData)
                  else if (parent.modelData.onlyMenu) openContextMenu()
                  else parent.modelData.activate()
                }
                onWheel: function(wheel) { parent.modelData.scroll(Math.round(wheel.angleDelta.y / 8), false) }
              }
              TapHandler {
                acceptedButtons: Qt.RightButton
                gesturePolicy: TapHandler.WithinBounds
                onTapped: trayMouse.openContextMenu()
              }
              HoverTip { mouse: trayMouse; text: modelData.title || modelData.id || "Tray item" }
            }
          }

            BarItem {
              visible: root.trayHiddenCount() > 0
              width: 22
              hovered: trayExpandMouse.containsMouse
              Text {
                anchors.centerIn: parent
                text: root.trayExpanded ? "󰅂" : "󰅁"
                color: root.trayExpanded ? root.accent : root.overlay
                font.family: root.fontFamily
                font.pixelSize: 13
              }
              MouseArea { id: trayExpandMouse; anchors.fill: parent; hoverEnabled: true; onClicked: root.trayPinned = !root.trayPinned }
              HoverTip {
                mouse: trayExpandMouse
                text: root.trayPinned ? "Keep the tray open · click to unpin"
                  : root.trayHiddenCount() + " hidden tray icon" + (root.trayHiddenCount() === 1 ? "" : "s")
              }
            }
          }

          BarItem {
            module: "camera"
            visible: root.barModulePinned("camera") && (root.systemData.cameraActive || (root.systemData.cameraDevices && root.systemData.cameraDevices.length > 0))
            width: 30
            hovered: cameraMouse.containsMouse
            active: root.panelHere("camera", barWindow.modelData)
            Text { anchors.centerIn: parent; text: root.systemData.cameraActive ? "󰄀" : "󰄁"; color: root.systemData.cameraActive ? root.red : root.text; font.family: root.fontFamily; font.pixelSize: 14 }
            BarModuleArea { id: cameraMouse; module: "camera"; onActivated: root.toggleControl("camera", barWindow.modelData.name) }
            HoverTip { mouse: cameraMouse; text: root.systemData.cameraActive ? "Camera in use" : "Camera" }
          }

          Repeater {
            model: root.activeAgents()
            BarItem {
              required property var modelData
              id: agentBadgeItem

              readonly property color stateColor: root.agentColor(modelData.status)
              width: 28
              hovered: agentBadgeMouse.containsMouse
              Column {
                anchors.centerIn: parent
                spacing: 2
                Image {
                  visible: modelData.id === "claude"
                  anchors.horizontalCenter: parent.horizontalCenter
                  width: 13
                  height: 13
                  source: "claude-code.svg"
                  fillMode: Image.PreserveAspectFit
                  smooth: true
                  mipmap: true
                }
                Text {
                  visible: modelData.id !== "claude"
                  anchors.horizontalCenter: parent.horizontalCenter
                  text: root.agentBadge(modelData.id)
                  color: agentBadgeItem.stateColor
                  font.family: root.fontFamily
                  font.pixelSize: 10
                  font.bold: true
                }
                Rectangle {
                  id: agentStateBar
                  anchors.horizontalCenter: parent.horizontalCenter
                  width: 14; height: 3; radius: 1.5
                  color: agentBadgeItem.stateColor
                  opacity: modelData.status === "running" ? 0.4 : 1

                  SequentialAnimation on opacity {
                    running: modelData.status === "working" || modelData.status === "input"
                    loops: Animation.Infinite
                    NumberAnimation { from: 1; to: 0.25; duration: modelData.status === "input" ? 600 : 900; easing.type: Easing.InOutQuad }
                    NumberAnimation { from: 0.25; to: 1; duration: modelData.status === "input" ? 600 : 900; easing.type: Easing.InOutQuad }
                  }
                }
              }
              MouseArea { id: agentBadgeMouse; anchors.fill: parent; hoverEnabled: true; onPressed: root.toggleAgents(barWindow.modelData.name) }
              HoverTip { mouse: agentBadgeMouse; text: modelData.name + " · " + (modelData.status === "input" ? "needs input" : modelData.status) }
            }
          }


          BarItem {
            width: aiBarContent.implicitWidth + 14
            hovered: aiMouse.containsMouse
            active: root.agentsHere(barWindow.modelData)
            visible: root.agentData.launchers && root.agentData.launchers.length > 0
            Row {
              id: aiBarContent
              readonly property int codexCapacity: root.menuBarCapacity("codex")
              readonly property int claudeCapacity: root.menuBarCapacity("claude")
              anchors.centerIn: parent
              height: parent.height
              spacing: 5
              Text { anchors.verticalCenter: parent.verticalCenter; text: "󱚣"; color: root.accent; font.family: root.fontFamily; font.pixelSize: 15 }
              Text {
                visible: aiBarContent.codexCapacity >= 0
                anchors.verticalCenter: parent.verticalCenter
                text: "Codex " + aiBarContent.codexCapacity + "%"
                color: aiBarContent.codexCapacity <= 15 ? root.red : root.text
                font.family: root.fontFamily
                font.pixelSize: 11
                font.bold: true
              }
              Text {
                visible: aiBarContent.codexCapacity >= 0 && aiBarContent.claudeCapacity >= 0
                anchors.verticalCenter: parent.verticalCenter
                text: "·"
                color: root.overlay
                font.family: root.fontFamily
                font.pixelSize: 11
              }
              Text {
                visible: aiBarContent.claudeCapacity >= 0
                anchors.verticalCenter: parent.verticalCenter
                text: "Claude " + aiBarContent.claudeCapacity + "%"
                color: aiBarContent.claudeCapacity <= 15 ? root.red : root.text
                font.family: root.fontFamily
                font.pixelSize: 11
                font.bold: true
              }
              Text {
                visible: aiBarContent.codexCapacity < 0 && aiBarContent.claudeCapacity < 0
                anchors.verticalCenter: parent.verticalCenter
                text: "AI"
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: 11
                font.bold: true
              }
            }
            MouseArea {
              id: aiMouse
              anchors.fill: parent
              hoverEnabled: true
              acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
              onPressed: function(mouse) {
                if (mouse.button === Qt.RightButton) root.runAgent("pi", "")
                else if (mouse.button === Qt.MiddleButton) root.refreshAgents()
                else root.toggleAgents(barWindow.modelData.name)
              }
            }
            HoverTip { mouse: aiMouse; text: "AI cockpit · middle-click to refresh · right-click to launch Pi" }
          }

          BarItem {
            module: "airpods"
            visible: !!(root.systemData.headphones || {}).connected && root.barModulePinned("airpods")
            width: 30
            hovered: airpodsMouse.containsMouse
            active: root.panelHere("airpods", barWindow.modelData)
            HeadphonesIcon { anchors.centerIn: parent; kind: String((root.systemData.headphones || {}).kind || "airpods"); tint: root.accent }
            BarModuleArea { id: airpodsMouse; module: "airpods"; onActivated: root.toggleControl("airpods", barWindow.modelData.name) }
            HoverTip { mouse: airpodsMouse; text: (root.systemData.headphones || {}).name || "Headphones" }
          }

          BarItem {
            module: "bluetooth"
            visible: root.systemData.bluetoothAvailable && root.barModulePinned("bluetooth")
            width: 30
            hovered: bluetoothMouse.containsMouse
            active: root.panelHere("bluetooth", barWindow.modelData)
            Text {
              anchors.centerIn: parent
              text: root.systemData.bluetoothPowered ? "󰂯" : "󰂲"
              color: root.systemData.bluetoothConnected > 0 ? root.accent : root.text
              font.family: root.fontFamily
              font.pixelSize: 14
            }
            BarModuleArea { id: bluetoothMouse; module: "bluetooth"; onActivated: root.toggleControl("bluetooth", barWindow.modelData.name) }
            HoverTip { mouse: bluetoothMouse; text: "Bluetooth · " + (root.systemData.bluetoothPowered ? root.systemData.bluetoothConnected + " connected" : "off") }
          }

          BarItem {
            module: "vpn"
            visible: root.barModulePinned("vpn")
            width: 30
            hovered: vpnMouse.containsMouse
            active: root.panelHere("vpn", barWindow.modelData)
            Text {
              anchors.centerIn: parent
              text: "󰒃"
              color: root.privateNetworkActive() ? root.accent : root.text
              font.family: root.fontFamily
              font.pixelSize: 14
            }
            BarModuleArea { id: vpnMouse; module: "vpn"; onActivated: root.toggleControl("vpn", barWindow.modelData.name) }
            HoverTip { mouse: vpnMouse; text: "VPN · " + root.privateNetworkDetail() }
          }

          BarItem {
            module: "network"
            visible: root.barModulePinned("network")
            width: 30
            hovered: networkMouse.containsMouse
            active: root.panelHere("network", barWindow.modelData)
            Text {
              anchors.centerIn: parent
              text: root.systemData.connection === "Disconnected" ? "󰖪" : root.systemData.connectionType.indexOf("wireless") >= 0 ? "󰖩" : "󰈀"
              color: root.privateNetworkActive() ? root.accent : root.systemData.connectivity === "full" ? root.text : root.yellow
              font.family: root.fontFamily
              font.pixelSize: 14
            }
            BarModuleArea { id: networkMouse; module: "network"; onActivated: root.toggleControl("network", barWindow.modelData.name) }
            HoverTip {
              mouse: networkMouse
              text: "Network · " + (root.systemData.connection || "Disconnected")
                + (root.systemData.tailscale && root.systemData.tailscale.connected ? " · Tailscale" : "")
                + (root.systemData.protonVpn && root.systemData.protonVpn.connected ? " · Proton VPN" : "")
            }
          }


          BarItem {
            id: audioBarItem

            readonly property int shownVolume: root.volumeDrag >= 0 ? root.volumeDrag : Number(root.systemData.volume)
            module: "audio"
            visible: root.barModulePinned("audio")
            width: audioBarContent.implicitWidth + 14
            hovered: audioMouse.containsMouse
            active: root.panelHere("audio", barWindow.modelData)
            Row {
              id: audioBarContent
              anchors.centerIn: parent
              height: parent.height
              spacing: 4
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.systemData.muted ? "󰝟" : audioBarItem.shownVolume > 55 ? "󰕾" : "󰖀"
                color: root.systemData.muted ? root.red : root.text
                font.family: root.fontFamily
                font.pixelSize: 14
              }
              Text { anchors.verticalCenter: parent.verticalCenter; text: audioBarItem.shownVolume + "%"; color: root.text; font.family: root.fontFamily; font.pixelSize: 10 }
            }
            BarModuleArea {
              id: audioMouse
              module: "audio"
              acceptedButtons: Qt.LeftButton | Qt.MiddleButton
              onActivated: function(mouse) {
                if (mouse.button === Qt.MiddleButton) {
                  if (root.runControl("volume", "mute")) root.patchSystemData({ muted: !root.systemData.muted })
                } else {
                  root.toggleControl("audio", barWindow.modelData.name)
                }
              }
              onWheel: function(wheel) { root.adjustAudioFromWheel(wheel, false) }
            }
            HoverTip { mouse: audioMouse; text: "Volume · " + (root.systemData.muted ? "muted" : audioBarItem.shownVolume + "%") }
          }

          BarItem {
            width: 30
            hovered: controlCenterMouse.containsMouse
            active: root.panelHere("control-center", barWindow.modelData)
            Text { anchors.centerIn: parent; text: "󰘮"; color: root.text; font.family: root.fontFamily; font.pixelSize: 14 }
            MouseArea { id: controlCenterMouse; anchors.fill: parent; hoverEnabled: true; onPressed: root.toggleControl("control-center", barWindow.modelData.name) }
            HoverTip { mouse: controlCenterMouse; text: "Control Center" }
          }

          BarItem {
            width: notificationBarContent.implicitWidth + 14
            hovered: notificationMouse.containsMouse
            active: root.panelHere("notifications", barWindow.modelData)
            Row {
              id: notificationBarContent
              anchors.centerIn: parent
              height: parent.height
              spacing: 4
              Text { anchors.verticalCenter: parent.verticalCenter; text: root.systemData.dnd ? "󰂛" : "󰂚"; color: root.systemData.dnd ? root.yellow : root.text; font.family: root.fontFamily; font.pixelSize: 14 }
              Text { visible: Number(root.systemData.notifications.count || 0) > 0; anchors.verticalCenter: parent.verticalCenter; text: String(root.systemData.notifications.count); color: root.text; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
            }
            MouseArea { id: notificationMouse; anchors.fill: parent; hoverEnabled: true; onPressed: root.toggleControl("notifications", barWindow.modelData.name) }
            HoverTip { mouse: notificationMouse; text: "Notifications · " + (root.systemData.dnd ? "do not disturb" : root.systemData.notifications.count || 0) }
          }

          BarItem {
            id: batteryBarItem

            readonly property var entry: root.batteryPrimary()
            visible: root.batteryEntries().length > 0
            width: batteryBarContent.implicitWidth + 14
            hovered: batteryMouse.containsMouse
            active: root.panelHere("battery", barWindow.modelData)
            Row {
              id: batteryBarContent
              anchors.centerIn: parent
              height: parent.height
              spacing: 4
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.batteryIcon(batteryBarItem.entry)
                color: root.batteryColor(batteryBarItem.entry)
                font.family: root.fontFamily
                font.pixelSize: 14
              }
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: batteryBarItem.entry ? Number(batteryBarItem.entry.percent) + "%" : ""
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: 10
              }
            }
            MouseArea { id: batteryMouse; anchors.fill: parent; hoverEnabled: true; onPressed: root.toggleControl("battery", barWindow.modelData.name) }
            HoverTip { mouse: batteryMouse; text: "Battery · " + (batteryBarItem.entry ? batteryBarItem.entry.name + " " + Number(batteryBarItem.entry.percent) + "%" : "unavailable") }
          }

          // Where a module dragged out of the Control Center will land. Only the
          // "add" direction needs it: an entry being pulled off the bar previews
          // its own removal by disappearing.
          BarItem {
            visible: root.dragKind === "add" && root.dragOverBar
            width: visible ? dragGhostRow.implicitWidth + 14 : 0
            active: true
            Row {
              id: dragGhostRow
              anchors.centerIn: parent
              height: parent.height
              spacing: 5
              Text { anchors.verticalCenter: parent.verticalCenter; text: root.moduleGlyph(root.dragModule); color: root.accent; font.family: root.fontFamily; font.pixelSize: 14 }
              Text { anchors.verticalCenter: parent.verticalCenter; text: root.moduleLabel(root.dragModule); color: root.accent; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
            }
          }

          BarItem {
            width: 30
            hovered: sessionMouse.containsMouse
            active: root.panelHere("system", barWindow.modelData)
            Text { anchors.centerIn: parent; text: "󰐥"; color: root.windowsCountdown >= 0 ? root.yellow : root.text; font.family: root.fontFamily; font.pixelSize: 14 }
            MouseArea { id: sessionMouse; anchors.fill: parent; hoverEnabled: true; onPressed: root.toggleControl("system", barWindow.modelData.name) }
            HoverTip { mouse: sessionMouse; text: "Power and session" }
          }
        }

        SurfaceGrain {}

        // The bar is the drop target for a module drag, so it says so while one
        // is in flight and brightens once the pointer is actually over it.
        Rectangle {
          visible: root.dragModule !== ""
          anchors.fill: parent
          z: 1
          color: root.dragOverBar ? root.selectedColor : root.hoverColor

          Behavior on color { ColorAnimation { duration: 110 } }
        }

        Rectangle {
          anchors.bottom: parent.bottom
          width: parent.width
          height: root.dragModule !== "" ? 2 : 1
          z: 1
          color: root.dragModule !== "" ? root.accent : root.alpha(root.accent, 0.2)
        }
      }
    }
  }

  // Click-away catcher ---------------------------------------------------------
  // Keep this surface mapped with an empty input mask while idle. Mapping it
  // under a stationary pointer can make Hyprland defer the next click until
  // pointer motion. The bar strip stays clickable so one press can toggle or
  // switch panels.
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: clickAwayWindow
      required property var modelData
      // A held bar entry may be starting a drag straight down through this
      // surface, so it stands aside until the pointer is released.
      readonly property bool active: root.barPressModule === ""
        && (root.controlPanel !== "" || root.agentsOpen || root.trayMenuOpen)
      screen: modelData
      visible: true
      anchors { top: true; bottom: true; left: true; right: true }
      margins { top: root.barHeight }
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      mask: Region {
        width: clickAwayWindow.active ? clickAwayWindow.width : 0
        height: clickAwayWindow.active ? clickAwayWindow.height : 0
      }
      WlrLayershell.layer: WlrLayer.Top
      WlrLayershell.namespace: "seele-shell-clickaway"

      MouseArea {
        anchors.fill: parent
        enabled: clickAwayWindow.active
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        onPressed: root.closeOverlays()
      }
    }
  }

  // Calendar ------------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: calendarWindow
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "calendar" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true }
      margins.top: root.barHeight + root.panelGap
      implicitWidth: 390
      implicitHeight: Math.min(modelData.height - 60, 470)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-calendar"

      onVisibleChanged: if (visible) Qt.callLater(function() { calendarMonths.positionViewAtIndex(60, ListView.Beginning) })

      PanelSurface {
        id: calendarSurface

        Column {
          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          Row {
            width: parent.width
            height: 42
            PanelGlyph { text: "󰃭"; font.pixelSize: 24 }
            Column {
              width: parent.width - 116
              Text { text: Qt.formatDate(root.now, "dddd"); color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
              Text { text: Qt.formatDate(root.now, "d MMMM yyyy") + " · week " + Time.isoWeek(root.now); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
            }
            Rectangle {
              width: 84; height: 30; radius: root.radius
              anchors.verticalCenter: parent.verticalCenter
              color: todayMouse.pressed ? root.pressColor : todayMouse.containsMouse ? root.hoverColor : root.surface
              Text { anchors.centerIn: parent; text: "Today"; color: root.text; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
              MouseArea { id: todayMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: calendarMonths.positionViewAtIndex(60, ListView.Beginning) }
            }
          }

          SeeleListView {
            id: calendarMonths
            width: parent.width
            height: parent.height - 52
            model: 121
            spacing: 8
            clip: true
            delegate: Item {
              id: monthDelegate
              required property int modelData
              readonly property int monthOffset: modelData - 60
              readonly property date month: Time.monthDate(root.now, monthOffset)
              width: ListView.view.width
              height: 258

              Column {
                anchors.fill: parent
                spacing: 6
                Text {
                  width: parent.width
                  height: 28
                  text: Qt.formatDate(monthDelegate.month, "MMMM yyyy")
                  color: monthDelegate.monthOffset === 0 ? root.accent : root.text
                  font.family: root.fontFamily
                  font.pixelSize: 14
                  font.bold: true
                  verticalAlignment: Text.AlignVCenter
                }
                Grid {
                  width: parent.width
                  height: 22
                  columns: 8
                  Repeater {
                    model: ["Wk", "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
                    Text {
                      required property string modelData
                      width: parent.width / 8
                      height: 22
                      text: modelData
                      color: root.mutedText
                      font.family: root.fontFamily
                      font.pixelSize: 9
                      font.bold: true
                      horizontalAlignment: Text.AlignHCenter
                      verticalAlignment: Text.AlignVCenter
                    }
                  }
                }
                Grid {
                  id: monthGrid
                  width: parent.width
                  height: 198
                  columns: 8
                  Repeater {
                    model: Time.calendarCells(root.now, monthDelegate.monthOffset)
                    Item {
                      id: calendarCell
                      required property var modelData
                      width: monthGrid.width / 8
                      height: 33
                      Rectangle {
                        visible: !calendarCell.modelData.week && calendarCell.modelData.today
                        anchors.centerIn: parent
                        width: 27; height: 27; radius: 13.5
                        color: root.accent
                      }
                      Text {
                        anchors.centerIn: parent
                        text: calendarCell.modelData.week ? "W" + calendarCell.modelData.label : calendarCell.modelData.day
                        color: calendarCell.modelData.week ? root.mutedText : calendarCell.modelData.today ? root.base : calendarCell.modelData.inMonth ? root.text : root.mutedText
                        opacity: calendarCell.modelData.week || calendarCell.modelData.inMonth || calendarCell.modelData.today ? 1 : 0.72
                        font.family: root.fontFamily
                        font.pixelSize: calendarCell.modelData.week ? 9 : 10
                        font.bold: !!calendarCell.modelData.today || !!calendarCell.modelData.week
                      }
                    }
                  }
                }
              }
            }
            ScrollBar.vertical: SlimScrollBar { popupHovered: calendarSurface.hovered }
          }
        }
      }
    }
  }

  // World clock ---------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: clockWindow
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "clock" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true }
      margins.top: root.barHeight + root.panelGap
      implicitWidth: 430
      implicitHeight: Math.min(modelData.height - 60, 560)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-clock"
      onVisibleChanged: if (visible) Qt.callLater(function() {
        timezoneSearch.forceActiveFocus()
        timezoneSearch.selectAll()
      })

      PanelSurface {
        id: clockSurface

        Column {
          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          Row {
            width: parent.width
            height: 42
            PanelGlyph { text: "󰥔"; font.pixelSize: 24 }
            Column {
              width: parent.width - 140
              Text { text: "World clock"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
              Text { text: "Pin zones to the top"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 9 }
            }
            Text { anchors.verticalCenter: parent.verticalCenter; text: Qt.formatDateTime(root.now, "HH:mm:ss"); color: root.accent; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true; horizontalAlignment: Text.AlignRight }
          }

          TextField {
            id: timezoneSearch
            width: parent.width
            height: 38
            placeholderText: "Search PST, UTC, Europe/London, city…"
            color: root.text
            placeholderTextColor: root.overlay
            selectionColor: root.accent
            selectedTextColor: root.base
            font.family: root.fontFamily
            font.pixelSize: 10
            leftPadding: 12
            rightPadding: 12
            background: Rectangle { radius: root.radius; color: root.surface; border.color: timezoneSearch.activeFocus ? root.accent : "transparent"; border.width: 1 }
          }

          SeeleListView {
            id: timezoneList
            width: parent.width
            height: parent.height - 100
            model: root.filteredTimezones(timezoneSearch.text)
            spacing: 4
            clip: true
            delegate: Rectangle {
              id: timezoneRow
              required property var modelData
              readonly property bool pinned: root.timezonePinned(modelData.id)
              width: ListView.view.width
              height: 54
              radius: root.radius
              color: timezoneRowMouse.containsMouse ? root.surface : root.alpha(root.surface, 0.45)

              Text { visible: modelData.kind === "city"; anchors.left: parent.left; anchors.leftMargin: 10; anchors.verticalCenter: parent.verticalCenter; width: 25; text: modelData.flag; font.pixelSize: 16; horizontalAlignment: Text.AlignHCenter }
              Column {
                anchors.left: parent.left
                anchors.leftMargin: modelData.kind === "city" ? 43 : 12
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (modelData.kind === "city" ? 176 : 145)
                Text { width: parent.width; text: modelData.label; color: root.text; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true; elide: Text.ElideRight }
                Text { width: parent.width; text: modelData.id + " · " + modelData.abbreviation + " " + modelData.offset; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 8; elide: Text.ElideRight }
              }
              Column {
                anchors.right: pinTimezoneButton.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 76
                Text { width: parent.width; text: Time.offsetTime(root.now, modelData.offset, false) || modelData.time; color: root.accent; font.family: root.fontFamily; font.pixelSize: 13; font.bold: true; horizontalAlignment: Text.AlignRight }
                Text { width: parent.width; text: modelData.day; color: root.mutedText; font.family: root.fontFamily; font.pixelSize: 8; horizontalAlignment: Text.AlignRight }
              }
              MouseArea {
                id: timezoneRowMouse
                anchors.left: parent.left; anchors.right: pinTimezoneButton.left; anchors.top: parent.top; anchors.bottom: parent.bottom
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
              }
              Rectangle {
                id: pinTimezoneButton
                anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                width: 38; height: 30; radius: root.radius
                color: pinTimezoneMouse.pressed ? root.pressColor : timezoneRow.pinned ? root.selectedColor : pinTimezoneMouse.containsMouse ? root.hoverColor : root.mantle
                Text { anchors.centerIn: parent; text: timezoneRow.pinned ? "Unpin" : "Pin"; color: timezoneRow.pinned ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 8; font.bold: true }
                MouseArea { id: pinTimezoneMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.pinTimezone(timezoneRow.modelData.id) }
              }
            }
            ScrollBar.vertical: SlimScrollBar { popupHovered: clockSurface.hovered }
          }
        }
      }
    }
  }

  // Tray menu -----------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: trayMenuWindow
      required property var modelData
      screen: modelData
      visible: root.trayMenuOpen && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 310
      implicitHeight: Math.min(420, 58 + Math.max(1, trayMenuOpener.children.values.length) * 36)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-tray-menu"
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

      PanelSurface {
        Column {
          anchors.fill: parent
          anchors.margins: root.panelSpacing
          spacing: 6

          Row {
            width: parent.width
            height: 30
            spacing: 8
            IconImage {
              visible: source !== ""
              anchors.verticalCenter: parent.verticalCenter
              implicitWidth: 18; implicitHeight: 18
              source: root.activeTrayItem ? (root.activeTrayItem.icon || "") : ""
            }
            Text {
              width: parent.width - 132
              anchors.verticalCenter: parent.verticalCenter
              text: root.activeTrayItem ? (root.activeTrayItem.title || root.activeTrayItem.id || "Tray menu") : "Tray menu"
              elide: Text.ElideRight
              color: root.text
              font.family: root.fontFamily
              font.pixelSize: 12
              font.bold: true
            }
            Rectangle {
              width: 72; height: 26; radius: root.radius
              anchors.verticalCenter: parent.verticalCenter
              color: trayHideMouse.pressed ? root.pressColor : trayHideMouse.containsMouse ? root.hoverColor : root.surface
              Text {
                anchors.centerIn: parent
                text: root.trayItemHidden(root.activeTrayItem) ? "Show icon" : "Hide icon"
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: 9
                font.bold: true
              }
              MouseArea {
                id: trayHideMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                  root.toggleTrayItemHidden(root.activeTrayItem)
                  root.closeTrayMenu()
                }
              }
            }
            Rectangle {
              width: 30; height: 30; radius: root.radius
              color: trayMenuCloseMouse.pressed ? root.pressColor : trayMenuCloseMouse.containsMouse ? root.hoverColor : "transparent"
              Text { anchors.centerIn: parent; text: "󰅖"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 11 }
              MouseArea { id: trayMenuCloseMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.closeTrayMenu() }
            }
          }

          SeeleListView {
            id: trayMenuList
            width: parent.width
            height: parent.height - 36
            spacing: 2
            clip: true
            model: trayMenuOpener.children
            delegate: Item {
              required property var modelData
              width: trayMenuList.width
              height: modelData.isSeparator ? 9 : 34
              opacity: modelData.enabled ? 1 : 0.45

              Rectangle {
                visible: parent.modelData.isSeparator
                anchors.left: parent.left; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: 8; anchors.rightMargin: 8
                height: 1
                color: root.alpha(root.overlay, 0.5)
              }

              Rectangle {
                visible: !parent.modelData.isSeparator
                anchors.fill: parent
                radius: root.radius
                color: trayMenuEntryMouse.pressed ? root.pressColor : trayMenuEntryMouse.containsMouse && parent.modelData.enabled ? root.hoverColor : "transparent"
              }

              IconImage {
                id: trayMenuEntryIcon
                visible: !parent.modelData.isSeparator && source !== ""
                anchors.left: parent.left; anchors.leftMargin: 8; anchors.verticalCenter: parent.verticalCenter
                implicitWidth: 16; implicitHeight: 16
                source: parent.modelData.icon || ""
              }

              Text {
                visible: !parent.modelData.isSeparator && parent.modelData.buttonType !== QsMenuButtonType.None
                anchors.left: parent.left; anchors.leftMargin: 8; anchors.verticalCenter: parent.verticalCenter
                width: 16
                text: parent.modelData.checkState === Qt.Checked ? "✓" : ""
                color: root.accent
                horizontalAlignment: Text.AlignHCenter
                font.family: root.fontFamily
                font.pixelSize: 11
              }

              Text {
                visible: !parent.modelData.isSeparator
                anchors.left: parent.left; anchors.leftMargin: trayMenuEntryIcon.visible ? 32 : 28
                anchors.right: trayMenuSubmenu.left; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                text: parent.modelData.text || ""
                elide: Text.ElideRight
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: 11
              }

              Text {
                id: trayMenuSubmenu
                visible: !parent.modelData.isSeparator && parent.modelData.hasChildren
                anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                text: "›"
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: 15
              }

              MouseArea {
                id: trayMenuEntryMouse
                anchors.fill: parent
                hoverEnabled: true
                enabled: !parent.modelData.isSeparator && parent.modelData.enabled
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: {
                  if (parent.modelData.hasChildren) {
                    parent.modelData.display(trayMenuWindow, 10, parent.y + parent.height)
                  } else {
                    parent.modelData.triggered()
                    root.closeTrayMenu()
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  // AI cockpit ----------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: agentsWindow
      required property var modelData
      readonly property bool active: root.agentsOpen && root.pinnedScreen(root.overlayScreen, modelData)
      screen: modelData
      visible: true
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 500
      implicitHeight: Math.min(modelData.height - 60, 760)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      mask: Region {
        width: agentsWindow.active ? agentsWindow.width : 0
        height: agentsWindow.active ? agentsWindow.height : 0
      }
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: active ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-agents"

      PanelSurface {
        id: agentsSurface

        visible: agentsWindow.active

        SeeleFlickable {
          id: agentsScroll

          anchors.fill: parent
          anchors.margins: root.panelMargin
          anchors.rightMargin: root.scrollInset
          clip: true
          contentWidth: width
          contentHeight: agentsContent.implicitHeight

          ScrollBar.vertical: SlimScrollBar { popupHovered: agentsSurface.hovered }

          Column {
            id: agentsContent

            width: agentsScroll.width - root.panelMargin + root.scrollInset
            spacing: 14

            Item {
              width: parent.width
              height: 42

              Text {
                id: cockpitGlyph
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: "󱚣"
                color: root.accent
                font.family: root.fontFamily
                font.pixelSize: 28
              }
              Column {
                anchors.left: cockpitGlyph.right
                anchors.leftMargin: 10
                anchors.right: cockpitRefresh.left
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                Text { text: "AI cockpit"; color: root.text; font.family: root.fontFamily; font.pixelSize: 20; font.bold: true }
                Text {
                  width: parent.width
                  elide: Text.ElideRight
                  text: root.agentRefreshing ? "Refreshing usage…" : root.agentError !== "" ? "Usage unavailable" : root.subscriptionSummary()
                  color: root.agentError !== "" ? root.red : root.subtext
                  font.family: root.fontFamily; font.pixelSize: 11
                }
              }
              Rectangle {
                id: cockpitRefresh
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 34; height: 34; radius: root.radius
                color: refreshMouse.pressed ? root.pressColor : root.agentRefreshing ? root.activeTint : refreshMouse.containsMouse ? root.hoverColor : "transparent"
                RefreshGlyph { anchors.centerIn: parent; width: 20; height: 20; spinning: root.agentRefreshing }
                MouseArea { id: refreshMouse; anchors.fill: parent; enabled: !root.agentRefreshing; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.refreshAgents() }
                HoverTip { mouse: refreshMouse; text: "Refresh usage" }
              }
            }

            Text { text: "CHANGE THIS SYSTEM"; color: root.mutedText; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }

            Rectangle {
              width: parent.width; height: 48; radius: root.radius
              color: osSessionMouse.pressed ? root.pressColor : osSessionMouse.containsMouse ? root.hoverColor : root.surface
              border.color: root.alpha(root.accent, 0.45)
              border.width: 1
              Row {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                spacing: 12
                Text { anchors.verticalCenter: parent.verticalCenter; text: "󱄅"; color: root.accent; font.family: root.fontFamily; font.pixelSize: 22 }
                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 46; text: "Describe a change to Seele"; color: root.text; font.family: root.fontFamily; font.pixelSize: 13; font.bold: true }
              }
              MouseArea {
                id: osSessionMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.startOsSession()
              }
            }

            Text { text: "LAUNCH"; color: root.mutedText; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }

            Grid {
              width: parent.width
              columns: 2
              spacing: 8
              Repeater {
                model: root.agentData.launchers || []
                Rectangle {
                  required property var modelData
                  readonly property string status: root.agentStatus(modelData.id)
                  width: (parent.width - 8) / 2; height: 68; radius: root.radius
                  color: launchMouse.pressed ? root.pressColor : launchMouse.containsMouse ? root.hoverColor : root.surface
                  border.color: status === "input" ? root.yellow : status === "working" ? root.accent : status === "finished" ? root.green : root.alpha(root.overlay, 0.35)
                  border.width: status === "idle" ? 1 : 2
                  Column {
                    anchors.centerIn: parent
                    spacing: 3
                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.name; color: root.text; font.family: root.fontFamily; font.pixelSize: 13; font.bold: true }
                    Text {
                      anchors.horizontalCenter: parent.horizontalCenter
                      text: parent.parent.status === "working" ? "● working" : parent.parent.status === "input" ? "◆ input needed" : parent.parent.status === "finished" ? "✓ finished" : modelData.id === "pi" ? "Primary" : "Ready"
                      color: parent.parent.status === "input" ? root.yellow : parent.parent.status === "working" ? root.accent : parent.parent.status === "finished" ? root.green : root.subtext
                      font.family: root.fontFamily
                      font.pixelSize: 9
                    }
                  }
                  MouseArea { id: launchMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runAgent(parent.modelData.id, "") }
                }
              }
            }

            Text { text: "SUBSCRIPTION CAPACITY"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }

            Repeater {
              model: root.agentData.subscriptions || []
              Column {
                required property var modelData
                width: parent.width
                spacing: 6
                Row {
                  width: parent.width
                  Text {
                    width: parent.width * 0.62
                    text: modelData.name + (modelData.plan ? " · " + modelData.plan : "")
                    elide: Text.ElideRight
                    color: root.text
                    font.family: root.fontFamily
                    font.pixelSize: 12
                    font.bold: true
                  }
                  Text {
                    width: parent.width * 0.38
                    text: modelData.credits !== null && modelData.credits !== undefined && Number(modelData.credits) > 0 ? Number(modelData.credits) + " credits" : modelData.source
                    color: root.overlay
                    font.family: root.fontFamily
                    font.pixelSize: 9
                    horizontalAlignment: Text.AlignRight
                  }
                }
                Repeater {
                  model: modelData.limits || []
                  Column {
                    required property var modelData
                    width: parent.width
                    spacing: 5
                    Row {
                      width: parent.width
                      Text { width: parent.width * 0.45; text: modelData.name; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 11 }
                      Text { width: parent.width * 0.3; text: root.freePercent(modelData) + "% free"; color: root.freePercent(modelData) <= 15 ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter }
                      Text { width: parent.width * 0.25; text: "resets " + root.resetText(modelData.resetsAt); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10; horizontalAlignment: Text.AlignRight }
                    }
                    Rectangle {
                      width: parent.width; height: 8; radius: 4; color: root.surface
                      Rectangle { width: parent.width * root.freePercent(modelData) / 100; height: parent.height; radius: 4; color: root.freePercent(modelData) <= 15 ? root.red : root.accent }
                    }
                  }
                }
                Text {
                  visible: (modelData.limits || []).length === 0
                  text: "No usage window reported"
                  color: root.overlay
                  font.family: root.fontFamily
                  font.pixelSize: 9
                }
              }
            }

            Row {
              width: parent.width
              height: 28
              spacing: 4

              Repeater {
                model: [
                  { id: "day", label: "Day" },
                  { id: "week", label: "Week" },
                  { id: "month", label: "Month" },
                  { id: "all", label: "All time" }
                ]

                Rectangle {
                  required property var modelData
                  width: (parent.width - 12) / 4
                  height: 28
                  radius: root.radius
                  color: metricPeriodMouse.pressed
                    ? root.pressColor
                    : root.agentMetricPeriod === modelData.id
                      ? root.selectedColor
                      : metricPeriodMouse.containsMouse
                        ? root.hoverColor
                        : root.surface
                  border.width: root.agentMetricPeriod === modelData.id ? 1 : 0
                  border.color: root.alpha(root.accent, 0.55)

                  Text {
                    anchors.centerIn: parent
                    text: modelData.label
                    color: root.agentMetricPeriod === modelData.id ? root.accent : root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: 10
                    font.bold: root.agentMetricPeriod === modelData.id
                  }

                  MouseArea {
                    id: metricPeriodMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.agentMetricPeriod = parent.modelData.id
                  }
                }
              }
            }

            Row {
              width: parent.width
              spacing: 8
              Rectangle {
                width: (parent.width - 8) / 2; height: 68; radius: root.radius; color: root.surface
                Column { anchors.centerIn: parent; spacing: 4
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.formatTokens(root.agentMetricData.totalTokens || 0); color: root.accent; font.family: root.fontFamily; font.pixelSize: 20; font.bold: true }
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: "tokens · " + root.agentMetricPeriodLabel(); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
                }
              }
              Rectangle {
                width: (parent.width - 8) / 2; height: 68; radius: root.radius; color: root.surface
                Column { anchors.centerIn: parent; spacing: 4
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: "$" + Number(root.agentMetricData.totalCost || 0).toFixed(2); color: root.green; font.family: root.fontFamily; font.pixelSize: 20; font.bold: true }
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: "estimated · " + root.agentMetricPeriodLabel(); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
                }
              }
            }

            Item {
              width: parent.width
              height: 18
              Row {
                anchors.fill: parent
                Text { width: parent.width - 20; anchors.verticalCenter: parent.verticalCenter; text: "LAST 7 DAYS"; color: usageHeaderMouse.pressed ? root.accent : usageHeaderMouse.containsMouse ? root.text : root.overlay; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
                Text { width: 20; anchors.verticalCenter: parent.verticalCenter; text: root.agentUsageOpen ? "󰅃" : "󰅀"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 11; horizontalAlignment: Text.AlignRight }
              }
              MouseArea { id: usageHeaderMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.agentUsageOpen = !root.agentUsageOpen }
            }

            Column {
              width: parent.width
              spacing: 4
              visible: root.agentUsageOpen
              Repeater {
                model: root.agentData.local.daily || []
                Row {
                  required property var modelData
                  width: parent.width; height: 24; spacing: 8
                  readonly property real peak: {
                    var days = root.agentData.local.daily || []
                    var value = 1
                    for (var i = 0; i < days.length; i++) value = Math.max(value, Number(days[i].totalTokens || 0))
                    return value
                  }
                  Text { width: 76; anchors.verticalCenter: parent.verticalCenter; text: modelData.date || ""; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
                  Rectangle {
                    width: parent.width - 150; height: 7; anchors.verticalCenter: parent.verticalCenter; radius: 4; color: root.surface
                    Rectangle { width: parent.width * Number(modelData.totalTokens || 0) / parent.parent.peak; height: parent.height; radius: 4; color: root.accent }
                  }
                  Text { width: 58; anchors.verticalCenter: parent.verticalCenter; text: root.formatTokens(modelData.totalTokens || 0); color: root.text; font.family: root.fontFamily; font.pixelSize: 10; horizontalAlignment: Text.AlignRight }
                }
              }
            }

            Item {
              width: parent.width
              height: 18
              Row {
                anchors.fill: parent
                Text { width: parent.width - 20; anchors.verticalCenter: parent.verticalCenter; text: "TOP MODELS"; color: modelsHeaderMouse.pressed ? root.accent : modelsHeaderMouse.containsMouse ? root.text : root.overlay; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
                Text { width: 20; anchors.verticalCenter: parent.verticalCenter; text: root.agentModelsOpen ? "󰅃" : "󰅀"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 11; horizontalAlignment: Text.AlignRight }
              }
              MouseArea { id: modelsHeaderMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.agentModelsOpen = !root.agentModelsOpen }
            }
            Column {
              width: parent.width
              spacing: 2
              visible: root.agentModelsOpen
              Repeater {
                model: root.agentMetricData.models || []
                Row {
                  required property var modelData
                  width: parent.width; height: 25
                  Text { width: parent.width * 0.62; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 11 }
                  Text { width: parent.width * 0.2; text: root.formatTokens(modelData.tokens); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10; horizontalAlignment: Text.AlignRight }
                  Text { width: parent.width * 0.18; text: "$" + Number(modelData.cost || 0).toFixed(2); color: root.green; font.family: root.fontFamily; font.pixelSize: 10; horizontalAlignment: Text.AlignRight }
                }
              }
            }

          }
        }
      }
    }
  }

  // Control Center ---------------------------------------------------------------
  // Every module here also keeps its own menu bar entry and its own panel. This
  // panel is the one place that carries all of them at once, so its modules stay
  // laid out even when the thing behind one of them is absent, where the bar
  // hides an entry it has nothing to say about.
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: controlCenterWindow

      // A Wayland client only keeps receiving pointer motion while the pointer
      // stays inside the surface that was pressed, so a drop target living on
      // another layer surface loses the gesture halfway across. This surface
      // therefore reaches up over the bar itself, and the strip above the panel
      // takes input only while a drag is in flight so bar entries stay clickable
      // the rest of the time. The geometry never changes mid-gesture, only the
      // input region does.
      readonly property int barReach: root.barHeight + root.panelGap
      readonly property bool dragging: root.dragKind === "add"

      required property var modelData
      screen: modelData
      visible: root.controlPanel === "control-center" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: 0; right: root.panelGap }
      implicitWidth: 400
      implicitHeight: controlCenterContent.implicitHeight + root.panelMargin * 2 + controlCenterWindow.barReach
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-control-center"
      mask: Region {
        y: controlCenterWindow.dragging ? 0 : controlCenterWindow.barReach
        width: controlCenterWindow.width
        height: controlCenterWindow.height - (controlCenterWindow.dragging ? 0 : controlCenterWindow.barReach)
      }

      Item {
        anchors.fill: parent
        anchors.topMargin: controlCenterWindow.barReach

        PanelSurface {
          Column {
            id: controlCenterContent

            anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing

            // The title sits a little lower in a slightly shorter row: it was
            // crowding the panel's top edge while everything under it sat low.
            Row {
              width: parent.width
              height: root.panelHeaderHeight - 3
              PanelGlyph { text: "󰘮"; anchors.verticalCenterOffset: 4 }
              Text { anchors.verticalCenter: parent.verticalCenter; anchors.verticalCenterOffset: 4; text: "Control Center"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
            }

            ControlCenterGrid {
              width: parent.width
              screenName: controlCenterWindow.modelData.name
            }
          }
        }
      }
    }
  }

  // Media controls -------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: mediaWindow

      required property var modelData
      readonly property var player: root.mediaPanelPlayer || root.nowPlayingPlayer()
      screen: modelData
      visible: root.controlPanel === "media" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 400
      implicitHeight: mediaContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-media"

      PanelSurface {
        Column {
          id: mediaContent

          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: "󰎆" }
            Text { anchors.verticalCenter: parent.verticalCenter; text: "Now Playing"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
          }

          Row {
            width: parent.width
            height: 150
            spacing: 14

            Item {
              width: 150
              height: 150

              Image {
                id: mediaPanelArt

                anchors.fill: parent
                visible: false
                source: mediaWindow.player ? String(mediaWindow.player.trackArtUrl || "") : ""
                fillMode: Image.PreserveAspectCrop
                sourceSize.width: width * 2
                sourceSize.height: height * 2
                smooth: true
                mipmap: true
                asynchronous: true
                cache: true
              }

              RoundedSource {
                anchors.fill: parent
                source: mediaPanelArt
                radius: root.radius
                visible: mediaPanelArt.status === Image.Ready
              }

              Rectangle {
                anchors.fill: parent
                visible: mediaPanelArt.status !== Image.Ready
                radius: root.radius
                color: root.mantle
                Text {
                  anchors.centerIn: parent
                  text: "󰎆"
                  color: mediaWindow.player ? root.accent : root.overlay
                  font.family: root.fontFamily
                  font.pixelSize: 42
                }
              }
            }

            Item {
              width: parent.width - 164
              height: parent.height

              Column {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                spacing: 4

                Text {
                  width: parent.width
                  text: mediaWindow.player ? (root.mediaTitle(mediaWindow.player) || "Unknown track") : "Nothing playing"
                  elide: Text.ElideRight
                  color: mediaWindow.player ? root.text : root.subtext
                  font.family: root.fontFamily
                  font.pixelSize: 15
                  font.bold: true
                }
                Text {
                  width: parent.width
                  text: mediaWindow.player ? root.mediaSubtitle(mediaWindow.player) : "Start a track to see it here"
                  elide: Text.ElideRight
                  color: root.subtext
                  font.family: root.fontFamily
                  font.pixelSize: 11
                }
              }

              Row {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                spacing: 8

                MediaButton {
                  icon: "󰒮"
                  enabled: !!mediaWindow.player && mediaWindow.player.canGoPrevious
                  onActivated: mediaWindow.player.previous()
                }
                MediaButton {
                  icon: mediaWindow.player && mediaWindow.player.isPlaying ? "󰏤" : "󰐊"
                  primary: true
                  enabled: !!mediaWindow.player
                  onActivated: mediaWindow.player.togglePlaying()
                }
                MediaButton {
                  icon: "󰒭"
                  enabled: !!mediaWindow.player && mediaWindow.player.canGoNext
                  onActivated: mediaWindow.player.next()
                }
              }
            }
          }

          MediaTimeline {
            width: parent.width
            player: mediaWindow.player
          }
        }
      }
    }
  }

  // Audio controls -------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: audioControlsWindow

      readonly property int outputHeight: Math.max(1, Math.min(4, root.audioDevices("output").length)) * 32
      readonly property int inputHeight: Math.max(1, Math.min(4, root.audioDevices("input").length)) * 32
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "audio" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 350
      implicitHeight: audioContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-audio"

      PanelSurface {
        Column {
          id: audioContent

          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: "󰕾" }
            Text { anchors.verticalCenter: parent.verticalCenter; text: "Audio"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
          }
          AudioLevelRow { width: parent.width }
          AudioLevelRow { width: parent.width; microphone: true }
          Text { text: "OUTPUT DEVICE"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
          SeeleListView {
            width: parent.width
            height: audioControlsWindow.outputHeight
            spacing: 4
            clip: true
            model: root.audioDevices("output")
            delegate: Rectangle {
              required property var modelData
              readonly property bool busy: root.controlBusy("audio-device", String(modelData.id))
              readonly property bool complete: root.controlCompleted("audio-device", String(modelData.id))
              width: ListView.view.width; height: 28; radius: root.radius
              color: outputDeviceMouse.pressed ? root.pressColor : busy ? root.selectedColor : modelData.default || complete ? root.hoverColor : outputDeviceMouse.containsMouse ? root.surface : root.alpha(root.surface, 0.5)
              Row {
                visible: !parent.busy
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                Text { anchors.verticalCenter: parent.verticalCenter; text: parent.parent.complete || modelData.default ? "󰄬" : "󰓃"; color: parent.parent.complete || modelData.default ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 12 }
                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 10 }
              }
              RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
              MouseArea { id: outputDeviceMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.setAudioDevice(parent.modelData.id, parent.modelData.profile) }
            }
          }
          Text { text: "INPUT DEVICE"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
          SeeleListView {
            width: parent.width
            height: audioControlsWindow.inputHeight
            spacing: 4
            clip: true
            model: root.audioDevices("input")
            delegate: Rectangle {
              required property var modelData
              readonly property bool busy: root.controlBusy("audio-device", String(modelData.id))
              readonly property bool complete: root.controlCompleted("audio-device", String(modelData.id))
              width: ListView.view.width; height: 28; radius: root.radius
              color: inputDeviceMouse.pressed ? root.pressColor : busy ? root.selectedColor : modelData.default || complete ? root.hoverColor : inputDeviceMouse.containsMouse ? root.surface : root.alpha(root.surface, 0.5)
              Row {
                visible: !parent.busy
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                Text { anchors.verticalCenter: parent.verticalCenter; text: parent.parent.complete || modelData.default ? "󰄬" : "󰍬"; color: parent.parent.complete || modelData.default ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 12 }
                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 10 }
              }
              RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
              MouseArea { id: inputDeviceMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.setAudioDevice(parent.modelData.id, parent.modelData.profile) }
            }
          }
        }
      }
    }
  }

  // Network controls -----------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "network" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 390
      implicitHeight: networkContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-network"

      PanelSurface {
        Column {
          id: networkContent

          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: 6
          Row {
            width: parent.width; height: 34; spacing: 8
            PanelGlyph { text: "󰤨" }
            Text { width: root.systemData.wifiAvailable ? parent.width - 128 : parent.width - 40; anchors.verticalCenter: parent.verticalCenter; text: "Network"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
            Text { visible: root.systemData.wifiAvailable; anchors.verticalCenter: parent.verticalCenter; text: "Wi-Fi"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
            ControlSwitch {
              visible: root.systemData.wifiAvailable
              anchors.verticalCenter: parent.verticalCenter
              checked: root.systemData.wifiEnabled
              busy: root.controlBusy("wifi", "toggle")
              onToggled: if (root.runControl("wifi", "toggle")) root.patchSystemData({ wifiEnabled: !root.systemData.wifiEnabled })
            }
          }
          Row {
            width: parent.width; height: 22
            Text { width: parent.width * 0.64; text: root.systemData.connection || "Disconnected"; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 12; font.bold: true }
            Text { width: parent.width * 0.36; text: root.systemData.connectivity; color: root.systemData.connectivity === "full" ? root.green : root.yellow; font.family: root.fontFamily; font.pixelSize: 9; horizontalAlignment: Text.AlignRight }
          }
          Rectangle {
            width: parent.width; height: 60; radius: root.radius; color: root.surface
            Column {
              anchors.fill: parent; anchors.margins: 9; spacing: 4
              Text { text: "IP address    " + (root.systemData.ipAddress || "Unavailable"); color: root.text; font.family: root.fontFamily; font.pixelSize: 9 }
              Text { text: "Gateway       " + (root.systemData.gateway || "Unavailable") + " · " + (root.systemData.connectionType || "None"); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 9 }
            }
          }

          Rectangle {
            id: speedtestCard

            width: parent.width; height: 204; radius: root.radius; color: root.surface
            Column {
              anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 10; rightMargin: 10; topMargin: 8 }
              spacing: 8
              Item {
                width: parent.width; height: 30
                Column {
                  anchors.centerIn: parent
                  spacing: 0
                  Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "PING"
                    color: root.overlay
                    font.family: root.fontFamily
                    font.pixelSize: 8
                    font.bold: true
                  }
                  Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.speedtestPingText()
                    color: root.speedtestError !== "" ? root.red : root.text
                    font.family: root.fontFamily
                    font.pixelSize: 10
                    font.bold: true
                  }
                }
                Rectangle {
                  anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                  width: 64; height: 24; radius: root.radius
                  color: speedtestMouse.pressed ? root.pressColor : speedtestProcess.running ? root.selectedColor : speedtestMouse.containsMouse ? root.hoverColor : root.mantle
                  Text { visible: !speedtestProcess.running; anchors.centerIn: parent; text: root.speedtestReceived ? "Again" : "Run"; color: root.text; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
                  RefreshGlyph { visible: speedtestProcess.running; anchors.centerIn: parent; width: 14; height: 14; spinning: visible; font.pixelSize: 10 }
                  MouseArea { id: speedtestMouse; anchors.fill: parent; enabled: !speedtestProcess.running; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.startSpeedtest() }
                }
              }
              Row {
                width: parent.width; height: 140; spacing: 8
                SpeedGauge {
                  width: (parent.width - 8) / 2; height: parent.height; radius: root.radius
                  label: "DOWNLOAD"
                  icon: "󰇚"
                  value: Number(root.speedtestData.download)
                  maximum: root.speedtestScale()
                  tint: root.accent
                  active: root.speedtestPhase === "download"
                }
                SpeedGauge {
                  width: (parent.width - 8) / 2; height: parent.height; radius: root.radius
                  label: "UPLOAD"
                  icon: "󰕒"
                  value: Number(root.speedtestData.upload)
                  maximum: root.speedtestScale()
                  tint: root.green
                  active: root.speedtestPhase === "upload"
                }
              }
            }
          }

          Row {
            width: parent.width; spacing: 8
            Repeater {
              model: [
                {label:"Copy IP", action:"copy-ip", value:""},
                {label:"Settings", action:"network-settings", value:""},
                {label:"Allestörungen", action:"outages", value:""}
              ]
              Rectangle {
                required property var modelData
                readonly property bool busy: root.controlBusy(modelData.action, modelData.value)
                readonly property bool complete: root.controlCompleted(modelData.action, modelData.value)
                readonly property bool failed: root.controlFailed(modelData.action, modelData.value)
                width: (parent.width - 16) / 3; height: 38; radius: root.radius
                color: networkActionMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : networkActionMouse.containsMouse ? root.hoverColor : root.surface
                Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Failed" : parent.complete ? (modelData.action === "copy-ip" ? "✓ Copied" : "✓ Opened") : modelData.label; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 18; height: 18; spinning: visible; font.pixelSize: 13 }
                MouseArea {
                  id: networkActionMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.runControl(parent.modelData.action, parent.modelData.value)
                }
              }
            }
          }
        }
      }
    }
  }

  // VPN -------------------------------------------------------------------------
  // The private networks own their own panel rather than sitting at the bottom
  // of the network panel, so the module can carry its own menu bar entry.
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "vpn" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 390
      implicitHeight: vpnContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-vpn"

      PanelSurface {
        Column {
          id: vpnContent

          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing

          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: "󰒃" }
            Text { anchors.verticalCenter: parent.verticalCenter; text: "VPN"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
          }

          Rectangle {
            id: tailscaleCard
            readonly property var state: root.systemData.tailscale || ({})
            readonly property var trayItem: root.trayItemNamed("tailscale")
            readonly property string action: state.connected ? "down" : state.needsLogin ? "login" : "up"
            readonly property bool busy: root.controlBusy("tailscale", action)
            readonly property bool failed: root.controlFailed("tailscale", action)
            width: parent.width; height: 66; radius: root.radius
            color: failed ? root.dangerTint : tailscaleMenuMouse.pressed ? root.pressColor : tailscaleMenuMouse.containsMouse ? root.hoverColor : state.connected ? root.activeTint : root.surface
            MouseArea {
              id: tailscaleMenuMouse
              anchors.fill: parent
              anchors.rightMargin: 58
              enabled: !!tailscaleCard.trayItem
              hoverEnabled: true
              cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
              onClicked: root.openTrayItemMenu(tailscaleCard.trayItem, root.overlayScreen)
            }
            HoverTip { mouse: tailscaleMenuMouse; text: "Open Tailscale menu" }
            Row {
              anchors.fill: parent; anchors.margins: 10; spacing: 9
              Text { width: 24; anchors.verticalCenter: parent.verticalCenter; text: "󰛳"; color: tailscaleCard.state.connected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 17; horizontalAlignment: Text.AlignHCenter }
              Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 82
                spacing: 3
                Text { text: "Tailscale"; color: root.text; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
                Text { width: parent.width; text: tailscaleCard.failed ? "Action failed" : root.tailscaleDetail(); elide: Text.ElideRight; color: tailscaleCard.failed ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 8 }
              }
              ControlSwitch {
                anchors.verticalCenter: parent.verticalCenter
                enabled: !!tailscaleCard.state.available
                checked: !!tailscaleCard.state.connected
                busy: tailscaleCard.busy
                onToggled: root.runControl("tailscale", tailscaleCard.action)
              }
            }
          }

          Rectangle {
            id: sshServerCard
            readonly property var state: root.systemData.sshServer || ({})
            readonly property bool busy: root.pendingControlAction === "ssh-server"
            readonly property bool failed: root.failedControlAction === "ssh-server"
            readonly property string mode: busy ? root.pendingControlValue : String(state.mode || "off")
            width: parent.width; height: 94; radius: root.radius
            color: failed ? root.dangerTint : mode !== "off" ? root.activeTint : root.surface
            Column {
              anchors.fill: parent; anchors.margins: 10; spacing: 8
              Row {
                width: parent.width; spacing: 9
                Text { width: 24; text: "󰆍"; color: sshServerCard.mode !== "off" ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 17; horizontalAlignment: Text.AlignHCenter }
                Column {
                  width: parent.width - 33; spacing: 3
                  Text { text: "SSH access"; color: root.text; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
                  Text {
                    width: parent.width
                    text: sshServerCard.failed ? "Mode change failed" : sshServerCard.mode === "mixed" ? "Both paths active · choose one" : sshServerCard.mode === "tailscale" ? "Incoming through Tailscale" : sshServerCard.mode === "ssh" ? "Port 22 · public keys only" : "No incoming SSH"
                    elide: Text.ElideRight
                    color: sshServerCard.failed ? root.red : root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: 8
                  }
                }
              }
              Row {
                width: parent.width; spacing: 6
                Repeater {
                  model: [
                    { label: "Off", mode: "off", available: true },
                    { label: "Tailscale", mode: "tailscale", available: !!sshServerCard.state.tailscaleAvailable },
                    { label: "SSH", mode: "ssh", available: !!sshServerCard.state.sshAvailable }
                  ]
                  Rectangle {
                    required property var modelData
                    readonly property bool selected: sshServerCard.mode === modelData.mode
                    readonly property bool busy: root.controlBusy("ssh-server", modelData.mode)
                    width: (parent.width - 12) / 3; height: 28; radius: root.radius
                    opacity: modelData.available ? 1 : 0.42
                    color: sshModeMouse.pressed ? root.pressColor : busy || selected ? root.selectedColor : sshModeMouse.containsMouse ? root.hoverColor : root.mantle
                    Text { visible: !parent.busy; anchors.centerIn: parent; text: modelData.label; color: parent.selected ? root.accent : root.text; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
                    RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 14; height: 14; spinning: visible; font.pixelSize: 10 }
                    MouseArea {
                      id: sshModeMouse
                      anchors.fill: parent
                      enabled: parent.modelData.available && !sshServerCard.busy && !parent.selected
                      hoverEnabled: true
                      cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: root.runControl("ssh-server", parent.modelData.mode)
                    }
                  }
                }
              }
            }
          }

          Rectangle {
            id: protonVpnCard
            readonly property var state: root.systemData.protonVpn || ({})
            readonly property string action: state.connected ? "disconnect" : "connect"
            readonly property bool busy: root.controlBusy("proton-vpn", action)
            readonly property bool failed: root.controlFailed("proton-vpn", action)
            width: parent.width; height: 66; radius: root.radius
            color: failed ? root.dangerTint : state.connected ? root.activeTint : root.surface
            Row {
              anchors.fill: parent; anchors.margins: 10; spacing: 9
              Text { width: 24; anchors.verticalCenter: parent.verticalCenter; text: "󰒃"; color: protonVpnCard.state.connected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 17; horizontalAlignment: Text.AlignHCenter }
              Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 123
                spacing: 3
                Text { text: "Proton VPN"; color: root.text; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
                Text { width: parent.width; text: protonVpnCard.failed ? "Quick connect failed · open the app" : root.protonVpnDetail(); elide: Text.ElideRight; color: protonVpnCard.failed ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 8 }
              }
              Rectangle {
                width: 32; height: 32; radius: root.radius
                anchors.verticalCenter: parent.verticalCenter
                color: protonAppMouse.pressed ? root.pressColor : protonAppMouse.containsMouse ? root.hoverColor : root.mantle
                Text { anchors.centerIn: parent; text: "󰏌"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 12 }
                MouseArea { id: protonAppMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runControl("proton-vpn", "open") }
                HoverTip { mouse: protonAppMouse; text: "Open Proton VPN for sign-in and location selection" }
              }
              ControlSwitch {
                anchors.verticalCenter: parent.verticalCenter
                enabled: !!protonVpnCard.state.available
                checked: !!protonVpnCard.state.connected
                busy: protonVpnCard.busy
                onToggled: root.runControl("proton-vpn", protonVpnCard.action)
              }
            }
          }
        }
      }
    }
  }

  // Bluetooth controls ---------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: bluetoothWindow

      required property var modelData
      readonly property var devices: root.bluetoothDevices()
      readonly property int listHeight: Math.min(6, devices.length) * 44
      screen: modelData
      visible: root.controlPanel === "bluetooth" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 360
      implicitHeight: root.systemData.bluetoothPowered
        ? 124 + (devices.length === 0 ? 26 : listHeight) + 44
        : 88
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-bluetooth"

      PanelSurface {
        id: bluetoothSurface

        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width; spacing: 8
            PanelGlyph { text: "󰂯" }
            Column {
              width: parent.width - 88
              anchors.verticalCenter: parent.verticalCenter
              spacing: 2
              Text { text: "Bluetooth"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
              Text {
                text: root.systemData.bluetoothPowered ? root.systemData.bluetoothConnected + " connected device" + (root.systemData.bluetoothConnected === 1 ? "" : "s") : "Radio is off"
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: 11
              }
            }
            ControlSwitch {
              anchors.verticalCenter: parent.verticalCenter
              checked: root.systemData.bluetoothPowered
              busy: bluetoothProcess.running && root.bluetoothAction === "toggle"
              onToggled: root.toggleBluetoothPower()
            }
          }
          Row {
            visible: root.systemData.bluetoothPowered
            width: parent.width
            height: 34
            spacing: 8
            Text {
              anchors.verticalCenter: parent.verticalCenter
              width: 18
              text: "󰂰"
              color: root.bluetoothReceiverActive ? root.accent : root.subtext
              font.family: root.fontFamily
              font.pixelSize: 15
            }
            Column {
              anchors.verticalCenter: parent.verticalCenter
              width: parent.width - 74
              spacing: 1
              Text { width: parent.width; text: "Receive audio"; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 12 }
              Text {
                width: parent.width
                text: root.bluetoothReceiverDetail()
                elide: Text.ElideRight
                color: root.bluetoothReceiverActive ? root.subtext : root.overlay
                font.family: root.fontFamily
                font.pixelSize: 9
              }
            }
            ControlSwitch {
              anchors.verticalCenter: parent.verticalCenter
              checked: root.bluetoothReceiverActive
              busy: bluetoothProcess.running && root.bluetoothAction === "receiver"
              onToggled: root.toggleBluetoothReceiver()
            }
          }
          Row {
            visible: root.systemData.bluetoothPowered
            width: parent.width; spacing: 8
            Text {
              width: parent.width - 42
              anchors.verticalCenter: parent.verticalCenter
              text: root.bluetoothScanActive ? "Discovering · this PC is visible" : "Find a device, or let one find this PC"
              color: root.bluetoothScanActive ? root.accent : root.subtext
              font.family: root.fontFamily
              font.pixelSize: 11
            }
            Rectangle {
              width: 34; height: 34; radius: root.radius
              color: bluetoothScanMouse.pressed ? root.pressColor : root.bluetoothScanActive ? root.activeTint : bluetoothScanMouse.containsMouse ? root.hoverColor : "transparent"
              RefreshGlyph { anchors.centerIn: parent; width: 20; height: 20; spinning: root.bluetoothScanActive }
              MouseArea {
                id: bluetoothScanMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.setBluetoothScanning(!root.bluetoothScanActive)
              }
              HoverTip { mouse: bluetoothScanMouse; text: root.bluetoothScanActive ? "Stop discovering" : "Search and stay visible for two minutes" }
            }
          }
          SeeleListView {
            visible: root.systemData.bluetoothPowered && bluetoothWindow.devices.length > 0
            width: parent.width
            height: bluetoothWindow.listHeight
            spacing: 4
            clip: true
            model: bluetoothWindow.devices
            ScrollBar.vertical: SlimScrollBar { popupHovered: bluetoothSurface.hovered }
            delegate: Rectangle {
              required property var modelData
              readonly property bool busy: root.bluetoothBusy === modelData.address
              readonly property bool forgetArmed: root.bluetoothForget === modelData.address
              readonly property bool rowActions: !busy && modelData.paired && (deviceMouse.containsMouse || forgetMouse.containsMouse || autoConnectMouse.containsMouse || forgetArmed)
              width: ListView.view.width; height: 40; radius: root.radius
              color: deviceMouse.pressed ? root.pressColor : busy ? root.selectedColor : deviceMouse.containsMouse ? root.hoverColor : modelData.connected ? root.activeTint : root.alpha(root.surface, 0.55)
              Row {
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 10
                Text {
                  anchors.verticalCenter: parent.verticalCenter
                  text: root.bluetoothIcon(modelData)
                  color: modelData.connected ? root.accent : root.subtext
                  font.family: root.fontFamily
                  font.pixelSize: 15
                }
                Column {
                  anchors.verticalCenter: parent.verticalCenter
                  width: parent.width - 62
                  spacing: 1
                  Text { width: parent.width; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 11 }
                  Text {
                    width: parent.width
                    text: root.bluetoothDetail(modelData)
                    elide: Text.ElideRight
                    color: forgetArmed ? root.red : modelData.connected ? root.green : root.bluetoothBusy === modelData.address ? root.yellow : root.overlay
                    font.family: root.fontFamily
                    font.pixelSize: 9
                  }
                }
                Text {
                  anchors.verticalCenter: parent.verticalCenter
                  visible: !rowActions && !parent.parent.busy
                  text: root.bluetoothSignal(modelData)
                  color: root.overlay
                  font.family: root.fontFamily
                  font.pixelSize: 11
                }
              }
              RefreshGlyph { visible: parent.busy; anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
              MouseArea { id: deviceMouse; anchors.fill: parent; enabled: !parent.busy; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.toggleBluetoothDevice(parent.modelData) }
              Rectangle {
                visible: rowActions
                anchors.right: parent.right
                anchors.rightMargin: 38
                anchors.verticalCenter: parent.verticalCenter
                width: 44; height: 24; radius: root.radius
                color: autoConnectMouse.pressed ? root.pressColor : modelData.trusted ? root.selectedColor : autoConnectMouse.containsMouse ? root.hoverColor : root.alpha(root.surface, 0.95)
                Text { anchors.centerIn: parent; text: "Auto"; color: modelData.trusted ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
                MouseArea { id: autoConnectMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runBluetooth("trust", modelData.address) }
                HoverTip { mouse: autoConnectMouse; text: modelData.trusted ? "Autoconnect on" : "Autoconnect off" }
              }
              Rectangle {
                visible: rowActions
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 24; height: 24; radius: root.radius
                color: forgetMouse.pressed || forgetArmed ? root.dangerPress : forgetMouse.containsMouse ? root.dangerColor : root.alpha(root.surface, 0.95)
                Text { anchors.centerIn: parent; text: "󰅖"; color: forgetArmed || forgetMouse.containsMouse ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 11 }
                MouseArea { id: forgetMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.forgetBluetoothDevice(modelData) }
              }
            }
          }
          Text {
            visible: root.systemData.bluetoothPowered && bluetoothWindow.devices.length === 0
            width: parent.width
            text: root.bluetoothScanActive ? "Looking for nearby devices…" : "No devices yet · start a search"
            color: root.overlay
            font.family: root.fontFamily
            font.pixelSize: 10
          }
        }
      }
    }
  }

  // Bluetooth pairing prompt ---------------------------------------------------
  // The trust decision this carries is the whole point of the pairing window,
  // so it gets the shell's own surface rather than a terminal: same card as the
  // YubiKey prompt, centred on the output that was focused when BlueZ asked.
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: pairingWindow

      required property var modelData
      readonly property string kind: String((root.pairingRequest || {}).kind || "confirm")
      readonly property string deviceName: String((root.pairingRequest || {}).name || "This device")
      screen: modelData
      visible: root.pairingPrompting && root.pinnedScreen(root.pairingScreen, modelData)
      exclusionMode: ExclusionMode.Ignore
      implicitWidth: 360
      implicitHeight: pairingCard.implicitHeight + 44
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      // Only the models that ask this end to type a code need the keyboard, so
      // the prompt takes focus only then and gives it straight back.
      WlrLayershell.keyboardFocus: visible && root.pairingWantsCode() ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-bluetooth-pairing"
      onVisibleChanged: if (visible && root.pairingWantsCode()) Qt.callLater(function() {
        pairingCodeField.forceActiveFocus()
        pairingCodeField.selectAll()
      })

      PanelSurface {
        Column {
          id: pairingCard

          anchors.centerIn: parent
          width: parent.width - 44
          spacing: 13

          Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "󰂰"
            color: root.accent
            font.family: root.fontFamily
            font.pixelSize: 34
          }

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: pairingWindow.kind === "display" || root.pairingWantsCode()
              ? "Pairing with " + pairingWindow.deviceName
              : "Pair with " + pairingWindow.deviceName + "?"
            color: root.text
            font.family: root.fontFamily
            font.pixelSize: 13
            font.bold: true
          }

          Rectangle {
            visible: root.pairingCode() !== "" && !root.pairingWantsCode()
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            height: 52
            radius: root.radius
            color: root.alpha(root.surface, 0.55)
            Text {
              anchors.centerIn: parent
              text: root.pairingCode()
              color: root.accent
              font.family: root.fontFamily
              font.pixelSize: 26
              font.bold: true
              font.letterSpacing: 4
            }
          }

          TextField {
            id: pairingCodeField
            visible: root.pairingWantsCode()
            width: parent.width
            height: 52
            horizontalAlignment: TextInput.AlignHCenter
            placeholderText: pairingWindow.kind === "pincode" ? "PIN" : "000000"
            inputMethodHints: pairingWindow.kind === "pincode" ? Qt.ImhNone : Qt.ImhDigitsOnly
            maximumLength: pairingWindow.kind === "pincode" ? 16 : 6
            color: root.accent
            placeholderTextColor: root.overlay
            selectionColor: root.accent
            selectedTextColor: root.base
            font.family: root.fontFamily
            font.pixelSize: 26
            font.bold: true
            font.letterSpacing: 4
            background: Rectangle {
              radius: root.radius
              color: root.alpha(root.surface, 0.55)
              border.color: pairingCodeField.activeFocus ? root.accent : "transparent"
              border.width: 1
            }
            onAccepted: root.answerBluetoothPairing("accept", pairingCodeField.text)
            Keys.onEscapePressed: root.answerBluetoothPairing("reject", "")
          }

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: pairingWindow.kind === "display" ? "Enter this code on the device."
              : pairingWindow.kind === "authorize" ? "This device cannot show a code. Only accept it if you started this."
              : root.pairingWantsCode() ? "Type the code the device is showing."
              : "Accept only if the device shows the same code."
            color: root.subtext
            font.family: root.fontFamily
            font.pixelSize: 10
          }

          Row {
            visible: pairingWindow.kind !== "display"
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            Rectangle {
              width: (pairingCard.width - 8) / 2
              height: 30
              radius: root.radius
              color: pairingRejectMouse.pressed ? root.dangerPress : pairingRejectMouse.containsMouse ? root.dangerColor : root.alpha(root.surface, 0.95)
              Text { anchors.centerIn: parent; text: "Reject"; color: pairingRejectMouse.containsMouse ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
              MouseArea { id: pairingRejectMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.answerBluetoothPairing("reject", "") }
            }
            Rectangle {
              width: (pairingCard.width - 8) / 2
              height: 30
              radius: root.radius
              color: pairingAcceptMouse.pressed ? root.pressColor : pairingAcceptMouse.containsMouse ? root.hoverColor : root.selectedColor
              Text { anchors.centerIn: parent; text: "Confirm"; color: root.accent; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
              MouseArea { id: pairingAcceptMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.answerBluetoothPairing("accept", pairingCodeField.text) }
            }
          }

          Rectangle {
            visible: pairingWindow.kind === "display"
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            height: 30
            radius: root.radius
            color: pairingDismissMouse.pressed ? root.pressColor : pairingDismissMouse.containsMouse ? root.hoverColor : root.alpha(root.surface, 0.95)
            Text { anchors.centerIn: parent; text: "Dismiss"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: 11; font.bold: true }
            MouseArea { id: pairingDismissMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.clearBluetoothPairing() }
          }
        }
      }
    }
  }

  // Headphone controls ---------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      readonly property var headphones: root.systemData.headphones || ({})
      readonly property bool nothingHeadphones: headphones.kind === "nothing"
      screen: modelData
      visible: root.controlPanel === "airpods" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 340
      implicitHeight: nothingHeadphones ? 150 : 252
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-airpods"

      PanelSurface {
        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width; spacing: 8
            HeadphonesIcon { anchors.verticalCenter: parent.verticalCenter; width: 22; height: 22; kind: String(headphones.kind || "airpods"); tint: root.accent }
            Column {
              width: parent.width - 32
              anchors.verticalCenter: parent.verticalCenter
              spacing: 2
              Text { text: headphones.name || "Headphones"; elide: Text.ElideRight; width: parent.width; color: root.text; font.family: root.fontFamily; font.pixelSize: 16; font.bold: true }
              Text {
                text: root.headphonesBatteryText() || "Connected"
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: 11
              }
            }
          }
          Text {
            text: "NOISE CONTROL" + (nothingHeadphones && !headphones.controls ? " · CONNECTING" : "")
            color: root.overlay
            font.family: root.fontFamily
            font.pixelSize: 9
            font.bold: true
          }
          Row {
            width: parent.width
            spacing: 6
            opacity: nothingHeadphones && !headphones.controls ? 0.42 : 1
            Repeater {
              model: [{label:"Off", mode:"off"}, {label:"ANC", mode:"anc"}, {label:"Aware", mode:"transparency"}, {label:"Adaptive", mode:"adaptive"}]
              Rectangle {
                required property var modelData
                readonly property bool busy: root.controlBusy("airpods", modelData.mode)
                readonly property bool complete: root.controlCompleted("airpods", modelData.mode)
                readonly property bool failed: root.controlFailed("airpods", modelData.mode)
                readonly property bool selected: headphones.noiseMode === modelData.mode
                width: (parent.width - 18) / 4; height: 40; radius: root.radius
                color: airpodsModeMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy || selected ? root.selectedColor : airpodsModeMouse.containsMouse ? root.hoverColor : root.surface
                Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "×" : parent.complete ? "✓ " + modelData.label : modelData.label; color: parent.failed ? root.red : parent.complete ? root.green : parent.selected ? root.accent : root.text; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
                MouseArea { id: airpodsModeMouse; anchors.fill: parent; enabled: !nothingHeadphones || headphones.controls; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: root.runControl("headphones", parent.modelData.mode) }
              }
            }
          }
          Row {
            visible: !nothingHeadphones
            width: parent.width; spacing: 8
            Column {
              width: parent.width - 48
              anchors.verticalCenter: parent.verticalCenter
              spacing: 1
              Text { text: "Auto play and pause"; color: root.text; font.family: root.fontFamily; font.pixelSize: 11 }
              Text { text: "Ear detection through librepods"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 9 }
            }
            ControlSwitch {
              anchors.verticalCenter: parent.verticalCenter
              checked: root.systemData.airpodsEarDetection
              busy: root.controlBusy("airpods", "ear-detection", "toggle")
              onToggled: if (root.runControl("headphones", "ear-detection", "toggle")) root.patchSystemData({ airpodsEarDetection: !root.systemData.airpodsEarDetection })
            }
          }
          Rectangle {
            visible: !nothingHeadphones
            readonly property bool busy: root.controlBusy("airpods", "open")
            readonly property bool complete: root.controlCompleted("airpods", "open")
            readonly property bool failed: root.controlFailed("airpods", "open")
            width: parent.width; height: 38; radius: root.radius
            color: airpodsDetailsMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : airpodsDetailsMouse.containsMouse ? root.hoverColor : root.surface
            Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Could not open" : parent.complete ? "✓ Opened" : nothingHeadphones ? "More Nothing controls" : "Battery and AirPods settings"; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
            RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
            MouseArea { id: airpodsDetailsMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runControl("headphones", "open") }
          }
        }
      }
    }
  }

  // Battery ---------------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: batteryWindow

      required property var modelData
      readonly property var entries: root.batteryEntries()
      screen: modelData
      visible: root.controlPanel === "battery" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 330
      implicitHeight: 78 + Math.max(1, Math.min(5, entries.length)) * 50
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-battery"

      PanelSurface {
        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: "󰁹" }
            Text { anchors.verticalCenter: parent.verticalCenter; text: "Batteries"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
          }
          SeeleListView {
            visible: batteryWindow.entries.length > 0
            width: parent.width
            height: Math.min(5, batteryWindow.entries.length) * 50
            spacing: 6
            clip: true
            model: batteryWindow.entries
            delegate: Column {
              required property var modelData
              width: ListView.view.width
              spacing: 5
              Row {
                width: parent.width; spacing: 8
                Text { anchors.verticalCenter: parent.verticalCenter; text: root.batteryIcon(modelData); color: root.batteryColor(modelData); font.family: root.fontFamily; font.pixelSize: 14 }
                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 90; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 11 }
                Text {
                  anchors.verticalCenter: parent.verticalCenter
                  width: 60
                  text: Number(modelData.percent) + "%" + (root.batteryCharging(modelData) ? " ⚡" : "")
                  color: root.batteryColor(modelData)
                  font.family: root.fontFamily
                  font.pixelSize: 11
                  horizontalAlignment: Text.AlignRight
                }
              }
              Rectangle {
                width: parent.width; height: 7; radius: 4; color: root.surface
                Rectangle {
                  width: parent.width * Math.max(0, Math.min(1, Number(modelData.percent) / 100))
                  height: parent.height
                  radius: 4
                  color: root.batteryColor(modelData)
                }
              }
            }
          }
          Text {
            visible: batteryWindow.entries.length === 0
            text: "No batteries reported"
            color: root.overlay
            font.family: root.fontFamily
            font.pixelSize: 10
          }
        }
      }
    }
  }

  // Notification center --------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: notificationWindow

      required property var modelData
      readonly property var entries: root.notificationHistoryOpen
        ? (root.systemData.notifications.history || [])
        : (root.systemData.notifications.items || [])
      property int stableHeight: 162
      screen: modelData
      visible: root.controlPanel === "notifications" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 400
      implicitHeight: stableHeight
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-notifications"

      // Rows are as tall as the notification they hold, so the opening height
      // comes from what the list actually measured rather than a row count.
      function suggestedHeight() {
        var notifications = root.systemData.notifications || { items: [], history: [] }
        var count = Math.max((notifications.items || []).length, (notifications.history || []).length)
        if (count === 0) return 162
        var content = root.notificationHistoryOpen ? notificationHistoryList.contentHeight : notificationCurrentList.contentHeight
        return Math.min(560, 146 + Math.max(66, content))
      }

      function toggleHistory() {
        root.notificationHistoryOpen = !root.notificationHistoryOpen
      }

      // Deferred, because the lists have not laid out their rows at the moment
      // the surface becomes visible and would still measure zero.
      onVisibleChanged: if (visible) Qt.callLater(function() { stableHeight = suggestedHeight() })
      // `notificationHistoryOpen` belongs to root, so the change has to be
      // taken from there rather than declared as a handler on this window.
      Connections {
        target: root
        function onNotificationHistoryOpenChanged() {
          if (notificationWindow.visible) Qt.callLater(function() {
            notificationWindow.stableHeight = notificationWindow.suggestedHeight()
          })
        }
      }

      PanelSurface {
        id: notificationSurface

        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: root.systemData.dnd ? "󰂛" : "󰂚" }
            Text {
              width: parent.width - 152
              anchors.verticalCenter: parent.verticalCenter
              text: root.notificationHistoryOpen ? "Last 24 hours" : "Notifications"
              color: root.text
              font.family: root.fontFamily
              font.pixelSize: 18
              font.bold: true
            }
            Text { width: 40; anchors.verticalCenter: parent.verticalCenter; text: String(notificationWindow.entries.length); color: root.subtext; font.family: root.fontFamily; font.pixelSize: 11; horizontalAlignment: Text.AlignRight }
            Text { width: 40; anchors.verticalCenter: parent.verticalCenter; leftPadding: 10; text: "DND"; color: root.systemData.dnd ? root.yellow : root.subtext; font.family: root.fontFamily; font.pixelSize: 10 }
            ControlSwitch {
              anchors.verticalCenter: parent.verticalCenter
              checked: root.systemData.dnd
              busy: root.controlBusy("dnd", "")
              onToggled: if (root.runControl("dnd", "")) root.patchSystemData({ dnd: !root.systemData.dnd })
            }
          }
          Row {
            width: parent.width; spacing: 8
            Rectangle {
              width: (parent.width - 8) / 2; height: 36; radius: root.radius
              color: historyMouse.pressed ? root.pressColor : root.notificationHistoryOpen ? root.selectedColor : historyMouse.containsMouse ? root.hoverColor : root.surface
              Text {
                anchors.centerIn: parent
                text: root.notificationHistoryOpen ? "Back" : "History"
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: 10
                font.bold: true
              }
              MouseArea { id: historyMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: notificationWindow.toggleHistory() }
              HoverTip { mouse: historyMouse; text: root.notificationHistoryOpen ? "Show current notifications" : "Show the past 24 hours" }
            }
            Rectangle {
              readonly property bool busy: root.controlBusy("notifications", "clear")
              readonly property bool complete: root.controlCompleted("notifications", "clear")
              readonly property bool failed: root.controlFailed("notifications", "clear")
              width: (parent.width - 8) / 2; height: 36; radius: root.radius
              color: clearMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : clearMouse.containsMouse ? root.hoverColor : root.surface
              Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Failed" : parent.complete ? "✓ Cleared" : "Clear"; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
              RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
              MouseArea { id: clearMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.clearNotifications() }
            }
          }
          Item {
            id: notificationViewport

            width: parent.width
            height: notificationWindow.entries.length > 0 ? parent.height - 78 : 46
            clip: true
            NotificationList {
              id: notificationCurrentList
              visible: !root.notificationHistoryOpen && (root.systemData.notifications.items || []).length > 0
              anchors.fill: parent
              ScrollBar.vertical: SlimScrollBar { popupHovered: notificationSurface.hovered }
            }
            NotificationList {
              id: notificationHistoryList
              history: true
              visible: root.notificationHistoryOpen && (root.systemData.notifications.history || []).length > 0
              anchors.fill: parent
              ScrollBar.vertical: SlimScrollBar { popupHovered: notificationSurface.hovered }
            }
            Item {
              visible: notificationWindow.entries.length === 0
              anchors.fill: parent
              Text {
                anchors.top: parent.top; anchors.topMargin: 14; anchors.horizontalCenter: parent.horizontalCenter
                text: root.notificationHistoryOpen ? "Nothing arrived in the past 24 hours" : "No notifications right now"
                color: root.overlay
                font.family: root.fontFamily
                font.pixelSize: 10
                horizontalAlignment: Text.AlignHCenter
              }
            }
          }
        }
      }
    }
  }
  // Camera controls ------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: cameraWindow

      required property var modelData
      readonly property var camera: root.previewCamera()
      readonly property int deviceCount: (root.systemData.cameraDevices || []).length
      screen: modelData
      visible: root.controlPanel === "camera" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 360
      implicitHeight: cameraContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-camera"

      PanelSurface {
        id: cameraSurface

        Column {
          id: cameraContent

          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: "󰄀" }
            Text { anchors.verticalCenter: parent.verticalCenter; text: "Camera"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
          }
          Text { text: root.systemData.cameraActive ? "Camera is in use" : cameraWindow.deviceCount + " camera device" + (cameraWindow.deviceCount === 1 ? "" : "s"); color: root.systemData.cameraActive ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: 11 }
          Text { text: "PREVIEW DEVICE"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: 9; font.bold: true }
          SeeleListView {
            width: parent.width
            height: Math.max(1, Math.min(4, cameraWindow.deviceCount)) * 32
            spacing: 4
            clip: true
            model: root.systemData.cameraDevices || []
            delegate: Rectangle {
              required property var modelData
              readonly property bool selected: cameraWindow.camera && String(cameraWindow.camera.device || "") === String(modelData.device || "")
              width: ListView.view.width; height: 28; radius: root.radius
              color: cameraDeviceMouse.pressed ? root.pressColor : selected ? root.selectedColor : cameraDeviceMouse.containsMouse ? root.hoverColor : root.alpha(root.surface, 0.5)
              Row {
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                Text { anchors.verticalCenter: parent.verticalCenter; text: parent.parent.selected ? "󰄬" : "󰄀"; color: parent.parent.selected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: 12 }
                Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 10 }
              }
              MouseArea {
                id: cameraDeviceMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.cameraPreviewDevice = String(parent.modelData.device || "")
              }
            }
            Text {
              visible: cameraWindow.deviceCount === 0
              anchors.centerIn: parent
              text: "No camera detected"
              color: root.overlay
              font.family: root.fontFamily
              font.pixelSize: 10
            }
            ScrollBar.vertical: SlimScrollBar { popupHovered: cameraSurface.hovered }
          }
          ClippingRectangle {
            z: 2
            width: parent.width; height: 176; radius: root.radius; color: root.mantle
            Loader {
              id: cameraPreviewLoader
              anchors.fill: parent
              active: cameraWindow.visible && root.systemData.cameraDevices.length > 0
              asynchronous: true
              source: "CameraPreview.qml"
            }
            Binding {
              target: cameraPreviewLoader.item
              property: "device"
              value: cameraWindow.camera ? String(cameraWindow.camera.device || "") : ""
              when: cameraPreviewLoader.status === Loader.Ready
            }
            Binding {
              target: cameraPreviewLoader.item
              property: "active"
              value: cameraWindow.visible
              when: cameraPreviewLoader.status === Loader.Ready
            }
            Text {
              anchors.centerIn: parent
              visible: cameraPreviewLoader.status !== Loader.Ready || !(cameraPreviewLoader.item && cameraPreviewLoader.item.ready)
              text: root.systemData.cameraDevices.length === 0 ? "No camera detected" : root.systemData.cameraActive ? "Camera in use by another app" : cameraPreviewLoader.status === Loader.Ready ? "Starting camera…" : "Preview unavailable"
              color: root.overlay
              font.family: root.fontFamily
              font.pixelSize: 10
            }
          }
          Row {
            width: parent.width; spacing: 8
            Repeater {
              model: [{label:"Preview window", action:"camera-preview"}, {label:"OpenLogi settings", action:"camera-settings"}]
              Rectangle {
                required property var modelData
                readonly property string device: cameraWindow.camera ? String(cameraWindow.camera.device || "") : ""
                readonly property bool busy: root.controlBusy(modelData.action, device)
                readonly property bool complete: root.controlCompleted(modelData.action, device)
                readonly property bool failed: root.controlFailed(modelData.action, device)
                width: (parent.width - 8) / 2; height: 42; radius: root.radius
                color: cameraActionMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : cameraActionMouse.containsMouse ? root.hoverColor : root.surface
                Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Failed" : parent.complete ? "✓ Opened" : modelData.label; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: 12 }
                MouseArea {
                  id: cameraActionMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: {
                    if (parent.modelData.action === "camera-preview") root.openCameraPreview(parent.device)
                    else root.openCameraSettings(parent.device)
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  // Session controls -----------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "system" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 420
      implicitHeight: 222
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-session"

      PanelSurface {
        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          Row {
            width: parent.width
            height: root.panelHeaderHeight
            PanelGlyph { text: "󰐥" }
            Text { anchors.verticalCenter: parent.verticalCenter; text: "Power"; color: root.text; font.family: root.fontFamily; font.pixelSize: 18; font.bold: true }
          }
          Grid {
            width: parent.width
            columns: 3
            columnSpacing: 8
            rowSpacing: 8
            Repeater {
              model: [
                {label:(root.windowsCountdown >= 0 ? "Windows · " + root.windowsCountdown + "s" : "Windows"), icon:"󰍲", action:"reboot-windows", variant:"default"},
                {label:"Lock", icon:"󰌾", action:"lock", variant:"default"},
                {label:"Log out", icon:"󰍃", action:"logout", variant:"default"},
                {label:"Suspend", icon:"󰒲", action:"lock-suspend", variant:"default"},
                {label:"Reboot", icon:"󰜉", action:"reboot", variant:"default"},
                {label:"Shut down", icon:"󰐥", action:"shutdown", variant:"destructive"}
              ]
              Rectangle {
                required property var modelData
                width: (parent.width - 16) / 3; height: 72; radius: root.radius
                color: modelData.variant === "destructive" ? (sessionActionMouse.pressed ? root.dangerPress : sessionActionMouse.containsMouse ? root.dangerColor : root.dangerTint) : sessionActionMouse.pressed ? root.pressColor : sessionActionMouse.containsMouse ? root.hoverColor : root.surface
                Column {
                  anchors.centerIn: parent
                  spacing: 4
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.icon; color: modelData.variant === "destructive" ? root.red : modelData.action === "reboot-windows" && root.windowsCountdown >= 0 ? root.yellow : root.accent; font.family: root.fontFamily; font.pixelSize: 18 }
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.label; color: root.text; font.family: root.fontFamily; font.pixelSize: 10; font.bold: true }
                }
                MouseArea {
                  id: sessionActionMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: {
                    if (parent.modelData.action === "reboot-windows") root.toggleWindowsReboot()
                    else {
                      root.closeOverlays()
                      root.runControl(parent.modelData.action)
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  // Shell OSD ------------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      visible: root.osdOpen && root.pinnedScreen(root.osdScreen, modelData)
      // The YubiKey OSD is the polkit dialog's card without the password field,
      // so it is placed where that dialog places its own: anchoring to no edge
      // leaves a layer surface centred on the output, which is where the dialog
      // centres its card. The other kinds stay the strip below the bar.
      anchors { top: root.osdKind !== "yubikey" }
      margins.top: root.osdKind === "yubikey" ? 0 : root.barHeight + root.osdGap
      // Same width as the dialog, and the height falls out of the same
      // content-plus-padding rule, so the two differ only by the missing field.
      implicitWidth: root.osdKind === "yubikey" ? 360 : 300
      implicitHeight: root.osdKind === "yubikey" ? yubikeyOsd.implicitHeight + 44 : 58
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-osd"
      PanelSurface {
        // Output and microphone share one level strip: they differ only in the
        // glyph and in which level they read. A mute nobody pressed on the
        // keyboard still deserves the acknowledgement the volume keys get.
        Row {
          id: levelOsd
          readonly property bool microphone: root.osdKind === "microphone"
          readonly property bool muted: microphone ? !!root.systemData.microphoneMuted : !!root.systemData.muted
          readonly property int level: microphone
            ? Number(root.microphoneDrag >= 0 ? root.microphoneDrag : root.systemData.microphoneVolume)
            : Number(root.volumeDrag >= 0 ? root.volumeDrag : root.systemData.volume)
          readonly property int maximum: microphone ? 100 : root.outputVolumeMaximum
          visible: microphone || root.osdKind === "volume"
          anchors.fill: parent; anchors.margins: 14; spacing: 12
          Text { anchors.verticalCenter: parent.verticalCenter; text: levelOsd.microphone ? (levelOsd.muted ? "󰍭" : "󰍬") : (levelOsd.muted ? "󰝟" : "󰕾"); color: levelOsd.muted ? root.red : root.accent; font.family: root.fontFamily; font.pixelSize: 20 }
          Rectangle {
            width: 205; height: 8; anchors.verticalCenter: parent.verticalCenter; radius: 4; color: root.surface
            Rectangle { width: parent.width * Math.max(0, Math.min(1, levelOsd.level / levelOsd.maximum)); height: parent.height; radius: 4; color: root.accent }
            Rectangle { visible: !levelOsd.microphone; x: parent.width * 100 / levelOsd.maximum; width: 1; height: parent.height; color: root.alpha(root.text, 0.35) }
          }
          Text { anchors.verticalCenter: parent.verticalCenter; text: levelOsd.level + "%"; color: root.text; font.family: root.fontFamily; font.pixelSize: 11 }
        }
        Row {
          visible: root.osdKind === "airpods"
          anchors.fill: parent; anchors.margins: 14; spacing: 12
          HeadphonesIcon { anchors.verticalCenter: parent.verticalCenter; width: 22; height: 22; kind: root.headphonesOsdKind; tint: root.headphonesOsdConnected ? root.accent : root.subtext }
          Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 34
            spacing: 2
            Text { width: parent.width; text: root.headphonesOsdName; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: 12; font.bold: true }
            Text { text: root.headphonesOsdConnected ? (root.headphonesBatteryText() || "Connected") : "Disconnected"; color: root.headphonesOsdConnected ? root.green : root.subtext; font.family: root.fontFamily; font.pixelSize: 9 }
          }
        }
        // This and the Seele Polkit dialog ask for the same thing, so they are
        // built to read as one object: same card width, same key glyph and
        // colour, same heading and spacing. This is the version without a
        // password field, because whatever raised it -- sudo, gpg -- owns its
        // own prompt on the terminal and only the touch is missing.
        Column {
          id: yubikeyOsd
          visible: root.osdKind === "yubikey"
          anchors.centerIn: parent
          width: parent.width - 44
          spacing: 13

          Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: ""
            color: root.yellow
            font.family: root.fontFamily
            font.pixelSize: 34
          }

          Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "Touch your YubiKey"
            color: root.text
            font.family: root.fontFamily
            font.pixelSize: 13
            font.bold: true
          }

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "Waiting for hardware confirmation"
            color: root.subtext
            font.family: root.fontFamily
            font.pixelSize: 11
            wrapMode: Text.WordWrap
          }
        }
      }
    }
  }
}
