import QtQuick
import QtTest
import "production" as Production
import "production/shared" as Shared

TestCase {
  id: testCase
  name: "Calculator"
  when: windowShown
  width: 520
  height: 760
  visible: true
  Shared.Theme { id: colors }
  Loader {
    id: loader
    width: 480
    sourceComponent: Production.CalculatorPanel { theme: colors; maximumHeight: 700 }
  }
  SignalSpy { id: copySpy; target: loader.item; signalName: "copyRequested" }
  SignalSpy { id: closeSpy; target: loader.item; signalName: "closeRequested" }
  function init() {
    loader.active = false
    loader.active = true
    verify(loader.item !== null)
    loader.item.focusInput()
    copySpy.clear()
    closeSpy.clear()
    wait(10)
  }
  function input() { return findChild(loader.item, "calculatorInput") }
  function test_focus_preview_commit_and_answer() {
    verify(input().activeFocus)
    loader.item.expression = "2 + 3 * 4"
    compare(loader.item.result, "14")
    keyClick(Qt.Key_Return)
    compare(loader.item.tape.length, 1)
    compare(loader.item.expression, "")
    compare(loader.item.result, "14")
    loader.item.expression = "ans / 2"
    keyClick(Qt.Key_Return)
    compare(loader.item.tape[0].result, "7")
    loader.item.expression = "draft"
    keyClick(Qt.Key_Up)
    compare(loader.item.expression, "ans / 2")
    keyClick(Qt.Key_Up)
    compare(loader.item.expression, "2 + 3 * 4")
    keyClick(Qt.Key_Down)
    compare(loader.item.expression, "ans / 2")
    keyClick(Qt.Key_Down)
    compare(loader.item.expression, "draft")
  }
  function test_conversion_copy_and_error() {
    loader.item.expression = "72 F to C"
    verify(loader.item.result.indexOf("22.222") === 0)
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(copySpy.count, 1)
    compare(copySpy.signalArguments[0][0], loader.item.result)
    compare(loader.item.tape.length, 0)
    loader.item.expression = "1/0"
    compare(loader.item.result, "")
    compare(findChild(loader.item, "calculatorResult").text, "—")
    verify(loader.item.preview.error.indexOf("zero") >= 0)
    keyClick(Qt.Key_Return)
    compare(loader.item.tape.length, 0)
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(copySpy.count, 1)
  }
  function test_tape_bound_clear_and_close_privacy() {
    for (var i = 0; i < 40; i++) { loader.item.expression = String(i); loader.item.commit() }
    compare(loader.item.tape.length, 32)
    compare(loader.item.tape[31].expression, "8")
    loader.item.clearTape()
    compare(loader.item.tape.length, 0)
    loader.item.expression = "secret input"
    keyClick(Qt.Key_Escape)
    compare(closeSpy.count, 1)
    loader.active = false
    loader.active = true
    compare(loader.item.expression, "")
    compare(loader.item.tape.length, 0)
    compare(loader.item.result, "")
    // The new field has no old undo stack either.
    loader.item.focusInput()
    keyClick(Qt.Key_Z, Qt.ControlModifier)
    compare(loader.item.expression, "")
  }
  function test_keyboard_buttons_and_layout() {
    loader.item.expression = "1 GiB to MB"
    var copy = findChild(loader.item, "calculatorCopy")
    copy.forceActiveFocus()
    keyClick(Qt.Key_Space)
    compare(copySpy.count, 1)
    compare(copySpy.signalArguments[0][0], "1073.741824 MB")
    loader.item.guideOpen = true
    wait(10)
    verify(loader.item.implicitHeight <= 700)
    verify(loader.item.implicitHeight > 0)
  }
}
