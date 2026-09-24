pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io

TextWorkbenchPanel {
  id: session
  property var activeTransport: null
  onPasteRequested: token => startClipboard("paste", token, "")
  onCopyRequested: (text, token) => startClipboard("copy", token, text)

  function startClipboard(action, token, payload) {
    const task = transport.createObject(session, {action: action, token: token, payload: payload})
    activeTransport = task
    task.running = true
  }

  // One QObject per request. Immutable identity and one completion guard mean
  // a timeout's late stream callback cannot impersonate a subsequent request.
  Component {
    id: transport
    Process {
      id: task
      required property string action
      required property int token
      required property string payload
      property bool completed: false
      property Timer deadline: Timer {
        running: true
        interval: 5000
        onTriggered: task.finish({ok: false, error: "Clipboard handoff timed out."})
      }
      command: ["seele-text-clipboard", action]
      stdinEnabled: action === "copy"
      function finish(response) {
        if (completed) return
        completed = true
        deadline.stop()
        running = false
        payload = ""
        if (session.activeTransport === task) {
          session.activeTransport = null
          if (action === "paste") session.receivePaste(token, response.ok === true, response.text || "", response.error || "")
          else session.receiveCopy(token, response.ok === true, response.error || "")
        }
        task.destroy()
      }
      onStarted: {
        if (task.action === "copy") {
          task.write(task.payload)
          task.payload = ""
          task.stdinEnabled = false
        }
      }
      stdout: StdioCollector {
        onStreamFinished: {
          let response
          try { response = JSON.parse(text) }
          catch (_) { response = {ok: false, error: "Clipboard handoff failed."} }
          task.finish(response)
        }
      }
    }
  }
  Component.onDestruction: {
    if (session.activeTransport) {
      session.activeTransport.completed = true
      session.activeTransport.payload = ""
      session.activeTransport.running = false
    }
  }
}
