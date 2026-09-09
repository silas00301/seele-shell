pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import Seele.Markdown
import "../shared" as Shared
import "notes.js" as Notes

// The writing area. The text it holds is exactly the file's bytes: formatting
// is applied over them by the highlighter rather than parsed out of them, so
// frontmatter, wikilinks, embeds and anything Markdown this editor does not
// model survive being edited around.
Item {
  id: editorRoot

  required property var theme
  property bool sourceMode: false
  property bool readOnly: false
  property string placeholder: "Start writing…"
  property bool popupHovered: false
  readonly property alias area: editor
  readonly property alias caret: editor.cursorPosition

  signal edited(string text)

  function load(text, keepCaret) {
    var position = editor.cursorPosition
    var offset = flick.contentY
    guard.loading = true
    editor.text = text
    guard.reported = text
    guard.loading = false
    if (keepCaret) {
      editor.cursorPosition = Math.min(position, editor.length)
      flick.contentY = Math.min(offset, Math.max(0, flick.contentHeight - flick.height))
    } else {
      editor.cursorPosition = 0
      flick.contentY = 0
    }
  }

  function focusBody() { editor.forceActiveFocus() }

  // Every command is one replacement in one edit block, so Ctrl+Z takes back
  // the whole command and lands on a document the writer has actually seen.
  function apply(command) {
    if (!command || editor.readOnly) return
    commands.replace(editor.textDocument, command.start, command.end, command.text)
    if (command.selectionEnd !== undefined && command.selectionEnd !== command.caret)
      editor.select(command.caret, command.selectionEnd)
    else
      editor.cursorPosition = Math.min(command.caret, editor.length)
  }

  function span() {
    return editor.selectionStart === editor.selectionEnd
      ? { start: editor.cursorPosition, end: editor.cursorPosition }
      : { start: editor.selectionStart, end: editor.selectionEnd }
  }

  function emphasize(marker) {
    var selection = span()
    apply(Notes.wrap(editor.text, selection.start, selection.end, marker))
  }
  function setHeading(level) { apply(Notes.heading(editor.text, editor.cursorPosition, level)) }
  function toggleTask() { apply(Notes.task(editor.text, editor.cursorPosition)) }
  function insertLink() {
    var selection = span()
    apply(Notes.link(editor.text, selection.start, selection.end))
  }
  function insertEmbed(name) { apply(Notes.embed(editor.text, editor.cursorPosition, name)) }
  function removeEmbed(name) { apply(Notes.unembed(editor.text, name)) }

  QtObject {
    id: guard

    property bool loading: false
    // Detaching the highlighter opens an empty edit block on the document,
    // which Qt reports as a content change even though not one character
    // moved. Reporting that as an edit would send the file straight back to
    // the worker every time the rendering was switched.
    property string reported: ""
  }

  MarkdownEdit { id: commands }

  Shared.SeeleFlickable {
    id: flick

    theme: editorRoot.theme
    anchors.fill: parent
    clip: true
    contentWidth: width
    contentHeight: editor.contentHeight + editorRoot.theme.spaceLarge * 2

    TextArea.flickable: TextArea {
      id: editor

      readOnly: editorRoot.readOnly
      placeholderText: editorRoot.placeholder
      color: editorRoot.theme.text
      placeholderTextColor: editorRoot.theme.overlay
      selectionColor: editorRoot.theme.selectedColor
      selectedTextColor: editorRoot.theme.text
      font.family: editorRoot.theme.fontFamily
      font.pixelSize: editorRoot.theme.textLead
      // Writing wants a taller line than a list row does; the ramp sets the
      // size and this only decides how much air goes between the lines.
      topPadding: editorRoot.theme.spaceLarge
      bottomPadding: editorRoot.theme.spaceLarge
      wrapMode: TextEdit.Wrap
      textFormat: TextEdit.PlainText
      selectByMouse: true
      persistentSelection: true
      background: Item {}

      onTextChanged: {
        if (guard.loading || guard.reported === text) return
        guard.reported = text
        editorRoot.edited(text)
      }

      Keys.onReturnPressed: event => {
        var command = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)
          ? null
          : Notes.newline(editor.text, editor.cursorPosition)
        if (!command) { event.accepted = false; return }
        event.accepted = true
        editorRoot.apply(command)
      }
    }

    ScrollBar.vertical: Shared.SlimScrollBar { theme: editorRoot.theme; popupHovered: editorRoot.popupHovered }
  }

  // Source mode drops the formatting rather than the syntax: the same bytes,
  // set uniformly, for a table that has to line up or an exact edit inside a
  // construct this editor does not model.
  MarkdownHighlighter {
    document: editorRoot.sourceMode ? null : editor.textDocument
    textColor: editorRoot.theme.text
    mutedColor: editorRoot.theme.overlay
    accentColor: editorRoot.theme.accent
    codeColor: editorRoot.theme.yellow
    codeBackground: editorRoot.theme.wellColor
    quoteColor: editorRoot.theme.subtext
    doneColor: editorRoot.theme.green
    baseSize: editorRoot.theme.textLead
    monoFamily: editorRoot.theme.fontFamily
  }
}
