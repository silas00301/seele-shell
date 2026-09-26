import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

// The Themes surfaces' view of `seele-theme`: the floating switcher, the
// Control Center panel and its tile. The catalog, the light and dark themes,
// the mode, the schedule, publication and every reload belong to that helper;
// what is kept here is what one open picker is looking at, and the requests it
// has yet to send. The theme on screen is read from the selection the helper
// publishes, so a switch made anywhere else is seen here without polling.
//
// The switcher is optimistic: it moves at once and tells the helper after.
// Every request publishes files and reloads applications, so requests run one
// at a time, a burst of arrow presses settles for a moment first, and a
// request still waiting is replaced by a newer one of the same kind.
Scope {
  id: store
  // Every preset in the catalog's order, kept as one model reconciled by ID
  // so a card that moves slides where it goes instead of being built again.
  property alias model: rows
  ListModel { id: rows }

  readonly property string stateDirectory: Quickshell.env("SEELE_THEME_STATE")
    || (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/seele-theme"
  property var catalog: ({ ok: false, current: "", themes: [] })
  // The helper's own description of both themes, the mode and the schedule.
  property var appearance: ({ mode: "dark", dark: "", light: "", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" }, next: null })
  property bool panelOpen: false
  // The mode and both themes, ahead of the helper while requests are pending.
  property string mode: "dark"
  property var slots: ({ dark: "", light: "" })
  property string highlighted: ""
  // Mode and both themes as the picker opened, so Escape can put them back.
  property var opened: null
  property var queue: []
  property var running: null
  property var settling: null
  property string selection: ""
  property string selectionName: ""
  property string error: ""
  property string actionError: ""
  property string reloadPending: ""
  readonly property bool busy: running !== null || list.running
  readonly property bool switching: running !== null || queue.length > 0 || settling !== null
  readonly property string current: store.selection !== "" ? store.selection : store.catalog.current || ""
  readonly property var themes: store.catalog.themes || []
  readonly property int total: store.themes.length
  readonly property var carousel: Bridge.call("themes.carousel", [store.themes, store.current])
  readonly property string focusedId: Bridge.call("themes.focus", [store.carousel, store.highlighted, store.current])
  readonly property string autoSource: (store.appearance.auto || {}).source || "off"
  // Light, Dark or Auto, as the Control Center shows it.
  readonly property string appearanceChoice: store.autoSource !== "off" ? "auto" : store.mode
  readonly property string appearanceGlyph: ({ light: "󰖙", dark: "󰖔", auto: "󰔎" })[store.appearanceChoice] || "󰔎"
  readonly property string appearanceLabel: ({ light: "Light", dark: "Dark", auto: "Auto" })[store.appearanceChoice] || ""
  readonly property string scheduleText: Bridge.call("themes.schedule", [store.appearance])
  // What the bar-level surfaces name. The published selection carries its own
  // display name, so a tile says which theme is applied before this picker has
  // ever been opened and the catalog read.
  readonly property string currentName: store.selectionName !== "" ? store.selectionName
    : Bridge.call("themes.name", [store.themes, store.current])

  function nameOf(id) {
    return id === "" ? "" : Bridge.call("themes.name", [store.themes, id])
  }
  function failure(code) {
    return Bridge.call("themes.failure", [code || ""])
  }
  function find(id) {
    for (var i = 0; i < store.themes.length; i++) if (store.themes[i].id === id) return store.themes[i]
    return null
  }
  function refresh() {
    if (list.running) return
    list.running = true
  }
  function accept(value) {
    var next = Bridge.call("themes.catalog", [value])
    if (!next.ok) { error = failure(next.error); return }
    catalog = next
    error = ""
    if (value.appearance) adopt(value.appearance)
  }
  // Takes the helper's word for both themes and the mode, unless the reader
  // is ahead of it: then the reply is kept and adopted once all is answered.
  function adopt(value) {
    appearance = value
    if (switching) return
    mode = value.mode
    slots = ({ dark: value.dark, light: value.light })
    // What an open picker first reads is what Escape puts back.
    if (panelOpen && opened === null) opened = ({ mode: mode, dark: slots.dark, light: slots.light })
  }

  // The switcher: moving picks the neighbouring preset, which becomes the
  // theme for its own mode, and the desktop follows at once.
  function step(direction) {
    var next = Bridge.call("themes.step", [carousel, focusedId, direction])
    if (next === "" || next === focusedId) return
    choose(next, false)
  }
  // A deliberate choice (a click) is sent at once; a keyboard move waits for
  // the burst it may belong to.
  function choose(id, now) {
    var preset = find(id)
    if (!preset) return
    highlighted = id
    mode = preset.mode
    var next = Object.assign({}, slots)
    next[preset.mode] = id
    slots = next
    var request = { op: "pick", id: id }
    if (now) { settle.stop(); settling = null; send(request) }
    else { settling = request; settle.restart() }
  }
  // Enter: whatever the switcher last moved to is sent now rather than after
  // the burst settles.
  function keep() {
    flushSettling()
  }
  // Escape: the mode and both themes as the picker found them.
  function cancel() {
    settle.stop()
    settling = null
    if (opened === null) return
    mode = opened.mode
    slots = ({ dark: opened.dark, light: opened.light })
    highlighted = ""
    queue = []
    send({ op: "restore", mode: opened.mode, dark: opened.dark, light: opened.light })
  }

  // The Control Center: Light, Dark or Auto; the schedule's source and
  // times; and which preset each mode wears.
  function setAppearance(value) {
    if (value === appearanceChoice) return
    // The sun where the timezone names a city, the saved times otherwise.
    if (value === "auto") { setAuto(appearance.place ? "sun" : "schedule"); return }
    if (value !== "dark" && value !== "light") return
    flushSettling()
    if (autoSource !== "off") send({ op: "auto", source: "off", times: [] })
    mode = value
    send({ op: "mode", mode: value })
  }
  // The tile's knob steps through the three in the order the segments show.
  function cycleAppearance() {
    setAppearance(({ light: "dark", dark: "auto", auto: "light" })[appearanceChoice] || "light")
  }
  function setAuto(source, lightAt, darkAt) {
    if (["off", "sun", "schedule"].indexOf(source) < 0) return
    var times = source === "schedule"
      ? [lightAt || (appearance.auto || {}).lightAt || "07:00", darkAt || (appearance.auto || {}).darkAt || "19:00"]
      : []
    send({ op: "auto", source: source, times: times })
  }
  // Makes the preset on screen the theme for `slot`, whatever its own mode.
  function useCurrentFor(slot) {
    if ((slot !== "dark" && slot !== "light") || current === "" || slots[slot] === current) return
    var next = Object.assign({}, slots)
    next[slot] = current
    slots = next
    send({ op: "slot", mode: slot, id: current })
  }

  function flushSettling() {
    if (settling === null) return
    settle.stop()
    var request = settling
    settling = null
    send(request)
  }
  // Queues a request, replacing a waiting one of the same kind (a slot only
  // for the same mode), and letting a restore replace everything that waits.
  function send(request) {
    var waiting = request.op === "restore" ? [] : queue.filter(function (item) {
      return item.op !== request.op || (request.op === "slot" && item.mode !== request.mode)
    })
    queue = waiting.concat([request])
    next()
  }
  function command(request) {
    if (request.op === "pick") return ["seele-theme", "pick", request.id]
    if (request.op === "slot") return ["seele-theme", "slot", request.mode, request.id]
    if (request.op === "mode") return ["seele-theme", "mode", request.mode]
    if (request.op === "restore") return ["seele-theme", "restore", request.mode, request.dark, request.light]
    return ["seele-theme", "auto", request.source].concat(request.times || [])
  }
  function next() {
    if (running !== null || queue.length === 0) return
    running = queue[0]
    queue = queue.slice(1)
    actionError = ""
    // The command carries the reviewed request rather than a binding, so
    // nothing starts a second helper by changing a property.
    set.command = command(running)
    set.running = true
    guard.restart()
  }
  function answered(value) {
    reloadPending = Bridge.call("themes.pending", [(value && value.pending) || []])
    if (value && value.appearance) adopt(value.appearance)
  }
  function finished(code) {
    if (code !== 0 && actionError === "") actionError = failure("failed")
    running = null
    guard.stop()
    next()
    if (switching) return
    // The reader has stopped: the helper's last word stands, and after a
    // refusal it is read again rather than assumed.
    if (code === 0) adopt(appearance)
    else refresh()
  }
  function openChanged() {
    if (!panelOpen) {
      // Closing mid-burst keeps the last move rather than dropping it.
      flushSettling()
      highlighted = ""
      actionError = ""
      reloadPending = ""
      opened = null
      return
    }
    opened = null
    refresh()
  }
  // Something else switched or chose (the schedule at a boundary, the
  // launcher): while nothing of its own is pending, the store reads both
  // themes, the mode and the schedule again, so the tile's knob and the panel
  // stay true while no picker is open.
  function changedElsewhere() {
    if (!switching) refresh()
  }

  onPanelOpenChanged: openChanged()
  onSelectionChanged: changedElsewhere()
  Component.onCompleted: refresh()
  onCarouselChanged: Models.reconcile(rows, carousel.items, "entry", function (item) { return item.id })

  // The published selection, watched rather than polled: the helper renames it
  // into place, and the shell's own palette follows that same file.
  FileView {
    path: store.stateDirectory + "/selection.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      try {
        var picked = Bridge.call("themes.selected", [JSON.parse(text())])
        store.selection = picked.id
        store.selectionName = picked.name
      } catch (_) {
        store.selection = ""
        store.selectionName = ""
      }
    }
    onLoadFailed: {
      store.selection = ""
      store.selectionName = ""
    }
  }

  // The helper's preferences, watched for the changes that publish nothing:
  // a schedule turned on, a theme given to the mode not in use.
  FileView {
    path: store.stateDirectory + "/preferences.json"
    watchChanges: true
    printErrors: false
    onFileChanged: store.changedElsewhere()
  }

  Process {
    id: list
    command: ["seele-theme", "list"]
    stdout: StdioCollector {
      onStreamFinished: {
        try { store.accept(JSON.parse(text)) } catch (_) { store.error = store.failure("invalid") }
      }
    }
    stderr: StdioCollector {}
    // A helper that could not answer leaves the catalog that was already read,
    // and says so instead of emptying the picker.
    onExited: function (code) {
      if (code !== 0) store.error = store.failure("unavailable")
    }
  }
  Process {
    id: set
    stdout: StdioCollector {
      onStreamFinished: {
        try { store.answered(JSON.parse(text)) } catch (_) { store.actionError = store.failure("invalid") }
      }
    }
    stderr: StdioCollector {}
    onExited: function (code) { store.finished(code) }
  }
  // Long enough to swallow a key's autorepeat, short enough to read as instant.
  Timer {
    id: settle
    interval: 160
    onTriggered: store.flushSettling()
  }
  // Publication and its reloads are bounded work. What this catches is a helper
  // that stopped answering: it is ended, and the picker says what it does not
  // know rather than waiting on it.
  Timer {
    id: guard
    interval: 30000
    onTriggered: {
      store.actionError = store.failure("timeout")
      set.running = false
    }
  }
}
