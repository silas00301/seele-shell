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
import "../shared" as Shared
import "media.js" as Media
import "media-speed.js" as MediaSpeed
import "player-volume.js" as PlayerVolume
import "network.js" as Network
import "time.js" as Time
import "notifications.js" as Notifications
import "notification-search.js" as NotificationSearch
import "uri-picker.js" as Uris
import "github.js" as GitHub

Shared.Theme {
  id: root

  FocusTimer {
    id: focusTimer
    onCompleted: Quickshell.execDetached(["notify-send", "--app-name=Seele Shell", "--icon=appointment-soon", "Focus timer", "Time is up."])
  }

  property bool agentsOpen: false
  // Panels stay on the screen they were opened from. Tracking Hyprland's
  // focused monitor instead would move an open panel to another output the
  // moment the pointer crossed a screen edge.
  property string overlayScreen: ""
  // Horizontal center of the menu bar item that opened the current overlay.
  // Panels keep that center until it would put them closer to a screen edge
  // than Hyprland puts ordinary windows.
  property real overlayAnchorX: -1
  property string osdScreen: ""
  property string notificationPopupScreen: ""
  property string controlPanel: ""
  property var applicationWindow: null
  property bool applicationForceConfirm: false
  // The media panel and Control Center share one selection. It survives the
  // panel closing, and the resolver falls back when the selected MPRIS client
  // leaves the bus.
  property var selectedMediaPlayer: null
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
  // Every level track runs to 100%, so a full bar means full volume on the
  // output as well as the microphone. Output gain goes further than that, but a
  // bar cannot draw more than full, so above 100% only the number moves.
  // Dragging maps across the same 100; the boost above it belongs to the wheel
  // and the volume keys, which clamp at `outputVolumeMaximum` instead.
  readonly property int audioTrackMaximum: 100
  property string cameraPreviewDevice: ""
  property bool agentUsageOpen: false
  property bool agentModelsOpen: false
  property string agentMetricPeriod: "day"
  property bool notificationHistoryOpen: false
  // Toast timers live with notification state, separately from inbox lifetime.
  property int notificationPopupHoverCount: 0
  // A toast is a place to notice something and the panel is a place to read it,
  // so the panel shows a notification whole and only the toast keeps it to one
  // line until asked.
  property var notificationUnfolded: ({})
  property var clockData: ({ pinned: [], zones: [], local: {} })
  property string clockError: ""
  readonly property string calendarDay: Qt.formatDate(now, "yyyy-MM-dd")
  readonly property date calendarDate: new Date(calendarDay + "T12:00:00")
  property var activeTrayItem: null
  property bool osdOpen: false
  property string osdKind: "volume"
  property bool headphonesOsdConnected: false
  property string headphonesOsdName: "Headphones"
  property string headphonesOsdKind: "headphones"
  property var yubikeyTouchSources: ({})
  property bool yubikeyTouchRequired: false
  property bool polkitPrompting: false
  property bool lockPrompting: false
  property bool statusInitialized: false
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
  readonly property SystemState systemData: SystemState {}
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

  function barItemCenter(item) {
    if (!item) return -1
    return item.mapToItem(null, item.width / 2, item.height / 2).x
  }

  function panelLeft(screen, panelWidth) {
    var rightmost = screen.width - panelWidth - panelGap
    var requested = overlayAnchorX >= 0 ? overlayAnchorX - panelWidth / 2 : rightmost
    return Math.max(panelGap, Math.min(requested, rightmost))
  }

  function requestedOverlayAnchor(anchorX) {
    var requested = Number(anchorX)
    return !isNaN(requested) && requested >= 0 ? requested : overlayAnchorX
  }

  function agentsHere(screen) {
    return agentsOpen && pinnedScreen(overlayScreen, screen)
  }

  function closeTrayMenu() {
    trayMenuOpen = false
    activeTrayItem = null
  }

  function closeOverlays() {
    uriPicker.close()
    cancelModuleDrag()
    agentsOpen = false
    controlPanel = ""
    applicationWindow = null
    applicationForceConfirm = false
    overlayScreen = ""
    overlayAnchorX = -1
    closeTrayMenu()
    windowsCountdown = -1
    windowsTimer.stop()
    bluetoothForget = ""
    notificationHistoryOpen = false
    bluetoothForgetTimer.stop()
  }

  function toggleUris() {
    // Capture the visible desktop, including any open shell panel. The frozen
    // surface covers it without changing what will be restored on dismissal.
    if (uriPicker.active) uriPicker.close()
    else uriPicker.open()
  }

  function toggleLauncher(mode) {
    closeOverlays()
    Quickshell.execDetached(["seele-control", "launcher-toggle"])
  }

  function toggleAgents(screen, anchorX) {
    var shouldOpen = !agentsOpen
    var nextAnchor = requestedOverlayAnchor(anchorX)
    closeOverlays()
    agentsOpen = shouldOpen
    if (!agentsOpen) return
    overlayScreen = screen || currentScreen()
    overlayAnchorX = nextAnchor
    if (!agentData.generatedAt || agentError !== "") refreshAgents()
  }

  function toggleControl(panel, screen, anchorX) {
    var shouldOpen = controlPanel !== panel
    var nextAnchor = requestedOverlayAnchor(anchorX)
    closeOverlays()
    controlPanel = shouldOpen ? panel : ""
    if (controlPanel === "") return
    if (panel === "clock") refreshClock()
    overlayScreen = screen || currentScreen()
    overlayAnchorX = nextAnchor
    if (panel === "home-assistant") {
      homeAssistantStore.refresh()
      return
    }
    var group = panel === "notifications" ? "notifications"
      : panel === "audio" ? "audio"
      : ["bluetooth", "airpods"].indexOf(panel) >= 0 ? "bluetooth"
      : panel === "network" ? "network"
      : ["control-center", "vpn"].indexOf(panel) >= 0 ? "all" : "aux"
    refreshStatus(group)
  }

  function toggleApplication(window, screen, anchorX) {
    var sameWindow = root.controlPanel === "application" && root.applicationWindow === window
    var shouldOpen = !!window && !(sameWindow && root.overlayScreen === (screen || root.currentScreen()))
    var nextAnchor = requestedOverlayAnchor(anchorX)
    closeOverlays()
    if (!shouldOpen) return
    controlPanel = "application"
    applicationWindow = window
    overlayScreen = screen || currentScreen()
    overlayAnchorX = nextAnchor
  }

  function quitApplication(force) {
    if (force && !root.applicationForceConfirm) {
      root.applicationForceConfirm = true
      return
    }
    var address = root.applicationWindow ? String(root.applicationWindow.address || "") : ""
    root.closeOverlays()
    if (address === "") return
    Quickshell.execDetached(["seele-control", "application", force ? "force-quit" : "quit", address])
  }

  function toggleMedia(player, screen, anchorX) {
    var samePlayer = root.controlPanel === "media" && root.nowPlayingPlayer() === player
    var shouldOpen = !!player && !(samePlayer && root.overlayScreen === (screen || root.currentScreen()))
    var nextAnchor = requestedOverlayAnchor(anchorX)
    root.closeOverlays()
    if (!shouldOpen) return
    root.selectedMediaPlayer = player
    root.controlPanel = "media"
    root.overlayScreen = screen || root.currentScreen()
    root.overlayAnchorX = nextAnchor
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

  function refreshStatus(group) {
    if (statusProcess.running) statusProcess.write(String(group || "all") + "\n")
    else statusProcess.running = true
  }

  function refreshBluetoothStatus() {
    root.refreshStatus("bluetooth")
  }

  function setNotificationPopupHovered(hovered) {
    root.notificationPopupHoverCount = Math.max(0, root.notificationPopupHoverCount + (hovered ? 1 : -1))
    notificationStore.controller.pause(root.notificationPopupHoverCount > 0, Date.now() / 1000)
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
        if (parsed.headphones !== undefined) {
          var nextHeadphones = parsed.headphones || ({})
          var currentHeadphones = root.systemData.headphones || ({})
          if (root.statusInitialized && !!nextHeadphones.connected !== !!currentHeadphones.connected) {
            root.headphonesOsdConnected = !!nextHeadphones.connected
            root.headphonesOsdName = String(nextHeadphones.name || currentHeadphones.name || "Headphones")
            root.headphonesOsdKind = /airpods/i.test(root.headphonesOsdName) ? "airpods" : "headphones"
            root.showTimedOsd("airpods")
          }
          root.statusInitialized = true
        }
        root.systemData.apply(parsed)
        if (parsed.volume !== undefined && root.volumeDrag >= 0 && Number(parsed.volume) === root.volumeDrag) root.volumeDrag = -1
        if (parsed.microphoneVolume !== undefined && root.microphoneDrag >= 0 && Number(parsed.microphoneVolume) === root.microphoneDrag) root.microphoneDrag = -1
        if (parsed.bluetoothScanning !== undefined) root.reconcileBluetoothScanIntent(!!parsed.bluetoothScanning)
        if (parsed.bluetoothReceiver !== undefined) root.reconcileBluetoothReceiverIntent(!!parsed.bluetoothReceiver)
      }
    } catch (error) {
      console.warn("seele-shell/status", error)
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

  function mediaPlayerName(player) {
    return Media.playerName(player)
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

  function seekMediaKey(player, event) {
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    var command = event.key === Qt.Key_Left ? "back" : event.key === Qt.Key_Right ? "forward"
      : event.key === Qt.Key_Home ? "start" : event.key === Qt.Key_End ? "end" : ""
    var target = Media.seekTarget(player, command, !!(event.modifiers & Qt.ShiftModifier))
    if (target === null) return
    player.position = target
    event.accepted = true
  }

  function mediaTimelineAvailable(player) {
    return Media.timelineAvailable(player)
  }

  function mediaIsLive(player) {
    return Media.liveStream(player)
  }

  function availableMediaPlayers() {
    return Media.availablePlayers(Mpris.players.values || [])
  }

  function selectMediaPlayer(player) {
    if (player) root.selectedMediaPlayer = player
  }

  // The bar keeps Spotify and the device player in separate entries. The
  // media panel and Control Center resolve their shared selection here.
  function nowPlayingPlayer() {
    return Media.selectedPlayer(Mpris.players.values || [], root.selectedMediaPlayer)
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

  function openTrayItemMenu(item, screen, anchorX) {
    if (!item) return false
    if (item.menu) {
      var sameMenu = root.trayMenuOpen && root.activeTrayItem === item
      var nextAnchor = requestedOverlayAnchor(anchorX)
      root.closeOverlays()
      if (!sameMenu) {
        root.activeTrayItem = item
        root.trayMenuOpen = true
        root.overlayScreen = screen || root.currentScreen()
        root.overlayAnchorX = nextAnchor
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

  function selectedAudioOutputs() {
    return root.audioDevices("output").filter(function(device) { return device.node && (device.selected || device.default) }).map(function(device) { return device.node })
  }

  function setAudioOutputs(nodes) {
    if (nodes.length) root.runControl("audio-outputs", JSON.stringify(nodes))
  }

  function toggleAudioOutput(node) {
    var nodes = root.selectedAudioOutputs()
    var index = nodes.indexOf(node)
    if (index >= 0) nodes.splice(index, 1)
    else nodes.push(node)
    root.setAudioOutputs(nodes)
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
    return !!actions && typeof actions.default === "string"
  }

  function activateNotification(id) {
    if (notificationStore.controller.invoke(id, "default")) root.closeOverlays()
  }

  function notificationPopupEntries() {
    return root.systemData.notifications.popups || []
  }

  function toggleNotificationUnfolded(id) {
    var key = String(id)
    var unfolded = {}
    for (var other in root.notificationUnfolded) unfolded[other] = root.notificationUnfolded[other]
    if (unfolded[key]) delete unfolded[key]
    else unfolded[key] = true
    root.notificationUnfolded = unfolded
  }

  function retireNotificationPopup(id) { notificationStore.controller.retire(id) }
  function dismissNotification(id) { notificationStore.controller.dismiss(id) }
  function clearNotifications() { notificationStore.controller.clear(root.notificationHistoryOpen) }

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
      airpods: root.headphonesLabel(),
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

  function headphonesIconKind() {
    var headphones = root.systemData.headphones || ({})
    return headphones.connected && /airpods/i.test(String(headphones.name || "")) ? "airpods" : "headphones"
  }

  function headphonesLabel() {
    var headphones = root.systemData.headphones || ({})
    return headphones.connected && headphones.name ? String(headphones.name) : "Headphones"
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

  // The vendored mark for a harness, or for the provider behind it. Anything
  // without one keeps its two-letter badge, so a provider this flake has never
  // heard of still reads on the bar.
  function agentMark(id) {
    var marks = { pi: "pi.svg", opencode: "opencode.svg", codex: "openai.svg", openai: "openai.svg", claude: "claude.svg" }
    return marks[String(id).toLowerCase()] || ""
  }

  function agentColor(status) {
    if (status === "input") return root.yellow
    if (status === "working") return root.accent
    if (status === "finished") return root.green
    return root.subtext
  }

  // Every harness the cockpit draws an indicator for: the launchers CodexBar
  // reports, plus any harness that published lifecycle state without one, so a
  // session started outside all of this is still on the readout.
  function agentIndicators() {
    var launchers = root.agentData.launchers || []
    var states = root.systemData.agentStates || {}
    var result = []
    var seen = {}
    for (var i = 0; i < launchers.length; i++) {
      result.push({ id: launchers[i].id, name: launchers[i].name })
      seen[launchers[i].id] = true
    }
    for (var id in states) {
      if (seen[id] || !states[id].active) continue
      result.push({ id: id, name: id })
    }
    return result
  }

  // Only `idle` means there is no session. A record whose chosen source reports
  // something else is passed through rather than called "not running", because
  // the menu bar draws this for a harness it already knows is active.
  function agentStatusText(status) {
    if (status === "working") return "working"
    if (status === "input") return "needs input"
    if (status === "finished") return "finished"
    if (status === "idle") return "not running"
    return String(status)
  }

  // The words behind the indicator row's dots, held at the end of its rule.
  // Nothing running says nothing at all.
  function agentSummary() {
    var states = root.systemData.agentStates || {}
    var working = 0
    var waiting = 0
    var finished = 0
    for (var id in states) {
      var status = String(states[id].status || "idle")
      if (status === "working") working++
      else if (status === "input") waiting++
      else if (status === "finished") finished++
    }
    var parts = []
    if (working > 0) parts.push(working + " working")
    if (waiting > 0) parts.push(waiting + (waiting === 1 ? " needs input" : " need input"))
    if (finished > 0) parts.push(finished + " finished")
    return parts.join(" · ")
  }

  // Usage figures are only worth what their age says they are. Until the first
  // scan lands there is no age to report, so the header names the providers
  // instead.
  function agentUpdatedText() {
    var generated = Date.parse(String(root.agentData.generatedAt || ""))
    return isNaN(generated) ? root.subscriptionSummary() : "Updated " + root.agoText(generated / 1000)
  }

  function agentRunning(id) {
    var states = root.systemData.agentStates || {}
    return !!(states[id] && states[id].active)
  }

  function subscriptionSummary() {
    var subscriptions = root.agentData.subscriptions || []
    if (subscriptions.length === 0) return "No subscriptions"
    var names = []
    for (var i = 0; i < subscriptions.length; i++) names.push(subscriptions[i].name)
    return names.join(" · ")
  }

  function patchSystemData(patch) {
    root.systemData.apply(patch)
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

  // The bar shows each provider's mark and a bare number. This is where that
  // number says which subscription it belongs to and what it is counting.
  function menuBarCapacityTip() {
    var capacities = root.menuBarCapacities()
    if (capacities.length === 0) return "AI cockpit"
    var parts = []
    for (var i = 0; i < capacities.length; i++) parts.push(capacities[i].name + " " + capacities[i].free + "% free")
    return parts.join(" · ")
  }

  // A lit indicator takes you to the session it reports on. `seele-control`
  // owns resolving which window that is, because the harness is a descendant
  // of the terminal holding it and only /proc says which one.
  function focusAgent(id) {
    closeOverlays()
    Quickshell.execDetached(["seele-control", "agent-focus", String(id)])
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

  function audioFillRatio(value) {
    var level = Number(value)
    if (isNaN(level)) return 0
    return Math.max(0, Math.min(1, level / root.audioTrackMaximum))
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

  // A window worth watching before it is a window already spent: the meter and
  // its number leave the accent once a third of the allowance is left.
  function capacityColor(free) {
    if (free <= 15) return root.red
    if (free <= 30) return root.yellow
    return root.accent
  }

  // Every subscription that has actually been spent against, in the order the
  // provider list reports them rather than by how spent each one is: a bar
  // entry whose parts reorder themselves is one the eye has to read twice. A
  // window still at full capacity is left off, because it needs the width to
  // say nothing.
  function menuBarCapacities() {
    var subscriptions = root.agentData.subscriptions || []
    var result = []
    for (var i = 0; i < subscriptions.length; i++) {
      var limit = root.subscriptionLimit(subscriptions[i].id)
      if (!limit) continue
      var free = root.freePercent(limit)
      if (free >= 100) continue
      result.push({ id: subscriptions[i].id, name: subscriptions[i].name, free: free })
    }
    return result
  }

  function refreshClock() {
    if (clockProcess.running) clockProcess.write("refresh\n")
    else clockProcess.running = true
  }

  function parseClockData(output) {
    try {
      var parsed = JSON.parse(String(output || ""))
      if (parsed && parsed.zones) { root.clockData = parsed; root.clockError = "" }
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
    command: ["seele-clock", "watch"]
    stdinEnabled: true
    stdout: SplitParser {
      onRead: data => root.parseClockData(data)
    }
    stderr: StdioCollector { onStreamFinished: if (text.trim()) root.clockError = text.trim() }
    onExited: { root.clockError = "World clocks are unavailable. Retrying…"; clockRestartTimer.restart() }
  }

  Timer {
    id: clockRestartTimer
    interval: 1000
    onTriggered: clockProcess.running = true
  }

  Process {
    id: clockActionProcess
    stderr: StdioCollector { onStreamFinished: if (text.trim()) root.clockError = text.trim() }
    onExited: code => { if (code === 0) root.refreshClock() }
  }

  Process {
    id: statusProcess
    command: ["seele-control", "watch-status"]
    stdinEnabled: true
    running: true
    stdout: SplitParser {
      onRead: data => root.parseSystemData(data)
    }
    onExited: statusRestartTimer.restart()
  }

  Process {
    id: controlProcess
    environment: ({ SEELE_CONTROL_NO_STATUS: "1" })
    onExited: function(exitCode) {
      var action = root.pendingControlAction
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
      var group = ["notifications", "dnd"].indexOf(action) >= 0 ? "notifications"
        : ["volume", "microphone", "audio-device", "audio-outputs"].indexOf(action) >= 0 ? "audio"
        : ["wifi", "proton-vpn"].indexOf(action) >= 0 ? "network" : "aux"
      root.refreshStatus(group)
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
    id: statusRestartTimer
    interval: 1000
    onTriggered: statusProcess.running = true
  }

  Timer {
    interval: 1000
    repeat: true
    running: true
    triggeredOnStart: true
    onTriggered: {
      var next = new Date()
      var oldMinute = Math.floor(root.now.getTime() / 60000)
      if (root.controlPanel === "clock" || Math.floor(next.getTime() / 60000) !== oldMinute) root.now = next
      if (root.clockData.zones.length === 0 || Math.floor(next.getTime() / 60000) !== oldMinute) root.refreshClock()
    }
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

  GitHubStore {
    id: githubStore
    active: root.controlPanel === "github"
  }

  NotificationStore {
    id: notificationStore
    onPublished: (view, dnd) => {
      root.systemData.apply({ notifications: view, dnd: dnd })
      var present = {}, unfolded = {}
      for (var i = 0; i < view.items.length; i++) present[String(view.items[i].id)] = true
      for (var j = 0; j < view.popups.length; j++) present[String(view.popups[j].id)] = true
      for (var key in root.notificationUnfolded) if (present[key]) unfolded[key] = true
      root.notificationUnfolded = unfolded
    }
    onArrived: root.notificationPopupScreen = root.currentScreen()
  }

  HomeAssistantStore { id: homeAssistantStore }
  NotificationClipboard { id: notificationClipboard }

  IpcHandler {
    target: "seele-shell"
    function notificationStatus(): string {
      return JSON.stringify({ notifications: notificationStore.controller.view(), dnd: notificationStore.controller.dnd })
    }
    function notificationCommand(action: string, id: string, key: string): string {
      var state = notificationStore.controller
      if (action === "invoke") return state.invoke(id, key || "default") ? "ok" : "unavailable"
      if (action === "dismiss") return state.dismiss(id) ? "ok" : "unavailable"
      if (action === "retire") return state.retire(id) ? "ok" : "unavailable"
      if (action === "pin") return state.pin(id) ? "ok" : "unavailable"
      if (action === "clear") { state.clear(false); return "ok" }
      if (action === "clear-history") { state.clear(true); return "ok" }
      if (action === "dnd") { state.setDnd(!state.dnd); return "ok" }
      return "unavailable"
    }
    function ping(): string { return "ok" }
    function toggleLauncher(mode: string): void { root.toggleLauncher(mode) }
    function toggleAgents(): void { root.toggleAgents() }
    function toggleUris(): void { root.toggleUris() }
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
  component CenteredGlyph: Shared.CenteredGlyph {}

  component SeeleListView: Shared.SeeleListView { theme: root }

  component SeeleFlickable: Shared.SeeleFlickable { theme: root }

  component RefreshGlyph: Shared.RefreshGlyph { theme: root }

  component ControlSwitch: Rectangle {
    id: control

    property bool checked: false
    property bool busy: false
    signal toggled()

    implicitWidth: 40
    implicitHeight: 22
    opacity: enabled ? 1 : 0.42
    radius: height / 2
    antialiasing: true
    // Off, the track is a well cut into the surface rather than a grey pill,
    // so an unset switch is quiet and a set one is the only lit thing in the
    // row.
    color: switchMouse.pressed ? root.alpha(control.checked ? root.accent : root.text, 0.6) : control.checked ? root.accent : root.wellColor
    border.width: 1
    border.color: control.busy ? root.accent
      : switchMouse.containsMouse ? root.alpha(root.accent, 0.55)
      : control.checked ? "transparent" : root.edgeLight

    Behavior on color { ColorAnimation { duration: root.durationFast } }
    Behavior on border.color { ColorAnimation { duration: root.durationFast } }

    Rectangle {
      visible: !control.busy
      width: parent.height - 6
      height: width
      radius: width / 2
      y: 3
      x: control.checked ? control.width - width - 3 : 3
      color: control.checked ? root.crust : root.alpha(root.text, 0.82)
      antialiasing: true

      Behavior on x { NumberAnimation { duration: root.durationFast; easing.type: Easing.OutCubic } }
      Behavior on color { ColorAnimation { duration: root.durationFast } }
    }

    RefreshGlyph {
      visible: control.busy
      anchors.centerIn: parent
      width: 16
      height: 16
      spinning: visible
      color: control.checked ? root.crust : root.text
      font.pixelSize: root.textBody
    }

    MouseArea { id: switchMouse; anchors.fill: parent; enabled: control.enabled && !control.busy; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: control.toggled() }
  }

  // Depth wash, drawn under a surface's content and inside its border. The
  // light gathers along the top edge, thins out across the middle, and the
  // surface settles into ink at the bottom, so a tall panel is lit rather
  // than merely tinted.
  component HoverWash: Shared.HoverWash { theme: root }

  component SurfaceWash: Shared.SurfaceWash { theme: root }

  // The light on a surface's inside edge. The grounding ring outside is what
  // cuts the panel out of the wallpaper; this is what keeps the cut reading as
  // glass rather than as a hole. A hairline runs the whole perimeter and a
  // brighter crown sits along the top, held clear of the corner arcs, because
  // a straight line drawn into a rounded corner reads as a nick in it.
  component SurfaceEdge: Shared.SurfaceEdge { theme: root }

  // Grain film, drawn over a surface's content so the texture is even across
  // the panel and the cards inside it. It accepts no input, so everything
  // underneath stays clickable. A tiled image cannot follow a rounded corner,
  // so `inset` pulls the film inside the arc: anything past
  // radius * (1 - 1 / sqrt(2)) stays within the surface.
  component SurfaceGrain: Shared.SurfaceGrain { theme: root }

  component HoverTip: PopupWindow {
    id: hoverTip

    property var mouse: null
    property string text: ""
    // A menu bar tip has to stand down while a panel is open, because the entry
    // it belongs to is sitting behind that panel. A tip on a control inside the
    // panel is the opposite case: the panel being open is the only time it can
    // be hovered at all, so the same guard would have hidden it always.
    property bool inOverlay: false

    visible: mouse !== null && mouse.containsMouse && text !== ""
      && (hoverTip.inOverlay || (root.controlPanel === "" && !root.agentsOpen && !root.trayMenuOpen))
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

      SurfaceWash { radius: root.radiusSmall - 1 }
      SurfaceEdge { radius: root.radiusSmall - 1 }

      Text {
        id: hoverTipLabel
        anchors.centerIn: parent
        text: hoverTip.text
        color: root.text
        font.family: root.fontFamily
        font.pixelSize: root.textLabel
      }
    }
  }

  // Qt cannot round an Image or a live video surface, so the source is drawn
  // through a mask instead. The source item must hide itself; the effect draws
  // it in its place.
  // A harness or provider drawn as its own mark. The vendored icons are flat,
  // so nothing here carries state: the blinking bar under a badge and the
  // number beside a capacity already do. Rasterize the vector well above the
  // size it is drawn at, because the OpenAI knot loses its loops when a 24px
  // raster is squeezed into the menu bar.
  component AgentMark: Image {
    sourceSize.width: 64
    sourceSize.height: 64
    fillMode: Image.PreserveAspectFit
    smooth: true
    mipmap: true
  }

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

  // Every panel introduces itself the same way: its mark in a tinted well, the
  // panel's name, an optional line of context beneath it, and a trailing slot
  // for whatever that panel keeps beside its title. Panels used to assemble this
  // row by hand and had drifted apart on glyph size, header height and
  // baseline, so the header is a component and the drift has nowhere to live.
  component PanelHeader: Shared.PanelHeader { theme: root }

  // The uppercase rule that introduces a group inside a panel. It had drifted
  // between two sizes and two colours; here it is one thing, and the tracking
  // is what keeps a run of capitals from reading as a shout.
  component SectionLabel: Shared.SectionLabel { theme: root }

  // The rule with everything a group's heading carries: the uppercase label,
  // the group's own live summary held at the far end of it, and, where the
  // group folds, the chevron that says so. Hand-assembled headings drift apart
  // on baseline and row height, and the extra space sits above the label so a
  // rule reads as belonging to what follows it rather than to what it left.
  component SectionRule: Item {
    id: sectionRule

    property string label: ""
    property string detail: ""
    property color detailColor: root.overlay
    property bool collapsible: false
    property bool expanded: false
    default property alias trailing: sectionRuleTrailing.data
    signal toggled()

    height: Math.max(26, sectionRuleTrailing.implicitHeight + root.spaceTight)

    SectionLabel {
      anchors.left: parent.left
      anchors.bottom: parent.bottom
      text: sectionRule.label
      color: sectionRule.collapsible && sectionRuleMouse.pressed
        ? root.accent
        : sectionRule.collapsible && sectionRuleMouse.containsMouse
          ? root.text
          : root.overlay
    }

    // Everything in the rule sits on its bottom edge, so a group's label, its
    // summary, its fold arrow and whatever control governs it share one line
    // and the rule's extra height stays above them all.
    Row {
      id: sectionRuleTrailing

      anchors.right: sectionRuleChevron.left
      anchors.rightMargin: sectionRule.collapsible ? root.spaceSmall : 0
      anchors.bottom: parent.bottom
      spacing: root.spaceMedium
    }

    Text {
      anchors.right: sectionRuleTrailing.left
      anchors.rightMargin: sectionRuleTrailing.width > 0 ? root.spaceMedium : 0
      anchors.bottom: parent.bottom
      text: sectionRule.detail
      color: sectionRule.detailColor
      font.family: root.fontFamily
      font.pixelSize: root.textCaption
    }

    Text {
      id: sectionRuleChevron

      visible: sectionRule.collapsible
      width: visible ? 14 : 0
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      anchors.bottomMargin: -2
      text: sectionRule.expanded ? "󰅃" : "󰅀"
      color: sectionRuleMouse.containsMouse ? root.text : root.overlay
      font.family: root.fontFamily
      font.pixelSize: root.textBody
      horizontalAlignment: Text.AlignRight
    }

    MouseArea {
      id: sectionRuleMouse

      anchors.fill: parent
      enabled: sectionRule.collapsible
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: sectionRule.toggled()
    }
  }

  // A filled track. Every meter in the shell -- capacity, daily usage, battery,
  // the volume OSD -- is this one shape, rounded on its own height rather than
  // on a literal that had outgrown the bar it was drawn in.
  component MeterBar: Rectangle {
    id: meterBar

    property real ratio: 0
    property color fill: root.accent

    implicitHeight: 7
    radius: height / 2
    color: root.wellColor
    border.width: 1
    border.color: root.alpha(root.text, 0.05)
    antialiasing: true

    // The filled part is graded along its length rather than laid down flat,
    // so a full meter still reads as a lit instrument instead of a block of
    // colour.
    Rectangle {
      width: parent.width * Math.max(0, Math.min(1, meterBar.ratio))
      height: parent.height
      radius: parent.radius
      antialiasing: true

      gradient: Gradient {
        orientation: Gradient.Horizontal
        GradientStop { position: 0.0; color: root.alpha(meterBar.fill, 0.62) }
        GradientStop { position: 1.0; color: meterBar.fill }
      }
    }
  }

  // The hairline that lifts a card off the panel material behind it, and the
  // light along its top edge -- the same two-part edge a panel is framed with,
  // one step quieter. Drawn as a child rather than as the card's own border,
  // so a card whose fill already tracks hover and press state keeps that
  // binding and still gets the edge.
  component CardEdge: Shared.CardEdge { theme: root }

  // A set of exclusive choices drawn as one well with the chosen one lit inside
  // it. Several outlined boxes side by side spend an outline on every
  // alternative to say what the fill of the one that is on already says, and
  // the well is what makes the group read as a single control.
  component SegmentWell: Rectangle {
    default property alias content: segmentWellRow.data

    implicitHeight: root.chipHeight
    radius: root.radius
    color: root.wellColor
    border.width: 1
    border.color: root.alpha(root.text, 0.05)
    antialiasing: true

    Row {
      id: segmentWellRow

      anchors.fill: parent
      anchors.margins: 2
    }
  }

  component Segment: Rectangle {
    id: segment

    property bool selected: false
    property bool hovered: false
    property bool pressed: false

    height: parent ? parent.height : root.chipHeight
    radius: root.radiusSmall
    color: segment.pressed ? root.pressColor : segment.selected ? root.selectedColor : root.clearColor
    antialiasing: true

    Behavior on color { ColorAnimation { duration: root.durationFast } }

    HoverWash { hovered: segment.hovered }
  }

  // A list of choices sits in a card the way every other group on a panel does,
  // so the rows inside it can take the lighter tint the elevation ramp gives
  // them instead of floating on the panel's own material.
  component DeviceListCard: Rectangle {
    property real listHeight: 0

    height: listHeight + root.cardPadding * 2
    radius: root.radius
    color: root.cardColor
    antialiasing: true

    CardEdge {}
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
    color: root.wellColor

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
      font.pixelSize: root.textLabel
      font.weight: root.weightMedium
      font.letterSpacing: root.trackingLabel
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
      font.pixelSize: root.textBody
      font.weight: root.weightStrong
    }
  }

  // Shared chrome for every floating panel, so panels differ only in what
  // they hold, never in how they are framed.
  component PanelSurface: Shared.PanelSurface { theme: root }

  // Scroll indicators are hairlines rather than the platform's full-width
  // bars, and panels reserve `scrollGutter` for them so a bar never sits on
  // top of the content's own edge.
  component SlimScrollBar: Shared.SlimScrollBar { theme: root }

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

    Behavior on opacity { NumberAnimation { duration: root.durationFast } }

    anchors.verticalCenter: parent.verticalCenter
    height: root.barHeight

    Rectangle {
      anchors.fill: parent
      anchors.topMargin: root.barPadding
      anchors.bottomMargin: root.barPadding
      anchors.leftMargin: root.barSpacing / 2
      anchors.rightMargin: root.barSpacing / 2
      radius: root.radius
      color: parent.active ? root.selectedColor : parent.hovered ? root.hoverColor : root.clearColor

      Behavior on color { ColorAnimation { duration: root.durationFast } }
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
      font.pixelSize: root.textLabel
    }

    Text {
      id: barLabelText

      anchors.left: parent.left
      anchors.right: parent.right
      anchors.baseline: barLabelReference.baseline
      elide: Text.ElideRight
      color: barLabel.color
      font.family: root.fontFamily
      font.pixelSize: root.textLabel
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
    cursorShape: Qt.PointingHandCursor

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
    readonly property bool muted: audioLevelRow.microphone ? !!root.systemData.microphoneMuted : !!root.systemData.muted

    spacing: 8

    Rectangle {
      width: audioLevelRow.width - 52
      height: 44
      radius: root.radius
      color: root.wellColor
      clip: true

      Rectangle {
        width: parent.width * root.audioFillRatio(audioLevelRow.shown)
        radius: parent.radius
        height: parent.height
        color: audioLevelRow.muted ? root.fillDanger : root.fillColor
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
          font.pixelSize: root.textBody
          font.weight: root.weightStrong
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: 46
          text: audioLevelRow.shown + "%"
          color: root.subtext
          font.family: root.fontFamily
          font.pixelSize: root.textBody
          horizontalAlignment: Text.AlignRight
        }
      }

      MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        function valueAt(x) { return Math.max(0, Math.min(root.audioTrackMaximum, Math.round(x / width * root.audioTrackMaximum))) }
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
      color: audioMuteMouse.pressed ? root.pressColor : audioLevelRow.muted ? root.dangerColor : audioMuteMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
      Behavior on color { ColorAnimation { duration: root.durationFast } }

      Text {
        anchors.centerIn: parent
        text: audioLevelRow.microphone ? (audioLevelRow.muted ? "󰍭" : "󰍬") : (audioLevelRow.muted ? "󰝟" : "󰕾")
        color: audioLevelRow.muted ? root.red : root.text
        font.family: root.fontFamily
        font.pixelSize: root.textSubhead
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
        inOverlay: true
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
    readonly property bool muted: controlLevel.microphone ? !!root.systemData.microphoneMuted : !!root.systemData.muted
    readonly property real fillRatio: root.audioFillRatio(controlLevel.shown)

    radius: root.radius
    color: root.wellColor
    clip: true

    Rectangle {
      width: parent.width * controlLevel.fillRatio
      height: parent.height
      radius: parent.radius
      color: controlLevel.muted ? root.fillDanger : root.fillColor
    }

    Text {
      anchors.right: parent.right
      anchors.rightMargin: 10
      anchors.verticalCenter: parent.verticalCenter
      text: controlLevel.shown + "%"
      color: root.text
      font.family: root.fontFamily
      font.pixelSize: root.textLabel
      font.weight: root.weightStrong
    }

    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      function valueAt(x) { return Math.max(0, Math.min(root.audioTrackMaximum, Math.round(x / width * root.audioTrackMaximum))) }
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
      color: controlLevelMuteMouse.pressed ? root.pressColor : controlLevel.muted ? root.dangerColor : controlLevelMuteMouse.containsMouse ? root.hoveredColor(root.alpha(root.crust, 0.7)) : root.alpha(root.crust, 0.7)
      Behavior on color { ColorAnimation { duration: root.durationFast } }

      Text {
        anchors.centerIn: parent
        text: controlLevel.microphone ? (controlLevel.muted ? "󰍭" : "󰍬") : (controlLevel.muted ? "󰝟" : "󰕾")
        color: controlLevel.muted ? root.red : root.text
        font.family: root.fontFamily
        font.pixelSize: root.textSubhead
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
        inOverlay: true
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
      color: connectivityKnobMouse.pressed ? root.pressColor : connectivityRow.active ? root.accent : root.wellColor
      border.width: connectivityRow.active ? 0 : 1
      border.color: root.edgeLight
      antialiasing: true
      Behavior on color { ColorAnimation { duration: root.durationFast } }

      HoverWash { hovered: connectivityKnobMouse.containsMouse }

      Text {
        visible: !connectivityRow.busy
        anchors.centerIn: parent
        text: connectivityRow.icon
        color: connectivityRow.active ? root.crust : root.text
        font.family: root.fontFamily
        font.pixelSize: root.textSubhead
      }

      RefreshGlyph {
        visible: connectivityRow.busy
        anchors.centerIn: parent
        width: 16
        height: 16
        spinning: visible
        color: connectivityRow.active ? root.crust : root.text
        font.pixelSize: root.textBody
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
      // Full row height. At 38 in a 40px row the highlight left a dead line
      // above and below itself, which the 4px between rows widened into a 6px
      // band the pointer crossed on its way from one row to the next — the
      // highlight blinking out between two rows that look adjacent.
      anchors.left: connectivityKnob.right
      anchors.leftMargin: 4
      anchors.right: parent.right
      anchors.top: parent.top
      anchors.bottom: parent.bottom
      radius: root.radius
      color: connectivityLabelMouse.pressed ? root.pressColor : connectivityLabelMouse.containsMouse ? root.hoverColor : root.clearColor
      Behavior on color { ColorAnimation { duration: root.durationFast } }

      Column {
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: parent.left
        anchors.leftMargin: 8
        anchors.right: parent.right
        anchors.rightMargin: 22
        spacing: 1

        Text { width: parent.width; text: connectivityRow.label; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
        Text { width: parent.width; text: connectivityRow.detail; elide: Text.ElideRight; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
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
        font.pixelSize: root.textStrong
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
    readonly property bool hovered: controlTileHover.hovered
    color: controlTileMouse.pressed ? root.pressColor : controlTile.hovered ? root.hoveredColor(controlTile.active ? root.activeTint : root.cardColor) : controlTile.active ? root.activeTint : root.cardColor
    Behavior on color { ColorAnimation { duration: root.durationFast } }

    CardEdge {}

    HoverHandler { id: controlTileHover }

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

        Text { width: parent.width; text: controlTile.label; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: controlTile.compact ? root.textLabel : root.textBody; font.weight: root.weightStrong }
        Text { visible: !controlTile.compact; width: parent.width; text: controlTile.detail; elide: Text.ElideRight; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
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
    readonly property real gap: root.spaceMedium
    readonly property real cellSize: (width - gap * 3) / 4
    readonly property real mediaSize: cellSize * 2 + gap
    readonly property real mediaHeight: root.mediaBodyHeight
    readonly property real controlSpacing: root.spaceTight
    readonly property real audioPadding: root.spaceLarge
    readonly property real audioSliderHeight: 46
    readonly property real controlsHeight: audioSliderHeight * 2 + audioPadding * 3
    readonly property real smallTileHeight: 55
    readonly property real controlsY: mediaHeight + gap
    readonly property real devicesY: controlsY + controlsHeight + gap

    height: devicesY + smallTileHeight

    Rectangle {
      id: controlCenterMedia

      readonly property var player: root.nowPlayingPlayer()
      // Every other module in the Control Center reports the pointer; this one
      // is a module too — it drags to the menu bar and opens the media panel —
      // and said nothing, so the largest card in the panel stayed dark while a
      // transport button lit under the pointer crossing it. The transport, the
      // art and the timeline are all hover areas over this fill, so the card
      // asks a handler rather than the drag area beneath them.
      readonly property bool hovered: controlCenterMediaHover.hovered
      width: parent.width
      height: controlGrid.mediaHeight
      radius: root.radius
      color: controlCenterMediaMouse.pressed ? root.pressColor
        : controlCenterMedia.hovered ? root.hoveredColor(root.cardColor)
        : root.cardColor
      opacity: root.dragModule === "media" ? 0.45 : 1

      Behavior on color { ColorAnimation { duration: root.durationFast } }

      HoverHandler { id: controlCenterMediaHover }

      CardEdge {}

      ModuleDragArea {
        id: controlCenterMediaMouse
        module: "media"
        cursorShape: Qt.ArrowCursor
        onActivated: root.toggleMedia(controlCenterMedia.player, controlGrid.screenName)
      }

      MediaBody {
        anchors.fill: parent
        player: controlCenterMedia.player
      }
    }

    Rectangle {
      y: controlGrid.controlsY
      width: controlGrid.mediaSize
      height: controlGrid.controlsHeight
      radius: root.radius
      color: root.cardColor

      CardEdge {}

      Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        spacing: controlGrid.controlSpacing

        ConnectivityRow {
          width: parent.width
          height: root.rowHeight
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
          height: root.rowHeight
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
          height: root.rowHeight
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
      id: controlCenterAudio

      // The two level tracks cover all of this card but its padding, and each
      // is a hover area of its own, so the card asks a handler whether the
      // pointer is on it. Reading the drag area underneath meant the tint was
      // lit only in the margins around the sliders and went out across the
      // sliders themselves — the card blinking under a pointer crossing it.
      readonly property bool hovered: controlCenterAudioHover.hovered

      x: controlGrid.mediaSize + controlGrid.gap
      y: controlGrid.controlsY
      width: controlGrid.mediaSize
      height: controlGrid.controlsHeight
      radius: root.radius
      color: controlCenterAudioMouse.pressed ? root.pressColor : controlCenterAudio.hovered ? root.hoveredColor(root.cardColor) : root.cardColor
      Behavior on color { ColorAnimation { duration: root.durationFast } }
      opacity: root.dragModule === "audio" ? 0.45 : 1

      HoverHandler { id: controlCenterAudioHover }

      CardEdge {}

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
        font.pixelSize: root.textIcon
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
      label: root.headphonesLabel()
      detail: root.headphonesDetail()
      active: true
      glyph: HeadphonesIcon { kind: root.headphonesIconKind(); tint: root.accent }
      onActivated: root.toggleControl("airpods", controlGrid.screenName)
    }
  }

  component NotificationButton: Button {
    id: notificationButton
    required property string label
    property string controlAction: ""
    property string value: ""
    property string extra: ""
    property string successLabel: "Done"
    property string actionIcon: ""
    property bool localBusy: false
    property bool localFailed: false
    property bool localComplete: false
    readonly property bool busy: localBusy || (controlAction !== "" && root.controlBusy(controlAction, value, extra))
    readonly property bool failed: localFailed || (controlAction !== "" && root.controlFailed(controlAction, value, extra))
    readonly property bool complete: localComplete || (controlAction !== "" && root.controlCompleted(controlAction, value, extra))
    implicitWidth: Math.max(buttonMeasure.width, feedbackMeasure.width) + root.spaceMedium * 2 + (actionIcon ? root.textLabel + root.spaceTight : 0)
    width: Math.min(implicitWidth, parent.width)
    implicitHeight: root.chipHeight
    TextMetrics {
      id: feedbackMeasure
      text: notificationButton.controlAction ? "Failed · retry" : ""
      font.family: root.fontFamily
      font.pixelSize: root.textCaption
    }
    TextMetrics {
      id: buttonMeasure
      text: notificationButton.label
      font.family: root.fontFamily
      font.pixelSize: root.textCaption
    }
    hoverEnabled: true
    activeFocusOnTab: true
    onClicked: if (controlAction !== "") root.runControl(controlAction, value, extra)
    contentItem: Text {
      id: buttonLabel
      leftPadding: notificationButton.actionIcon ? root.textLabel + root.spaceTight : 0
      IconImage {
        visible: notificationButton.actionIcon !== ""
        width: root.textLabel
        height: width
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        source: notificationButton.actionIcon ? Quickshell.iconPath(notificationButton.actionIcon) : ""
      }
      textFormat: Text.PlainText
      elide: Text.ElideRight
      text: notificationButton.busy ? "Working…" : notificationButton.failed ? "Failed · retry" : notificationButton.complete ? notificationButton.successLabel : notificationButton.label
      color: notificationButton.failed ? root.red : notificationButton.complete ? root.green : root.text
      font.family: root.fontFamily
      font.pixelSize: root.textCaption
      verticalAlignment: Text.AlignVCenter
      horizontalAlignment: Text.AlignHCenter
    }
    background: Rectangle {
      radius: root.radius
      color: notificationButton.down ? root.pressColor : notificationButton.hovered ? root.hoveredColor(root.wellColor) : root.wellColor
      border.width: notificationButton.activeFocus ? 1 : 0
      border.color: root.accent
      Behavior on color { ColorAnimation { duration: root.durationFast } }
    }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
  }

  component NotificationList: SeeleListView {
    id: notificationList

    property bool history: false
    property bool popup: false
    property string query: ""
    // Everywhere except a toast, a notification is shown in full without being
    // asked: the panel is where you go to read what you missed.
    readonly property bool alwaysUnfolded: !notificationList.popup

    property var expandedGroups: ({})
    readonly property var entries: NotificationSearch.filter(notificationList.history ? (root.systemData.notifications.history || [])
      : notificationList.popup ? root.notificationPopupEntries() : (root.systemData.notifications.items || []), query)
    function toggleGroup(key) {
      var expanded = Object.assign({}, expandedGroups)
      if (expanded[key]) delete expanded[key]
      else expanded[key] = true
      expandedGroups = expanded
    }
    onEntriesChanged: {
      var present = {}, expanded = {}
      for (var i = 0; i < entries.length; i++) present[Notifications.groupKey(entries[i])] = true
      for (var key in expandedGroups) if (present[key]) expanded[key] = true
      expandedGroups = expanded
    }
    spacing: root.spaceMedium
    footer: Item { height: root.spaceSmall }
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    model: Notifications.stackedRows(entries, expandedGroups)

    delegate: Rectangle {
      id: notificationEntry

      required property var modelData
      readonly property var entry: modelData.entry
      readonly property bool actionable: !notificationList.history && root.notificationActionable(entry)
      readonly property var offeredActions: notificationList.history ? [] : Notifications.actions(entry)
      readonly property string verificationCode: Notifications.verificationCode(entry)
      readonly property bool unfolded: notificationList.alwaysUnfolded || !!root.notificationUnfolded[String(entry.id)]
      // A single elided line reports its full width, which is the only way to
      // know there is more to show without measuring the text twice.
      readonly property bool truncated: notificationBody.implicitWidth > notificationBody.width
      // Only a toast folds, and only when there is something folded away.
      readonly property bool unfoldable: !notificationList.alwaysUnfolded && (truncated || unfolded || !!entry.image)
      readonly property string iconSource: {
        var icon = String(entry.app_icon || "").trim()
        return Notifications.localImage(icon) || (icon && icon.indexOf("://") < 0 ? Quickshell.iconPath(icon) : "")
      }
      x: modelData.first ? 0 : root.spaceMedium
      width: ListView.view.width - x
      height: Math.max(root.notificationRowHeight, notificationText.implicitHeight + (notificationEntry.unfolded ? 18 : 12))
      radius: root.radius
      // Asked of the card rather than of the pointer area covering it. The
      // unfold and dismiss buttons sit on top of that area with hover enabled
      // of their own, and a hovered child takes the event away from the parent
      // below it, so a fill reading `containsMouse` fell back to `cardColor`
      // the moment the pointer reached a button and lit again when it left. A
      // handler on the card is hovered for the whole card, buttons included.
      readonly property bool hovered: notificationHover.hovered

      color: notificationEntry.actionable && notificationOpenMouse.pressed ? root.pressColor
        : notificationEntry.actionable && notificationEntry.hovered ? root.hoveredColor(root.cardColor)
        : root.cardColor

      Behavior on color { ColorAnimation { duration: root.durationFast } }

      HoverHandler {
        id: notificationHover
        onHoveredChanged: if (notificationList.popup) root.setNotificationPopupHovered(hovered)
      }

      Repeater {
        model: notificationEntry.modelData.depth
        delegate: Rectangle {
          required property int index
          z: -1 - index
          x: root.spaceTight * (index + 1)
          y: parent.height - root.spaceSmall + root.spaceTight * (index + 1)
          width: parent.width - x * 2
          height: root.spaceSmall
          radius: root.radius
          color: root.cardColor
          CardEdge {}
        }
      }
      SurfaceWash { radius: root.radius - 1 }
      CardEdge {}
      SurfaceGrain { inset: root.radius * (1 - 1 / Math.sqrt(2)) }
      Component.onDestruction: if (notificationList.popup && notificationHover.hovered) root.setNotificationPopupHovered(false)

      Item {
        id: notificationIconFrame
        width: 34
        height: 34
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.leftMargin: 9
        anchors.topMargin: 9

        Text {
          anchors.fill: parent
          horizontalAlignment: Text.AlignHCenter
          verticalAlignment: Text.AlignVCenter
          text: "󰂚"
          color: root.accent
          font.family: root.fontFamily
          font.pixelSize: root.textSubhead
        }
        IconImage {
          anchors.fill: parent
          source: notificationEntry.iconSource
        }
      }

      MouseArea {
        id: notificationOpenMouse
        anchors.fill: parent
        enabled: notificationEntry.actionable
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.activateNotification(notificationEntry.entry.id)
      }

      Column {
        id: notificationText
        anchors.left: notificationIconFrame.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: root.spaceMedium
        anchors.rightMargin: 9
        anchors.topMargin: 9
        spacing: root.spaceTight
        Flow {
          visible: notificationEntry.modelData.first && notificationEntry.modelData.count > 1
          width: parent.width
          spacing: root.spaceTight
          NotificationButton {
            label: (notificationEntry.entry.app_name || "Notifications") + " · " + notificationEntry.modelData.count
              + (notificationEntry.modelData.expanded ? " · Show less" : " · Show all")
            onClicked: notificationList.toggleGroup(notificationEntry.modelData.group)
          }
          NotificationButton {
            visible: !notificationList.history && notificationList.query.trim() === ""
            label: notificationList.popup ? "Hide stack" : "Dismiss stack"
            onClicked: notificationStore.controller.group(notificationEntry.modelData.group, notificationList.popup)
          }
        }
        Row {
          width: parent.width
          height: 20
          Text { width: parent.width - (notificationEntry.unfoldable ? 104 : 90); height: parent.height; text: entry.summary || entry.app_name || "Notification"; textFormat: Text.PlainText; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong; verticalAlignment: Text.AlignVCenter }
          Item { width: 6; height: parent.height }
          Text { width: 58; height: parent.height; text: root.agoText(entry.time); color: root.overlay; font.family: root.fontFamily; font.pixelSize: root.textCaption; horizontalAlignment: Text.AlignRight; verticalAlignment: Text.AlignVCenter }
          Rectangle {
            visible: notificationEntry.unfoldable
            width: visible ? 20 : 0
            height: parent.height
            radius: root.radiusSmall
            color: notificationUnfoldMouse.pressed ? root.pressColor : notificationUnfoldMouse.containsMouse ? root.hoverColor : root.clearColor
            Behavior on color { ColorAnimation { duration: root.durationFast } }
            Text {
              anchors.centerIn: parent
              text: notificationEntry.unfolded ? "󰅃" : "󰅀"
              color: notificationUnfoldMouse.containsMouse ? root.accent : root.subtext
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
            }
            MouseArea {
              id: notificationUnfoldMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.toggleNotificationUnfolded(notificationEntry.entry.id)
            }
            HoverTip { mouse: notificationUnfoldMouse; inOverlay: true; text: notificationEntry.unfolded ? "Show less" : "Show the whole notification" }
          }
          Item { visible: !notificationEntry.unfoldable; width: visible ? 6 : 0; height: parent.height }
          Rectangle {
            readonly property bool busy: !notificationList.popup && root.controlBusy("notifications", "dismiss", String(notificationEntry.entry.id))
            visible: !notificationList.history
            width: visible ? 20 : 0
            height: parent.height
            radius: root.radiusSmall
            color: notificationDismissMouse.pressed ? root.dangerPress : busy ? root.selectedColor : notificationDismissMouse.containsMouse ? root.dangerColor : root.clearDanger
            Behavior on color { ColorAnimation { duration: root.durationFast } }
            Text { visible: !parent.busy; anchors.centerIn: parent; text: "󰅖"; color: notificationDismissMouse.containsMouse ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textLabel }
            RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 14; height: 14; spinning: visible; font.pixelSize: root.textLabel }
            MouseArea {
              id: notificationDismissMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: notificationList.popup ? root.retireNotificationPopup(notificationEntry.entry.id) : root.dismissNotification(notificationEntry.entry.id)
            }
            HoverTip { mouse: notificationDismissMouse; inOverlay: true; text: notificationList.popup ? "Hide toast" : "Dismiss" }
          }
        }
        Text {
          id: notificationBody
          width: parent.width
          text: Notifications.bodyMarkup(entry.body || entry.app_name || "")
          textFormat: Text.StyledText
          linkColor: root.accent
          onLinkActivated: link => { if (/^(https?:\/\/|mailto:)/i.test(link)) Qt.openUrlExternally(link) }
          HoverHandler { cursorShape: notificationBody.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
          color: root.subtext
          font.family: root.fontFamily
          font.pixelSize: root.textCaption
          wrapMode: notificationEntry.unfolded ? Text.WordWrap : Text.NoWrap
          elide: notificationEntry.unfolded ? Text.ElideNone : Text.ElideRight
          // Bounded, so one pathological notification cannot take the panel.
          maximumLineCount: notificationEntry.unfolded ? 1000 : 1
        }
        Item {
          visible: !!notificationEntry.entry.image && notificationEntry.unfolded
          width: parent.width
          height: visible ? Math.min(notificationImage.implicitHeight || root.rowHeight * 3, root.rowHeight * 3) : 0
          Image {
            id: notificationImage
            anchors.fill: parent
            visible: false
            source: notificationEntry.entry.image || ""
            sourceSize.width: width * 2
            fillMode: Image.PreserveAspectFit
            asynchronous: true
          }
          RoundedSource { anchors.fill: parent; source: notificationImage }
        }
        Text {
          visible: notificationEntry.entry.urgency === 2 || Notifications.permanent(notificationEntry.entry) || notificationEntry.entry.resident
          text: notificationEntry.entry.urgency === 2 ? "Critical · until dismissed"
            : Notifications.permanent(notificationEntry.entry) ? "Until dismissed" : "Ongoing"
          color: notificationEntry.entry.urgency === 2 ? root.red : root.subtext
          font.family: root.fontFamily
          font.pixelSize: root.textMicro
        }
        Row {
          visible: Number(notificationEntry.entry.progress) >= 0
          width: parent.width
          spacing: root.spaceSmall
          MeterBar {
            width: parent.width - progressLabel.width - parent.spacing
            anchors.verticalCenter: parent.verticalCenter
            ratio: Number(notificationEntry.entry.progress) / 100
          }
          Text {
            id: progressLabel
            text: Math.round(Number(notificationEntry.entry.progress)) + "%"
            color: root.subtext
            font.family: root.fontFamily
            font.pixelSize: root.textCaption
          }
        }
        Flow {
          width: parent.width
          spacing: root.spaceTight
          Repeater {
            model: notificationEntry.offeredActions
            delegate: NotificationButton {
              required property var modelData
              label: modelData.label
              controlAction: "notification-action"
              value: String(notificationEntry.entry.id)
              extra: modelData.key
              actionIcon: notificationEntry.entry.action_icons ? modelData.key : ""
            }
          }
          NotificationButton {
            visible: !notificationList.history && (notificationEntry.entry.pinned || !Notifications.permanent(notificationEntry.entry))
            label: notificationEntry.entry.pinned ? "Unpin" : "Keep visible"
            onClicked: notificationStore.controller.pin(notificationEntry.entry.id)
          }
          Repeater {
            model: notificationEntry.unfolded ? ["title", "body"] : []
            delegate: NotificationButton {
              id: notificationCopyButton
              required property string modelData
              readonly property string copyKey: String(notificationEntry.entry.id) + ":" + modelData
              readonly property bool selected: notificationClipboard.key === copyKey
              visible: modelData === "title" ? !!notificationEntry.entry.summary : !!notificationEntry.entry.body
              label: modelData === "title" ? "Copy title" : "Copy message"
              enabled: !notificationClipboard.pending
              localBusy: selected && notificationClipboard.pending
              localFailed: selected && notificationClipboard.status === "error"
              localComplete: selected && notificationClipboard.status === "success"
              successLabel: "Copied"
              onClicked: notificationClipboard.copy(notificationEntry.entry, modelData)
              HoverTip {
                mouse: notificationCopyButton
                inOverlay: true
                text: notificationCopyButton.selected ? notificationClipboard.message : ""
              }
            }
          }
          NotificationButton {
            visible: notificationEntry.verificationCode !== ""
            label: "Copy " + notificationEntry.verificationCode
            controlAction: "copy-code"
            value: notificationEntry.verificationCode
            extra: String(notificationEntry.entry.id)
            successLabel: "Copied"
          }
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
      onVisibleChanged: if (!visible && root.pinnedScreen(root.notificationPopupScreen, modelData)) {
        root.notificationPopupHoverCount = 0
        notificationStore.controller.pause(false, Date.now() / 1000)
      }
      screen: modelData
      visible: !uriPicker.presented && !root.systemData.dnd
        && root.controlPanel !== "notifications"
        && entries.length > 0
        && root.pinnedScreen(root.notificationPopupScreen, modelData)
      anchors { top: true; right: true }
      margins { top: root.barHeight + root.panelGap; right: root.panelGap }
      implicitWidth: 400
      // Rows size themselves to whatever is unfolded, so the surface follows
      // the list's own content rather than a fixed row height.
      implicitHeight: Math.min(430, Math.max(root.notificationRowHeight, notificationPopupList.contentHeight))
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-notifications"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

      NotificationList {
        id: notificationPopupList
        popup: true
        anchors.fill: parent
        ScrollBar.vertical: SlimScrollBar { popupHovered: root.notificationPopupHoverCount > 0 }
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
      font.pixelSize: root.textLead
    }

    Text {
      anchors.centerIn: parent
      visible: barMediaArt.paused
      text: "󰏤"
      color: barMediaArt.hasArt ? root.text : root.mutedText
      font.family: root.fontFamily
      font.pixelSize: barMediaArt.hasArt ? root.textBody : root.textLead
    }
  }

  component MediaButton: Rectangle {
    id: mediaButton

    property string icon: ""
    property bool primary: false
    property bool flat: false
    property bool active: false
    property string hint: ""
    signal activated()

    width: mediaButton.flat ? (mediaButton.primary ? 38 : 34) : mediaButton.primary ? 34 : 28
    height: mediaButton.flat ? 32 : 28
    radius: root.radius
    opacity: mediaButton.enabled ? 1 : 0.35
    activeFocusOnTab: enabled
    Keys.onPressed: event => {
      if (event.isAutoRepeat || (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return
      if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
        mediaButton.activated()
        event.accepted = true
      }
    }
    color: mediaButtonMouse.pressed ? root.pressColor
      : mediaButtonMouse.containsMouse
        ? mediaButton.flat
          ? root.hoverColor
          : root.hoveredColor(mediaButton.primary ? root.alpha(root.accent, 0.22) : root.cardColor)
        : mediaButton.flat ? root.clearColor : mediaButton.primary ? root.alpha(root.accent, 0.22) : root.cardColor
    Behavior on color { ColorAnimation { duration: root.durationFast } }

    Text {
      anchors.centerIn: parent
      text: mediaButton.icon
      color: mediaButton.active ? root.accent : root.text
      font.family: root.fontFamily
      font.pixelSize: mediaButton.flat
        ? (mediaButton.primary ? root.textDisplay : root.textTitle)
        : mediaButton.primary ? root.textSubhead : root.textLead
    }

    HoverHandler { id: mediaButtonHover }
    MouseArea { id: mediaButtonMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: mediaButton.activated() }
    HoverTip { mouse: mediaButtonMouse; inOverlay: true; text: mediaButton.hint }
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
    activeFocusOnTab: available && !live
    Keys.onPressed: event => root.seekMediaKey(player, event)
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
        color: root.wellColor
        border.width: 1
        border.color: mediaTimeline.activeFocus ? root.accent : root.alpha(root.text, 0.05)
        antialiasing: true

        Rectangle {
          width: parent.width * (mediaTimeline.length > 0 ? mediaTimeline.shownPosition / mediaTimeline.length : 0)
          height: parent.height
          radius: parent.radius
          antialiasing: true

          gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: root.alpha(root.accent, 0.62) }
            GradientStop { position: 1.0; color: root.accent }
          }
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
        onPressed: function(mouse) {
          mediaTimeline.forceActiveFocus()
          mediaTimeline.draggedPosition = positionAt(mouse.x)
        }
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
      HoverTip { mouse: timelineMouse; inOverlay: true; text: "Seek · Left/Right 5s · Shift 30s · Home/End" }

      Text {
        id: liveTimelineLabel
        visible: mediaTimeline.live
        anchors.centerIn: parent
        text: "LIVE"
        color: root.overlay
        font.family: root.fontFamily
        font.pixelSize: root.textLabel
        font.weight: root.weightMedium
        font.letterSpacing: root.trackingLabel
      }

      Rectangle {
        visible: mediaTimeline.live
        anchors.left: parent.left
        anchors.right: liveTimelineLabel.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        height: 5
        radius: height / 2
        color: root.alpha(root.text, 0.24)
      }

      Rectangle {
        visible: mediaTimeline.live
        anchors.left: liveTimelineLabel.right
        anchors.leftMargin: 8
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 5
        radius: height / 2
        color: root.alpha(root.text, 0.24)
      }
    }

    Row {
      visible: !mediaTimeline.live
      width: parent.width
      Text { width: parent.width / 2; text: root.formatMediaTime(mediaTimeline.shownPosition); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
      Text { width: parent.width / 2; text: root.formatMediaTime(mediaTimeline.length); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption; horizontalAlignment: Text.AlignRight }
    }

    Timer {
      interval: 1000
      repeat: true
      running: mediaTimeline.visible && !mediaTimeline.live && !!mediaTimeline.player && mediaTimeline.player.isPlaying && mediaTimeline.draggedPosition < 0
      onTriggered: mediaTimeline.tick++
    }
  }

  // The media block — art, title, transport, timeline — as one object. The
  // Control Center module and the Now Playing panel it opens used to arrange
  // the same parts differently: different art size and corner, a different type
  // ramp, a filled transport against a flat one, and a timeline that ran the
  // full width under the art here and beside it there. They draw this instead,
  // so the module and the panel are the same presentation at the same size.
  component MediaPlayerPicker: ComboBox {
    id: mediaPlayerPicker

    property var player: null

    implicitWidth: 148
    implicitHeight: root.chipHeight
    displayText: root.mediaPlayerName(mediaPlayerPicker.player)
    currentIndex: {
      for (var i = 0; i < mediaPlayerPicker.model.length; i++) {
        if (mediaPlayerPicker.model[i] === mediaPlayerPicker.player) return i
      }
      return -1
    }
    leftPadding: root.spaceMedium
    rightPadding: root.chipHeight

    contentItem: Text {
      text: mediaPlayerPicker.displayText
      color: root.text
      elide: Text.ElideRight
      verticalAlignment: Text.AlignVCenter
      font.family: root.fontFamily
      font.pixelSize: root.textLabel
      font.weight: root.weightStrong
    }

    indicator: CenteredGlyph {
      x: mediaPlayerPicker.width - width
      width: root.chipHeight
      height: mediaPlayerPicker.height
      text: mediaPlayerPicker.popup.visible ? "󰅀" : "󰅂"
      color: root.subtext
      font.family: root.fontFamily
      font.pixelSize: root.textIcon
    }

    background: Rectangle {
      radius: root.radius
      color: mediaPlayerPicker.pressed ? root.pressColor
        : mediaPlayerPicker.hovered ? root.hoveredColor(root.wellColor)
        : root.wellColor
      border.width: 1
      border.color: mediaPlayerPicker.popup.visible ? root.alpha(root.accent, 0.55) : root.cardBorder

      Behavior on color { ColorAnimation { duration: root.durationFast } }
    }

    delegate: ItemDelegate {
      id: mediaPlayerChoice

      required property int index
      required property var modelData

      width: mediaPlayerPicker.popup.width - mediaPlayerPicker.popup.leftPadding - mediaPlayerPicker.popup.rightPadding
      height: root.controlHeight
      leftPadding: root.spaceMedium
      rightPadding: root.spaceMedium
      highlighted: mediaPlayerPicker.highlightedIndex === index

      contentItem: Column {
        anchors.verticalCenter: parent.verticalCenter
        spacing: 1

        Text {
          width: parent.width
          text: root.mediaPlayerName(mediaPlayerChoice.modelData)
          color: mediaPlayerChoice.modelData === mediaPlayerPicker.player ? root.accent : root.text
          elide: Text.ElideRight
          font.family: root.fontFamily
          font.pixelSize: root.textBody
          font.weight: root.weightStrong
        }
        Text {
          width: parent.width
          text: root.mediaTitle(mediaPlayerChoice.modelData) || root.mediaSubtitle(mediaPlayerChoice.modelData)
          color: root.subtext
          elide: Text.ElideRight
          font.family: root.fontFamily
          font.pixelSize: root.textCaption
        }
      }

      background: Rectangle {
        radius: root.radiusSmall
        color: mediaPlayerChoice.pressed ? root.pressColor
          : mediaPlayerChoice.modelData === mediaPlayerPicker.player ? root.selectedColor
          : mediaPlayerChoice.hovered || mediaPlayerChoice.highlighted ? root.hoverColor
          : root.clearColor

        Behavior on color { ColorAnimation { duration: root.durationFast } }
      }
    }

    popup: Popup {
      x: mediaPlayerPicker.width - width
      y: mediaPlayerPicker.height + root.spaceTight
      width: 220
      implicitHeight: Math.min(contentItem.implicitHeight + topPadding + bottomPadding, root.controlHeight * 5 + topPadding + bottomPadding)
      topPadding: root.spaceTight
      bottomPadding: root.spaceTight
      leftPadding: root.spaceTight
      rightPadding: root.spaceTight

      contentItem: SeeleListView {
        implicitHeight: contentHeight
        clip: true
        model: mediaPlayerPicker.popup.visible ? mediaPlayerPicker.delegateModel : null
        currentIndex: mediaPlayerPicker.highlightedIndex
        ScrollBar.vertical: SlimScrollBar { popupHovered: mediaPlayerPicker.popup.visible }
      }

      background: Rectangle {
        radius: root.radius
        color: root.floatColor
        border.width: 1
        border.color: root.panelBorder

        SurfaceEdge { radius: parent.radius }
        SurfaceGrain { inset: 3 }
      }
    }

    onActivated: function(index) { root.selectMediaPlayer(mediaPlayerPicker.model[index]) }
  }

  component MediaBody: Item {
    id: mediaBody

    property var player: null

    implicitHeight: root.mediaBodyHeight

    Item {
      id: mediaBodyArtFrame

      anchors.top: parent.top
      anchors.bottom: parent.bottom
      anchors.left: parent.left
      anchors.margins: 12
      width: height

      Image {
        id: mediaBodyArt

        anchors.fill: parent
        visible: false
        source: mediaBody.player ? String(mediaBody.player.trackArtUrl || "") : ""
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
        source: mediaBodyArt
        radius: root.radius
        visible: mediaBodyArt.status === Image.Ready
      }

      Rectangle {
        anchors.fill: parent
        visible: mediaBodyArt.status !== Image.Ready
        radius: root.radius
        color: root.wellColor

        Text {
          anchors.centerIn: parent
          text: "󰎆"
          color: mediaBody.player ? root.accent : root.overlay
          font.family: root.fontFamily
          font.pixelSize: Math.round(parent.height * 0.26)
        }
      }
    }

    Item {
      anchors.top: parent.top
      anchors.bottom: parent.bottom
      anchors.left: mediaBodyArtFrame.right
      anchors.right: parent.right
      anchors.topMargin: 12
      anchors.bottomMargin: 9
      anchors.leftMargin: 12
      anchors.rightMargin: 12

      Text {
        id: mediaBodyTitle

        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        text: mediaBody.player ? (root.mediaTitle(mediaBody.player) || "Unknown track") : "Nothing playing"
        elide: Text.ElideRight
        color: root.text
        font.family: root.fontFamily
        font.pixelSize: root.textStrong
        font.weight: root.weightStrong
      }

      Text {
        visible: text !== ""
        anchors.top: mediaBodyTitle.bottom
        anchors.topMargin: 2
        anchors.left: parent.left
        anchors.right: parent.right
        text: mediaBody.player ? root.mediaSubtitle(mediaBody.player) : "Start a track to see it here"
        elide: Text.ElideRight
        color: root.subtext
        font.family: root.fontFamily
        font.pixelSize: root.textCaption
      }

      Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 4
        spacing: root.spaceTight

        MediaButton {
          flat: true
          width: root.controlHeight
          icon: "󰒟"
          hint: !mediaBody.player || !mediaBody.player.shuffleSupported ? "Shuffle unavailable" : mediaBody.player.shuffle ? "Shuffle on" : "Shuffle off"
          active: !!mediaBody.player && mediaBody.player.shuffleSupported && mediaBody.player.shuffle
          enabled: Media.canShuffle(mediaBody.player)
          onActivated: Media.toggleShuffle(mediaBody.player)
        }
        MediaButton {
          flat: true
          icon: "󰒮"
          hint: "Previous track"
          enabled: !!mediaBody.player && mediaBody.player.canGoPrevious
          onActivated: mediaBody.player.previous()
        }
        MediaButton {
          flat: true
          icon: mediaBody.player && mediaBody.player.isPlaying ? "󰏤" : "󰐊"
          primary: true
          hint: mediaBody.player && mediaBody.player.isPlaying ? "Pause" : "Play"
          enabled: !!mediaBody.player && mediaBody.player.canTogglePlaying
          onActivated: mediaBody.player.togglePlaying()
        }
        MediaButton {
          flat: true
          icon: "󰒭"
          hint: "Next track"
          enabled: !!mediaBody.player && mediaBody.player.canGoNext
          onActivated: mediaBody.player.next()
        }
        MediaButton {
          flat: true
          width: root.controlHeight
          icon: mediaBody.player && mediaBody.player.loopState === MprisLoopState.Track ? "󰑘" : "󰑖"
          hint: Media.repeatLabel(mediaBody.player, MprisLoopState)
          active: !!mediaBody.player && mediaBody.player.loopSupported && mediaBody.player.loopState !== MprisLoopState.None
          enabled: Media.canRepeat(mediaBody.player)
          onActivated: Media.cycleRepeat(mediaBody.player, MprisLoopState)
        }
      }

      MediaTimeline {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        player: mediaBody.player
      }
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

  // Frozen URI picker --------------------------------------------------------
  UriPicker { id: uriPicker }

  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: uriWindow
      required property var modelData
      readonly property var frame: uriPicker.frame(modelData.name)
      readonly property real badgeWidth: Math.max(root.chipHeight, uriNumberMetrics.advanceWidth + root.spaceSmall * 2)
      readonly property real badgeHeight: root.textStrong + root.spaceSmall * 2
      readonly property var positions: Uris.layout(uriPicker.allLinks, modelData.name,
        width, height, badgeWidth, badgeHeight, root.spaceTight)
      screen: modelData
      anchors { top: true; bottom: true; left: true; right: true }
      exclusionMode: ExclusionMode.Ignore
      visible: uriPicker.active && uriPicker.presented
      onVisibleChanged: if (visible) uriKeys.forceActiveFocus()
      color: root.crust
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-uris"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

      TextMetrics {
        id: uriNumberMetrics
        font.family: root.fontFamily
        font.pixelSize: root.textStrong
        font.weight: root.weightStrong
        text: String(Math.max(1, uriPicker.allLinks.length))
      }

      Image {
        anchors.fill: parent
        source: uriWindow.frame ? "file://" + uriWindow.frame.path.split("/").map(encodeURIComponent).join("/") : ""
        asynchronous: true
        cache: false
        smooth: false
        fillMode: Image.Stretch
        onStatusChanged: {
          if (status === Image.Ready) uriPicker.imageReady(uriWindow.modelData.name)
          else if (status === Image.Error && uriPicker.active) uriPicker.fail("Could not display the capture")
        }
      }

      Item {
        id: uriKeys
        anchors.fill: parent
        focus: true
        Keys.onPressed: event => uriPicker.key(event)
      }

      Repeater {
        model: uriPicker.links
        delegate: Item {
          id: uriHint
          required property int number
          required property string uri
          required property string text
          required property bool code
          required property string output
          required property real x0
          required property real y0
          required property real w
          required property real h
          readonly property bool matching: !uriPicker.digits || String(number).indexOf(uriPicker.digits) === 0
          readonly property var position: uriWindow.positions[number] || ({ x: 0, y: 0 })
          visible: output === uriWindow.modelData.name
          opacity: matching ? 1 : 0.25
          x: x0 * uriWindow.width
          y: y0 * uriWindow.height
          width: w * uriWindow.width
          height: h * uriWindow.height
          z: 2

          Rectangle {
            anchors.fill: parent
            anchors.margins: -1
            radius: root.radiusSmall
            color: uriLinkMouse.pressed ? root.pressColor : root.activeTint
            border.width: 1
            border.color: root.accent
            Rectangle { anchors.fill: parent; radius: parent.radius; color: uriHover.hovered ? root.hoverColor : root.clearColor }
            HoverHandler { id: uriHover }
            MouseArea {
              id: uriLinkMouse
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: mouse => uriPicker.launch(uriHint, !!(mouse.modifiers & Qt.ControlModifier))
            }
          }

          Rectangle {
            x: uriHint.position.x - uriHint.x
            y: uriHint.position.y - uriHint.y
            width: uriWindow.badgeWidth
            height: uriWindow.badgeHeight
            radius: root.radiusSmall
            color: root.floatColor
            border.width: 1
            border.color: root.panelBorder
            SurfaceWash { radius: parent.radius - 1 }
            Rectangle {
              anchors.fill: parent
              radius: parent.radius
              color: uriNumberMouse.pressed ? root.pressColor : uriNumberHover.hovered ? root.hoverColor : root.clearColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
            }
            Text {
              anchors.centerIn: parent
              text: uriHint.number
              color: root.accent
              font.family: root.fontFamily
              font.pixelSize: root.textStrong
              font.weight: root.weightStrong
            }
            SurfaceEdge { radius: root.radiusSmall - 1 }
            SurfaceGrain { inset: root.radiusSmall / 3 }
            HoverHandler { id: uriNumberHover }
            MouseArea {
              id: uriNumberMouse
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: mouse => uriPicker.launch(uriHint, !!(mouse.modifiers & Qt.ControlModifier))
            }
          }

          Rectangle {
            id: codeCaption
            readonly property var placement: Uris.caption(
              { x: uriHint.x, y: uriHint.y, w: uriHint.width, h: uriHint.height },
              uriWindow.width, uriWindow.height,
              Math.min(codeText.implicitWidth + root.spaceSmall * 2, root.controlHeight * 12),
              codeText.implicitHeight + root.spaceSmall * 2, root.spaceTight)
            visible: uriHint.code
            x: placement.x - uriHint.x
            y: placement.y - uriHint.y
            width: placement.w
            height: placement.h
            radius: root.radiusSmall
            color: root.floatColor
            border.width: 1
            border.color: root.panelBorder
            clip: true
            SurfaceWash { radius: parent.radius - 1 }
            Rectangle {
              anchors.fill: parent
              radius: parent.radius
              color: codeMouse.pressed ? root.pressColor : codeHover.hovered ? root.hoverColor : root.clearColor
            }
            Text {
              id: codeText
              x: root.spaceSmall
              y: root.spaceSmall
              width: Math.max(0, codeCaption.width - root.spaceSmall * 2)
              text: uriHint.text
              textFormat: Text.PlainText
              wrapMode: Text.WrapAnywhere
              color: root.text
              font.family: root.fontFamily
              font.pixelSize: root.textBody
            }
            SurfaceEdge { radius: root.radiusSmall - 1 }
            SurfaceGrain { inset: root.radiusSmall / 3 }
            HoverHandler { id: codeHover }
            MouseArea {
              id: codeMouse
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: mouse => uriPicker.launch(uriHint, !!(mouse.modifiers & Qt.ControlModifier))
            }
          }

          onVisibleChanged: if (!visible && uriPicker.hoveredUri === text) uriPicker.hoveredUri = ""
          Connections {
            target: uriHover
            function onHoveredChanged() { uriPicker.hoveredUri = uriHover.hovered ? uriHint.text : "" }
          }
          Connections {
            target: uriNumberHover
            function onHoveredChanged() { uriPicker.hoveredUri = uriNumberHover.hovered ? uriHint.text : "" }
          }
          Connections {
            target: codeHover
            function onHoveredChanged() { uriPicker.hoveredUri = codeHover.hovered ? uriHint.text : "" }
          }
        }
      }

      Connections {
        target: uriWindow.modelData
        function onWidthChanged() { if (uriPicker.active) uriPicker.close() }
        function onHeightChanged() { if (uriPicker.active) uriPicker.close() }
      }
    }
  }

  // The status card has its own input-transparent surface so an empty scan
  // can release the frozen images and keyboard while its result remains visible.
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      anchors.bottom: true
      margins.bottom: root.panelMargin
      implicitWidth: Math.min(modelData.width - root.panelMargin * 2, root.controlHeight * 16)
      implicitHeight: uriStatus.implicitHeight + root.cardPadding * 2
      visible: uriPicker.presented || uriPicker.notice !== ""
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      mask: Region {}
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-uris-status"
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

      Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.panelColor
        border.width: 1
        border.color: root.panelBorder
        SurfaceWash { radius: root.radius - 1 }
        Column {
          id: uriStatus
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: root.cardPadding
          spacing: root.spaceSmall
          PanelHeader {
            width: parent.width
            glyph: "󰌷"
            title: "Screen links and codes"
            detail: uriPicker.detail
            detailColor: uriPicker.error !== "" ? root.red : root.subtext
            RefreshGlyph {
              anchors.verticalCenter: parent.verticalCenter
              width: root.chipHeight
              height: width
              visible: !uriPicker.complete
              spinning: visible
            }
          }
          Text {
            width: parent.width
            visible: uriPicker.hoveredUri !== ""
            text: uriPicker.hoveredUri
            textFormat: Text.PlainText
            elide: Text.ElideMiddle
            color: root.accent
            font.family: root.fontFamily
            font.pixelSize: root.textBody
          }
        }
        SurfaceEdge {}
        SurfaceGrain { inset: root.radius / 3 }
      }
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
        // The strip is the one surface that is always on screen, so it is the
        // darkest material in the shell and the quietest: ink the wallpaper
        // shows through, and it closes on a hairline rather than on a coloured
        // rule. Nothing lights its top edge, because there is no wallpaper
        // above the screen for that edge to be lit against.
        color: root.alpha(root.crust, 0.86)

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
              cursorShape: Qt.PointingHandCursor
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
                NumberAnimation { duration: root.durationNormal; easing.type: Easing.OutCubic }
              }

              Rectangle {
                id: workspacePill
                width: parent.active ? 42 : 20
                height: root.barItemHeight
                anchors.centerIn: parent
                radius: root.radius
                // Three steps, and only the top one is lit: the workspace in
                // front of the user is accent, one holding windows is a tint of
                // the strip's own light, and an empty one is barely there.
                color: parent.active ? root.accent
                  : workspaceMouse.containsMouse ? root.alpha(root.accent, 0.5)
                  : parent.occupied ? root.alpha(root.text, 0.16)
                  : root.alpha(root.text, 0.07)
                Behavior on color { ColorAnimation { duration: root.durationFast } }

                Behavior on width {
                  NumberAnimation { duration: root.durationNormal; easing.type: Easing.OutCubic }
                }

                Text {
                  anchors.centerIn: parent
                  text: String(parent.parent.modelData)
                  color: parent.parent.active ? root.crust
                    : workspaceMouse.containsMouse ? root.crust
                    : parent.parent.occupied ? root.text
                    : root.alpha(root.text, 0.45)
                  font.family: root.fontFamily
                  font.pixelSize: root.textLabel
                  font.weight: parent.parent.active ? root.weightStrong : root.weightRegular
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
            active: root.panelHere("application", barWindow.modelData) && root.applicationWindow === window
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
            MouseArea {
              id: activeWindowMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onPressed: root.toggleApplication(parent.window, barWindow.modelData.name, root.barItemCenter(parent))
            }
            HoverTip { mouse: activeWindowMouse; text: root.windowTitle(activeWindowMouse.parent.window) }
          }

          BarItem {
            visible: homeAssistantStore.configured
            width: visible ? root.chipHeight : 0
            hovered: homeAssistantMouse.containsMouse
            active: root.panelHere("home-assistant", barWindow.modelData)
            CenteredGlyph {
              anchors.centerIn: parent
              text: "󰋜"
              color: homeAssistantStore.connected ? root.subtext : root.yellow
              font.pixelSize: root.textStrong
            }
            MouseArea {
              id: homeAssistantMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.toggleControl("home-assistant", barWindow.modelData.name, root.barItemCenter(parent))
            }
            HoverTip { mouse: homeAssistantMouse; text: homeAssistantStore.connected ? "Home Assistant" : "Home Assistant · unavailable" }
          }
          BarItem {
            width: 30
            hovered: voxtypeMouse.containsMouse
            Text {
              anchors.centerIn: parent
              text: dictation.status === "recording" ? "󰍬" : dictation.status === "transcribing" ? "󰔟" : "󰍭"
              color: dictation.status === "recording" ? root.red : dictation.status === "transcribing" ? root.yellow : root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textIcon
            }
            MouseArea { id: voxtypeMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runControl("voxtype") }
            HoverTip { mouse: voxtypeMouse; text: "Voxtype: " + dictation.status }
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
              font.pixelSize: root.textStrong
              font.weight: root.weightStrong
            }
            MouseArea {
              id: clockMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              acceptedButtons: Qt.LeftButton | Qt.RightButton
              onClicked: mouse => root.toggleControl(mouse.button === Qt.RightButton ? "focus" : "clock", barWindow.modelData.name, root.barItemCenter(parent))
            }
            HoverTip { mouse: clockMouse; text: "Time zones · Right click for focus timer" }
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
              font.pixelSize: root.textStrong
            }
            MouseArea { id: dateMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("calendar", barWindow.modelData.name, root.barItemCenter(parent)) }
            HoverTip { mouse: dateMouse; text: "Calendar" }
          }

          BarItem {
            visible: focusTimer.timerState.status !== "idle"
            width: visible ? focusBarLabel.implicitWidth + 14 : 0
            hovered: focusBarMouse.containsMouse
            active: root.panelHere("focus", barWindow.modelData)
            Text {
              id: focusBarLabel
              anchors.centerIn: parent
              text: "󰔟 " + focusTimer.label
              color: focusTimer.timerState.status === "done" ? root.green : root.accent
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
            }
            MouseArea {
              id: focusBarMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.toggleControl("focus", barWindow.modelData.name, root.barItemCenter(parent))
            }
            HoverTip { mouse: focusBarMouse; text: "Focus timer · " + focusTimer.timerState.status }
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
                font.pixelSize: root.textStrong
              }
            }
            MouseArea {
              id: microphoneMutedIndicator
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
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
              cursorShape: Qt.PointingHandCursor
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
            MouseArea { id: cameraActiveIndicator; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("camera", barWindow.modelData.name, root.barItemCenter(parent)) }
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
            active: root.panelHere("media", barWindow.modelData) && root.nowPlayingPlayer() === player
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
            BarModuleArea { id: deviceMediaMouse; module: "media"; onActivated: root.toggleMedia(parent.player, barWindow.modelData.name, root.barItemCenter(parent)) }
            HoverTip { mouse: deviceMediaMouse; text: root.mediaLabel(deviceMediaItem.player) }
          }

          BarItem {
            id: spotifyMediaItem

            readonly property var player: root.spotifyPlayer()
            module: "media"
            visible: player !== null && root.barModulePinned("media")
            width: visible ? Math.min(210, spotifyMediaRow.implicitWidth + 14) : 0
            hovered: spotifyMediaMouse.containsMouse
            active: root.panelHere("media", barWindow.modelData) && root.nowPlayingPlayer() === player
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
            BarModuleArea { id: spotifyMediaMouse; module: "media"; onActivated: root.toggleMedia(parent.player, barWindow.modelData.name, root.barItemCenter(parent)) }
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
                cursorShape: Qt.PointingHandCursor
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                preventStealing: true
                function openContextMenu() {
                  root.openTrayItemMenu(parent.modelData, barWindow.modelData.name, root.barItemCenter(parent))
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
                font.pixelSize: root.textLead
              }
              MouseArea { id: trayExpandMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.trayPinned = !root.trayPinned }
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
            Text { anchors.centerIn: parent; text: root.systemData.cameraActive ? "󰄀" : "󰄁"; color: root.systemData.cameraActive ? root.red : root.text; font.family: root.fontFamily; font.pixelSize: root.textIcon }
            BarModuleArea { id: cameraMouse; module: "camera"; onActivated: root.toggleControl("camera", barWindow.modelData.name, root.barItemCenter(parent)) }
            HoverTip { mouse: cameraMouse; text: root.systemData.cameraActive ? "Camera in use" : "Camera" }
          }

          Repeater {
            model: root.activeAgents()
            BarItem {
              required property var modelData
              id: agentBadgeItem

              readonly property color stateColor: root.agentColor(modelData.status)
              readonly property string mark: root.agentMark(modelData.id)
              width: 28
              hovered: agentBadgeMouse.containsMouse
              Column {
                anchors.centerIn: parent
                spacing: 2
                AgentMark {
                  visible: agentBadgeItem.mark !== ""
                  anchors.horizontalCenter: parent.horizontalCenter
                  width: 13
                  height: 13
                  source: agentBadgeItem.mark
                }
                Text {
                  visible: agentBadgeItem.mark === ""
                  anchors.horizontalCenter: parent.horizontalCenter
                  text: root.agentBadge(modelData.id)
                  color: agentBadgeItem.stateColor
                  font.family: root.fontFamily
                  font.pixelSize: root.textLabel
                  font.weight: root.weightStrong
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
              MouseArea { id: agentBadgeMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleAgents(barWindow.modelData.name, root.barItemCenter(parent)) }
              // The mark is the only thing on the strip naming this session, so
              // the tip is what spells it out.
              HoverTip { mouse: agentBadgeMouse; text: modelData.name + " · " + root.agentStatusText(modelData.status) }
            }
          }


          BarItem {
            width: aiBarContent.implicitWidth + 14
            hovered: aiMouse.containsMouse
            active: root.agentsHere(barWindow.modelData)
            visible: root.agentData.launchers && root.agentData.launchers.length > 0
            // Each spent provider is its own mark and its own number, so the
            // entry names the subscription without spending the bar's width on
            // spelling it, and grows or shrinks with however many CodexBar
            // reports rather than with the two that were once hard-coded.
            Row {
              id: aiBarContent

              readonly property var capacities: root.menuBarCapacities()

              anchors.centerIn: parent
              height: parent.height
              spacing: root.spaceMedium

              // Nothing spent yet, or nothing collected yet: the entry falls
              // back to saying only what it opens.
              Text {
                visible: aiBarContent.capacities.length === 0
                anchors.verticalCenter: parent.verticalCenter
                text: "󱚣"
                color: root.accent
                font.family: root.fontFamily
                font.pixelSize: root.textSubhead
              }

              Repeater {
                model: aiBarContent.capacities

                Row {
                  id: aiBarCapacity

                  required property var modelData
                  readonly property string mark: root.agentMark(modelData.id)
                  readonly property color tint: modelData.free <= 30 ? root.capacityColor(modelData.free) : root.text

                  height: parent.height
                  spacing: root.spaceTight

                  AgentMark {
                    visible: aiBarCapacity.mark !== ""
                    anchors.verticalCenter: parent.verticalCenter
                    width: 13
                    height: 13
                    source: aiBarCapacity.mark
                  }

                  Text {
                    visible: aiBarCapacity.mark === ""
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.agentBadge(aiBarCapacity.modelData.id)
                    color: root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: root.textLabel
                    font.weight: root.weightStrong
                  }

                  Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: aiBarCapacity.modelData.free + "%"
                    color: aiBarCapacity.tint
                    font.family: root.fontFamily
                    font.pixelSize: root.textBody
                    font.weight: root.weightStrong
                  }
                }
              }
            }
            MouseArea {
              id: aiMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
              onPressed: function(mouse) {
                if (mouse.button === Qt.RightButton) root.runAgent("pi", "")
                else if (mouse.button === Qt.MiddleButton) root.refreshAgents()
                else root.toggleAgents(barWindow.modelData.name, root.barItemCenter(parent))
              }
            }
            HoverTip { mouse: aiMouse; text: root.menuBarCapacityTip() + " · middle-click to refresh · right-click to launch Pi" }
          }

          BarItem {
            module: "airpods"
            visible: !!(root.systemData.headphones || {}).connected && root.barModulePinned("airpods")
            width: 30
            hovered: airpodsMouse.containsMouse
            active: root.panelHere("airpods", barWindow.modelData)
            HeadphonesIcon { anchors.centerIn: parent; kind: root.headphonesIconKind(); tint: root.accent }
            BarModuleArea { id: airpodsMouse; module: "airpods"; onActivated: root.toggleControl("airpods", barWindow.modelData.name, root.barItemCenter(parent)) }
            HoverTip { mouse: airpodsMouse; text: root.headphonesLabel() }
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
              font.pixelSize: root.textIcon
            }
            BarModuleArea { id: bluetoothMouse; module: "bluetooth"; onActivated: root.toggleControl("bluetooth", barWindow.modelData.name, root.barItemCenter(parent)) }
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
              font.pixelSize: root.textIcon
            }
            BarModuleArea { id: vpnMouse; module: "vpn"; onActivated: root.toggleControl("vpn", barWindow.modelData.name, root.barItemCenter(parent)) }
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
              font.pixelSize: root.textIcon
            }
            BarModuleArea { id: networkMouse; module: "network"; onActivated: root.toggleControl("network", barWindow.modelData.name, root.barItemCenter(parent)) }
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
                font.pixelSize: root.textIcon
              }
              Text { anchors.verticalCenter: parent.verticalCenter; text: audioBarItem.shownVolume + "%"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
            }
            BarModuleArea {
              id: audioMouse
              module: "audio"
              acceptedButtons: Qt.LeftButton | Qt.MiddleButton
              onActivated: function(mouse) {
                if (mouse.button === Qt.MiddleButton) {
                  if (root.runControl("volume", "mute")) root.patchSystemData({ muted: !root.systemData.muted })
                } else {
                  root.toggleControl("audio", barWindow.modelData.name, root.barItemCenter(parent))
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
            Text { anchors.centerIn: parent; text: "󰘮"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textIcon }
            MouseArea { id: controlCenterMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("control-center", barWindow.modelData.name, root.barItemCenter(parent)) }
            HoverTip { mouse: controlCenterMouse; text: "Control Center" }
          }

          BarItem {
            width: githubBarContent.implicitWidth + root.spaceLarge
            hovered: githubBarHover.hovered
            active: root.panelHere("github", barWindow.modelData)
            Row {
              id: githubBarContent
              anchors.centerIn: parent
              spacing: root.spaceTight
              CenteredGlyph { width: root.textIcon; height: root.barItemHeight; text: "󰊤"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textIcon }
              Text { visible: githubStore.snapshot.reviewTotal > 0; anchors.verticalCenter: parent.verticalCenter; text: githubStore.snapshot.reviewTotal; color: githubStore.snapshot.stale ? root.subtext : root.text; font.family: root.fontFamily; font.pixelSize: root.textCaption }
            }
            HoverHandler { id: githubBarHover }
            MouseArea { id: githubBarMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.toggleControl("github", barWindow.modelData.name, root.barItemCenter(parent)) }
            HoverTip { mouse: githubBarMouse; text: "GitHub · pull requests and requested reviews" }
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
              // A Material bell draws well outside its advance, and the
              // silenced one draws wider still -- 13px of ink against 11px,
              // for the 8.4px both of them advance. Measuring the button from
              // the advance therefore left the mode deciding how much padding
              // was left around the mark. The slot is the silenced bell's ink
              // in both modes, so the button is that bell plus its padding
              // whichever bell is drawn, and the count still adds to it.
              FontMetrics { id: notificationBarGlyphMetrics; font.family: root.fontFamily; font.pixelSize: root.textIcon }
              CenteredGlyph {
                anchors.verticalCenter: parent.verticalCenter
                width: notificationBarGlyphMetrics.tightBoundingRect("󰂛").width
                height: parent.height
                text: root.systemData.dnd ? "󰂛" : "󰂚"
                color: root.systemData.dnd ? root.yellow : root.text
                font.family: root.fontFamily
                font.pixelSize: root.textIcon
              }
              Text { visible: Number(root.systemData.notifications.count || 0) > 0; anchors.verticalCenter: parent.verticalCenter; text: String(root.systemData.notifications.count); color: root.text; font.family: root.fontFamily; font.pixelSize: root.textCaption; font.weight: root.weightStrong }
            }
            MouseArea { id: notificationMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("notifications", barWindow.modelData.name, root.barItemCenter(parent)) }
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
                font.pixelSize: root.textIcon
              }
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: batteryBarItem.entry ? Number(batteryBarItem.entry.percent) + "%" : ""
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textLabel
              }
            }
            MouseArea { id: batteryMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("battery", barWindow.modelData.name, root.barItemCenter(parent)) }
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
              Text { anchors.verticalCenter: parent.verticalCenter; text: root.moduleGlyph(root.dragModule); color: root.accent; font.family: root.fontFamily; font.pixelSize: root.textIcon }
              Text { anchors.verticalCenter: parent.verticalCenter; text: root.moduleLabel(root.dragModule); color: root.accent; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
            }
          }

          BarItem {
            width: 30
            hovered: sessionMouse.containsMouse
            active: root.panelHere("system", barWindow.modelData)
            Text { anchors.centerIn: parent; text: "󰐥"; color: root.windowsCountdown >= 0 ? root.yellow : root.text; font.family: root.fontFamily; font.pixelSize: root.textIcon }
            MouseArea { id: sessionMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPressed: root.toggleControl("system", barWindow.modelData.name, root.barItemCenter(parent)) }
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
          color: root.dragOverBar ? root.selectedColor : root.activeTint

          Behavior on color { ColorAnimation { duration: root.durationFast } }
        }

        Rectangle {
          anchors.bottom: parent.bottom
          width: parent.width
          height: root.dragModule !== "" ? 2 : 1
          z: 1
          color: root.dragModule !== "" ? root.accent : root.alpha(root.crust, 0.85)
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

  // Application menu ----------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "application"
        && root.applicationWindow !== null
        && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 320
      implicitHeight: root.panelMargin * 2 + applicationContent.implicitHeight
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-application"

      PanelSurface {
        Column {
          id: applicationContent
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          PanelHeader {
            width: parent.width
            title: root.windowLabel(root.applicationWindow)
            detail: root.windowTitle(root.applicationWindow)
            mark: Component {
              Item {
                width: root.textDisplay
                height: root.textDisplay
                IconImage {
                  id: applicationIcon
                  anchors.fill: parent
                  source: root.windowIcon(root.applicationWindow)
                }
                CenteredGlyph {
                  visible: applicationIcon.source === ""
                  anchors.fill: parent
                  text: "󰣆"
                  color: root.accent
                  font.family: root.fontFamily
                  font.pixelSize: root.textSubhead
                }
              }
            }
          }

          Column {
            width: parent.width
            spacing: root.spaceTight
            Repeater {
              model: [
                { label: "Quit", glyph: "󰅖", force: false },
                { label: root.applicationForceConfirm ? "Confirm force quit" : "Force quit", glyph: "󰜺", force: true }
              ]
              Rectangle {
                required property var modelData
                readonly property bool hovered: applicationActionHover.hovered
                width: parent.width
                height: root.controlHeight
                radius: root.radius
                color: modelData.force
                  ? applicationActionMouse.pressed ? root.dangerPress : hovered ? root.dangerColor : root.dangerTint
                  : applicationActionMouse.pressed ? root.pressColor : hovered ? root.hoveredColor(root.cardColor) : root.cardColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                CardEdge { border.color: modelData.force ? root.alpha(root.red, 0.22) : root.cardBorder }

                HoverHandler { id: applicationActionHover }

                Row {
                  anchors.fill: parent
                  anchors.leftMargin: root.cardPadding
                  spacing: root.spaceMedium
                  CenteredGlyph {
                    anchors.verticalCenter: parent.verticalCenter
                    width: root.textIcon
                    height: root.textIcon
                    text: modelData.glyph
                    color: modelData.force ? root.red : root.accent
                    font.family: root.fontFamily
                    font.pixelSize: root.textIcon
                  }
                  Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.label
                    color: root.text
                    font.family: root.fontFamily
                    font.pixelSize: root.textBody
                    font.weight: root.weightStrong
                  }
                }

                MouseArea {
                  id: applicationActionMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.quitApplication(parent.modelData.force)
                }
              }
            }
          }
        }
      }
    }
  }

  // Focus timer ---------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: focusWindow
      required property var modelData
      screen: modelData
      visible: root.panelHere("focus", modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 350
      implicitHeight: focusContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-focus"
      onVisibleChanged: if (visible) Qt.callLater(function() { focusContent.forceActiveFocus() })

      PanelSurface {
        Column {
          id: focusContent
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing
          Keys.onEscapePressed: root.closeOverlays()
          Keys.onPressed: event => {
            if (event.isAutoRepeat || event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
            if (event.key === Qt.Key_1) focusTimer.command("start", 25)
            else if (event.key === Qt.Key_2) focusTimer.command("start", 50)
            else if (event.key === Qt.Key_3) focusTimer.command("start", 5)
            else if (event.key === Qt.Key_Delete) focusTimer.command("cancel")
            else if (event.key === Qt.Key_Space) {
              if (focusTimer.timerState.status === "running") focusTimer.command("pause")
              else if (focusTimer.timerState.status === "paused") focusTimer.command("resume")
              else focusTimer.command("start", 25)
            } else return
            event.accepted = true
          }
          PanelHeader { width: parent.width; glyph: "󰔟"; title: "Focus timer"; detail: "Focus, then take a break" }
          Text {
            width: parent.width
            text: focusTimer.label
            color: focusTimer.timerState.status === "done" ? root.green : root.accent
            font.family: root.fontFamily
            font.pixelSize: root.textHero
            font.weight: root.weightLight
            horizontalAlignment: Text.AlignHCenter
          }
          Text {
            width: parent.width
            text: focusTimer.timerState.status === "idle" ? "Choose a duration" : focusTimer.timerState.status === "done" ? "Time is up" : focusTimer.timerState.status === "paused" ? "Paused" : "In progress"
            color: root.subtext
            font.family: root.fontFamily
            font.pixelSize: root.textBody
            horizontalAlignment: Text.AlignHCenter
          }
          Row {
            width: parent.width
            spacing: root.spaceSmall
            Repeater {
              model: [{minutes:25,label:"25 min"}, {minutes:50,label:"50 min"}, {minutes:5,label:"5 min break"}]
              Rectangle {
                required property var modelData
                width: (parent.width - root.spaceSmall * 2) / 3
                height: root.controlHeight
                radius: root.radius
                color: presetMouse.pressed ? root.pressColor : presetHover.hovered ? root.hoveredColor(root.cardColor) : root.cardColor
                CardEdge {}
                HoverHandler { id: presetHover }
                Text { anchors.centerIn: parent; text: modelData.label; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                MouseArea { id: presetMouse; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: focusTimer.command("start", modelData.minutes) }
              }
            }
          }
          Row {
            width: parent.width
            spacing: root.spaceSmall
            Repeater {
              model: [focusTimer.timerState.status === "running" ? "Pause" : focusTimer.timerState.status === "paused" ? "Resume" : "Start", focusTimer.timerState.status === "done" ? "Done" : "Cancel"]
              Rectangle {
                required property string modelData
                required property int index
                width: (parent.width - root.spaceSmall) / 2
                height: root.controlHeight
                radius: root.radius
                color: actionMouse.pressed ? root.pressColor : actionHover.hovered ? root.hoveredColor(root.cardColor) : root.cardColor
                CardEdge {}
                HoverHandler { id: actionHover }
                Text { anchors.centerIn: parent; text: modelData; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                MouseArea {
                  id: actionMouse
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  onClicked: {
                    if (index === 1) focusTimer.command("cancel")
                    else if (focusTimer.timerState.status === "running") focusTimer.command("pause")
                    else if (focusTimer.timerState.status === "paused") focusTimer.command("resume")
                    else focusTimer.command("start", 25)
                  }
                }
              }
            }
          }
          Text { width: parent.width; text: "1 / 2 / 3 presets · Space pause/resume · Delete cancel"; color: root.mutedText; font.family: root.fontFamily; font.pixelSize: root.textCaption; wrapMode: Text.Wrap }
        }
      }
    }
  }

  // Calendar ------------------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: calendarWindow
      property string selectedDate: ""
      property string copyStatus: ""
      property bool copyPending: false

      function copyCalendarDate(cell) {
        var value = Time.calendarCopyDate(cell)
        if (!value || copyPending || calendarClipboard.running) return
        selectedDate = value
        copyStatus = "Copying " + value + "…"
        copyPending = true
        calendarClipboard.payload = value
        calendarCopyTimeout.restart()
        calendarClipboard.stdinEnabled = true
        calendarClipboard.running = true
      }

      function moveCalendarSelection(days, today) {
        if (copyPending || calendarClipboard.running) return
        var selection = Time.moveCalendarDate(root.now, today ? "" : selectedDate, days)
        if (!selection) return
        selectedDate = selection.date
        copyStatus = ""
        calendarMonths.positionViewAtIndex(selection.monthOffset + 60, ListView.Contain)
      }

      function finishCalendarCopy(exitCode, exitStatus) {
        if (!copyPending) return
        calendarCopyTimeout.stop()
        copyPending = false
        copyStatus = exitCode === 0 && exitStatus === 0
          ? "Copied " + calendarClipboard.payload : "Could not copy date"
      }

      Process {
        id: calendarClipboard
        property string payload: ""
        command: ["wl-copy", "--type", "text/plain;charset=utf-8"]
        onStarted: {
          write(payload)
          stdinEnabled = false
        }
        onExited: (exitCode, exitStatus) => calendarWindow.finishCalendarCopy(exitCode, exitStatus)
      }

      // Failed startup has no exited signal. Bound the pending state as well
      // as a compositor that does not acknowledge the clipboard request.
      Timer {
        id: calendarCopyTimeout
        interval: 5000
        onTriggered: {
          calendarWindow.finishCalendarCopy(-1, 1)
          calendarClipboard.running = false
        }
      }

      required property var modelData
      screen: modelData
      visible: root.controlPanel === "calendar" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 390
      implicitHeight: Math.min(modelData.height - 60, 470)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-calendar"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

      onVisibleChanged: if (visible) {
        if (!copyPending) {
          selectedDate = ""
          copyStatus = ""
        }
        Qt.callLater(function() {
          calendarMonths.positionViewAtIndex(60, ListView.Beginning)
          calendarSurface.forceActiveFocus()
        })
      }

      PanelSurface {
        id: calendarSurface
        focus: true
        Keys.onPressed: event => {
          if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier | Qt.ControlModifier)) return
          if (event.key === Qt.Key_Escape) root.closeOverlays()
          else if (event.key === Qt.Key_Left) calendarWindow.moveCalendarSelection(-1, false)
          else if (event.key === Qt.Key_Right) calendarWindow.moveCalendarSelection(1, false)
          else if (event.key === Qt.Key_Up) calendarWindow.moveCalendarSelection(-7, false)
          else if (event.key === Qt.Key_Down) calendarWindow.moveCalendarSelection(7, false)
          else if (event.key === Qt.Key_Home) calendarWindow.moveCalendarSelection(0, true)
          else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            calendarWindow.copyCalendarDate({ inMonth: true, date: calendarWindow.selectedDate || Time.calendarDate(root.now) })
          else return
          event.accepted = true
        }

        Column {
          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          PanelHeader {
            id: calendarHeader
            width: parent.width
            glyph: "󰃭"
            title: Qt.formatDate(root.now, "dddd")
            detail: calendarWindow.copyStatus || (calendarWindow.selectedDate
              ? calendarWindow.selectedDate + " · Enter to copy"
              : Qt.formatDate(root.now, "d MMMM yyyy") + " · week " + Time.isoWeek(root.now))

            Rectangle {
              width: 84
              height: root.controlHeight
              radius: root.radius
              color: todayMouse.pressed ? root.pressColor : todayMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              CardEdge {}
              Text { anchors.centerIn: parent; text: "Today"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
              MouseArea {
                id: todayMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                  if (!calendarWindow.copyPending) {
                    calendarWindow.selectedDate = ""
                    calendarWindow.copyStatus = ""
                  }
                  calendarMonths.positionViewAtIndex(60, ListView.Beginning)
                }
              }
            }
          }

          SeeleListView {
            id: calendarMonths
            width: parent.width
            // The list takes whatever the header and the gap under it leave,
            // so a change to either cannot quietly clip the last week of a
            // month or reserve a strip nothing draws in.
            height: parent.height - calendarHeader.height - root.panelSpacing
            model: 121
            spacing: 8
            clip: true
            delegate: Item {
              id: monthDelegate
              required property int modelData
              readonly property int monthOffset: modelData - 60
              readonly property date month: Time.monthDate(root.calendarDate, monthOffset)
              // A month occupies four, five or six Monday-first rows, and is
              // drawn at the height it needs rather than padded out to a fixed
              // block with the neighbouring months' days.
              readonly property int weeks: Time.calendarWeeks(root.calendarDate, monthOffset)
              readonly property int weekdayHeight: root.barItemHeight
              readonly property int cellHeight: root.controlHeight
              width: ListView.view.width
              height: root.chipHeight + root.spaceSmall + weekdayHeight
                + root.spaceSmall + weeks * cellHeight

              Column {
                anchors.fill: parent
                spacing: root.spaceSmall
                Text {
                  width: parent.width
                  height: root.chipHeight
                  text: Qt.formatDate(monthDelegate.month, "MMMM yyyy")
                  color: monthDelegate.monthOffset === 0 ? root.accent : root.text
                  font.family: root.fontFamily
                  font.pixelSize: root.textIcon
                  font.weight: root.weightStrong
                  verticalAlignment: Text.AlignVCenter
                }
                Grid {
                  width: parent.width
                  height: monthDelegate.weekdayHeight
                  columns: 8
                  Repeater {
                    model: ["Wk", "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
                    Text {
                      required property string modelData
                      width: parent.width / 8
                      height: monthDelegate.weekdayHeight
                      text: modelData
                      color: root.mutedText
                      font.family: root.fontFamily
                      font.pixelSize: root.textCaption
                      font.weight: root.weightStrong
                      horizontalAlignment: Text.AlignHCenter
                      verticalAlignment: Text.AlignVCenter
                    }
                  }
                }
                Grid {
                  id: monthGrid
                  width: parent.width
                  height: monthDelegate.weeks * monthDelegate.cellHeight
                  columns: 8
                  Repeater {
                    model: Time.calendarCells(root.calendarDate, monthDelegate.monthOffset)
                    Item {
                      id: calendarCell
                      required property var modelData
                      readonly property string copyDate: Time.calendarCopyDate(modelData)
                      readonly property bool selected: copyDate !== "" && copyDate === calendarWindow.selectedDate
                      width: monthGrid.width / 8
                      height: monthDelegate.cellHeight
                      Rectangle {
                        visible: !calendarCell.modelData.week && calendarCell.modelData.today
                        anchors.centerIn: parent
                        width: root.chipHeight; height: width; radius: width / 2
                        color: root.accent
                      }
                      Text {
                        anchors.centerIn: parent
                        text: calendarCell.modelData.week ? "W" + calendarCell.modelData.label
                          : calendarCell.modelData.inMonth ? calendarCell.modelData.day : ""
                        color: calendarCell.modelData.week ? root.mutedText : calendarCell.modelData.today ? root.crust : root.text
                        font.family: root.fontFamily
                        font.pixelSize: calendarCell.modelData.week ? root.textCaption : root.textLabel
                        font.weight: calendarCell.modelData.today || calendarCell.modelData.week ? root.weightStrong : root.weightRegular
                      }
                      HoverHandler { id: calendarDayHover; enabled: calendarCell.copyDate !== "" }
                      MouseArea {
                        id: calendarDayMouse
                        anchors.fill: parent
                        enabled: calendarCell.copyDate !== "" && !calendarWindow.copyPending
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: calendarWindow.copyCalendarDate(calendarCell.modelData)
                      }
                      HoverTip { mouse: calendarDayMouse; inOverlay: true; text: "Copy " + calendarCell.copyDate }
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

  DictationState { id: dictation; currentScreen: () => root.currentScreen() }

  Variants {
    model: Quickshell.screens
    PanelWindow {
      required property var modelData
      screen: modelData
      visible: dictation.active && root.pinnedScreen(dictation.output, modelData)
      anchors { bottom: true }
      margins.bottom: root.osdGap
      implicitWidth: root.waveformWidth + root.panelMargin * 2
      implicitHeight: dictationContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      mask: Region {}
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-dictation"
      PanelSurface {
        Column {
          id: dictationContent
          anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
          anchors.margins: root.panelMargin
          spacing: root.spaceSmall
          Shared.Waveform {
            id: dictationWave
            theme: root
            width: parent.width
            height: root.rowHeight
            visible: dictation.status === "recording"
            Connections {
              target: dictation
              function onLevel(value) { dictationWave.push(value) }
              function onStatusChanged() { if (dictation.status === "recording") dictationWave.clear() }
            }
          }
          Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: root.spaceMedium
            RefreshGlyph { visible: dictation.status === "transcribing"; width: root.textIcon; height: width; spinning: visible }
            Text {
              text: dictation.status === "recording" ? "Listening" : "Transcribing…"
              color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody
            }
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
      property string copyStatus: ""
      property bool copyPending: false

      function copyClockTimestamp(offset, label) {
        var value = Time.clockTimestamp(root.now, offset)
        if (!value || copyPending || clockClipboard.running) return
        copyStatus = "Copying timestamp…"
        copyPending = true
        clockClipboard.payload = value
        clockClipboard.label = label
        clockCopyTimeout.restart()
        clockClipboard.stdinEnabled = true
        clockClipboard.running = true
      }

      function copyClockSelection() {
        var zone = timezoneList.model[timezoneList.currentIndex]
        if (zone) copyClockTimestamp(zone.offset, zone.label)
      }

      function finishClockCopy(exitCode, exitStatus) {
        if (!copyPending) return
        clockCopyTimeout.stop()
        copyPending = false
        copyStatus = exitCode === 0 && exitStatus === 0
          ? "Copied " + clockClipboard.label : "Could not copy timestamp"
      }

      Process {
        id: clockClipboard
        property string payload: ""
        property string label: ""
        command: ["wl-copy", "--type", "text/plain;charset=utf-8"]
        onStarted: {
          write(payload)
          stdinEnabled = false
        }
        onExited: (exitCode, exitStatus) => clockWindow.finishClockCopy(exitCode, exitStatus)
      }

      Timer {
        id: clockCopyTimeout
        interval: 5000
        onTriggered: {
          clockWindow.finishClockCopy(-1, 1)
          clockClipboard.running = false
        }
      }

      required property var modelData
      screen: modelData
      visible: root.controlPanel === "clock" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: Math.min(root.clockWidth, modelData.width - root.panelGap * 2)
      implicitHeight: Math.min(modelData.height - root.barHeight - root.panelGap * 2,
        root.panelMargin * 2 + clockHeader.height + localClockCard.height + timezoneSearch.height
        + clockStatus.height + timezoneHeading.height + root.panelSpacing * 5
        + root.clockRows * (root.notificationRowHeight + root.spaceTight))
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-clock"
      onVisibleChanged: if (visible) Qt.callLater(function() {
        if (!clockWindow.copyPending) clockWindow.copyStatus = ""
        timezoneSearch.forceActiveFocus()
        timezoneSearch.selectAll()
      })

      PanelSurface {
        id: clockSurface
        Column {
          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing
          PanelHeader {
            id: clockHeader
            width: parent.width
            glyph: "󰥔"
            title: "World clock"
            detail: clockWindow.copyStatus || "Enter copies zone · Ctrl+Enter local"
          }
          Rectangle {
            id: localClockCard
            width: parent.width
            height: localClockContents.implicitHeight + root.cardPadding * 2
            radius: root.radius
            color: root.cardColor
            RowLayout {
              id: localClockContents
              anchors.left: parent.left; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
              anchors.margins: root.cardPadding
              Column {
                Layout.fillWidth: true
                spacing: root.spaceTight
                SectionLabel { text: "LOCAL TIME" }
                Text { text: Qt.formatDate(root.now, "yyyy-MM-dd") + " · " + ((root.clockData.local || {}).abbreviation || ""); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody }
              }
              Text {
                text: Qt.formatTime(root.now, "HH:mm:ss")
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textHero
                font.weight: root.weightLight
                MouseArea {
                  id: localTimeCopyMouse
                  anchors.fill: parent
                  enabled: !clockWindow.copyPending
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: clockWindow.copyClockTimestamp(undefined, "local time")
                }
                HoverTip { mouse: localTimeCopyMouse; inOverlay: true; text: "Copy local ISO timestamp" }
              }
            }
            CardEdge {}
          }
          TextField {
            id: timezoneSearch
            width: parent.width
            height: root.controlHeight
            placeholderText: "Search city, country, zone, or UTC offset…"
            color: root.text; placeholderTextColor: root.subtext
            selectionColor: root.accent; selectedTextColor: root.base
            font.family: root.fontFamily; font.pixelSize: root.textBody
            leftPadding: root.spaceLarge; rightPadding: root.spaceLarge
            background: Rectangle { radius: root.radius; color: root.wellColor; border.color: timezoneSearch.activeFocus ? root.accent : root.cardBorder; border.width: 1 }
            onTextChanged: {
              timezoneList.currentIndex = 0
              timezoneList.positionViewAtBeginning()
            }
            Keys.onPressed: event => {
              if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier)) return
              if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                if (event.modifiers & Qt.ControlModifier) clockWindow.copyClockTimestamp(undefined, "local time")
                else clockWindow.copyClockSelection()
              } else if (event.key === Qt.Key_Down) {
                timezoneList.currentIndex = Math.min(timezoneList.count - 1, timezoneList.currentIndex + 1)
                timezoneList.positionViewAtIndex(timezoneList.currentIndex, ListView.Contain)
              } else if (event.key === Qt.Key_Up) {
                timezoneList.currentIndex = Math.max(0, timezoneList.currentIndex - 1)
                timezoneList.positionViewAtIndex(timezoneList.currentIndex, ListView.Contain)
              } else if (event.key === Qt.Key_Escape) root.closeOverlays()
              else return
              event.accepted = true
            }
          }
          Text {
            id: clockStatus
            width: parent.width
            height: visible ? implicitHeight : 0
            visible: root.clockError !== "" || root.clockData.zones.length === 0 || timezoneList.count === 0
            text: root.clockError || (root.clockData.zones.length === 0 ? "Loading timezones…" : "No matching timezones")
            color: root.clockError ? root.red : root.subtext
            font.family: root.fontFamily; font.pixelSize: root.textBody; wrapMode: Text.Wrap
          }
          SectionLabel { id: timezoneHeading; text: timezoneSearch.text.trim() ? "SEARCH RESULTS" : (root.clockData.pinned.length ? "PINNED FIRST · ALL TIMEZONES" : "TIMEZONES"); width: parent.width }
          SeeleListView {
            id: timezoneList
            width: parent.width
            height: Math.max(0, parent.height - y)
            model: root.filteredTimezones(timezoneSearch.text)
            currentIndex: 0
            spacing: root.spaceTight
            clip: true
            delegate: Rectangle {
              id: timezoneRow
              required property var modelData
              required property int index
              readonly property bool pinned: root.timezonePinned(modelData.id)
              width: ListView.view.width
              height: root.notificationRowHeight
              radius: root.radius
              color: timezoneList.currentIndex === index ? root.selectedColor : rowHover.hovered ? root.hoveredColor(root.rowColor) : root.rowColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              RowLayout {
                anchors.fill: parent
                anchors.margins: root.spaceMedium
                spacing: root.spaceMedium
                Text { text: timezoneRow.modelData.flag || "◷"; Layout.preferredWidth: root.chipHeight; color: root.subtext; font.pixelSize: root.textCard; horizontalAlignment: Text.AlignHCenter }
                ColumnLayout {
                  Layout.fillWidth: true
                  spacing: root.spaceTight
                  Text { Layout.fillWidth: true; text: timezoneRow.modelData.label; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong; elide: Text.ElideRight }
                  Text { Layout.fillWidth: true; text: timezoneRow.modelData.id + " · " + Time.formatOffset(timezoneRow.modelData.offset); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption; elide: Text.ElideMiddle }
                }
                Column {
                  id: zoneTimeLabels
                  spacing: root.spaceTight
                  Text { width: parent.width; text: timezoneRow.modelData.time; color: timezoneRow.pinned ? root.accent : root.text; font.family: root.fontFamily; font.pixelSize: root.textDisplay; font.weight: root.weightLight; horizontalAlignment: Text.AlignRight }
                  Text { text: timezoneRow.modelData.day; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
                  HoverHandler { id: zoneTimeHover }
                  MouseArea {
                    id: zoneTimeCopyMouse
                    anchors.fill: parent
                    enabled: !clockWindow.copyPending
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                      timezoneList.currentIndex = timezoneRow.index
                      clockWindow.copyClockTimestamp(timezoneRow.modelData.offset, timezoneRow.modelData.label)
                    }
                  }
                  HoverTip { mouse: zoneTimeCopyMouse; inOverlay: true; text: "Copy ISO timestamp · " + timezoneRow.modelData.id }
                }
                Shared.ActionButton {
                  theme: root
                  text: timezoneRow.pinned ? "Unpin" : "Pin"
                  selected: timezoneRow.pinned
                  enabled: !clockActionProcess.running
                  onClicked: root.pinTimezone(timezoneRow.modelData.id)
                }
              }
              HoverHandler { id: rowHover }
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 310
      implicitHeight: Math.min(420, root.panelMargin * 2 + root.controlHeight + root.spaceSmall
        + Math.max(1, trayMenuOpener.children.values.length) * 36)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-tray-menu"
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

      PanelSurface {
        Column {
          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.spaceSmall

          Row {
            width: parent.width
            height: root.controlHeight
            spacing: root.spaceMedium
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
              font.pixelSize: root.textStrong
              font.weight: root.weightStrong
            }
            Rectangle {
              width: 72; height: 26; radius: root.radius
              anchors.verticalCenter: parent.verticalCenter
              color: trayHideMouse.pressed ? root.pressColor : trayHideMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              Text {
                anchors.centerIn: parent
                text: root.trayItemHidden(root.activeTrayItem) ? "Show icon" : "Hide icon"
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textLabel
                font.weight: root.weightStrong
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
              color: trayMenuCloseMouse.pressed ? root.pressColor : trayMenuCloseMouse.containsMouse ? root.hoverColor : root.clearColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              Text { anchors.centerIn: parent; text: "󰅖"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody }
              MouseArea { id: trayMenuCloseMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.closeTrayMenu() }
            }
          }

          SeeleListView {
            id: trayMenuList
            width: parent.width
            height: parent.height - root.controlHeight - root.spaceSmall
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
                color: root.separatorColor
              }

              Rectangle {
                visible: !parent.modelData.isSeparator
                anchors.fill: parent
                radius: root.radius
                color: trayMenuEntryMouse.pressed ? root.pressColor : trayMenuEntryMouse.containsMouse && parent.modelData.enabled ? root.hoverColor : root.clearColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
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
                font.pixelSize: root.textBody
              }

              Text {
                visible: !parent.modelData.isSeparator
                anchors.left: parent.left; anchors.leftMargin: trayMenuEntryIcon.visible ? 32 : 28
                anchors.right: trayMenuSubmenu.left; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                text: parent.modelData.text || ""
                elide: Text.ElideRight
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textBody
              }

              Text {
                id: trayMenuSubmenu
                visible: !parent.modelData.isSeparator && parent.modelData.hasChildren
                anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                text: "›"
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textSubhead
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 500
      // The panel is as tall as what it holds, up to the screen. A stated
      // height left a band of empty material below the folds while they were
      // closed, which is most of the time.
      implicitHeight: Math.min(modelData.height - 60, agentsContent.implicitHeight + root.panelMargin * 2)
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
        focus: true

        Keys.onEscapePressed: root.closeOverlays()

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
            spacing: root.panelSpacing

            PanelHeader {
              width: parent.width
              glyph: "󱚣"
              title: "AI cockpit"
              // What the usage figures below are worth is how recently they
              // were collected, and this sits directly beside the control that
              // collects them again.
              detail: root.agentRefreshing ? "Refreshing usage…" : root.agentError !== "" ? "Usage unavailable" : root.agentUpdatedText()
              detailColor: root.agentError !== "" ? root.red : root.subtext

              Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: root.controlHeight
                height: root.controlHeight
                radius: root.radius
                color: refreshMouse.pressed ? root.pressColor : root.agentRefreshing ? root.activeTint : refreshMouse.containsMouse ? root.hoverColor : root.clearColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                RefreshGlyph { anchors.centerIn: parent; width: 20; height: 20; spinning: root.agentRefreshing }
                MouseArea { id: refreshMouse; anchors.fill: parent; enabled: !root.agentRefreshing; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.refreshAgents() }
                HoverTip { mouse: refreshMouse; text: "Refresh usage"; inOverlay: true }
              }
            }

            SectionRule {
              width: parent.width
              label: "SESSIONS"
              detail: root.agentSummary()
            }

            // One indicator per harness, read rather than pressed. The row is a
            // single card and only a live session lights a cell inside it: the
            // four launch buttons this replaced each carried the same state on
            // a target big enough to invite a click, and the panel spent a
            // third of its height saying nothing.
            Rectangle {
              readonly property var indicators: root.agentIndicators()

              width: parent.width
              height: root.chipHeight + root.spaceMedium
              radius: root.radius
              color: root.cardColor
              visible: indicators.length > 0
              antialiasing: true

              CardEdge {}

              Row {
                anchors.fill: parent
                anchors.margins: root.spaceTight

                Repeater {
                  model: parent.parent.indicators

                  Rectangle {
                    id: agentIndicator

                    required property var modelData
                    readonly property string status: root.agentStatus(modelData.id)
                    readonly property bool live: status !== "idle"
                    // A session whose process has already gone keeps reporting
                    // for five minutes, and there is no window left to send
                    // anyone to, so only a running one answers the pointer.
                    readonly property bool running: root.agentRunning(modelData.id)
                    readonly property color stateColor: root.agentColor(status)

                    width: parent.width / Math.max(1, parent.parent.indicators.length)
                    height: parent.height
                    radius: root.radiusSmall
                    color: agentIndicatorMouse.pressed && agentIndicator.running
                      ? root.pressColor
                      : agentIndicator.live ? root.alpha(agentIndicator.stateColor, 0.14) : root.clearColor
                    antialiasing: true
                    Behavior on color { ColorAnimation { duration: root.durationFast } }

                    HoverWash { hovered: agentIndicatorMouse.containsMouse && agentIndicator.running }

                    Row {
                      anchors.centerIn: parent
                      spacing: root.spaceSmall

                      // Lit while a session is running and a well in the card
                      // while nothing is, beating only while the session is
                      // actually doing something.
                      Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 7
                        height: 7
                        radius: width / 2
                        antialiasing: true
                        color: agentIndicator.live ? agentIndicator.stateColor : root.wellColor
                        border.width: agentIndicator.live ? 0 : 1
                        border.color: root.edgeLight

                        SequentialAnimation on opacity {
                          running: agentIndicator.status === "working" || agentIndicator.status === "input"
                          loops: Animation.Infinite
                          NumberAnimation { from: 1; to: 0.25; duration: agentIndicator.status === "input" ? 600 : 900; easing.type: Easing.InOutQuad }
                          NumberAnimation { from: 0.25; to: 1; duration: agentIndicator.status === "input" ? 600 : 900; easing.type: Easing.InOutQuad }
                        }
                      }

                      Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: agentIndicator.modelData.name
                        elide: Text.ElideRight
                        color: agentIndicator.live ? root.text : root.subtext
                        font.family: root.fontFamily
                        font.pixelSize: root.textLabel
                        font.weight: root.weightStrong
                      }
                    }

                    MouseArea {
                      id: agentIndicatorMouse
                      anchors.fill: parent
                      hoverEnabled: true
                      cursorShape: agentIndicator.running ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: if (agentIndicator.running) root.focusAgent(agentIndicator.modelData.id)
                    }
                    HoverTip {
                      mouse: agentIndicatorMouse
                      text: agentIndicator.modelData.name + " · " + root.agentStatusText(agentIndicator.status)
                        + (agentIndicator.running ? " · click to focus" : "")
                      inOverlay: true
                    }
                  }
                }
              }
            }

            // The one action on this panel that changes the machine, so it is
            // the one card that arrives already lit.
            Rectangle {
              width: parent.width
              height: 52
              radius: root.radius
              color: osSessionMouse.pressed ? root.pressColor : osSessionMouse.containsMouse ? root.selectedColor : root.activeTint
              Behavior on color { ColorAnimation { duration: root.durationFast } }

              CardEdge { border.color: root.alpha(root.accent, 0.28) }

              Row {
                anchors.fill: parent
                anchors.leftMargin: root.cardPadding + 2
                anchors.rightMargin: root.cardPadding + 2
                spacing: root.spaceLarge

                Text { anchors.verticalCenter: parent.verticalCenter; text: "󱄅"; color: root.accent; font.family: root.fontFamily; font.pixelSize: root.textDisplay }

                Column {
                  anchors.verticalCenter: parent.verticalCenter
                  width: parent.width - 46
                  spacing: 1

                  Text { width: parent.width; text: "Describe a change to Seele"; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLead; font.weight: root.weightStrong }
                  Text { width: parent.width; text: "Opens the flake, rebuilds, then offers to record it"; elide: Text.ElideRight; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
                }
              }

              // The hand-off is only worth advertising under the pointer, the
              // way a Control Center row advertises its own.
              Text {
                visible: osSessionMouse.containsMouse
                anchors.right: parent.right
                anchors.rightMargin: root.spaceMedium
                anchors.verticalCenter: parent.verticalCenter
                text: "󰅂"
                color: root.accent
                font.family: root.fontFamily
                font.pixelSize: root.textStrong
              }

              MouseArea {
                id: osSessionMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.startOsSession()
              }
            }

            SectionRule { width: parent.width; label: "CAPACITY" }

            Rectangle {
              width: parent.width
              height: capacityContent.implicitHeight + root.cardPadding * 2
              radius: root.radius
              color: root.cardColor
              antialiasing: true

              CardEdge {}

              Column {
                id: capacityContent

                anchors.fill: parent
                anchors.margins: root.cardPadding
                spacing: root.spaceLarge

                Repeater {
                  model: root.agentData.subscriptions || []

                  Column {
                    required property var modelData

                    width: parent.width
                    spacing: root.spaceMedium

                    Item {
                      width: parent.width
                      height: 16

                      Text {
                        anchors.left: parent.left
                        anchors.right: subscriptionSource.left
                        anchors.rightMargin: root.spaceMedium
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.name + (modelData.plan ? " · " + modelData.plan : "")
                        elide: Text.ElideRight
                        color: root.text
                        font.family: root.fontFamily
                        font.pixelSize: root.textStrong
                        font.weight: root.weightStrong
                      }

                      // Which OAuth flow CodexBar read the figures through is
                      // not something the row is about. Credits are, and so is
                      // a provider that reported nothing.
                      Text {
                        id: subscriptionSource

                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: Number(modelData.credits) > 0 ? Number(modelData.credits) + " credits" : modelData.source === "unavailable" ? "unavailable" : ""
                        color: root.overlay
                        font.family: root.fontFamily
                        font.pixelSize: root.textCaption
                      }
                    }

                    Repeater {
                      model: modelData.limits || []

                      Column {
                        id: limitColumn

                        required property var modelData
                        readonly property int free: root.freePercent(modelData)

                        width: parent.width
                        spacing: root.spaceTight

                        Item {
                          width: parent.width
                          height: 14

                          Text {
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.name
                            color: root.subtext
                            font.family: root.fontFamily
                            font.pixelSize: root.textBody
                          }

                          Row {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: root.spaceTight

                            Text {
                              anchors.verticalCenter: parent.verticalCenter
                              text: limitColumn.free + "% free"
                              color: limitColumn.free <= 30 ? root.capacityColor(limitColumn.free) : root.text
                              font.family: root.fontFamily
                              font.pixelSize: root.textBody
                              font.weight: root.weightStrong
                            }

                            Text {
                              visible: root.resetText(limitColumn.modelData.resetsAt) !== ""
                              anchors.verticalCenter: parent.verticalCenter
                              text: "· resets " + root.resetText(limitColumn.modelData.resetsAt)
                              color: root.overlay
                              font.family: root.fontFamily
                              font.pixelSize: root.textCaption
                            }
                          }
                        }

                        MeterBar {
                          width: parent.width
                          ratio: limitColumn.free / 100
                          fill: root.capacityColor(limitColumn.free)
                        }
                      }
                    }

                    Text {
                      visible: (modelData.limits || []).length === 0
                      text: "No usage window reported"
                      color: root.overlay
                      font.family: root.fontFamily
                      font.pixelSize: root.textCaption
                    }
                  }
                }

                Text {
                  visible: (root.agentData.subscriptions || []).length === 0
                  text: "No subscriptions reporting"
                  color: root.overlay
                  font.family: root.fontFamily
                  font.pixelSize: root.textCaption
                }
              }
            }

            SectionRule { width: parent.width; label: "USAGE" }

            Rectangle {
              width: parent.width
              height: usageContent.implicitHeight + root.cardPadding * 2
              radius: root.radius
              color: root.cardColor
              antialiasing: true

              CardEdge {}

              Column {
                id: usageContent

                anchors.fill: parent
                anchors.margins: root.cardPadding
                spacing: root.spaceLarge

                SegmentWell {
                  width: parent.width

                  Repeater {
                    model: [
                      { id: "day", label: "Day" },
                      { id: "week", label: "Week" },
                      { id: "month", label: "Month" },
                      { id: "all", label: "All time" }
                    ]

                    Segment {
                      id: metricPeriod

                      required property var modelData

                      width: parent.width / 4
                      selected: root.agentMetricPeriod === modelData.id
                      hovered: metricPeriodMouse.containsMouse
                      pressed: metricPeriodMouse.pressed

                      Text {
                        anchors.centerIn: parent
                        text: metricPeriod.modelData.label
                        color: metricPeriod.selected ? root.accent : root.subtext
                        font.family: root.fontFamily
                        font.pixelSize: root.textLabel
                        font.weight: metricPeriod.selected ? root.weightStrong : root.weightRegular
                      }

                      MouseArea {
                        id: metricPeriodMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.agentMetricPeriod = metricPeriod.modelData.id
                      }
                    }
                  }
                }

                // Tokens and their estimated cost cover the same range, so they
                // are one instrument split by a hairline rather than two cards
                // that happen to sit beside each other.
                Item {
                  width: parent.width
                  height: 44

                  Column {
                    anchors.left: parent.left
                    anchors.right: usageDivider.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2

                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.formatTokens(root.agentMetricData.totalTokens || 0); color: root.accent; font.family: root.fontFamily; font.pixelSize: root.textDisplay; font.weight: root.weightLight }
                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: "tokens · " + root.agentMetricPeriodLabel(); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                  }

                  Rectangle {
                    id: usageDivider

                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.verticalCenter: parent.verticalCenter
                    width: 1
                    height: parent.height - root.spaceSmall
                    color: root.separatorColor
                  }

                  Column {
                    anchors.left: usageDivider.right
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2

                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: "$" + Number(root.agentMetricData.totalCost || 0).toFixed(2); color: root.green; font.family: root.fontFamily; font.pixelSize: root.textDisplay; font.weight: root.weightLight }
                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: "estimated · " + root.agentMetricPeriodLabel(); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                  }
                }
              }
            }

            SectionRule {
              width: parent.width
              label: "LAST 7 DAYS"
              collapsible: true
              expanded: root.agentUsageOpen
              onToggled: root.agentUsageOpen = !root.agentUsageOpen
            }

            Rectangle {
              width: parent.width
              height: dailyFold.implicitHeight + root.cardPadding * 2
              radius: root.radius
              color: root.cardColor
              visible: root.agentUsageOpen && (root.agentData.local.daily || []).length > 0
              antialiasing: true

              CardEdge {}

              Column {
                id: dailyFold

                // The tallest day scales every track, so it is measured once
                // for the fold rather than once per row inside it.
                readonly property real peak: {
                  var days = root.agentData.local.daily || []
                  var value = 1
                  for (var i = 0; i < days.length; i++) value = Math.max(value, Number(days[i].totalTokens || 0))
                  return value
                }

                anchors.fill: parent
                anchors.margins: root.cardPadding
                spacing: root.spaceTight

                Repeater {
                  model: root.agentData.local.daily || []

                  Item {
                    required property var modelData

                    width: parent.width
                    height: 22

                    Text {
                      id: dailyDate

                      anchors.left: parent.left
                      anchors.verticalCenter: parent.verticalCenter
                      width: 42
                      text: String(modelData.date || "").substring(5)
                      color: root.subtext
                      font.family: root.fontFamily
                      font.pixelSize: root.textLabel
                    }

                    MeterBar {
                      anchors.left: dailyDate.right
                      anchors.right: dailyCost.left
                      anchors.rightMargin: root.spaceMedium
                      anchors.verticalCenter: parent.verticalCenter
                      ratio: Number(modelData.totalTokens || 0) / dailyFold.peak
                    }

                    // Fixed columns: a cost that sized itself would move every
                    // track's right edge and leave the days incomparable.
                    Text {
                      id: dailyCost

                      anchors.right: dailyTokens.left
                      anchors.rightMargin: root.spaceMedium
                      anchors.verticalCenter: parent.verticalCenter
                      width: 34
                      text: "$" + Number(modelData.cost || 0).toFixed(0)
                      color: root.overlay
                      font.family: root.fontFamily
                      font.pixelSize: root.textLabel
                      horizontalAlignment: Text.AlignRight
                    }

                    Text {
                      id: dailyTokens

                      anchors.right: parent.right
                      anchors.verticalCenter: parent.verticalCenter
                      width: 52
                      text: root.formatTokens(modelData.totalTokens || 0)
                      color: root.text
                      font.family: root.fontFamily
                      font.pixelSize: root.textLabel
                      horizontalAlignment: Text.AlignRight
                    }
                  }
                }
              }
            }

            SectionRule {
              width: parent.width
              label: "TOP MODELS"
              collapsible: true
              expanded: root.agentModelsOpen
              onToggled: root.agentModelsOpen = !root.agentModelsOpen
            }

            Rectangle {
              width: parent.width
              height: modelFold.implicitHeight + root.cardPadding * 2
              radius: root.radius
              color: root.cardColor
              visible: root.agentModelsOpen && (root.agentMetricData.models || []).length > 0
              antialiasing: true

              CardEdge {}

              Column {
                id: modelFold

                readonly property real peak: {
                  var models = root.agentMetricData.models || []
                  var value = 1
                  for (var i = 0; i < models.length; i++) value = Math.max(value, Number(models[i].tokens || 0))
                  return value
                }

                anchors.fill: parent
                anchors.margins: root.cardPadding
                spacing: root.spaceTight

                Repeater {
                  model: root.agentMetricData.models || []

                  Item {
                    required property var modelData

                    width: parent.width
                    height: 22

                    Text {
                      id: modelName

                      anchors.left: parent.left
                      anchors.verticalCenter: parent.verticalCenter
                      width: 132
                      text: modelData.name
                      elide: Text.ElideRight
                      color: root.text
                      font.family: root.fontFamily
                      font.pixelSize: root.textLabel
                    }

                    MeterBar {
                      anchors.left: modelName.right
                      anchors.right: modelCost.left
                      anchors.rightMargin: root.spaceMedium
                      anchors.verticalCenter: parent.verticalCenter
                      ratio: Number(modelData.tokens || 0) / modelFold.peak
                    }

                    Text {
                      id: modelCost

                      anchors.right: modelTokens.left
                      anchors.rightMargin: root.spaceMedium
                      anchors.verticalCenter: parent.verticalCenter
                      width: 34
                      text: "$" + Number(modelData.cost || 0).toFixed(0)
                      color: root.overlay
                      font.family: root.fontFamily
                      font.pixelSize: root.textLabel
                      horizontalAlignment: Text.AlignRight
                    }

                    Text {
                      id: modelTokens

                      anchors.right: parent.right
                      anchors.verticalCenter: parent.verticalCenter
                      width: 52
                      text: root.formatTokens(modelData.tokens)
                      color: root.text
                      font.family: root.fontFamily
                      font.pixelSize: root.textLabel
                      horizontalAlignment: Text.AlignRight
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

  // GitHub pull requests ------------------------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: githubWindow
      required property var modelData
      property string tab: "reviews"
      readonly property var entries: tab === "reviews" ? githubStore.snapshot.reviews : githubStore.snapshot.authored
      readonly property int total: tab === "reviews" ? githubStore.snapshot.reviewTotal : githubStore.snapshot.authoredTotal
      screen: modelData
      visible: root.controlPanel === "github" && root.pinnedScreen(root.overlayScreen, modelData)
      onVisibleChanged: if (visible) Qt.callLater(function() { githubSurface.forceActiveFocus() })
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 440
      implicitHeight: githubContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-github"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

      PanelSurface {
        id: githubSurface
        focus: true
        Keys.onPressed: event => {
          if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
          if (event.isAutoRepeat && event.key !== Qt.Key_J && event.key !== Qt.Key_K
              && event.key !== Qt.Key_Up && event.key !== Qt.Key_Down) return
          if (event.key === Qt.Key_Escape) { root.closeOverlays(); event.accepted = true }
          else if (event.key === Qt.Key_J || event.key === Qt.Key_Down) { githubList.incrementCurrentIndex(); event.accepted = true }
          else if (event.key === Qt.Key_K || event.key === Qt.Key_Up) { githubList.decrementCurrentIndex(); event.accepted = true }
          else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) { githubWindow.tab = githubWindow.tab === "reviews" ? "authored" : "reviews"; githubList.currentIndex = 0; event.accepted = true }
          else if (event.key === Qt.Key_R) { githubStore.refresh(true); event.accepted = true }
          else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && githubList.currentIndex >= 0 && githubList.currentIndex < githubWindow.entries.length) { githubStore.openPull(githubWindow.entries[githubList.currentIndex].url); event.accepted = true }
        }
        Column {
          id: githubContent
          anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          PanelHeader {
            width: parent.width
            glyph: "󰊤"
            title: "GitHub"
            detail: githubStore.snapshot.viewer !== "" ? githubStore.snapshot.viewer + " · " + githubStore.snapshot.host : "Pull requests and requested reviews"
            Rectangle {
              width: root.chipHeight; height: root.chipHeight; radius: root.radius
              color: githubRefreshHover.hovered ? root.hoverColor : root.clearColor
              RefreshGlyph { anchors.centerIn: parent; width: root.textCard; height: width; spinning: githubStore.refreshing; color: githubStore.canRefresh ? root.text : root.subtext }
              HoverHandler { id: githubRefreshHover }
              MouseArea { id: githubRefreshMouse; anchors.fill: parent; hoverEnabled: true; enabled: githubStore.canRefresh; cursorShape: Qt.PointingHandCursor; onClicked: githubStore.refresh(true) }
              HoverTip { mouse: githubRefreshMouse; text: "Refresh GitHub"; inOverlay: true }
            }
          }

          Row {
            width: parent.width
            spacing: root.spaceSmall
            Repeater {
              model: [{ id: "reviews", label: "Requested reviews" }, { id: "authored", label: "My pull requests" }]
              Rectangle {
                required property var modelData
                width: (githubContent.width - root.spaceSmall) / 2
                height: root.controlHeight
                radius: root.radius
                color: githubWindow.tab === modelData.id ? root.selectedColor : root.wellColor
                Rectangle { anchors.fill: parent; radius: parent.radius; color: githubTabHover.hovered ? root.hoverColor : root.clearColor }
                Text { anchors.centerIn: parent; text: modelData.label; textFormat: Text.PlainText; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: githubWindow.tab === modelData.id ? root.weightStrong : root.weightMedium }
                HoverHandler { id: githubTabHover }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { githubWindow.tab = modelData.id; githubList.currentIndex = 0; githubSurface.forceActiveFocus() } }
              }
            }
          }

          Text {
            visible: githubStore.snapshot.state !== "ready"
            width: parent.width
            text: githubStore.refreshing && githubStore.snapshot.state === "idle" ? "Loading pull requests…" : githubStore.snapshot.message + (githubStore.snapshot.stale ? " Showing the last successful refresh." : "")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: githubStore.snapshot.state === "idle" ? root.subtext : root.yellow
            font.family: root.fontFamily; font.pixelSize: root.textBody
          }

          Text {
            visible: githubStore.snapshot.state === "auth-required"
            width: parent.width
            text: "Run gh auth login --hostname " + githubStore.snapshot.host + " in a terminal to connect your account."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.subtext
            font.family: root.fontFamily; font.pixelSize: root.textCaption
          }

          SeeleListView {
            id: githubList
            width: parent.width
            height: Math.min(contentHeight, root.rowHeight * 7)
            visible: githubWindow.entries.length > 0
            clip: true
            spacing: root.spaceSmall
            model: githubWindow.entries
            currentIndex: 0
            keyNavigationEnabled: true
            ScrollBar.vertical: SlimScrollBar { popupHovered: githubSurface.hovered }
            delegate: Rectangle {
              id: githubPullRow
              required property var modelData
              required property int index
              width: githubList.width - root.scrollGutter
              height: githubPullContent.implicitHeight + root.spaceMedium * 2
              radius: root.radius
              color: githubSurface.activeFocus && githubList.currentIndex === index ? root.selectedColor : root.rowColor
              Rectangle { anchors.fill: parent; radius: parent.radius; color: githubPullHover.hovered ? root.hoverColor : root.clearColor }
              Column {
                id: githubPullContent
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: root.spaceMedium
                spacing: root.spaceTight
                Text { width: parent.width; text: githubPullRow.modelData.title; textFormat: Text.PlainText; wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
                Text { width: parent.width; text: githubPullRow.modelData.repository + " #" + githubPullRow.modelData.number; textFormat: Text.PlainText; elide: Text.ElideRight; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
                Text { width: parent.width; text: GitHub.checksLabel(githubPullRow.modelData.checks) + " · " + GitHub.reviewLabel(githubPullRow.modelData); textFormat: Text.PlainText; elide: Text.ElideRight; color: ["FAILURE", "ERROR"].indexOf(githubPullRow.modelData.checks) >= 0 ? root.red : githubPullRow.modelData.checks === "SUCCESS" ? root.green : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
              }
              HoverHandler { id: githubPullHover }
              MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { githubList.currentIndex = githubPullRow.index; githubStore.openPull(githubPullRow.modelData.url) } }
            }
          }

          Text {
            visible: githubWindow.entries.length === 0 && githubStore.snapshot.state === "ready"
            width: parent.width
            text: githubWindow.tab === "reviews" ? "No reviews are waiting for you." : "You have no open pull requests."
            textFormat: Text.PlainText
            color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody
          }

          Text {
            visible: githubStore.snapshot.updatedAt !== ""
            width: parent.width
            text: "Showing " + githubWindow.entries.length + " of " + githubWindow.total + " · " + (githubStore.snapshot.stale ? "Last updated " : "Updated ") + Qt.formatTime(new Date(githubStore.snapshot.updatedAt), "HH:mm") + " · Tab to switch · R to refresh"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption
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
      anchors { top: true; left: true }
      margins { top: 0; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 400
      implicitHeight: controlCenterContent.implicitHeight + root.panelMargin * 2 + controlCenterWindow.barReach
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-control-center"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      onVisibleChanged: if (visible) Qt.callLater(function() { controlCenterContent.forceActiveFocus() })
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
            Keys.onEscapePressed: root.closeOverlays()

            anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing

            PanelHeader { width: parent.width; glyph: "󰘮"; title: "Control Center" }

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

      function cyclePlaybackSpeed() {
        return MediaSpeed.cycle(mediaWindow.player)
      }

      required property var modelData
      readonly property var players: root.availableMediaPlayers()
      readonly property var player: root.nowPlayingPlayer()
      screen: modelData
      visible: root.controlPanel === "media" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 400
      implicitHeight: mediaContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-media"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      onVisibleChanged: if (visible) Qt.callLater(function() { mediaContent.forceActiveFocus() })

      PanelSurface {
        Column {
          id: mediaContent
          Keys.onEscapePressed: root.closeOverlays()

          anchors.fill: parent
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          PanelHeader {
            width: parent.width
            glyph: "󰎆"
            title: "Now Playing"

            MediaPlayerPicker {
              visible: mediaWindow.players.length > 1
              width: visible ? implicitWidth : 0
              model: mediaWindow.players
              player: mediaWindow.player
            }
          }

          MediaBody {
            width: parent.width
            height: root.mediaBodyHeight
            player: mediaWindow.player
          }
          Rectangle {
            id: playerVolumeCard
            readonly property var player: mediaWindow.player
            readonly property bool writable: PlayerVolume.writable(player)
            width: parent.width
            height: root.controlHeight + root.cardPadding * 2
            radius: root.radius
            color: playerVolumeHover.hovered ? root.hoveredColor(root.cardColor) : root.cardColor
            HoverHandler { id: playerVolumeHover }
            CardEdge {}
            Text {
              anchors { left: parent.left; verticalCenter: parent.verticalCenter; leftMargin: root.cardPadding }
              text: "Player volume"
              color: root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
            }
            Row {
              anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: root.cardPadding }
              spacing: root.spaceSmall
              NotificationButton {
                label: "−"
                Accessible.name: "Decrease player volume"
                enabled: playerVolumeCard.writable && playerVolumeCard.player.volume > 0
                onClicked: PlayerVolume.adjust(playerVolumeCard.player, -0.05)
              }
              Text {
                width: root.controlHeight * 2
                anchors.verticalCenter: parent.verticalCenter
                text: PlayerVolume.supported(playerVolumeCard.player) ? PlayerVolume.percent(playerVolumeCard.player) + "%" : "Unavailable"
                horizontalAlignment: Text.AlignHCenter
                color: playerVolumeCard.writable ? root.text : root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textCaption
              }
              NotificationButton {
                label: "+"
                Accessible.name: "Increase player volume"
                enabled: playerVolumeCard.writable && playerVolumeCard.player.volume < 1
                onClicked: PlayerVolume.adjust(playerVolumeCard.player, 0.05)
              }
            }
          }
          Rectangle {
            width: parent.width
            height: root.controlHeight + root.spaceMedium * 2
            radius: root.radius
            color: root.cardColor
            CardEdge {}
            Text {
              anchors { left: parent.left; right: playbackSpeedButton.left; verticalCenter: parent.verticalCenter; margins: root.spaceMedium }
              text: "Playback speed"
              color: root.text
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
              elide: Text.ElideRight
            }
            NotificationButton {
              id: playbackSpeedButton
              readonly property bool containsMouse: hovered
              anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: root.spaceMedium }
              label: MediaSpeed.label(mediaWindow.player)
              enabled: MediaSpeed.nextRate(mediaWindow.player) !== null
              autoRepeat: false
              onClicked: mediaWindow.cyclePlaybackSpeed()
              Keys.onPressed: event => {
                if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space) return
                event.accepted = true
                if (event.isAutoRepeat || (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return
                mediaWindow.cyclePlaybackSpeed()
              }
              HoverTip {
                mouse: playbackSpeedButton
                inOverlay: true
                text: "Cycle supported speeds · " + MediaSpeed.rates(mediaWindow.player).join("× / ") + "×"
              }
            }
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
      property bool multipleOutputs: false
      onVisibleChanged: if (visible) multipleOutputs = root.selectedAudioOutputs().length > 1

      // Four rows of twenty-eight with a four-pixel gap between them. The gap
      // after the last row is not drawn, so it is not reserved either.
      readonly property int outputHeight: Math.max(0, Math.min(4, root.audioDevices("output").length) * 32 - root.spaceTight)
      readonly property int inputHeight: Math.max(0, Math.min(4, root.audioDevices("input").length) * 32 - root.spaceTight)
      required property var modelData
      screen: modelData
      visible: root.controlPanel === "audio" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
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
          PanelHeader { width: parent.width; glyph: "󰕾"; title: "Audio" }
          AudioLevelRow { width: parent.width }
          AudioLevelRow { width: parent.width; microphone: true }
          // The switch governs the group the rule opens, so it rides on the
          // rule rather than standing in a row of its own above it.
          SectionRule {
            width: parent.width
            label: "OUTPUT DEVICES"
            detail: root.failedControlAction === "audio-outputs" ? "Could not change outputs" : ""
            detailColor: root.red

            Text {
              anchors.verticalCenter: parent.verticalCenter
              text: "Multiple"
              color: root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
            }
            RefreshGlyph { anchors.verticalCenter: parent.verticalCenter; visible: root.pendingControlAction === "audio-outputs"; width: 16; height: 16; spinning: visible }
            ControlSwitch {
              anchors.verticalCenter: parent.verticalCenter
              checked: audioControlsWindow.multipleOutputs
              enabled: !controlProcess.running
              onToggled: {
                var nodes = root.selectedAudioOutputs()
                if (audioControlsWindow.multipleOutputs && nodes.length > 1) root.setAudioOutputs([nodes[0]])
                audioControlsWindow.multipleOutputs = !audioControlsWindow.multipleOutputs
              }
            }
          }
          DeviceListCard {
            width: parent.width
            listHeight: audioControlsWindow.outputHeight
            visible: root.audioDevices("output").length > 0

            SeeleListView {
              anchors.fill: parent
              anchors.margins: root.cardPadding
              spacing: root.spaceTight
              clip: true
              model: root.audioDevices("output")
              delegate: Rectangle {
                required property var modelData
                readonly property bool busy: root.controlBusy("audio-device", String(modelData.id))
                readonly property bool complete: root.controlCompleted("audio-device", String(modelData.id))
                width: ListView.view.width; height: root.chipHeight; radius: root.radius
                color: outputDeviceMouse.pressed ? root.pressColor : busy ? root.activeTint : (modelData.selected || modelData.default) || complete ? root.selectedColor : root.rowColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                HoverWash { hovered: outputDeviceMouse.containsMouse }
                Row {
                  visible: !parent.busy
                  anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                  Text { anchors.verticalCenter: parent.verticalCenter; text: parent.parent.complete || (modelData.selected || modelData.default) ? "󰄬" : "󰓃"; color: parent.parent.complete || (modelData.selected || modelData.default) ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textStrong }
                  Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
                MouseArea { id: outputDeviceMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; enabled: !controlProcess.running
                  onClicked: {
                    if (!parent.modelData.node) root.setAudioDevice(parent.modelData.id, parent.modelData.profile)
                    else if (audioControlsWindow.multipleOutputs) root.toggleAudioOutput(parent.modelData.node)
                    else root.setAudioOutputs([parent.modelData.node])
                  } }
              }
            }
          }
          SectionRule { width: parent.width; label: "INPUT DEVICE" }
          DeviceListCard {
            width: parent.width
            listHeight: audioControlsWindow.inputHeight
            visible: root.audioDevices("input").length > 0

            SeeleListView {
              anchors.fill: parent
              anchors.margins: root.cardPadding
              spacing: root.spaceTight
              clip: true
              model: root.audioDevices("input")
              delegate: Rectangle {
                required property var modelData
                readonly property bool busy: root.controlBusy("audio-device", String(modelData.id))
                readonly property bool complete: root.controlCompleted("audio-device", String(modelData.id))
                width: ListView.view.width; height: root.chipHeight; radius: root.radius
                color: inputDeviceMouse.pressed ? root.pressColor : busy ? root.activeTint : modelData.default || complete ? root.selectedColor : root.rowColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                HoverWash { hovered: inputDeviceMouse.containsMouse }
                Row {
                  visible: !parent.busy
                  anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                  Text { anchors.verticalCenter: parent.verticalCenter; text: parent.parent.complete || modelData.default ? "󰄬" : "󰍬"; color: parent.parent.complete || modelData.default ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textStrong }
                  Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
                MouseArea { id: inputDeviceMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.setAudioDevice(parent.modelData.id, parent.modelData.profile) }
              }
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 390
      implicitHeight: networkContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-network"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      onVisibleChanged: if (visible) {
        root.refreshStatus("aux")
        Qt.callLater(function() { networkContent.forceActiveFocus() })
      }

      PanelSurface {
        Column {
          id: networkContent
          Keys.onEscapePressed: root.closeOverlays()

          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          PanelHeader {
            width: parent.width
            glyph: "󰤨"
            title: "Network"

            Text {
              visible: root.systemData.wifiAvailable
              anchors.verticalCenter: parent.verticalCenter
              text: "Wi-Fi"
              color: root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
            }
            ControlSwitch {
              visible: root.systemData.wifiAvailable
              anchors.verticalCenter: parent.verticalCenter
              checked: root.systemData.wifiEnabled
              busy: root.controlBusy("wifi", "toggle")
              onToggled: if (root.runControl("wifi", "toggle")) root.patchSystemData({ wifiEnabled: !root.systemData.wifiEnabled })
            }
          }
          SectionRule { width: parent.width; label: "CONNECTION" }

          // The connection's name led a line of its own above this card while
          // the card held the addresses that belong to it. They are one thing,
          // so they are one card, and it is as tall as what it holds.
          Rectangle {
            width: parent.width
            height: connectionContent.implicitHeight + root.cardPadding * 2
            radius: root.radius
            color: root.cardColor
            antialiasing: true

            CardEdge {}

            Column {
              id: connectionContent

              anchors.fill: parent
              anchors.margins: root.cardPadding
              spacing: root.spaceSmall

              Item {
                width: parent.width
                height: 20

                Text {
                  anchors.left: parent.left
                  anchors.right: connectivityState.left
                  anchors.rightMargin: root.spaceMedium
                  anchors.verticalCenter: parent.verticalCenter
                  text: root.systemData.connection || "Disconnected"
                  elide: Text.ElideRight
                  color: root.text
                  font.family: root.fontFamily
                  font.pixelSize: root.textStrong
                  font.weight: root.weightStrong
                }

                Text {
                  id: connectivityState

                  anchors.right: parent.right
                  anchors.verticalCenter: parent.verticalCenter
                  text: root.systemData.connectivity
                  color: root.systemData.connectivity === "full" ? root.green : root.yellow
                  font.family: root.fontFamily
                  font.pixelSize: root.textCaption
                }
              }

              Text {
                width: parent.width
                text: root.systemData.networkInterface ? "Route via " + root.systemData.networkInterface : "No routed interface"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textCaption
              }
              Repeater {
                model: Network.addresses(root.systemData.networkAddresses, root.systemData.networkInterface)
                Item {
                  id: addressRow
                  required property var modelData
                  width: parent.width
                  height: root.controlHeight
                  Text {
                    anchors { left: parent.left; right: addressCopy.left; verticalCenter: parent.verticalCenter; rightMargin: root.spaceSmall }
                    text: addressRow.modelData.label + " · " + addressRow.modelData.value
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                    color: root.text
                    font.family: root.fontFamily
                    font.pixelSize: root.textCaption
                    MouseArea { id: addressHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                    HoverTip { mouse: addressHover; inOverlay: true; text: addressRow.modelData.detail }
                  }
                  NotificationButton {
                    id: addressCopy
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    label: "Copy"
                    successLabel: "Copied"
                    controlAction: "copy-address"
                    value: addressRow.modelData.value
                    enabled: !controlProcess.running
                  }
                }
              }
              Text {
                width: parent.width
                visible: Network.addresses(root.systemData.networkAddresses, root.systemData.networkInterface).length === 0
                text: "No usable IP addresses"
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textCaption
              }
              Text {
                width: parent.width
                text: "Gateway · " + (root.systemData.gateway || "Unavailable")
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textCaption
              }
            }
          }

          SectionRule { width: parent.width; label: "SPEED TEST"; detail: root.speedtestData.server || "" }

          Rectangle {
            id: speedtestCard

            width: parent.width; height: 204; radius: root.radius; color: root.cardColor
            CardEdge {}
            Column {
              anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 10; rightMargin: 10; topMargin: 8 }
              spacing: 8
              Item {
                width: parent.width; height: 30
                Column {
                  anchors.centerIn: parent
                  spacing: 0
                  SectionLabel {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "PING"
                  }
                  Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.speedtestPingText()
                    color: root.speedtestError !== "" ? root.red : root.text
                    font.family: root.fontFamily
                    font.pixelSize: root.textLabel
                    font.weight: root.weightStrong
                  }
                }
                Rectangle {
                  anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                  width: 64; height: 24; radius: root.radius
                  color: speedtestMouse.pressed ? root.pressColor : speedtestProcess.running ? root.selectedColor : speedtestMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
                  Behavior on color { ColorAnimation { duration: root.durationFast } }
                  Text { visible: !speedtestProcess.running; anchors.centerIn: parent; text: root.speedtestReceived ? "Again" : "Run"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
                  RefreshGlyph { visible: speedtestProcess.running; anchors.centerIn: parent; width: 14; height: 14; spinning: visible; font.pixelSize: root.textLabel }
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
                {label:"Settings", action:"network-settings", value:""},
                {label:"Allestörungen", action:"outages", value:""}
              ]
              Rectangle {
                required property var modelData
                readonly property bool busy: root.controlBusy(modelData.action, modelData.value)
                readonly property bool complete: root.controlCompleted(modelData.action, modelData.value)
                readonly property bool failed: root.controlFailed(modelData.action, modelData.value)
                width: (parent.width - 8) / 2; height: 38; radius: root.radius
                color: networkActionMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : networkActionMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Failed" : parent.complete ? "✓ Opened" : modelData.label; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 18; height: 18; spinning: visible; font.pixelSize: root.textLead }
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
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

          PanelHeader { width: parent.width; glyph: "󰒃"; title: "VPN" }

          Rectangle {
            id: tailscaleCard
            readonly property var state: root.systemData.tailscale || ({})
            readonly property var trayItem: root.trayItemNamed("tailscale")
            readonly property string action: state.connected ? "down" : state.needsLogin ? "login" : "up"
            readonly property bool busy: root.controlBusy("tailscale", action)
            readonly property bool failed: root.controlFailed("tailscale", action)
            // The click area stops short of the switch on purpose — tapping the
            // switch toggles Tailscale rather than opening its menu — but the
            // pointer is still on the card there, and the switch is a hover
            // area besides, so the tint comes from a handler over the whole
            // card. It still only lights where there is a menu to open.
            readonly property bool hovered: tailscaleCardHover.hovered && !!tailscaleCard.trayItem
            width: parent.width; height: 66; radius: root.radius
            color: failed ? root.dangerTint : tailscaleMenuMouse.pressed ? root.pressColor : tailscaleCard.hovered ? root.hoveredColor(state.connected ? root.activeTint : root.cardColor) : state.connected ? root.activeTint : root.cardColor
            Behavior on color { ColorAnimation { duration: root.durationFast } }
            HoverHandler { id: tailscaleCardHover }
            CardEdge {}
            MouseArea {
              id: tailscaleMenuMouse
              anchors.fill: parent
              anchors.rightMargin: 58
              enabled: !!tailscaleCard.trayItem
              hoverEnabled: true
              cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
              onClicked: root.openTrayItemMenu(tailscaleCard.trayItem, root.overlayScreen)
            }
            HoverTip { mouse: tailscaleMenuMouse; inOverlay: true; text: "Open Tailscale menu" }
            Row {
              anchors.fill: parent; anchors.margins: 10; spacing: 9
              Text { width: 24; anchors.verticalCenter: parent.verticalCenter; text: "󰛳"; color: tailscaleCard.state.connected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCard; horizontalAlignment: Text.AlignHCenter }
              Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 82
                spacing: 3
                Text { text: "Tailscale"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
                Text { width: parent.width; text: tailscaleCard.failed ? "Action failed" : root.tailscaleDetail(); elide: Text.ElideRight; color: tailscaleCard.failed ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
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
            color: failed ? root.dangerTint : mode !== "off" ? root.activeTint : root.cardColor
            CardEdge {}
            Column {
              anchors.fill: parent; anchors.margins: 10; spacing: 8
              Row {
                width: parent.width; spacing: 9
                Text { width: 24; text: "󰆍"; color: sshServerCard.mode !== "off" ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCard; horizontalAlignment: Text.AlignHCenter }
                Column {
                  width: parent.width - 33; spacing: 3
                  Text { text: "SSH access"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
                  Text {
                    width: parent.width
                    text: sshServerCard.failed ? "Mode change failed" : sshServerCard.mode === "mixed" ? "Both paths active · choose one" : sshServerCard.mode === "tailscale" ? "Incoming through Tailscale" : sshServerCard.mode === "ssh" ? "Port 22 · public keys only" : "No incoming SSH"
                    elide: Text.ElideRight
                    color: sshServerCard.failed ? root.red : root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: root.textCaption
                  }
                }
              }
              SegmentWell {
                width: parent.width

                Repeater {
                  model: [
                    { label: "Off", mode: "off", available: true },
                    { label: "Tailscale", mode: "tailscale", available: !!sshServerCard.state.tailscaleAvailable },
                    { label: "SSH", mode: "ssh", available: !!sshServerCard.state.sshAvailable }
                  ]

                  Segment {
                    id: sshMode

                    required property var modelData
                    readonly property bool busy: root.controlBusy("ssh-server", modelData.mode)

                    width: parent.width / 3
                    opacity: modelData.available ? 1 : 0.42
                    selected: sshServerCard.mode === modelData.mode || busy
                    hovered: sshModeMouse.containsMouse
                    pressed: sshModeMouse.pressed

                    Text { visible: !sshMode.busy; anchors.centerIn: parent; text: sshMode.modelData.label; color: sshMode.selected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: sshMode.selected ? root.weightStrong : root.weightRegular }
                    RefreshGlyph { visible: sshMode.busy; anchors.centerIn: parent; width: 14; height: 14; spinning: visible; font.pixelSize: root.textLabel }
                    MouseArea {
                      id: sshModeMouse
                      anchors.fill: parent
                      enabled: sshMode.modelData.available && !sshServerCard.busy && sshServerCard.mode !== sshMode.modelData.mode
                      hoverEnabled: true
                      cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                      onClicked: root.runControl("ssh-server", sshMode.modelData.mode)
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
            color: failed ? root.dangerTint : state.connected ? root.activeTint : root.cardColor
            CardEdge {}
            Row {
              anchors.fill: parent; anchors.margins: 10; spacing: 9
              Text { width: 24; anchors.verticalCenter: parent.verticalCenter; text: "󰒃"; color: protonVpnCard.state.connected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCard; horizontalAlignment: Text.AlignHCenter }
              Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 123
                spacing: 3
                Text { text: "Proton VPN"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
                Text { width: parent.width; text: protonVpnCard.failed ? "Quick connect failed · open the app" : root.protonVpnDetail(); elide: Text.ElideRight; color: protonVpnCard.failed ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
              }
              Rectangle {
                width: 32; height: 32; radius: root.radius
                anchors.verticalCenter: parent.verticalCenter
                color: protonAppMouse.pressed ? root.pressColor : protonAppMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                Text { anchors.centerIn: parent; text: "󰏌"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textStrong }
                MouseArea { id: protonAppMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runControl("proton-vpn", "open") }
                HoverTip { mouse: protonAppMouse; inOverlay: true; text: "Open Proton VPN for sign-in and location selection" }
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
      // Six rows of forty with a four-pixel gap between them, and no gap after
      // the last one.
      readonly property int listHeight: Math.max(0, Math.min(6, devices.length) * 44 - root.spaceTight)
      screen: modelData
      visible: root.controlPanel === "bluetooth" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 360
      implicitHeight: bluetoothContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-bluetooth"

      PanelSurface {
        id: bluetoothSurface

        Column {
          id: bluetoothContent

          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          PanelHeader {
            width: parent.width
            glyph: "󰂯"
            title: "Bluetooth"
            detail: root.systemData.bluetoothPowered
              ? root.systemData.bluetoothConnected + " connected device" + (root.systemData.bluetoothConnected === 1 ? "" : "s")
              : "Radio is off"

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
            height: root.controlHeight
            spacing: 8
            Text {
              anchors.verticalCenter: parent.verticalCenter
              width: 18
              text: "󰂰"
              color: root.bluetoothReceiverActive ? root.accent : root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textSubhead
            }
            Column {
              anchors.verticalCenter: parent.verticalCenter
              width: parent.width - 74
              spacing: 1
              Text { width: parent.width; text: "Receive audio"; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
              Text {
                width: parent.width
                text: root.bluetoothReceiverDetail()
                elide: Text.ElideRight
                color: root.bluetoothReceiverActive ? root.subtext : root.overlay
                font.family: root.fontFamily
                font.pixelSize: root.textCaption
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
              font.pixelSize: root.textBody
            }
            Rectangle {
              width: root.controlHeight; height: root.controlHeight; radius: root.radius
              color: bluetoothScanMouse.pressed ? root.pressColor : root.bluetoothScanActive ? root.activeTint : bluetoothScanMouse.containsMouse ? root.hoverColor : root.clearColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              RefreshGlyph { anchors.centerIn: parent; width: 20; height: 20; spinning: root.bluetoothScanActive }
              MouseArea {
                id: bluetoothScanMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.setBluetoothScanning(!root.bluetoothScanActive)
              }
              HoverTip { mouse: bluetoothScanMouse; inOverlay: true; text: root.bluetoothScanActive ? "Stop discovering" : "Search and stay visible for two minutes" }
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
              // The Auto and forget buttons float over the row's own pointer
              // area and take its hover, so both the fill and the buttons ask
              // the row itself whether the pointer is on it. Reading
              // `deviceMouse.containsMouse` instead left the row dropping back
              // to its resting fill under a pointer that had only moved onto
              // one of the buttons the hover had just revealed.
              readonly property bool hovered: deviceHover.hovered
              readonly property bool rowActions: !busy && modelData.paired && (hovered || forgetArmed)
              width: ListView.view.width; height: root.rowHeight; radius: root.radius
              color: deviceMouse.pressed ? root.pressColor : busy ? root.selectedColor : hovered ? root.hoveredColor(modelData.connected ? root.activeTint : root.rowColor) : modelData.connected ? root.activeTint : root.rowColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }

              HoverHandler { id: deviceHover }
              Row {
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 10
                Text {
                  anchors.verticalCenter: parent.verticalCenter
                  text: root.bluetoothIcon(modelData)
                  color: modelData.connected ? root.accent : root.subtext
                  font.family: root.fontFamily
                  font.pixelSize: root.textSubhead
                }
                Column {
                  anchors.verticalCenter: parent.verticalCenter
                  width: parent.width - 62
                  spacing: 1
                  Text { width: parent.width; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
                  Text {
                    width: parent.width
                    text: root.bluetoothDetail(modelData)
                    elide: Text.ElideRight
                    color: forgetArmed ? root.red : modelData.connected ? root.green : root.bluetoothBusy === modelData.address ? root.yellow : root.overlay
                    font.family: root.fontFamily
                    font.pixelSize: root.textCaption
                  }
                }
                Text {
                  anchors.verticalCenter: parent.verticalCenter
                  visible: !rowActions && !parent.parent.busy
                  text: root.bluetoothSignal(modelData)
                  color: root.overlay
                  font.family: root.fontFamily
                  font.pixelSize: root.textBody
                }
              }
              RefreshGlyph { visible: parent.busy; anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
              MouseArea { id: deviceMouse; anchors.fill: parent; enabled: !parent.busy; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.toggleBluetoothDevice(parent.modelData) }
              Rectangle {
                visible: rowActions
                anchors.right: parent.right
                anchors.rightMargin: 38
                anchors.verticalCenter: parent.verticalCenter
                width: 44; height: 24; radius: root.radius
                color: autoConnectMouse.pressed ? root.pressColor : modelData.trusted ? root.selectedColor : root.floatColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                HoverWash { hovered: autoConnectMouse.containsMouse }
                Text { anchors.centerIn: parent; text: "Auto"; color: modelData.trusted ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
                MouseArea { id: autoConnectMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.runBluetooth("trust", modelData.address) }
                HoverTip { mouse: autoConnectMouse; inOverlay: true; text: modelData.trusted ? "Autoconnect on" : "Autoconnect off" }
              }
              Rectangle {
                visible: rowActions
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 24; height: 24; radius: root.radius
                color: forgetMouse.pressed || forgetArmed ? root.dangerPress : forgetMouse.containsMouse ? root.dangerColor : root.floatColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                Text { anchors.centerIn: parent; text: "󰅖"; color: forgetArmed || forgetMouse.containsMouse ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody }
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
            font.pixelSize: root.textLabel
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
            font.pixelSize: root.textHero
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
            font.pixelSize: root.textLead
            font.weight: root.weightStrong
          }

          Rectangle {
            visible: root.pairingCode() !== "" && !root.pairingWantsCode()
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            height: 52
            radius: root.radius
            color: root.wellColor
            Text {
              anchors.centerIn: parent
              text: root.pairingCode()
              color: root.accent
              font.family: root.fontFamily
              font.pixelSize: root.textCode
              font.weight: root.weightStrong
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
            font.pixelSize: root.textCode
            font.weight: root.weightStrong
            font.letterSpacing: 4
            background: Rectangle {
              radius: root.radius
              color: root.wellColor
              border.color: pairingCodeField.activeFocus ? root.accent : "transparent"
              border.width: 1
            }
            onAccepted: root.answerBluetoothPairing("accept", pairingCodeField.text)
            Keys.onEscapePressed: root.answerBluetoothPairing("reject", "")
            Keys.onShortcutOverride: function(event) { event.accepted = event.key === Qt.Key_Escape }
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
            font.pixelSize: root.textLabel
          }

          Row {
            visible: pairingWindow.kind !== "display"
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            Rectangle {
              width: (pairingCard.width - 8) / 2
              height: 30
              radius: root.radius
              color: pairingRejectMouse.pressed ? root.dangerPress : pairingRejectMouse.containsMouse ? root.dangerColor : root.floatColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              Text { anchors.centerIn: parent; text: "Reject"; color: pairingRejectMouse.containsMouse ? root.red : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
              MouseArea { id: pairingRejectMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.answerBluetoothPairing("reject", "") }
            }
            Rectangle {
              width: (pairingCard.width - 8) / 2
              height: 30
              radius: root.radius
              color: pairingAcceptMouse.pressed ? root.pressColor : pairingAcceptMouse.containsMouse ? root.hoveredColor(root.selectedColor) : root.selectedColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              Text { anchors.centerIn: parent; text: "Confirm"; color: root.accent; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
              MouseArea { id: pairingAcceptMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.answerBluetoothPairing("accept", pairingCodeField.text) }
            }
          }

          Rectangle {
            visible: pairingWindow.kind === "display"
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            height: 30
            radius: root.radius
            color: pairingDismissMouse.pressed ? root.pressColor : pairingDismissMouse.containsMouse ? root.hoveredColor(root.floatColor) : root.floatColor
            Behavior on color { ColorAnimation { duration: root.durationFast } }
            Text { anchors.centerIn: parent; text: "Dismiss"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 340
      implicitHeight: headphonesContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-airpods"

      PanelSurface {
        Column {
          id: headphonesContent

          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: root.panelMargin
          spacing: root.panelSpacing

          PanelHeader {
            width: parent.width
            mark: HeadphonesIcon { width: 16; height: 16; kind: root.headphonesIconKind(); tint: root.accent }
            title: root.headphonesLabel()
            detail: root.headphonesBatteryText() || "Connected"
          }
          SectionRule {
            width: parent.width
            label: "NOISE CONTROL"
            detail: nothingHeadphones && !headphones.controls ? "Connecting…" : ""
          }
          SegmentWell {
            width: parent.width
            implicitHeight: root.rowHeight
            opacity: nothingHeadphones && !headphones.controls ? 0.42 : 1

            Repeater {
              model: [{label:"Off", mode:"off"}, {label:"ANC", mode:"anc"}, {label:"Aware", mode:"transparency"}, {label:"Adaptive", mode:"adaptive"}]

              Segment {
                id: noiseMode

                required property var modelData
                readonly property bool busy: root.controlBusy("headphones", modelData.mode)
                readonly property bool complete: root.controlCompleted("headphones", modelData.mode)
                readonly property bool failed: root.controlFailed("headphones", modelData.mode)

                width: parent.width / 4
                // Acknowledgement outranks the resting choice for the moment it
                // lasts, so a mode that failed says so where it was pressed.
                color: noiseMode.failed ? root.dangerColor : noiseMode.complete ? root.successColor : noiseMode.pressed ? root.pressColor : noiseMode.selected ? root.selectedColor : root.clearColor
                selected: headphones.noiseMode === modelData.mode || busy
                hovered: airpodsModeMouse.containsMouse
                pressed: airpodsModeMouse.pressed

                Text { visible: !noiseMode.busy; anchors.centerIn: parent; text: noiseMode.failed ? "×" : noiseMode.complete ? "✓ " + noiseMode.modelData.label : noiseMode.modelData.label; color: noiseMode.failed ? root.red : noiseMode.complete ? root.green : noiseMode.selected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: noiseMode.selected ? root.weightStrong : root.weightRegular }
                RefreshGlyph { visible: noiseMode.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
                MouseArea { id: airpodsModeMouse; anchors.fill: parent; enabled: !nothingHeadphones || headphones.controls; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: root.runControl("headphones", noiseMode.modelData.mode) }
              }
            }
          }
          Row {
            visible: headphones.connected
            width: parent.width; spacing: 8
            Column {
              width: parent.width - 48
              anchors.verticalCenter: parent.verticalCenter
              spacing: 1
              Text { text: "Auto play and pause"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
              Text { text: nothingHeadphones && typeof headphones.earDetection !== "boolean" ? "Reading ear detection…" : "Pause when removed, resume when worn"; color: root.overlay; font.family: root.fontFamily; font.pixelSize: root.textCaption }
            }
            ControlSwitch {
              anchors.verticalCenter: parent.verticalCenter
              enabled: headphones.connected && (!nothingHeadphones || (headphones.controls && typeof headphones.earDetection === "boolean"))
              checked: nothingHeadphones ? headphones.earDetection === true : root.systemData.airpodsEarDetection
              busy: root.controlBusy("headphones", "ear-detection", "toggle")
              onToggled: root.runControl("headphones", "ear-detection", "toggle")
            }
          }
          Rectangle {
            visible: headphones.connected && root.headphonesIconKind() === "airpods"
            readonly property bool busy: root.controlBusy("headphones", "open")
            readonly property bool complete: root.controlCompleted("headphones", "open")
            readonly property bool failed: root.controlFailed("headphones", "open")
            width: parent.width; height: 38; radius: root.radius
            color: airpodsDetailsMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : airpodsDetailsMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
            Behavior on color { ColorAnimation { duration: root.durationFast } }
            Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Could not open" : parent.complete ? "✓ Opened" : "Battery and AirPods settings"; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
            RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
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
      readonly property int batteryRowHeight: 38
      readonly property int listHeight: Math.max(1, Math.min(5, entries.length)) * (batteryRowHeight + root.spaceSmall) - root.spaceSmall
      screen: modelData
      visible: root.controlPanel === "battery" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 330
      implicitHeight: batteryContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-battery"

      PanelSurface {
        Column {
          id: batteryContent

          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          PanelHeader {
            width: parent.width
            glyph: "󰁹"
            title: "Batteries"
            detail: batteryWindow.entries.length === 0 ? "None reported" : batteryWindow.entries.length + " device" + (batteryWindow.entries.length === 1 ? "" : "s")
          }
          DeviceListCard {
            visible: batteryWindow.entries.length > 0
            width: parent.width
            listHeight: batteryWindow.listHeight

            SeeleListView {
              anchors.fill: parent
              anchors.margins: root.cardPadding
              spacing: root.spaceSmall
              clip: true
              model: batteryWindow.entries
              delegate: Column {
                required property var modelData
                width: ListView.view.width
                height: batteryWindow.batteryRowHeight
                spacing: root.spaceSmall
                Row {
                  width: parent.width; spacing: 8
                  Text { anchors.verticalCenter: parent.verticalCenter; text: root.batteryIcon(modelData); color: root.batteryColor(modelData); font.family: root.fontFamily; font.pixelSize: root.textIcon }
                  Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 90; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong }
                  Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 60
                    text: Number(modelData.percent) + "%" + (root.batteryCharging(modelData) ? " ⚡" : "")
                    color: root.batteryColor(modelData)
                    font.family: root.fontFamily
                    font.pixelSize: root.textBody
                    horizontalAlignment: Text.AlignRight
                  }
                }
                MeterBar {
                  width: parent.width
                  ratio: Number(modelData.percent) / 100
                  fill: root.batteryColor(modelData)
                }
              }
            }
          }
        }
      }
    }
  }

  // Selected Home Assistant entities -----------------------------------------
  Variants {
    model: Quickshell.screens
    PanelWindow {
      id: homeAssistantWindow
      required property var modelData
      screen: modelData
      visible: root.panelHere("home-assistant", modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 400
      implicitHeight: homeAssistantContent.implicitHeight + root.panelMargin * 2
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-home-assistant"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      onVisibleChanged: if (visible) Qt.callLater(function() { homeAssistantContent.forceActiveFocus() })

      PanelSurface {
        HoverHandler { id: homeAssistantHover }
        Column {
          id: homeAssistantContent
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: root.panelMargin }
          spacing: root.panelSpacing
          Keys.onEscapePressed: root.closeOverlays()
          PanelHeader {
            width: parent.width
            glyph: "󰋜"
            title: "Home Assistant"
            detail: homeAssistantStore.busy ? "Updating…" : homeAssistantStore.connected ? "Selected entities" : "Unavailable"
            detailColor: homeAssistantStore.connected ? root.subtext : root.yellow
            NotificationButton {
              id: homeAssistantRefresh
              label: "Refresh"
              enabled: !homeAssistantStore.busy
              onClicked: homeAssistantStore.refresh()
            }
          }
          Text {
            width: parent.width
            visible: text !== ""
            text: homeAssistantStore.error || (!homeAssistantStore.configured ? "Add a private home-assistant.json configuration to connect." : homeAssistantStore.entities.length === 0 && !homeAssistantStore.busy ? "No selected entities." : "")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: homeAssistantStore.error ? root.yellow : root.subtext
            font.family: root.fontFamily
            font.pixelSize: root.textLabel
          }
          SeeleListView {
            width: parent.width
            height: Math.min(contentHeight, root.rowHeight * 6)
            visible: homeAssistantStore.entities.length > 0
            clip: true
            spacing: root.spaceSmall
            model: homeAssistantStore.entities
            delegate: Rectangle {
              id: homeAssistantRow
              required property var modelData
              width: ListView.view.width - root.scrollGutter
              height: root.rowHeight + root.spaceMedium
              radius: root.radiusSmall
              color: activeFocus ? root.selectedColor : root.cardColor
              readonly property bool actionable: homeAssistantStore.connected && modelData.available && modelData.controllable && !homeAssistantStore.busy
              activeFocusOnTab: actionable
              function changeState() {
                if (actionable) homeAssistantStore.setState(modelData, modelData.state === "on" ? "off" : "on")
              }
              Keys.onPressed: event => {
                if (event.isAutoRepeat || (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                  changeState()
                  event.accepted = true
                }
              }
              CardEdge {}
              Column {
                anchors { left: parent.left; right: homeAssistantToggle.left; verticalCenter: parent.verticalCenter; leftMargin: root.cardPadding; rightMargin: root.spaceMedium }
                spacing: root.spaceTight
                Text { width: parent.width; text: homeAssistantRow.modelData.name; textFormat: Text.PlainText; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
                Text { width: parent.width; text: homeAssistantRow.modelData.state + (homeAssistantRow.modelData.unit ? " " + homeAssistantRow.modelData.unit : "") + (!homeAssistantStore.connected ? " · stale" : ""); textFormat: Text.PlainText; elide: Text.ElideRight; color: homeAssistantStore.connected && homeAssistantRow.modelData.available ? root.subtext : root.yellow; font.family: root.fontFamily; font.pixelSize: root.textCaption }
              }
              ControlSwitch {
                id: homeAssistantToggle
                anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: root.cardPadding }
                visible: homeAssistantRow.modelData.controllable
                enabled: homeAssistantRow.actionable
                checked: homeAssistantStore.pendingEntity === homeAssistantRow.modelData.entity_id ? homeAssistantStore.pendingValue === "on" : homeAssistantRow.modelData.state === "on"
                busy: homeAssistantStore.busy && homeAssistantStore.pendingEntity === homeAssistantRow.modelData.entity_id
                onToggled: homeAssistantRow.changeState()
              }
            }
            ScrollBar.vertical: SlimScrollBar { popupHovered: homeAssistantHover.hovered }
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
      readonly property var entries: NotificationSearch.filter(root.notificationHistoryOpen
        ? (root.systemData.notifications.history || [])
        : (root.systemData.notifications.items || []), notificationSearch.text)
      // Everything above the list: the panel's own padding, the header, the
      // history and clear row, and the gap on either side of it.
      readonly property int chromeHeight: root.panelMargin * 2 + root.panelHeaderHeight
        + root.panelSpacing + 36 + root.panelSpacing + root.chipHeight + root.panelSpacing
        + root.rowHeight + root.panelSpacing
      // An empty list is worth exactly one card: the panel says there is
      // nothing here in the space one notification would have taken, rather
      // than holding open a void the size of several.
      readonly property int emptyHeight: chromeHeight + root.notificationRowHeight
      property int stableHeight: emptyHeight
      screen: modelData
      visible: root.controlPanel === "notifications" && root.pinnedScreen(root.overlayScreen, modelData)
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 400
      implicitHeight: stableHeight
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-notifications"
      WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

      // Rows are as tall as the notification they hold, so the opening height
      // comes from what the list actually measured rather than a row count.
      // It measures the list that is on screen, so the height follows the
      // side of the panel being read instead of whichever side is longer.
      function suggestedHeight() {
        if (entries.length === 0) return emptyHeight
        var content = root.notificationHistoryOpen ? notificationHistoryList.contentHeight : notificationCurrentList.contentHeight
        return Math.min(560, chromeHeight + Math.max(root.notificationRowHeight, content))
      }

      // Deferred, because the lists have not laid out their rows at the moment
      // the height is asked for and would still measure zero.
      function remeasure() {
        if (!visible) return
        Qt.callLater(function() { notificationWindow.stableHeight = notificationWindow.suggestedHeight() })
      }

      function toggleHistory() {
        root.notificationHistoryOpen = !root.notificationHistoryOpen
      }

      onVisibleChanged: {
        remeasure()
        if (visible) Qt.callLater(function() { notificationSearch.forceActiveFocus(); notificationSearch.selectAll() })
      }
      // Clearing or dismissing while the panel is open has to shrink it; the
      // height is stored rather than bound, so it only follows the list if the
      // list says it changed.
      onEntriesChanged: remeasure()
      // `notificationHistoryOpen` belongs to root, so the change has to be
      // taken from there rather than declared as a handler on this window.
      Connections {
        target: root
        function onNotificationHistoryOpenChanged() { notificationWindow.remeasure() }
      }

      PanelSurface {
        id: notificationSurface

        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          PanelHeader {
            width: parent.width
            glyph: root.systemData.dnd ? "󰂛" : "󰂚"
            title: root.notificationHistoryOpen ? "Last 24 hours" : "Notifications"

            // The count is small print beside a large title, and centring
            // both line boxes in the same row leaves the digits floating
            // above the title: the shorter face has the shorter box, so its
            // baseline lands higher. The offset is the distance between the
            // two baselines, which puts the count on the title's line.
            FontMetrics { id: notificationTitleMetrics; font.family: root.fontFamily; font.pixelSize: root.textTitle }
            FontMetrics { id: notificationCountMetrics; font.family: root.fontFamily; font.pixelSize: root.textBody }
            Text {
              anchors.verticalCenter: parent.verticalCenter
              anchors.verticalCenterOffset: Math.round((notificationCountMetrics.height - notificationTitleMetrics.height) / 2
                + notificationTitleMetrics.ascent - notificationCountMetrics.ascent)
              text: String(notificationWindow.entries.length)
              color: root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textBody
            }
            // Silence is an action, not a setting with a caption: the header
            // mark already reports whether the shell is muted, so the control
            // beside it is the same square button every other panel header
            // uses rather than a labelled switch wedged into the title row.
            Rectangle {
              readonly property bool busy: root.controlBusy("dnd", "")

              anchors.verticalCenter: parent.verticalCenter
              width: root.chipHeight
              height: root.chipHeight
              radius: root.radius
              color: dndMouse.pressed ? root.pressColor
                : root.systemData.dnd ? root.alpha(root.yellow, dndMouse.containsMouse ? 0.24 : 0.14)
                : dndMouse.containsMouse ? root.hoverColor
                : root.clearColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              Text {
                visible: !parent.busy
                anchors.centerIn: parent
                text: "󰂛"
                color: root.systemData.dnd ? root.yellow : dndMouse.containsMouse ? root.text : root.subtext
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                font.family: root.fontFamily
                font.pixelSize: root.textStrong
              }
              RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
              MouseArea {
                id: dndMouse
                anchors.fill: parent
                enabled: !parent.busy
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: notificationStore.controller.setDnd(!notificationStore.controller.dnd)
              }
              HoverTip { mouse: dndMouse; inOverlay: true; text: root.systemData.dnd ? "Do not disturb is on" : "Silence notifications" }
            }
          }
          Row {
            width: parent.width; spacing: 8
            Rectangle {
              width: (parent.width - 8) / 2; height: 36; radius: root.radius
              color: historyMouse.pressed ? root.pressColor : root.notificationHistoryOpen ? root.selectedColor : root.cardColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              HoverWash { hovered: historyMouse.containsMouse }
              Text {
                anchors.centerIn: parent
                text: root.notificationHistoryOpen ? "Back" : "History"
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textLabel
                font.weight: root.weightStrong
              }
              MouseArea { id: historyMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: notificationWindow.toggleHistory() }
              HoverTip { mouse: historyMouse; inOverlay: true; text: root.notificationHistoryOpen ? "Show current notifications" : "Show the past 24 hours" }
            }
            Rectangle {
              readonly property bool busy: root.controlBusy("notifications", "clear")
              readonly property bool complete: root.controlCompleted("notifications", "clear")
              readonly property bool failed: root.controlFailed("notifications", "clear")
              width: (parent.width - 8) / 2; height: 36; radius: root.radius
              color: clearMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : clearMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
              Behavior on color { ColorAnimation { duration: root.durationFast } }
              Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Failed" : parent.complete ? "✓ Cleared" : "Clear"; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
              RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
              MouseArea { id: clearMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.clearNotifications() }
            }
          }
          Row {
            id: quietPresets
            width: parent.width
            height: root.chipHeight
            spacing: root.spaceSmall
            Text {
              width: 104
              height: parent.height
              text: root.systemData.notifications.dndUntil > 0
                ? "Until " + Qt.formatDateTime(new Date(root.systemData.notifications.dndUntil * 1000), "HH:mm") : "Quiet for"
              color: root.systemData.dnd ? root.yellow : root.subtext
              font.family: root.fontFamily
              font.pixelSize: root.textLabel
              verticalAlignment: Text.AlignVCenter
            }
            Repeater {
              model: [{minutes:15,label:"15 min"}, {minutes:60,label:"1 hour"}, {minutes:240,label:"4 hours"}]
              Rectangle {
                required property var modelData
                width: (quietPresets.width - 104 - root.spaceSmall * 3) / 3
                height: root.chipHeight
                radius: root.radiusSmall
                activeFocusOnTab: enabled
                color: quietMouse.pressed ? root.pressColor : quietHover.hovered ? root.hoveredColor(root.cardColor) : root.cardColor
                function activate() { notificationStore.controller.snooze(modelData.minutes, Date.now() / 1000) }
                Keys.onPressed: event => {
                  if (event.isAutoRepeat || (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return
                  if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                    activate()
                    event.accepted = true
                  }
                }
                CardEdge {}
                HoverHandler { id: quietHover }
                Text { anchors.centerIn: parent; text: modelData.label; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                MouseArea { id: quietMouse; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: parent.activate() }
              }
            }
          }
          TextField {
            id: notificationSearch
            width: parent.width
            height: root.rowHeight
            maximumLength: 256
            placeholderText: "Search app, title, or message…"
            color: root.text
            placeholderTextColor: root.overlay
            selectionColor: root.accent
            selectedTextColor: root.base
            font.family: root.fontFamily
            font.pixelSize: root.textLabel
            leftPadding: root.spaceMedium
            rightPadding: root.spaceMedium
            background: Rectangle {
              radius: root.radius
              color: root.wellColor
              border.color: notificationSearch.activeFocus ? root.accent : root.cardBorder
              border.width: 1
            }
            onTextChanged: { notificationCurrentList.positionViewAtBeginning(); notificationHistoryList.positionViewAtBeginning(); notificationWindow.remeasure() }
            Keys.onEscapePressed: { if (text !== "") clear(); else root.closeOverlays() }
          }
          Item {
            id: notificationViewport

            width: parent.width
            height: parent.height - root.panelHeaderHeight - root.panelSpacing - 36 - root.panelSpacing
              - quietPresets.height - root.panelSpacing - notificationSearch.height - root.panelSpacing
            clip: true
            NotificationList {
              id: notificationCurrentList
              query: notificationSearch.text
              onContentHeightChanged: notificationWindow.remeasure()
              visible: !root.notificationHistoryOpen && entries.length > 0
              anchors.fill: parent
              ScrollBar.vertical: SlimScrollBar { popupHovered: notificationSurface.hovered }
            }
            NotificationList {
              id: notificationHistoryList
              query: notificationSearch.text
              onContentHeightChanged: notificationWindow.remeasure()
              history: true
              visible: root.notificationHistoryOpen && entries.length > 0
              anchors.fill: parent
              ScrollBar.vertical: SlimScrollBar { popupHovered: notificationSurface.hovered }
            }
            Item {
              visible: notificationWindow.entries.length === 0
              anchors.fill: parent
              Text {
                anchors.centerIn: parent
                width: parent.width
                text: notificationSearch.text.trim() !== "" ? "No notifications match your search" : root.notificationHistoryOpen ? "Nothing arrived in the past 24 hours" : "No notifications right now"
                color: root.overlay
                font.family: root.fontFamily
                font.pixelSize: root.textLabel
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
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
          PanelHeader {
            width: parent.width
            glyph: "󰄀"
            title: "Camera"
            // What the panel is about, said where every other panel says it,
            // instead of on a line of its own under the title.
            detail: root.systemData.cameraActive ? "Camera is in use" : cameraWindow.deviceCount + " camera device" + (cameraWindow.deviceCount === 1 ? "" : "s")
            detailColor: root.systemData.cameraActive ? root.red : root.subtext
          }
          SectionRule { width: parent.width; label: "PREVIEW DEVICE"; detail: cameraWindow.deviceCount === 0 ? "No camera detected" : "" }
          DeviceListCard {
            visible: cameraWindow.deviceCount > 0
            width: parent.width
            listHeight: Math.max(0, Math.min(4, cameraWindow.deviceCount) * 32 - root.spaceTight)

            SeeleListView {
              anchors.fill: parent
              anchors.margins: root.cardPadding
              spacing: root.spaceTight
              clip: true
              model: root.systemData.cameraDevices || []
              delegate: Rectangle {
                required property var modelData
                readonly property bool selected: cameraWindow.camera && String(cameraWindow.camera.device || "") === String(modelData.device || "")
                width: ListView.view.width; height: root.chipHeight; radius: root.radius
                color: cameraDeviceMouse.pressed ? root.pressColor : selected ? root.selectedColor : root.rowColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                HoverWash { hovered: cameraDeviceMouse.containsMouse }
                Row {
                  anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                  Text { anchors.verticalCenter: parent.verticalCenter; text: parent.parent.selected ? "󰄬" : "󰄀"; color: parent.parent.selected ? root.accent : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textStrong }
                  Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: modelData.name; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel }
                }
                MouseArea {
                  id: cameraDeviceMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.cameraPreviewDevice = String(parent.modelData.device || "")
                }
              }
              ScrollBar.vertical: SlimScrollBar { popupHovered: cameraSurface.hovered }
            }
          }
          ClippingRectangle {
            z: 2
            width: parent.width; height: 176; radius: root.radius; color: root.wellColor
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
              font.pixelSize: root.textLabel
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
                color: cameraActionMouse.pressed ? root.pressColor : failed ? root.dangerColor : complete ? root.successColor : busy ? root.selectedColor : cameraActionMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                Text { visible: !parent.busy; anchors.centerIn: parent; text: parent.failed ? "× Failed" : parent.complete ? "✓ Opened" : modelData.label; color: parent.failed ? root.red : parent.complete ? root.green : root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
                RefreshGlyph { visible: parent.busy; anchors.centerIn: parent; width: 16; height: 16; spinning: visible; font.pixelSize: root.textStrong }
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
      anchors { top: true; left: true }
      margins { top: root.barHeight + root.panelGap; left: root.panelLeft(modelData, implicitWidth) }
      implicitWidth: 420
      // Padding, header, gap, two rows of buttons and the gap between them.
      implicitHeight: root.panelMargin * 2 + root.panelHeaderHeight + root.panelSpacing + 72 * 2 + root.spaceMedium
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.namespace: "seele-shell-session"

      PanelSurface {
        Column {
          anchors.fill: parent; anchors.margins: root.panelMargin; spacing: root.panelSpacing
          PanelHeader { width: parent.width; glyph: "󰐥"; title: "Power" }
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
                color: modelData.variant === "destructive" ? (sessionActionMouse.pressed ? root.dangerPress : sessionActionMouse.containsMouse ? root.dangerColor : root.dangerTint) : sessionActionMouse.pressed ? root.pressColor : sessionActionMouse.containsMouse ? root.hoveredColor(root.cardColor) : root.cardColor
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                CardEdge { border.color: modelData.variant === "destructive" ? root.alpha(root.red, 0.22) : root.cardBorder }
                Column {
                  anchors.centerIn: parent
                  spacing: root.spaceTight
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.icon; color: modelData.variant === "destructive" ? root.red : modelData.action === "reboot-windows" && root.windowsCountdown >= 0 ? root.yellow : root.accent; font.family: root.fontFamily; font.pixelSize: root.textTitle }
                  Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.label; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textLabel; font.weight: root.weightStrong }
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
      visible: !uriPicker.presented && root.osdOpen && root.pinnedScreen(root.osdScreen, modelData)
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
          visible: microphone || root.osdKind === "volume"
          anchors.fill: parent; anchors.margins: 14; spacing: 12
          Text { anchors.verticalCenter: parent.verticalCenter; text: levelOsd.microphone ? (levelOsd.muted ? "󰍭" : "󰍬") : (levelOsd.muted ? "󰝟" : "󰕾"); color: levelOsd.muted ? root.red : root.accent; font.family: root.fontFamily; font.pixelSize: root.textDisplay }
          MeterBar {
            width: 205
            height: 8
            anchors.verticalCenter: parent.verticalCenter
            ratio: root.audioFillRatio(levelOsd.level)
          }
          Text { anchors.verticalCenter: parent.verticalCenter; text: levelOsd.level + "%"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody }
        }
        Row {
          visible: root.osdKind === "airpods"
          anchors.fill: parent; anchors.margins: 14; spacing: 12
          HeadphonesIcon { anchors.verticalCenter: parent.verticalCenter; width: 22; height: 22; kind: root.headphonesOsdKind; tint: root.headphonesOsdConnected ? root.accent : root.subtext }
          Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 34
            spacing: 2
            Text { width: parent.width; text: root.headphonesOsdName; elide: Text.ElideRight; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textStrong; font.weight: root.weightStrong }
            Text { text: root.headphonesOsdConnected ? (root.headphonesBatteryText() || "Connected") : "Disconnected"; color: root.headphonesOsdConnected ? root.green : root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
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
            font.pixelSize: root.textHero
          }

          Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "Touch your YubiKey"
            color: root.text
            font.family: root.fontFamily
            font.pixelSize: root.textLead
            font.weight: root.weightStrong
          }

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "Waiting for hardware confirmation"
            color: root.subtext
            font.family: root.fontFamily
            font.pixelSize: root.textBody
            wrapMode: Text.WordWrap
          }
        }
      }
    }
  }
}
