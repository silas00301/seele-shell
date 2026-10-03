import QtQuick

ListView {
  id: seeleListView
  required property var theme
  activeFocusOnTab: true
  keyNavigationEnabled: true
  boundsBehavior: Flickable.DragAndOvershootBounds
  flickDeceleration: seeleListView.theme.scrollDeceleration
  maximumFlickVelocity: seeleListView.theme.scrollFlickVelocity
  rebound: Transition {
    NumberAnimation { properties: "x,y"; duration: seeleListView.theme.scrollRebound; easing.type: Easing.OutCubic }
  }
}
