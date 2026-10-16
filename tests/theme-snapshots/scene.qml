// @imports@
import QtTest
import "shared/Palette.js" as Palette

// The snapshot scene. tests/theme-snapshots.js fills the markers from the
// shell.qml under test: its imports, every root property, function and inline
// component, and each surface's layer window as a Component. What stays here
// is what Quickshell would otherwise supply: the stores' state on one fixed
// morning, the notifications that arrived before it, and the loop that draws
// every surface in every theme.
Shared.Theme {
  id: root
  width: 1920
  height: 1080

  // @production@

  // The clock, every age and every countdown read this one moment.
  property date now: new Date(2026, 9, 15, 9, 41, 0)
  readonly property real fixtureEpoch: now.getTime() / 1000

  // Quickshell's popup window. A tip is only drawn while hovered.
  component HoverTip: QtObject {
    property var mouse: null
    property string text: ""
    property bool inOverlay: false
    property bool below: false
  }
  // Quickshell's themed icon. Icon lookup is left unresolved
  // (fixture/Quickshell.js), so there is never an image to draw.
  component IconImage: Item {
    property string source: ""
    property bool mipmap: false
    property real implicitSize: 0
    property bool asynchronous: false
  }

  QtObject {
    id: focusTimer
    property var timerState: ({ status: "running", remaining: 1452, duration: 1500, task: "Baseline snapshots" })
    property string label: "24:12"
    property bool canExtend: true
    property string extensionHint: "+5 min"
  }
  QtObject { id: dictation; property string status: "idle"; property bool active: false }
  QtObject {
    id: homeAssistantStore
    property bool configured: true
    property bool connected: true
    property string summaryText: "21.5°"
  }
  QtObject { id: aiActivity; property string indicator: "" }
  QtObject { id: transfersStore; property bool attention: false; property string barText: "" }
  QtObject { id: hermesStore; property bool enabled: false; property var projection: ({ glyph: "", tint: "text", label: "" }) }
  QtObject {
    id: caffeinateStore
    property bool active: true
    property string barText: "󰅶 1:20"
    property string hoverText: "Caffeinate · until 11:00"
  }
  QtObject {
    id: githubStore
    property var snapshot: Object.assign(GitHub.initial("github.com"), { reviewTotal: 2, stale: false })
  }
  QtObject { id: integrationHealth; property int attentionCount: 0 }
  QtObject { id: maintenance; property int count: 1; property string urgency: "eventually" }
  QtObject {
    id: calendarStore
    property var indicator: ({ key: "review", day: "2026-10-15", title: "Design review", label: "in 19m", extra: 0, color: "#7986cb", detail: "10:00–10:45 · Design review" })
  }
  QtObject {
    id: prFocusStore
    property bool configured: false
    property bool active: false
    property string label: ""
    property var snapshot: ({ title: "", checks: "", state: "", comment: "", commentAuthor: "", message: "" })
    function enter() {}
    function exit() {}
  }
  QtObject { id: driftStore; property int drifted: 0 }
  QtObject { id: portsStore; property int total: 3 }
  QtObject { id: bluetoothProcess; property bool running: false }
  QtObject { id: controlProcess; property bool running: false }
  QtObject { id: uriPicker; property bool presented: false }
  QtObject { id: colorPicker; property bool presented: false }
  QtObject { id: presentingRetained; property bool manual: false }
  QtObject { id: notificationStore; property var controller: null }
  Timer { id: volumeDragTimer }
  Timer { id: microphoneDragTimer }
  Timer { id: streamDragTimer }
  ThemeStore { id: themeStore }

  // What the status monitor reports on that morning, through the production
  // SystemState schema.
  readonly property var systemFixture: ({
    volume: 64, muted: false, microphoneVolume: 80, microphoneMuted: false, microphoneActive: false,
    connection: "Seele", connectionType: "802-11-wireless", connectivity: "full",
    wifiEnabled: true, wifiAvailable: true, ipAddress: "192.168.1.20", gateway: "192.168.1.1", networkInterface: "wlan0",
    tailscale: { available: true, backend: "Running", connected: true, needsLogin: false, name: "nerv", ip: "100.64.0.2", tailnet: "seele", peers: 4, onlinePeers: 2 },
    protonVpn: { available: false, connected: false, connection: "" },
    bluetoothAvailable: true, bluetoothPowered: true, bluetoothConnected: 1,
    bluetoothDevices: [{ address: "AA:BB:CC:DD:EE:01", name: "Nothing Headphone (1)", icon: "audio-headphones", connected: true, paired: true, trusted: true, battery: 80 }],
    headphones: { connected: true, name: "Nothing Headphone (1)", kind: "over-ear", battery: { left: 80, right: 80, case: null }, controls: true, noiseMode: "anc" },
    voxtypeStatus: "idle",
    cameraDevices: [{ id: "/dev/video0", name: "Logitech C920" }], cameraDevice: "/dev/video0", cameraActive: false,
    audioDevices: [
      { id: 51, kind: "output", name: "Speakers", node: "alsa_output.usb", profile: null, selected: true, default: true },
      { id: 52, kind: "output", name: "Nothing Headphone (1)", node: "bluez_output.nothing", profile: null, selected: false, default: false },
      { id: 61, kind: "input", name: "USB Microphone", node: "alsa_input.usb", profile: null, selected: true, default: true }
    ],
    audioStreams: [
      { id: 71, name: "Zen", detail: "Seele Themes", icon: "", volume: 100, muted: false, playing: true },
      { id: 72, name: "mpv", detail: "Komm, süsser Tod", icon: "", volume: 80, muted: false, playing: true }
    ],
    batteries: [{ name: "Nothing Headphone (1)", kind: "headphones", percent: 80, state: "discharging" }],
    agentStates: { codex: { active: true, status: "running" } }
  })

  // Notifications arrive through the production native policy, exactly as
  // NotificationStore receives them from the bus.
  function notification(id, appName, summary, body, urgency, actions) {
    return {
      id: id, appName: appName, appIcon: "", desktopEntry: "", summary: summary, body: body,
      hasActionIcons: false, image: "", urgency: urgency, resident: false, transient: false,
      expireTimeout: -1, hints: {}, lastGeneration: false, tracked: true,
      actions: (actions || []).map(function(action) { return { identifier: action[0], text: action[1], invoke: function() {} } }),
      dismiss: function() {}, expire: function() {}
    }
  }
  function receiveNotifications() {
    var controller = Notifications.createStore(function(view, dnd) { root.systemData.apply({ notifications: view, dnd: dnd }) },
      function() {}, fixtureEpoch - 3600)
    notificationStore.controller = controller
    controller.restore({})
    var arrivals = [
      [fixtureEpoch - 2400, notification(11, "Seele Calendar", "Design review", "Starts at 10:00 in Room 2", 1)],
      [fixtureEpoch - 600, notification(12, "Seele Shell", "Focus timer", "Time is up.", 1)],
      // The last three are still on screen as toasts.
      [fixtureEpoch - 20, notification(13, "Signal", "Rei", "Are the baselines in? The new theme work starts after them.", 1, [["default", "Open"], ["reply", "Reply"]])],
      [fixtureEpoch - 12, notification(14, "Seele Transfers", "Received baseline.png", "From asuka · 412 KB", 0)],
      [fixtureEpoch - 5, notification(15, "nh", "Rebuild failed", "nixosConfigurations.nerv: evaluation error in modules/features/themes", 2, [["default", "Show"]])]
    ]
    for (var i = 0; i < arrivals.length; i++) {
      controller.advance(arrivals[i][0])
      controller.receive(arrivals[i][1], arrivals[i][0], false)
    }
    controller.advance(fixtureEpoch)
  }

  // Two subscriptions spent against and one harness running.
  property var agentData: ({
    launchers: [{ id: "codex", name: "Codex" }, { id: "claude", name: "Claude" }],
    subscriptions: [
      { id: "codex", name: "Codex", limits: [{ usedPercent: 38 }] },
      { id: "claude", name: "Claude", limits: [{ usedPercent: 81 }] }
    ],
    local: { today: {}, daily: [], periods: {}, models: [], totalTokens: 0, totalCost: 0 },
    generatedAt: "2026-10-15T09:35:00Z"
  })

  Component.onCompleted: {
    systemData.apply(systemFixture)
    receiveNotifications()
  }

  // @surfaces@

  Loader {
    id: scene
    active: false
  }

  TestCase {
    id: snapshots
    name: "ThemeSnapshots"
    when: windowShown

    readonly property var themes: "@themes@"
    readonly property var surfaces: "@surfaces@"
    readonly property string output: "@output@"
    readonly property var components: ({
      "bar": barSurface, "control-center": controlCenterSurface, "notifications": notificationsSurface,
      "notification-center": notificationCenterSurface, "media": mediaSurface, "audio": audioSurface,
      "session": sessionSurface, "osd": osdSurface, "theme-switcher": themeSwitcherSurface,
      "theme-settings": themeSettingsSurface
    })
    // Root state each surface resets, so one surface's state never leaks into the next.
    readonly property var idle: ({ controlPanel: "", osdOpen: false, osdKind: "volume", themesOpen: false })

    // The catalog as `seele-theme list` reports it with this theme applied.
    function catalogFor(theme) {
      var presets = themes.filter(function(entry) { return !entry.fallback })
      var current = theme.fallback ? "catppuccin-mocha" : theme.id
      var mode = theme.fallback ? "dark" : theme.mode
      return {
        current: current,
        themes: presets,
        appearance: {
          mode: mode, dark: mode === "dark" ? current : "catppuccin-mocha", light: mode === "light" ? current : "catppuccin-latte",
          auto: { source: "off", lightAt: "07:00", darkAt: "19:00" }, next: null, place: "Berlin", sun: null
        }
      }
    }

    function apply(theme) {
      // The fallback is the shell before any theme is published.
      Palette.assign(root, theme.fallback ? Palette.fallback : theme)
      // The store as the shell starts with this theme, rather than as an
      // earlier row left it: an emptied catalog, the published selection,
      // then the helper's list.
      themeStore.model.clear()
      themeStore.catalog = ({ ok: false, current: "", themes: [] })
      themeStore.selection = theme.fallback ? "" : theme.id
      themeStore.selectionName = theme.fallback ? "" : theme.name
      themeStore.accept(catalogFor(theme))
    }

    function settle(item, tag) {
      var previous = null
      for (var attempt = 0; attempt < 20; attempt++) {
        wait(root.durationGlance)
        var frame = grabImage(item)
        if (previous !== null && frame.equals(previous)) return
        previous = frame
      }
      fail(tag + " never stopped moving")
    }

    function test_render_data() {
      var rows = []
      for (var i = 0; i < themes.length; i++)
        for (var j = 0; j < surfaces.length; j++)
          rows.push({ tag: themes[i].id + "/" + surfaces[j].name, theme: themes[i], surface: surfaces[j] })
      return rows
    }

    function test_render(data) {
      // A panel is hidden, never destroyed, on the desktop; hiding it first
      // lets its deferred measurements see that it is gone.
      if (scene.item) scene.item.visible = false
      wait(0)
      scene.active = false
      apply(data.theme)
      var state = Object.assign({}, idle, data.surface.state)
      for (var key in state) root[key] = state[key]
      scene.sourceComponent = components[data.surface.name]
      scene.active = true
      tryCompare(scene, "status", Loader.Ready)
      var item = scene.item.snapshotTarget || scene.item
      verify(item.width > 0 && item.height > 0, data.surface.name + " has a size")
      // Panels measure themselves in deferred calls and cards slide into
      // place, so the frame is taken once two in a row, longer apart than
      // the theme's longest transition, are the same.
      settle(item, data.tag)
      var saved = false
      verify(item.grabToImage(function(result) {
        saved = result.saveToFile(output + "/" + data.theme.id + "/" + data.surface.name + ".png")
      }))
      tryVerify(function() { return saved }, 5000, "saved " + data.tag)
      wait(20)
    }
  }
}
