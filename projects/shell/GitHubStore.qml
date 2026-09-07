import QtQuick
import Quickshell
import Quickshell.Io
import "github.js" as GitHub

Item {
  id: store

  property bool active: false
  property var snapshot: GitHub.initial(String(Quickshell.env("SEELE_GITHUB_HOST") || "github.com"))
  property double lastAttempt: 0
  property bool received: false
  property bool requestPending: false
  property int requestId: 0
  property var worker: null
  readonly property bool refreshing: requestPending
  readonly property bool canRefresh: !requestPending && !cooldown.running

  onActiveChanged: if (active) refresh(false)

  function refresh(manual) {
    if ((!manual && !active) || requestPending || !GitHub.due(lastAttempt, Date.now(), snapshot.state, !!manual)) return false
    lastAttempt = Date.now()
    received = false
    requestPending = true
    requestId += 1
    cooldown.restart()
    watchdog.restart()
    worker = workerFactory.createObject(store, { token: requestId })
    if (!worker) {
      finish(requestId, "GitHub refresh could not start.")
      return false
    }
    worker.running = true
    return true
  }

  function accept(token, output) {
    if (!requestPending || token !== requestId) return
    received = true
    try {
      snapshot = GitHub.receive(snapshot, JSON.parse(output))
    } catch (_) {
      snapshot = GitHub.receive(snapshot, { state: "error", message: "GitHub returned an unreadable response." })
    }
  }

  function finish(token, message) {
    if (!requestPending || token !== requestId) return
    watchdog.stop()
    if (message || !received)
      snapshot = GitHub.receive(snapshot, { state: "error", message: message || "GitHub refresh could not start." })
    const previous = worker
    worker = null
    requestPending = false
    if (previous) {
      previous.running = false
      previous.destroy()
    }
  }

  function openPull(url) {
    if (!GitHub.safeUrl(url, snapshot.host)) return false
    Qt.openUrlExternally(url)
    return true
  }

  Timer {
    id: cooldown
    interval: 5000
  }

  Timer {
    interval: 60000
    running: store.active
    repeat: true
    onTriggered: store.refresh(false)
  }

  Timer {
    id: watchdog
    interval: 35000
    onTriggered: store.finish(store.requestId, "GitHub refresh timed out. Try again.")
  }

  Component {
    id: workerFactory
    Process {
      id: requestWorker
      property int token: 0
      command: ["seele-github-status"]
      stdout: StdioCollector {
        waitForEnd: true
        onStreamFinished: store.accept(requestWorker.token, text)
      }
      onExited: {
        const completedToken = token
        Qt.callLater(function() { store.finish(completedToken, "") })
      }
    }
  }
}
