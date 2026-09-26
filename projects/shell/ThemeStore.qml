import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

// The picker's view of `seele-theme`. The catalog, both slots, the mode, the
// schedule, publication and every reload belong to that helper; what is kept
// here is what one open picker is looking at, and the requests it has yet to
// send. The theme on screen is read from the selection the helper publishes,
// so a switch made anywhere else is seen here without polling.
//
// The picker is optimistic: the mode and both slots change here the moment
// the reader moves, and the helper is told after. Every request publishes
// files and reloads applications, so requests run one at a time, a burst of
// arrow presses settles for a moment first, and a request still waiting is
// replaced by a newer one of the same kind rather than queued behind it.
Scope {
  id: store
  // The carousel's entries, kept as one model reconciled by ID so a card that
  // moves slides where it goes instead of being built again.
  property alias model: rows
  ListModel { id: rows }

  readonly property string stateDirectory: Quickshell.env("SEELE_THEME_STATE")
    || (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/seele-theme"
  property var catalog: ({ ok: false, current: "", themes: [] })
  // The helper's own description of the slots, the mode and the schedule, as
  // the last reply left it.
  property var appearance: ({ mode: "dark", dark: "", light: "", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" }, next: null })
  property bool panelOpen: false
  property string query: ""
  // The mode the carousel edits, and both slots, ahead of the helper.
  property string mode: "dark"
  property var slots: ({ dark: "", light: "" })
  // The carousel shows the edited mode's presets unless the reader asks for all.
  property bool showAll: false
  property string highlighted: ""
  // Mode and both slots as the picker opened, so a cancel can put them back.
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
  readonly property string slot: store.slots[store.mode] || ""
  readonly property var carousel: Bridge.call("themes.carousel", [store.themes, store.slot, store.query, store.showAll ? "all" : store.mode])
  readonly property string focusedId: Bridge.call("themes.focus", [store.carousel, store.highlighted, store.slot])
  readonly property string scheduleText: Bridge.call("themes.schedule", [store.appearance])
  readonly property string autoSource: (store.appearance.auto || {}).source || "off"
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
  function known(id) {
    for (var i = 0; i < store.themes.length; i++) if (store.themes[i].id === id) return true
    return false
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
  // Takes the helper's word for the slots and the mode, unless the reader has
  // already moved on and the helper has yet to hear about it: then the reply
  // is kept and adopted once the last request is answered.
  function adopt(value) {
    appearance = value
    if (switching) return
    mode = value.mode
    slots = ({ dark: value.dark, light: value.light })
    // What an open picker first reads is what a cancel puts back.
    if (panelOpen && opened === null) opened = ({ mode: mode, dark: slots.dark, light: slots.light })
  }

  // Moves the carousel and switches the edited mode's slot to where it lands.
  function step(direction) {
    var next = Bridge.call("themes.step", [carousel, focusedId, direction])
    if (next === "" || next === focusedId) return
    choose(next, false)
  }
  // A deliberate choice (a click, Enter) is sent at once; a keyboard move
  // waits for the burst it may belong to.
  function choose(id, now) {
    if (id === "" || !known(id)) return
    highlighted = id
    var next = Object.assign({}, slots)
    next[mode] = id
    slots = next
    var request = { op: "slot", mode: mode, id: id }
    if (now) { settle.stop(); settling = null; send(request) }
    else { settling = request; settle.restart() }
  }
  function setMode(value) {
    if (value !== "dark" && value !== "light" || value === mode) return
    flushSettling()
    mode = value
    highlighted = ""
    send({ op: "mode", mode: value })
  }
  function setAuto(source, lightAt, darkAt) {
    if (["off", "sun", "schedule"].indexOf(source) < 0) return
    var times = source === "schedule" ? [lightAt || (appearance.auto || {}).lightAt || "07:00", darkAt || (appearance.auto || {}).darkAt || "19:00"] : []
    send({ op: "auto", source: source, times: times })
  }
  function toggleAll() {
    showAll = !showAll
  }
  // Puts back the mode and both slots as the picker found them.
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
  function flushSettling() {
    if (settling === null) return
    settle.stop()
    var request = settling
    settling = null
    send(request)
  }
  // Queues a request, replacing a waiting one of the same kind: a slot choice
  // replaces the waiting choice for that slot, a mode or schedule change the
  // waiting one, and a restore everything that waits.
  function send(request) {
    var waiting = request.op === "restore" ? [] : queue.filter(function (item) {
      return item.op !== request.op || (request.op === "slot" && item.mode !== request.mode)
    })
    queue = waiting.concat([request])
    next()
  }
  function command(request) {
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
  function resetFilters() {
    query = ""
    showAll = false
  }
  function openChanged() {
    if (!panelOpen) {
      // Closing mid-burst keeps the last move rather than dropping it.
      flushSettling()
      resetFilters()
      highlighted = ""
      actionError = ""
      reloadPending = ""
      opened = null
      return
    }
    opened = null
    refresh()
  }
  // Something else switched (the schedule at a boundary, the launcher): an
  // open picker reads the slots and the mode again, unless it is mid-switch
  // itself and that change is its own.
  function selectionMoved() {
    if (panelOpen && !switching) refresh()
  }

  onPanelOpenChanged: openChanged()
  onCarouselChanged: Models.reconcile(rows, carousel.items, "entry", function (item) { return item.id })
  onSelectionChanged: selectionMoved()

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
