import QtQuick
import QtQuick.Controls
import QtTest
import "shared" as Shared

Item {
  id: scene
  width: 640
  height: 480
  property int primary: 0
  property int secondary: 0
  property int commits: 0
  property int selected: -1
  QtObject {
    id: theme
    property color accent: "#aabbff"
    property int radius: 8
    function alpha(color, value) { return Qt.rgba(color.r, color.g, color.b, value) }
  }
  Shared.KeyboardNavigation { id: navigation; theme: theme }
  TextField { id: query; x: 20; y: 20; width: 200; property bool keyboardSearch: true }
  Rectangle {
    x: 20; y: 80; width: 100; height: 40
    Shared.ActionArea {
      id: first
      anchors.fill: parent
      acceptedButtons: Qt.LeftButton | Qt.RightButton
      onTriggered: event => {
        if (event.button === Qt.RightButton) scene.secondary++
        else scene.primary++
      }
    }
  }
  Button { id: right; x: 150; y: 80; width: 100; height: 40; text: "Right"; onClicked: scene.primary++ }
  Button { id: below; x: 20; y: 150; width: 100; height: 40; text: "Below" }
  Slider {
    id: slider; x: 150; y: 150; width: 200; from: 0; to: 100; stepSize: 5
    Keys.onReleased: event => { if (event.key === Qt.Key_Right) scene.commits++ }
  }
  Button { id: disabled; x: 20; y: 200; enabled: false; text: "Disabled" }
  Button { id: hidden; x: 20; y: 250; visible: false; text: "Hidden" }
  ComboBox { id: choice; x: 20; y: 300; model: ["One", "Two", "Three"] }
  Item {
    id: menu
    x: 270; y: 240; width: 150; height: 100
    visible: false
    readonly property bool keyboardScope: true
    Button { id: menuFirst; width: 150; height: 40; text: "First" }
    Button { id: menuLast; y: 50; width: 150; height: 40; text: "Last" }
  }
  ListView {
    id: list
    x: 380; y: 20; width: 200; height: 150
    model: 100
    activeFocusOnTab: true
    keyNavigationEnabled: true
    clip: true
    delegate: Rectangle {
      required property int index
      width: list.width; height: 40
      Shared.ActionArea {
        anchors.fill: parent
        onTriggered: scene.selected = parent.index
      }
    }
  }
  TestCase {
    name: "KeyboardNavigation"
    when: windowShown
    function init() {
      choice.popup.close()
      menu.visible = false
      query.text = ""
      scene.primary = 0
      scene.secondary = 0
      scene.commits = 0
      first.forceActiveFocus()
    }
    function test_activate() {
      keyClick(Qt.Key_Return)
      compare(scene.primary, 1)
      keyClick(Qt.Key_Space)
      compare(scene.primary, 2)
      keyClick(Qt.Key_Menu)
      compare(scene.secondary, 1)
      keyClick(Qt.Key_F10, Qt.ShiftModifier)
      compare(scene.secondary, 2)
    }
    function test_native_button_enter() {
      right.forceActiveFocus()
      keyClick(Qt.Key_Return)
      compare(scene.primary, 1)
    }
    function test_press_action_does_not_double_activate() {
      first.activateOnPress = true
      mouseClick(first, 10, 10)
      compare(scene.primary, 1)
      first.forceActiveFocus()
      keyClick(Qt.Key_Return)
      compare(scene.primary, 2)
      first.activateOnPress = false
    }
    function test_tab_from_input_indicates_destination() {
      query.forceActiveFocus()
      keyClick(Qt.Key_Tab)
      verify(first.activeFocus)
      compare(navigation.focusItem, first)
    }
    function test_geometry() {
      keyClick(Qt.Key_L)
      verify(right.activeFocus)
      keyClick(Qt.Key_H)
      verify(first.activeFocus)
      keyClick(Qt.Key_J)
      verify(below.activeFocus)
      keyClick(Qt.Key_K)
      verify(first.activeFocus)
      compare(navigation.focusItem, first)
    }
    function test_typing_and_escape() {
      query.forceActiveFocus()
      for (var ch of "hjkl/gi") keyClick(ch)
      compare(query.text, "hjkl/gi")
      keyClick(Qt.Key_Escape)
      verify(first.activeFocus)
      keyClick(Qt.Key_L)
      verify(right.activeFocus)
      compare(query.text, "hjkl/gi")
      keyClick(Qt.Key_I)
      keyClick("j")
      compare(query.text, "hjkl/gij")
    }
    function test_leave_editor() {
      query.forceActiveFocus()
      keyClick(Qt.Key_J, Qt.AltModifier)
      verify(first.activeFocus)
      keyClick(Qt.Key_Slash)
      verify(query.activeFocus)
      first.forceActiveFocus()
      keyClick(Qt.Key_Slash, Qt.ShiftModifier)
      verify(query.activeFocus)
    }
    function test_tab_and_boundaries() {
      keyClick(Qt.Key_Tab)
      verify(right.activeFocus)
      keyClick(Qt.Key_Backtab)
      verify(first.activeFocus)
      keyClick(Qt.Key_G)
      keyClick(Qt.Key_G)
      verify(query.activeFocus)
      first.forceActiveFocus()
      keyClick(Qt.Key_G, Qt.ShiftModifier)
      verify(navigation.focusItem !== disabled && navigation.focusItem !== hidden)
    }
    function test_slider_release() {
      slider.value = 50
      slider.forceActiveFocus()
      keyClick(Qt.Key_L)
      compare(slider.value, 55)
      compare(scene.commits, 1)
    }
    function test_popup_scope() {
      choice.forceActiveFocus()
      keyClick(Qt.Key_Space)
      tryCompare(choice.popup, "opened", true)
      keyClick(Qt.Key_J)
      keyClick(Qt.Key_Return)
      compare(choice.currentIndex, 1)
      compare(choice.popup.opened, false)
    }
    function test_custom_menu_scope() {
      menu.visible = true
      menuFirst.forceActiveFocus()
      keyClick(Qt.Key_J)
      verify(menuLast.activeFocus)
      keyClick(Qt.Key_Tab)
      verify(menuFirst.activeFocus)
      keyClick(Qt.Key_G, Qt.ShiftModifier)
      verify(menuLast.activeFocus)
      keyClick(Qt.Key_H, Qt.AltModifier)
      verify(menuLast.activeFocus)
    }
    function test_long_list_activation() {
      list.currentIndex = 0
      list.positionViewAtBeginning()
      list.currentItem.children[0].forceActiveFocus()
      keyClick(Qt.Key_J)
      keyClick(Qt.Key_Return)
      compare(scene.selected, 1)
      for (var index = 0; index < 20; ++index) keyClick(Qt.Key_J)
      keyClick(Qt.Key_Return)
      compare(scene.selected, 21)
      verify(list.contentY > 0)
      keyClick(Qt.Key_D, Qt.ControlModifier)
      keyClick(Qt.Key_Return)
      verify(scene.selected > 21)
      keyClick(Qt.Key_G, Qt.ShiftModifier)
      keyClick(Qt.Key_Return)
      compare(scene.selected, 99)
      keyClick(Qt.Key_G)
      keyClick(Qt.Key_G)
      keyClick(Qt.Key_Return)
      compare(scene.selected, 0)
    }
  }
}
