import QtQuick
import Quickshell
import Quickshell.Io
import "uri-picker.js" as Uris

Scope {
  id: picker

  property bool active: false
  property bool presented: false
  property bool complete: false
  property int generation: 0
  property string digits: ""
  property bool confirmPending: false
  property string error: ""
  property string hoveredUri: ""
  property int failedAreas: 0
  property var frames: []
  property var allLinks: []
  property var loaded: ({})
  property alias links: linkModel
  readonly property string detail: error !== "" ? error
    : !complete ? "Finding links…"
    : allLinks.length === 0 ? (failedAreas ? "Could not read this screen" : "No links found")
    : digits !== "" ? "Number " + digits + " · Enter to open · Backspace to edit"
    : "Type a number to open · Esc to dismiss" + (failedAreas ? " · Some areas unreadable" : "")

  ListModel { id: linkModel }

  function send(message) {
    if (worker.running) worker.write(JSON.stringify(message) + "\n")
  }

  function open() {
    generation++
    active = true
    presented = false
    complete = false
    digits = ""
    confirmPending = false
    error = ""
    hoveredUri = ""
    failedAreas = 0
    frames = []
    loaded = ({})
    allLinks = []
    linkModel.clear()
    watchdog.restart()
    if (worker.running) requestCapture()
    else worker.running = true
  }

  function requestCapture() {
    send({ command: "capture", id: generation,
      outputs: Quickshell.screens.map(function(screen) { return screen.name }) })
  }

  function close() {
    if (!active) return
    active = false
    presented = false
    watchdog.stop()
    frames = []
    loaded = ({})
    allLinks = []
    linkModel.clear()
    hoveredUri = ""
    send({ command: "cancel", id: generation })
  }

  function frame(output) {
    for (var i = 0; i < frames.length; i++)
      if (frames[i].output === output) return frames[i]
    return null
  }

  function imageReady(output) {
    if (!active) return
    loaded[output] = true
    if (frames.length && frames.every(function(frame) { return picker.loaded[frame.output] }))
      presented = true
  }

  function fail(message) {
    error = message
    complete = true
    watchdog.stop()
    // Preserve successfully loaded frames while keeping Escape available.
    presented = true
    send({ command: "cancel", id: generation })
  }

  function accept(message) {
    if (!active || message.id !== generation || error !== "") return
    if (message.event === "frames") {
      frames = message.frames
    } else if (message.event === "links") {
      for (var i = 0; i < message.links.length; i++) linkModel.append(message.links[i])
      allLinks = allLinks.concat(message.links)
      if (confirmPending) choose(true)
    } else if (message.event === "done") {
      complete = true
      failedAreas = message.failedAreas
      watchdog.stop()
      choose(confirmPending)
    } else if (message.event === "error") {
      fail("Screen links unavailable · Esc to dismiss")
    }
  }

  function launch(link) {
    var uri = link.uri
    close()
    // A separate argv element, never shell source. The worker only supplies
    // absolute URIs, so visible text cannot become xdg-open options.
    Quickshell.execDetached(["xdg-open", uri])
  }

  function choose(confirm) {
    var link = Uris.selection(allLinks, digits, complete, confirm)
    if (link) launch(link)
    else if (!complete) confirmPending = confirm
  }

  function key(event) {
    event.accepted = true
    if (event.isAutoRepeat) return
    if (event.key === Qt.Key_Escape) { close(); return }
    if (event.key === Qt.Key_Backspace) { digits = digits.slice(0, -1); confirmPending = false; return }
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { choose(true); return }
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    if (event.key >= Qt.Key_0 && event.key <= Qt.Key_9 && digits.length < 6) {
      var digit = String(event.key - Qt.Key_0)
      if (digits === "" && digit === "0") return
      digits += digit
      choose(false)
    }
  }

  Process {
    id: worker
    command: ["seele-uri-worker"]
    running: true
    stdinEnabled: true
    onStarted: if (picker.active) picker.requestCapture()
    stdout: SplitParser {
      onRead: data => {
        try { picker.accept(JSON.parse(data)) }
        catch (_) { if (picker.active) picker.fail("Screen links unavailable · Esc to dismiss") }
      }
    }
    onRunningChanged: {
      if (!running && picker.active) picker.fail("Screen links unavailable · Esc to dismiss")
    }
  }

  Timer {
    id: watchdog
    interval: 15000
    onTriggered: picker.fail("Reading the screens timed out · Esc to dismiss")
  }

  Connections {
    target: Quickshell
    function onScreensChanged() {
      // Old geometry must never be painted over a newly arranged desktop.
      if (picker.active) picker.close()
    }
  }
}
