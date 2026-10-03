import QtQuick

// Pointer geometry stays unchanged. Every custom action shares keyboard
// activation and can express secondary clicks without pointer emulation.
MouseArea {
  id: area
  activeFocusOnTab: !parent.activeFocusOnTab
  property bool activateOnPress: false
  signal triggered(var event)
  signal keyPressed(var event)
  onPressed: mouse => { if (activateOnPress) triggered(mouse) }
  onClicked: mouse => { if (!activateOnPress) triggered(mouse) }
  Keys.onPressed: event => {
    event.accepted = false
    area.keyPressed(event)
    if (event.accepted) return
    var secondary = event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && event.modifiers === Qt.ShiftModifier)
    if (!secondary && event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space) return
    if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier)) return
    var middle = (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
      && (event.modifiers & Qt.ShiftModifier) && (acceptedButtons & Qt.MiddleButton)
    var button = secondary ? Qt.RightButton : middle ? Qt.MiddleButton : Qt.LeftButton
    if (!(acceptedButtons & button)) return
    event.accepted = true
    if (!event.isAutoRepeat) area.triggered({button: button, modifiers: event.modifiers, x: width / 2, y: height / 2, accepted: true})
  }
}
