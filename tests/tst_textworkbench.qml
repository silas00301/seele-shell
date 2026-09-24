import QtQuick
import QtTest
import "production" as Production
import "production/shared" as Shared

TestCase {
  id: test
  name: "TextWorkbench"
  width: 900
  height: 650
  when: windowShown
  visible: true
  Shared.Theme { id: styleTheme }
  Component { id: component; Production.TextWorkbenchPanel { theme: styleTheme; width: styleTheme.textWorkbenchWidth } }
  property var panel: null
  SignalSpy { id: pasteSpy; target: test.panel; signalName: "pasteRequested" }
  SignalSpy { id: copySpy; target: test.panel; signalName: "copyRequested" }
  SignalSpy { id: closeSpy; target: test.panel; signalName: "dismissed" }
  function init() { panel = createTemporaryObject(component, test); verify(panel); pasteSpy.clear(); copySpy.clear(); closeSpy.clear(); wait(250) }
  function cleanup() { panel = null }
  function test_no_implicit_clipboard_and_unicode() {
    compare(pasteSpy.count, 0)
    panel.choice = 4
    panel.input = "Grüße 🦀"
    tryCompare(panel, "canCopy", true)
    compare(panel.result.output, "R3LDvMOfZSDwn6aA")
    let encoded = panel.result.output
    panel.choice = 5
    panel.input = encoded
    tryCompare(panel, "canCopy", true)
    compare(panel.result.output, "Grüße 🦀")
    compare(pasteSpy.count, 0)
  }
  function test_invalid_and_debounce_disable_copy() {
    panel.input = '{"x":1}'
    tryCompare(panel, "canCopy", true)
    panel.input = '{"x":'
    compare(panel.canCopy, false)
    panel.copy()
    compare(copySpy.count, 0)
    wait(250)
    compare(panel.result.valid, false)
    verify(panel.result.error.indexOf("Invalid JSON") >= 0)
    compare(panel.result.output, "")
    panel.choice = 3
    panel.input = "%FF"
    wait(250)
    compare(panel.canCopy, false)
    panel.input = "x".repeat(65537)
    compare(panel.canCopy, false)
    verify(panel.result.error.indexOf("64 KiB") >= 0)
  }
  function test_copy_action_exact_and_stale_paste() {
    panel.choice = 7
    panel.input = "hello\nhello\n世界"
    tryCompare(panel, "canCopy", true)
    panel.copy()
    compare(copySpy.count, 1)
    compare(copySpy.signalArguments[0][0], "hello\n世界")
    panel.receiveCopy(panel.revision, true, "")
    compare(panel.notice, "Copied")
    panel.paste()
    compare(pasteSpy.count, 1)
    let token = panel.revision
    panel.input = "newer input"
    panel.receivePaste(token, true, "stale clipboard", "")
    compare(panel.input, "newer input")
    panel.paste()
    panel.receivePaste(panel.revision, false, "", "Clipboard bytes are not valid UTF-8 text.")
    compare(panel.input, "newer input")
    verify(panel.notice.indexOf("UTF-8") >= 0)
  }
  function test_keyboard_and_close_document_lifecycle() {
    let editor = findChild(panel, "workbenchInput")
    verify(editor)
    editor.forceActiveFocus()
    keyClick(Qt.Key_5, Qt.AltModifier)
    compare(panel.choice, 4)
    panel.input = "private"
    tryCompare(panel, "canCopy", true)
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(copySpy.count, 1)
    panel.receiveCopy(panel.revision, true, "")
    keyClick(Qt.Key_V, Qt.ControlModifier)
    compare(pasteSpy.count, 1)
    keyClick(Qt.Key_Escape)
    compare(closeSpy.count, 1)
    panel.destroy()
    wait(0)
    panel = createTemporaryObject(component, test)
    compare(panel.input, "")
    editor = findChild(panel, "workbenchInput")
    editor.forceActiveFocus()
    keyClick(Qt.Key_Z, Qt.ControlModifier)
    compare(panel.input, "")
    compare(panel.result.output, "")
  }
}
