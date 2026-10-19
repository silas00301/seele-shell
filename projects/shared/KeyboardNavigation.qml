import QtQuick
import Seele.Navigation

// One per application; the adapter only sees this process's own Qt windows.
Navigator {
  id: navigation
  required property var theme
  // The Navigator owns this object; parent below changes only its visual
  // parent. Destroying a delegate must not own the indicator's lifetime.
  property FocusRing indicator: FocusRing {
    // A surface that draws its own selection, such as the notification
    // panel's cursor, sets keyboardRing: false so it is not ringed twice. The
    // ring is then not laid inside it at all: a positioner such as a Column
    // cannot hold an anchored child.
    readonly property bool wanted: navigation.focusItem !== null && navigation.focusItem.keyboardRing !== false
    parent: wanted ? navigation.focusItem : null
    theme: navigation.theme
    shown: wanted
    z: 10000
  }
}
