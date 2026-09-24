pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared
import "../shared/Native.js" as Native

// Created only while open. Destroying this object destroys the QTextDocument,
// including its undo stack, rather than clearing a resident editor's text.
FocusScope {
  id: panel
  required property var theme
  property alias input: editor.text
  property int choice: 0
  property int revision: 0
  property var result: ({valid: false, output: "", error: ""})
  property string notice: ""
  property bool clipboardBusy: false
  readonly property var modes: [
    {id: "json-format", label: "Format JSON", hint: "Indent JSON while preserving every value and duplicate key."},
    {id: "json-minify", label: "Minify JSON", hint: "Remove JSON whitespace; strings and number precision stay exact."},
    {id: "url-encode", label: "URL encode", hint: "Encode one UTF-8 component, including spaces, slashes and punctuation."},
    {id: "url-decode", label: "URL decode", hint: "Decode percent escapes as UTF-8. A literal + stays +; this is not form decoding."},
    {id: "base64-encode", label: "Base64 encode", hint: "Encode UTF-8 text with the standard Base64 alphabet and padding."},
    {id: "base64-decode", label: "Base64 decode", hint: "Standard padded Base64 only. Whitespace and non-UTF-8 bytes are rejected."},
    {id: "lines-clean", label: "Clean lines", hint: "Trim each line, remove blank lines and use LF endings. Preview before copying."},
    {id: "lines-unique", label: "Unique lines", hint: "Keep the first occurrence of each exact line, in its original order."}
  ]
  readonly property string mode: modes[choice].id
  readonly property bool canCopy: result.valid === true && !previewTimer.running && !clipboardBusy
  readonly property int inputLimit: 65536
  signal pasteRequested(int revision)
  signal copyRequested(string text, int revision)
  signal dismissed()
  implicitHeight: body.implicitHeight

  function focusInput() { editor.forceActiveFocus() }
  function invalidate() {
    revision++
    notice = ""
    result = ({valid: false, output: "", error: ""})
    if (editor.text.length > inputLimit) {
      previewTimer.stop()
      result = ({valid: false, output: "", error: "Input exceeds 64 KiB. Nothing was transformed."})
    } else previewTimer.restart()
  }
  function preview() {
    result = editor.text.length === 0 ? {valid: false, output: "", error: ""}
      : Native.call("text_workbench.transform", [editor.text, mode])
  }
  function paste() {
    if (clipboardBusy) return
    notice = ""
    clipboardBusy = true
    pasteRequested(revision)
  }
  function receivePaste(token, ok, text, error) {
    clipboardBusy = false
    if (token !== revision) return
    if (ok) { editor.text = text; focusInput() }
    else notice = error
  }
  function copy() {
    if (!canCopy) return
    clipboardBusy = true
    notice = ""
    copyRequested(result.output, revision)
  }
  function receiveCopy(token, ok, error) {
    clipboardBusy = false
    if (token === revision) notice = ok ? "Copied" : error
  }
  function clear() { editor.text = ""; invalidate(); focusInput() }
  onChoiceChanged: invalidate()
  Keys.priority: Keys.BeforeItem
  Keys.onPressed: event => {
    if (event.key === Qt.Key_Escape) { dismissed(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) { copy(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_V) { paste(); event.accepted = true }
    else if ((event.modifiers & Qt.AltModifier) && event.key >= Qt.Key_1 && event.key <= Qt.Key_8) { choice = event.key - Qt.Key_1; event.accepted = true }
  }
  Timer { id: previewTimer; interval: 200; onTriggered: panel.preview() }
  Component.onCompleted: { previewTimer.restart(); focusInput() }

  Column {
    id: body
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.PanelHeader {
      theme: panel.theme
      width: parent.width
      glyph: "󰦨"
      title: "Text workbench"
      detail: "Local transforms · cleared when closed"
      Shared.GlyphButton { theme: panel.theme; glyph: "󰅖"; text: "Close · Esc"; onClicked: panel.dismissed() }
    }
    Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Transform"; detail: "Alt + 1–8" }
    Shared.SegmentWell {
      theme: panel.theme
      width: parent.width
      height: choices.implicitHeight + panel.theme.spaceTight
      Grid {
        id: choices
        width: parent.width
        columns: 4
        spacing: panel.theme.spaceTight
        Repeater {
          model: panel.modes
          delegate: Shared.SegmentChoice {
            id: modeButton
            required property var modelData
            required property int index
            theme: panel.theme
            width: (choices.width - choices.spacing * (choices.columns - 1)) / choices.columns
            text: modelData.label
            selected: panel.choice === index
            onClicked: panel.choice = index
          }
        }
      }
    }
    Text {
      width: parent.width
      text: panel.modes[panel.choice].hint
      textFormat: Text.PlainText
      wrapMode: Text.Wrap
      color: panel.theme.subtext
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
    }
    Row {
      width: parent.width
      spacing: panel.theme.panelSpacing
      Column {
        width: (parent.width - parent.spacing) / 2
        spacing: panel.theme.spaceSmall
        Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Input"; detail: "64 KiB maximum" }
        Rectangle {
          width: parent.width
          height: panel.theme.textWorkbenchEditorHeight
          radius: panel.theme.radius
          color: panel.theme.wellColor
          Shared.CardEdge { theme: panel.theme }
          HoverHandler { id: inputHover }
          Shared.SeeleFlickable {
            theme: panel.theme
            anchors { fill: parent; margins: panel.theme.spaceMedium }
            clip: true
            TextArea.flickable: TextArea {
              id: editor
              objectName: "workbenchInput"
              color: panel.theme.text
              selectionColor: panel.theme.selectedColor
              selectedTextColor: panel.theme.text
              placeholderText: "Type text, or Paste explicitly…"
              placeholderTextColor: panel.theme.overlay
              textFormat: TextEdit.PlainText
              wrapMode: TextEdit.Wrap
              selectByMouse: true
              persistentSelection: true
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textBody
              background: null
              onTextChanged: panel.invalidate()
              // Selection paste would bypass the bounded UTF-8 clipboard reader.
              MouseArea { anchors.fill: parent; acceptedButtons: Qt.MiddleButton }

              Keys.priority: Keys.BeforeItem
              Keys.forwardTo: [panel]
              // All clipboard paste routes use the same bounded native reader.
              Keys.onPressed: event => {
                if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_V || (event.modifiers & Qt.ShiftModifier) && event.key === Qt.Key_Insert) { panel.paste(); event.accepted = true }
              }
            }
            ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: inputHover.hovered }
          }
        }
        Row {
          spacing: panel.theme.spaceSmall
          Shared.ActionButton { theme: panel.theme; text: "Paste"; enabled: !panel.clipboardBusy; onClicked: panel.paste() }
          Shared.ActionButton { theme: panel.theme; text: "Clear"; enabled: panel.input.length > 0; onClicked: panel.clear() }
        }
      }
      Column {
        width: (parent.width - parent.spacing) / 2
        spacing: panel.theme.spaceSmall
        Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Preview"; detail: panel.result.valid ? panel.result.outputBytes + " bytes" : "" }
        Rectangle {
          width: parent.width
          height: panel.theme.textWorkbenchEditorHeight
          radius: panel.theme.radius
          color: panel.theme.cardColor
          Shared.CardEdge { theme: panel.theme }
          HoverHandler { id: outputHover }
          Shared.SeeleFlickable {
            theme: panel.theme
            anchors { fill: parent; margins: panel.theme.spaceMedium }
            clip: true
            TextArea.flickable: TextArea {
              objectName: "workbenchOutput"
              text: panel.result.output || ""
              textFormat: TextEdit.PlainText
              readOnly: true
              selectByMouse: true
              wrapMode: TextEdit.Wrap
              color: panel.theme.text
              selectionColor: panel.theme.selectedColor
              selectedTextColor: panel.theme.text
              placeholderText: previewTimer.running ? "Preparing preview…" : panel.result.valid ? "Empty result" : "The valid result appears here"
              placeholderTextColor: panel.theme.overlay
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textBody
              background: null
              Keys.forwardTo: [panel]
            }
            ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: outputHover.hovered }
          }
        }
        Row {
          spacing: panel.theme.spaceSmall
          Shared.ActionButton { objectName: "workbenchCopy"; theme: panel.theme; text: "Copy result"; selected: true; enabled: panel.canCopy; onClicked: panel.copy() }
          Text { anchors.verticalCenter: parent.verticalCenter; text: "Ctrl + Enter"; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
        }
      }
    }
    Text {
      width: parent.width
      text: panel.result.error || panel.notice || (panel.clipboardBusy ? "Clipboard…" : panel.result.valid ? panel.result.characters + " characters · " + panel.result.lines + " lines" : "")
      textFormat: Text.PlainText
      wrapMode: Text.Wrap
      color: panel.result.error ? panel.theme.red : panel.notice === "Copied" ? panel.theme.green : panel.theme.subtext
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
      // Reserve one caption line so successful clipboard acknowledgement never
      // moves the editors. Multi-line parse errors expand naturally.
      height: Math.max(implicitHeight, panel.theme.textLead)
    }
  }
}
