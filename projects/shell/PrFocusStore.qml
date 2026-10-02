import QtQuick
import Quickshell
import Quickshell.Io
import "github.js" as GitHub

// One configured pull request. Entering focus is explicit; leaving it drops
// the pin and releases notifications that were held for it.
Item {
  id: store

  property string host: String(Quickshell.env("SEELE_GITHUB_HOST") || "github.com")
  property string url: ""
  property bool active: false
  property var snapshot: ({
    state: "idle", message: "", host: store.host, number: 0, title: "", url: "",
    repository: "", draft: false, checks: "", review: "", comment: "", commentAuthor: "", updatedAt: ""
  })
  property double lastAttempt: 0
  property bool received: false
  property bool requestPending: false
  property int requestId: 0
  property var worker: null
  readonly property bool configured: GitHub.safeUrl(url, host)
  readonly property string label: {
    if (snapshot.repository && snapshot.number) return snapshot.repository + " #" + snapshot.number
    var match = String(url).match(/\/pull\/([1-9][0-9]*)$/)
    return match ? "Pull request #" + match[1] : "Pull request"
  }

  function idleSnapshot() {
    return {
      state: "idle", message: "", host: host, number: 0, title: "", url: url,
      repository: "", draft: false, checks: "", review: "", comment: "", commentAuthor: "", updatedAt: ""
    }
  }

  function enter() {
    if (!configured) return false
    if (!active) {
      active = true
      refresh(true)
    }
    return true
  }

  function exit() {
    if (!active && !requestPending) return true
    active = false
    requestId += 1
    requestPending = false
    received = false
    watchdog.stop()
    var previous = worker
    worker = null
    if (previous) {
      previous.running = false
      previous.destroy()
    }
    snapshot = idleSnapshot()
    return true
  }

  function toggle() {
    return active ? exit() : enter()
  }

  function refresh(manual) {
    if (!active || !configured || requestPending) return false
    if (!manual && Date.now() - lastAttempt < 60000) return false
    lastAttempt = Date.now()
    received = false
    requestPending = true
    requestId += 1
    watchdog.restart()
    worker = workerFactory.createObject(store, { token: requestId, target: url })
    if (!worker) {
      finish(requestId, "GitHub refresh could not start.")
      return false
    }
    worker.running = true
    return true
  }

  function accept(token, output) {
    if (!active || !requestPending || token !== requestId) return
    received = true
    var next = null
    try { next = JSON.parse(output) } catch (_) { next = null }
    if (!next || !next.state || next.state !== "ready") {
      snapshot = {
        state: next && next.state ? next.state : "error",
        message: next && next.message ? next.message : "GitHub returned an unreadable response.",
        host: host, url: url,
        number: snapshot.number || 0, title: snapshot.title || "", repository: snapshot.repository || "",
        draft: !!snapshot.draft, checks: snapshot.checks || "", review: snapshot.review || "",
        comment: snapshot.comment || "", commentAuthor: snapshot.commentAuthor || "", updatedAt: snapshot.updatedAt || ""
      }
      return
    }
    snapshot = next
  }

  function finish(token, message) {
    if (!requestPending || token !== requestId) return
    watchdog.stop()
    if (message || !received)
      snapshot = { state: "error", message: message || "GitHub refresh could not start.", url: url, checks: snapshot.checks || "", comment: snapshot.comment || "", commentAuthor: snapshot.commentAuthor || "", repository: snapshot.repository || "", number: snapshot.number || 0, title: snapshot.title || "" }
    var previous = worker
    worker = null
    requestPending = false
    if (previous) {
      previous.running = false
      previous.destroy()
    }
  }

  function applyConfig(text) {
    if (String(Quickshell.env("SEELE_FOCUS_PULL") || "")) return
    try {
      var data = JSON.parse(text)
      if (data && data.url) url = String(data.url)
    } catch (_) {}
  }

  Component.onCompleted: {
    var fromEnv = String(Quickshell.env("SEELE_FOCUS_PULL") || "")
    if (fromEnv) url = fromEnv
  }

  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || (Quickshell.env("HOME") + "/.config")) + "/seele-shell/focus.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: store.applyConfig(text())
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
      property string target: ""
      command: ["seele-github-status", "focus", target]
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
