import QtQuick

// An inert stand-in for Quickshell's Process: starting it runs nothing, so a
// snapshot cannot depend on the machine that renders it.
QtObject {
  property var command: []
  property var environment: ({})
  property string workingDirectory: ""
  property bool running: false
  property QtObject stdout: null
  property QtObject stderr: null
  property bool stdinEnabled: false
  signal started()
  signal exited(int code, int status)
  function startDetached() {}
  function write(data) {}
  function signal(number) {}
}
