import QtQuick
import QtQuick.Controls

ScrollBar {
  required property var theme
  id: scrollBar

  required property bool popupHovered

  policy: ScrollBar.AsNeeded
  implicitWidth: scrollBar.theme.scrollGutter
  padding: 2
  opacity: popupHovered || pressed ? 1 : 0
  enabled: popupHovered || pressed
  visible: policy !== ScrollBar.AlwaysOff && (policy === ScrollBar.AlwaysOn || size < 1)
  // An attached indicator is a sibling of the view's content, so without
  // this it renders behind the rows it belongs to.
  z: 2
  // A list of every timezone would otherwise grind the handle down to a few
  // pixels.
  minimumSize: height > 0 ? Math.min(0.5, 36 / height) : 0

  background: Item {}
  contentItem: Rectangle {
    implicitWidth: scrollBar.theme.scrollGutter - 4
    radius: width / 2
    opacity: 1
    color: scrollBar.theme.alpha(scrollBar.theme.text, scrollBar.pressed ? 0.6 : 0.32)
  }
}
