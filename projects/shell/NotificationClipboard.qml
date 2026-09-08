import QtQuick
import Quickshell.Io
import "notification-copy.js" as NotificationCopy

Item {
  id: clipboard

  property string status: "idle"
  property string key: ""
  property string message: ""
  property int requestId: 0
  property var worker: null
  readonly property bool pending: status === "pending"

  function copy(entry, part) {
    if (pending) return false
    const result = NotificationCopy.payload(entry, part)
    key = String(entry && entry.id) + ":" + part
    message = ""
    feedback.stop()
    if (!result.ok) {
      status = "error"
      message = result.message
      feedback.restart()
      return false
    }
    status = "pending"
    requestId += 1
    watchdog.restart()
    worker = workerFactory.createObject(clipboard, { token: requestId, payload: result.value })
    if (!worker) {
      complete(requestId, false, "Clipboard could not start.")
      return false
    }
    worker.running = true
    return true
  }

  function complete(token, success, detail) {
    if (!pending || token !== requestId) return
    watchdog.stop()
    const previous = worker
    worker = null
    status = success ? "success" : "error"
    message = success ? "Copied" : detail || "Could not copy text. Try again."
    if (previous) {
      previous.payload = ""
      previous.running = false
      previous.destroy()
    }
    feedback.restart()
  }

  Timer {
    id: feedback
    interval: 3000
    onTriggered: { clipboard.status = "idle"; clipboard.key = ""; clipboard.message = "" }
  }

  Timer {
    id: watchdog
    interval: 5000
    onTriggered: clipboard.complete(clipboard.requestId, false, "Clipboard did not respond. Try again.")
  }

  Component {
    id: workerFactory
    Process {
      id: copyWorker
      property int token: 0
      property string payload: ""
      command: ["wl-copy", "--type", "text/plain;charset=utf-8"]
      stdinEnabled: true
      function send() {
        write(payload)
        payload = ""
        stdinEnabled = false
      }
      onStarted: send()
      onExited: (exitCode, exitStatus) => {
        const completedToken = token
        const success = exitCode === 0 && exitStatus === 0
        Qt.callLater(function() { clipboard.complete(completedToken, success, "") })
      }
    }
  }
}
