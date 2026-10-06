import QtQuick
import "../shared" as Shared

// One decoded pixel per logical display pixel. Decode remains bounded even
// for enormous source files; this is not an unlimited-resolution image editor.
Flickable {
  id: view
  required property var theme
  property string source: ""
  property int sourceWidth: 0
  property int sourceHeight: 0
  readonly property bool pixelsAvailable: sourceWidth > 0 && sourceHeight > 0 && sourceWidth <= decodeLimit && sourceHeight <= decodeLimit
  property bool pixelMode: false
  readonly property int decodeLimit: 4096
  readonly property bool failed: picture.status === Image.Error
  objectName: "quickLookImage"
  clip: true
  boundsBehavior: Flickable.StopAtBounds
  interactive: pixelMode
  contentWidth: pixelMode ? Math.max(width, sourceWidth) : width
  contentHeight: pixelMode ? Math.max(height, sourceHeight) : height
  onSourceChanged: { pixelMode = false; contentX = 0; contentY = 0 }
  function pan(dx, dy) {
    contentX = Math.max(0, Math.min(Math.max(0, contentWidth - width), contentX + dx))
    contentY = Math.max(0, Math.min(Math.max(0, contentHeight - height), contentY + dy))
  }
  function imageKey(event) {
    if (event.modifiers === Qt.NoModifier && event.key === Qt.Key_Z) {
      event.accepted = true
      if (!event.isAutoRepeat && pixelsAvailable) { pixelMode = !pixelMode; contentX = 0; contentY = 0 }
      return true
    }
    if (pixelMode && event.modifiers === Qt.AltModifier) {
      var dx = event.key === Qt.Key_Left ? -64 : event.key === Qt.Key_Right ? 64 : 0
      var dy = event.key === Qt.Key_Up ? -64 : event.key === Qt.Key_Down ? 64 : 0
      if (dx || dy) { event.accepted = true; pan(dx, dy); return true }
    }
    return false
  }
  Image {
    id: picture
    objectName: "quickLookDecodedImage"
    source: view.source
    x: (view.contentWidth - width) / 2
    y: (view.contentHeight - height) / 2
    width: view.pixelMode ? view.sourceWidth : view.width
    height: view.pixelMode ? view.sourceHeight : view.height
    fillMode: Image.PreserveAspectFit
    asynchronous: true
    cache: false
    sourceSize: view.pixelMode ? Qt.size(view.sourceWidth, view.sourceHeight)
      : Qt.size(Math.max(1, Math.round(view.width)), Math.max(1, Math.round(view.height)))
  }
  Text {
    parent: view
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    text: view.pixelMode ? "1:1 pixels · Alt + arrows pan" : view.pixelsAvailable ? "Fit · Z for 1:1" : "Fit · 1:1 unavailable for this size or format"
    textFormat: Text.PlainText
    color: view.theme.subtext
    font.family: view.theme.fontFamily
    font.pixelSize: view.theme.textCaption
  }
}
