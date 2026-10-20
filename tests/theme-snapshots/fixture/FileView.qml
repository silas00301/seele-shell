import QtQuick

// An inert stand-in for Quickshell's FileView: nothing is read, so a store
// keeps whatever the fixture assigns to it directly.
QtObject {
  property string path: ""
  property bool watchChanges: false
  property bool printErrors: true
  signal fileChanged()
  signal loaded()
  signal loadFailed(var error)
  function reload() {}
  function text() { return "" }
}
