import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

TestCase {
  id: testCase
  name: "ColorLabInteraction"
  when: windowShown
  visible: true
  width: 480
  height: 760
  Shared.Theme { id: theme }
  Shell.ColorLabPanel {
    id: panel
    theme: theme
    width: theme.colorLabWidth - theme.panelMargin * 2
  }
  SignalSpy { id: copied; target: panel; signalName: "copyRequested" }
  SignalSpy { id: dismissed; target: panel; signalName: "dismissed" }
  function child(name) {
    var item = findChild(panel, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function init() {
    panel.foreground = "#000"
    panel.background = "#fff"
    panel.target = "foreground"
    panel.format = "hex"
    panel.notice = ""
    panel.sampledColor = ""
    copied.clear(); dismissed.clear()
    panel.focusInput()
    wait(30)
  }
  function test_native_projection_and_alpha_rejection() {
    compare(panel.result.ratioText, "21.00:1")
    compare(panel.result.grades.length, 5)
    compare(panel.tonalPalette.length, 9)
    panel.foreground = "#777"
    compare(panel.result.grades[0].pass, false)
    compare(panel.result.grades[2].pass, true)
    panel.foreground = "#000000ff"
    compare(panel.result.valid, false)
    compare(child("copyButton").enabled, false)
    panel.copy("css")
    compare(copied.count, 0)
    panel.background = "rgba(0,0,0,1)"
    compare(panel.result.backgroundValid, false)
  }
  function test_real_input_edit_updates_preview() {
    var field = child("foregroundField")
    verify(field.activeFocus)
    keyClick(Qt.Key_A, Qt.ControlModifier)
    keyClick(Qt.Key_NumberSign)
    keyClick(Qt.Key_F); keyClick(Qt.Key_0); keyClick(Qt.Key_0)
    compare(panel.foreground, "#f00")
    compare(panel.result.foreground.hex, "#ff0000")
    compare(panel.result.ratioText, "4.00:1")
    compare(field.maximumLength, 96)
  }
  function test_swap_button_and_keyboard() {
    mouseClick(child("swapButton"))
    compare(panel.foreground, "#fff")
    compare(panel.background, "#000")
    panel.focusInput()
    keyClick(Qt.Key_S, Qt.ControlModifier)
    compare(panel.foreground, "#000")
    compare(panel.background, "#fff")
  }
  function test_keyboard_copy_formats_and_target() {
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(copied.count, 1)
    compare(copied.signalArguments[0][0], "#000000")
    keyClick(Qt.Key_2, Qt.AltModifier)
    verify(child("backgroundField").activeFocus)
    compare(panel.target, "background")
    panel.format = "rgb"
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(copied.signalArguments[1][0], "rgb(255, 255, 255)")
    keyClick(Qt.Key_Return, Qt.ControlModifier | Qt.ShiftModifier)
    compare(copied.signalArguments[2][0], "color: #000000;\nbackground-color: #ffffff;")
    keyClick(Qt.Key_1, Qt.AltModifier)
    verify(child("foregroundField").activeFocus)
    keyClick(Qt.Key_Escape)
    compare(dismissed.count, 1)
  }
  function test_button_copy_palette_and_explicit_sample() {
    panel.sampledColor = "#ff0000"
    wait(30)
    compare(panel.foreground, "#000") // Merely exposing a sample does not apply it.
    mouseClick(child("sampleButton"), 10, 10)
    compare(panel.result.foreground.hex, "#ff0000")
    panel.target = "background"
    wait(30) // New palette delegates must complete Qt layout before pointer events.
    var swatch = panel.tonalPalette[3].hex
    mouseClick(child("swatch40"))
    compare(panel.background, swatch)
    wait(30)
    mouseClick(child("formathsl"))
    mouseClick(child("copyButton"))
    compare(copied.count, 1)
    compare(copied.signalArguments[0][0], panel.result.background.hsl)
  }
  function test_copy_shortcut_from_format_button() {
    child("formathsl").forceActiveFocus()
    keyClick(Qt.Key_Return, Qt.ControlModifier | Qt.ShiftModifier)
    compare(copied.count, 1)
    compare(copied.signalArguments[0][0], "color: #000000;\nbackground-color: #ffffff;")
    compare(panel.format, "hex")
  }
  function test_tab_moves_and_selection_copy_is_not_hijacked() {
    keyClick(Qt.Key_Tab)
    verify(!child("foregroundField").activeFocus)
    keyClick(Qt.Key_Tab)
    verify(child("backgroundField").activeFocus)
    keyClick(Qt.Key_A, Qt.ControlModifier)
    keyClick(Qt.Key_C, Qt.ControlModifier)
    compare(copied.count, 0)
  }
}
