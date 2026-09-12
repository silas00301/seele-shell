import QtQuick
import QtTest
import Seele.Markdown

// The editor is checked by driving it the way a person does: real key presses
// into the real component, so what is proved is the behaviour the application
// ships rather than a callback lifted out of it.
TestCase {
  id: suite

  name: "SeeleNotesEditor"
  width: 480
  height: 320
  visible: true
  when: windowShown

  property int edits: 0
  property string lastText: ""

  QtObject {
    id: theme

    readonly property color text: "#cdd6f4"
    readonly property color subtext: "#a6adc8"
    readonly property color overlay: "#6c7086"
    readonly property color accent: "#b4befe"
    readonly property color green: "#a6e3a1"
    readonly property color yellow: "#f9e2af"
    readonly property color wellColor: "#11111b"
    readonly property color selectedColor: "#33b4befe"
    readonly property string fontFamily: "monospace"
    readonly property int textLead: 13
    readonly property int spaceLarge: 12
    readonly property int scrollGutter: 8
    readonly property int scrollRebound: 130
    readonly property int scrollDeceleration: 9000
    readonly property int scrollFlickVelocity: 2200

    function alpha(color, opacity) { return Qt.rgba(color.r, color.g, color.b, opacity) }
  }

  MarkdownEditor {
    id: editor

    anchors.fill: parent
    theme: theme

    onEdited: value => { suite.edits += 1; suite.lastText = value }
  }

  MarkdownHighlighter { id: detachedHighlighter }
  Component { id: temporaryText; TextEdit { text: "**source**" } }
  function test_document_lifetime_clears_guarded_pointer() {
    var temporary = temporaryText.createObject(suite)
    detachedHighlighter.document = temporary.textDocument
    verify(detachedHighlighter.document !== null)
    temporary.destroy()
    wait(1)
    compare(detachedHighlighter.document, null)
  }

  function init() {
    suite.edits = 0
    suite.lastText = ""
    editor.sourceMode = false
    editor.readOnly = false
  }

  // Loading a document is not an edit. If it were, every external refresh
  // would look like something the user typed and get written straight back.
  function test_01_loading_is_not_an_edit() {
    editor.load("# Heading\n\nplain text\n", false)
    compare(suite.edits, 0)
    compare(editor.area.text, "# Heading\n\nplain text\n")
    compare(editor.caret, 0)
  }

  function test_02_focus_lands_in_the_body() {
    editor.load("", false)
    editor.focusBody()
    verify(editor.area.activeFocus)
  }

  function test_03_typing_reports_exactly_what_is_there() {
    editor.load("", false)
    editor.focusBody()
    keyClick(Qt.Key_A)
    keyClick(Qt.Key_B)
    compare(editor.area.text, "ab")
    compare(suite.lastText, "ab")
    compare(suite.edits, 2)
  }

  // Return continues a list through the production key handler rather than
  // through a helper called directly.
  function test_04_return_continues_a_list() {
    editor.load("", false)
    editor.focusBody()
    keyClick(Qt.Key_Minus)
    keyClick(Qt.Key_Space)
    keyClick(Qt.Key_M)
    keyClick(Qt.Key_Return)
    compare(editor.area.text, "- m\n- ")
    keyClick(Qt.Key_Return)
    compare(editor.area.text, "- m\n", "an empty item ends the list instead of adding another marker")
  }

  // A formatting command has to land on the same undo stack as the typing it
  // formats, or Ctrl+Z after Ctrl+B throws away the words as well.
  function test_05_commands_are_undoable() {
    editor.load("", false)
    editor.focusBody()
    keyClick(Qt.Key_W)
    keyClick(Qt.Key_O)
    keyClick(Qt.Key_W)
    editor.area.select(0, 3)
    editor.emphasize("**")
    compare(editor.area.text, "**wow**")
    compare(editor.area.selectionStart, 2)
    compare(editor.area.selectionEnd, 5, "the words stay selected under their new emphasis")
    editor.area.undo()
    compare(editor.area.text, "wow", "undo takes back the emphasis, not the typing")
    editor.area.redo()
    compare(editor.area.text, "**wow**")
  }

  function test_06_heading_and_task_commands() {
    editor.load("plan the week", false)
    editor.focusBody()
    editor.area.cursorPosition = 4
    editor.setHeading(2)
    compare(editor.area.text, "## plan the week")
    compare(editor.caret, 7, "the caret keeps its place in the words")
    editor.setHeading(2)
    compare(editor.area.text, "plan the week", "the same level again clears it")
    editor.toggleTask()
    compare(editor.area.text, "- [ ] plan the week")
  }

  // An external refresh of the note on screen must not move the caret or the
  // scroll position out from under whoever is reading it.
  function test_07_refresh_keeps_the_caret() {
    editor.load("one\ntwo\nthree\n", false)
    editor.area.cursorPosition = 5
    editor.load("one\ntwo\nthree\nfour\n", true)
    compare(editor.caret, 5)
    compare(suite.edits, 0, "a refresh is not reported as an edit")
    editor.load("something else\n", false)
    compare(editor.caret, 0)
  }

  function test_08_read_only_refuses_commands() {
    editor.load("text", false)
    editor.readOnly = true
    editor.emphasize("**")
    compare(editor.area.text, "text")
  }

  // The highlighter is the whole reason this editor exists, so the check is
  // that the same bytes are actually drawn differently with it and without it.
  function test_09_formatting_is_rendered() {
    editor.load("# Heading\n\n**bold** and `code`\n", false)
    editor.sourceMode = false
    wait(120)
    var live = grabImage(editor)
    editor.sourceMode = true
    wait(120)
    var plain = grabImage(editor)
    verify(!live.equals(plain), "live formatting has to look different from the source it formats")
    editor.sourceMode = false
    wait(120)
    verify(grabImage(editor).equals(live), "returning to live formatting restores the same rendering")
  }

  // Source mode is a different rendering of the same bytes, never a different
  // document.
  function test_10_source_mode_keeps_the_text() {
    editor.load("# Heading\n\n**bold**\n", false)
    editor.sourceMode = true
    compare(editor.area.text, "# Heading\n\n**bold**\n")
    compare(suite.edits, 0)
  }

  // A checked box and an empty one are the same characters in different
  // states, so the only way to know the box is being read at all is that the
  // two are drawn differently.
  function test_11_task_boxes_are_lit() {
    editor.load("- [ ] buy milk\n", false)
    wait(120)
    var open = grabImage(editor)
    editor.load("- [x] buy milk\n", false)
    wait(120)
    verify(!grabImage(editor).equals(open), "a finished task has to look finished")
  }

  function test_12_embeds_come_and_go_as_text() {
    editor.load("Some thoughts", false)
    editor.area.cursorPosition = editor.area.length
    editor.insertEmbed("Voice memo 1.wav")
    compare(editor.area.text, "Some thoughts\n\n![[Voice memo 1.wav]]\n")
    editor.removeEmbed("Voice memo 1.wav")
    compare(editor.area.text, "Some thoughts\n\n")
  }
}
