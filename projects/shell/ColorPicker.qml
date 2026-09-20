import QtQuick
import Quickshell
import Quickshell.Io
import "color-picker.js" as Colors

// The frozen screen colour picker. It borrows the URI picker's shape -- freeze
// every output into private runtime files, cover them with one layer surface
// per screen, and release everything on dismissal -- because the thing being
// sampled has to hold still while it is aimed at, and that problem was already
// solved once here.
Scope {
  id: picker

  // The shell's own palette, named by role. It is the only table a sampled
  // pixel is measured against, so the picker can never report a token this
  // desktop does not actually have.
  property var palette: ({})
  property bool active: false
  property bool presented: false
  property int generation: 0
  property var frames: []
  property var loaded: ({})
  // The point being aimed at, as a fraction of the output it is on. Normalized
  // rather than in pixels so it survives the surface and the capture
  // disagreeing about size under fractional scaling.
  property string output: ""
  property real pointX: 0.5
  property real pointY: 0.5
  property int asked: 0
  property int answered: 0
  property bool waiting: false
  property bool trailing: false
  // The colour under the point, as the policy describes it, and the exact text
  // Enter would copy for it right now.
  property var entry: null
  property string payloadText: ""
  property string format: "hex"
  property string commitFormat: "hex"
  // Session memory, in memory only. A colour sampled a minute ago is still
  // recoverable, and no pixel the user looked at is ever written to disk.
  property var history: []
  property string copying: ""
  property string copyFormat: ""
  // Held while wl-copy runs so the card does not blink out between the
  // overlay releasing the screen and the clipboard reporting what it did.
  property bool copyingNow: false
  property string error: ""
  property string notice: ""
  readonly property string detail: notice !== "" ? notice : error !== "" ? error
    : !presented ? "Freezing the screens…"
    : !entry ? "Move the pointer over a colour"
    : "Enter copies " + format.toUpperCase() + " · Tab changes format · hjkl nudges"
      + (history.length ? " · Ctrl + number recalls" : "") + " · Esc dismisses"

  function send(message) {
    if (worker.running) worker.write(JSON.stringify(message) + "\n")
  }

  function open() {
    dismissNotice()
    generation++
    active = true
    presented = false
    error = ""
    entry = null
    payloadText = ""
    format = "hex"
    commitFormat = "hex"
    output = ""
    pointX = 0.5
    pointY = 0.5
    asked = 0
    answered = 0
    waiting = false
    trailing = false
    frames = []
    loaded = ({})
    watchdog.restart()
    if (worker.running) requestCapture()
    else worker.running = true
  }

  function requestCapture() {
    send({ command: "capture", id: generation,
      outputs: Quickshell.screens.map(function(screen) { return screen.name }) })
  }

  // Releases the frozen images and the keyboard. The reading and history
  // survive it so the result card can still be read after the overlay is gone.
  function close() {
    if (!active) return
    active = false
    presented = false
    watchdog.stop()
    frames = []
    loaded = ({})
    output = ""
    waiting = false
    trailing = false
    send({ command: "cancel", id: generation })
  }

  function dismissNotice() {
    noticeTimer.stop()
    notice = ""
  }

  function showNotice(message) {
    notice = message
    noticeTimer.restart()
  }

  function frame(name) {
    for (var i = 0; i < frames.length; i++)
      if (frames[i].output === name) return frames[i]
    return null
  }

  function imageReady(name) {
    if (!active) return
    loaded[name] = true
    if (frames.length && frames.every(function(frame) { return picker.loaded[frame.output] })) {
      presented = true
      // The watchdog guards freezing the screens, not how long the user
      // spends aiming at them.
      watchdog.stop()
    }
  }

  function fail(message) {
    error = message
    close()
    showNotice(message)
  }

  function aim(name, x, y) {
    if (!active) return
    output = name
    pointX = Math.max(0, Math.min(1, x))
    pointY = Math.max(0, Math.min(1, y))
    flush()
  }

  // One request in flight at a time with the newest point coalesced behind it.
  // A sample is only a seek and three bytes, but a pointer crossing the screen
  // would still queue hundreds of answers that are stale before they are read,
  // and the readout would visibly trail the lens under them.
  function flush() {
    if (!active || output === "") return
    if (waiting) { trailing = true; return }
    trailing = false
    waiting = true
    asked++
    send({ command: "sample", id: generation, token: asked,
      output: output, x: pointX, y: pointY, commit: false })
  }

  // The copied colour is sampled again at the point the key was pressed rather
  // than reusing the coalesced readout, so what reaches the clipboard is never
  // one pointer movement out of date.
  function commit(named) {
    if (!active || output === "") return
    commitFormat = named || picker.format
    asked++
    send({ command: "sample", id: generation, token: asked,
      output: output, x: pointX, y: pointY, commit: true })
  }

  // The format the surface names is resolved against what this colour actually
  // offers, so the card can never promise a token for a pixel that is merely
  // near one.
  function present(described, named) {
    entry = described
    var resolved = Colors.payload(described, named || picker.format)
    format = resolved.format
    payloadText = resolved.text
  }

  function record(sample) {
    var described = Colors.describe(sample, picker.palette)
    var resolved = Colors.payload(described, picker.commitFormat)
    history = Colors.history(picker.history, described)
    close()
    present(described, picker.commitFormat)
    copy(resolved.text, resolved.format)
  }

  function accept(message) {
    if (!active || message.id !== generation || error !== "") return
    if (message.event === "frames") {
      frames = message.frames
    } else if (message.event === "sample") {
      if (message.commit) { record(message); return }
      waiting = false
      // An answer for a point the pointer has already left is dropped rather
      // than drawn, and so is a token already answered; the counter only ever
      // grows, so the newest answer is the only one that wins.
      if (message.token > answered) {
        answered = message.token
        present(Colors.describe(message, picker.palette))
      }
      if (trailing) flush()
    } else if (message.event === "error") {
      fail("Screen colour unavailable")
    }
  }

  function copy(text, named) {
    copying = text
    copyFormat = named
    copyingNow = true
    clipboard.payload = text
    clipboard.stdinEnabled = true
    clipboard.running = true
  }

  // Recalling an older pick copies it without disturbing the live reading or
  // reordering the history: the user is retrieving something, not sampling it
  // again.
  function recall(index) {
    var kept = picker.history[index]
    if (!kept) return
    var resolved = Colors.payload(kept, picker.format)
    copy(resolved.text, resolved.format)
  }

  function cycleFormat() {
    format = Colors.cycle(picker.entry, picker.format)
    if (picker.entry) present(picker.entry)
  }

  // Keys move the sample point by whole captured pixels. The pointer itself is
  // left where the user put it: warping it would move the cursor out from under
  // the hand that is about to press Enter.
  function nudge(dx, dy) {
    var target = frame(picker.output)
    if (!target) return
    aim(picker.output, picker.pointX + dx / target.width, picker.pointY + dy / target.height)
  }

  function key(event) {
    event.accepted = true
    if (event.key === Qt.Key_Escape) { close(); return }
    if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier)) return
    var step = event.modifiers & Qt.ShiftModifier ? 10 : 1
    if (event.key === Qt.Key_Left || event.key === Qt.Key_H) { nudge(-step, 0); return }
    if (event.key === Qt.Key_Right || event.key === Qt.Key_L) { nudge(step, 0); return }
    if (event.key === Qt.Key_Up || event.key === Qt.Key_K) { nudge(0, -step); return }
    if (event.key === Qt.Key_Down || event.key === Qt.Key_J) { nudge(0, step); return }
    if (event.isAutoRepeat) return
    if (event.key === Qt.Key_Tab) { cycleFormat(); return }
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { commit(); return }
    if (event.key >= Qt.Key_1 && event.key <= Qt.Key_9 && event.modifiers & Qt.ControlModifier)
      recall(event.key - Qt.Key_1)
  }

  Process {
    id: clipboard
    property string payload: ""
    command: ["wl-copy", "--type", "text/plain;charset=utf-8"]
    onStarted: {
      write(payload)
      stdinEnabled = false
      payload = ""
    }
    onExited: (exitCode, exitStatus) => {
      picker.copyingNow = false
      picker.showNotice(exitCode === 0 && exitStatus === 0
        ? "Copied " + picker.copying + " as " + picker.copyFormat.toUpperCase()
        : "Could not copy the colour")
    }
  }

  Process {
    id: worker
    command: ["seele-color-worker"]
    running: true
    stdinEnabled: true
    onStarted: if (picker.active) picker.requestCapture()
    stdout: SplitParser {
      onRead: data => {
        try { picker.accept(JSON.parse(data)) }
        catch (_) { if (picker.active) picker.fail("Screen colour unavailable") }
      }
    }
    onRunningChanged: {
      if (!running && picker.active) picker.fail("Screen colour unavailable")
    }
  }

  Timer {
    id: watchdog
    interval: 15000
    onTriggered: picker.fail("Freezing the screens timed out")
  }

  Timer {
    id: noticeTimer
    interval: 5000
    onTriggered: picker.dismissNotice()
  }

  Connections {
    target: Quickshell
    function onScreensChanged() {
      // Old geometry must never be sampled against a newly arranged desktop.
      if (picker.active) picker.close()
    }
  }
}
