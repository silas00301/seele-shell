import QtQuick
import Seele.Navigation

// One per application; the adapter only sees this process's own Qt windows.
Navigator {
  id: navigation
  required property var theme
  // The Navigator owns this object; parent below changes only its visual
  // parent. Destroying a delegate must not own the indicator's lifetime.
  property FocusRing indicator: FocusRing {
    parent: navigation.focusItem
    theme: navigation.theme
    // A surface that draws its own selection, such as the notification
    // panel's cursor, sets keyboardRing: false so it is not ringed twice.
    shown: navigation.focusItem !== null && navigation.focusItem.keyboardRing !== false
    z: 10000
  }
}
