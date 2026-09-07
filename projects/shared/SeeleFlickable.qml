import QtQuick

Flickable {
  id: seeleFlickable
  required property var theme
  boundsBehavior: Flickable.DragAndOvershootBounds
  flickDeceleration: seeleFlickable.theme.scrollDeceleration
  maximumFlickVelocity: seeleFlickable.theme.scrollFlickVelocity
  rebound: Transition {
    NumberAnimation { properties: "x,y"; duration: seeleFlickable.theme.scrollRebound; easing.type: Easing.OutCubic }
  }
}
