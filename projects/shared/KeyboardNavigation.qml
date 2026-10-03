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
    shown: navigation.focusItem !== null
    z: 10000
  }
}
