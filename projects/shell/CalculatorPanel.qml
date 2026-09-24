pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared
import "../shared/Native.js" as Native

// Only rendering, focus and clipboard intent live here. Rust owns arithmetic,
// units, error wording and the bounded tape. The parent destroys this component
// on close, including the input control's undo buffer.
FocusScope {
  id: panel
  required property var theme
  property real maximumHeight: theme.calculatorMaximumHeight
  property var session: ({})
  property bool guideOpen: false
  property string notice: ""
  property string copyError: ""
  property int recallIndex: -1
  property string recallDraft: ""
  property alias expression: input.text
  readonly property var preview: Native.call("calculator.preview", [input.text, session])
  readonly property var tape: session.tape || []
  readonly property string result: preview.result || (preview.empty && session.last ? session.last.result : "")
  readonly property string copyValue: result
  signal copyRequested(string text)
  signal closeRequested()
  implicitHeight: Math.min(content.implicitHeight, maximumHeight)

  function focusInput() { input.forceActiveFocus() }
  function insert(text) { recallIndex = -1; recallDraft = ""; input.text = text; input.cursorPosition = input.length; focusInput() }
  function commit() {
    var next = Native.call("calculator.commit", [input.text, session])
    if (next.error || next.empty) return
    session = next
    recallIndex = -1
    recallDraft = ""
    input.clear()
    notice = ""
    copyError = ""
    focusInput()
  }
  function copyResult() {
    if (copyValue === "") return
    notice = ""
    copyError = ""
    copyRequested(copyValue)
  }
  function clearTape() { session = ({}); recallIndex = -1; recallDraft = ""; notice = ""; copyError = ""; focusInput() }
  function recall(direction) {
    var next = recallIndex + direction
    if (next < -1 || next >= tape.length) return
    if (recallIndex === -1) recallDraft = input.text
    recallIndex = next
    input.text = next === -1 ? recallDraft : tape[next].expression
    input.cursorPosition = input.length
  }
  function handleKey(event) {
    event.accepted = false
    if (event.key === Qt.Key_Escape) { closeRequested(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) { copyResult(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_L) { input.selectAll(); focusInput(); event.accepted = true }
    else if (event.key === Qt.Key_Up && input.activeFocus && tape.length > 0) { recall(1); event.accepted = true }
    else if (event.key === Qt.Key_Down && input.activeFocus && recallIndex >= 0) { recall(-1); event.accepted = true }
  }
  Keys.onPressed: event => handleKey(event)
  onActiveFocusChanged: if (activeFocus) focusInput()

  HoverHandler { id: panelHover }

  Shared.SeeleFlickable {
    theme: panel.theme
    anchors.fill: parent
    contentWidth: width
    contentHeight: content.implicitHeight
    clip: true
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
    Column {
      id: content
      width: parent.width
      spacing: panel.theme.panelSpacing
      Shared.PanelHeader {
        theme: panel.theme
        width: parent.width
        glyph: "󰃬"
        title: "Calculator"
        detail: "Enter keeps · Ctrl + Enter copies · ↑ recalls"
        Shared.GlyphButton { theme: panel.theme; glyph: "󰅖"; text: "Close calculator"; onClicked: panel.closeRequested() }
      }
      Rectangle {
        width: parent.width
        height: calculation.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        Column {
          id: calculation
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          spacing: panel.theme.spaceMedium
          Shared.ValueField {
            id: input
            objectName: "calculatorInput"
            theme: panel.theme
            width: parent.width
            height: panel.theme.rowHeight
            maximumLength: 512
            selectByMouse: true
            placeholderText: "(48 + 16) / 4  or  12 km to mi"
            font.pixelSize: panel.theme.textLead
            leftPadding: panel.theme.spaceLarge
            rightPadding: panel.theme.spaceLarge
            Accessible.name: "Expression or unit conversion"
            onTextChanged: { panel.notice = ""; panel.copyError = "" }
            onTextEdited: { panel.recallIndex = -1; panel.recallDraft = "" }
            Keys.onPressed: event => {
              panel.handleKey(event)
              if (!event.accepted && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) { panel.commit(); event.accepted = true }
            }
            onAccepted: panel.commit()
          }
          Item {
            width: parent.width
            height: Math.max(resultText.implicitHeight, copyButton.height) + panel.theme.spaceMedium
            Text {
              id: resultText
              objectName: "calculatorResult"
              anchors { left: parent.left; right: copyButton.left; rightMargin: panel.theme.spaceLarge; verticalCenter: parent.verticalCenter }
              text: panel.result || (panel.preview.error ? "—" : "0")
              textFormat: Text.PlainText
              elide: Text.ElideRight
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textHero
              font.weight: panel.theme.weightLight
              color: panel.result ? panel.theme.accent : panel.theme.subtext
            }
            Shared.GlyphButton {
              id: copyButton
              objectName: "calculatorCopy"
              theme: panel.theme
              anchors { right: parent.right; verticalCenter: parent.verticalCenter }
              glyph: panel.notice ? "󰄬" : "󰆏"
              text: panel.copyValue ? "Copy " + panel.copyValue + " · Ctrl + Enter" : "Copy result · Ctrl + Enter"
              enabled: panel.copyValue !== ""
              onClicked: panel.copyResult()
            }
          }
          Text {
            width: parent.width
            text: panel.preview.error || panel.copyError || panel.notice || (panel.session.last ? "ans = " + panel.session.last.number + " · 12 significant digits" : "Local calculations · cleared when you close")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: panel.preview.error || panel.copyError ? panel.theme.red : panel.notice ? panel.theme.green : panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textCaption
          }
        }
      }
      Row {
        width: parent.width
        spacing: panel.theme.spaceSmall
        Repeater {
          model: ["15% * 240", "72 F to C", "1 GiB to MB"]
          Shared.ActionButton {
            required property string modelData
            theme: panel.theme
            text: modelData
            width: (content.width - panel.theme.spaceSmall * 2) / 3
            onClicked: panel.insert(modelData)
          }
        }
      }
      Shared.SectionRule {
        theme: panel.theme
        width: parent.width
        label: "Calculation tape"
        detail: panel.tape.length + " / 32"
        Shared.GlyphButton {
          theme: panel.theme
          glyph: "󰆴"
          text: "Clear calculation tape"
          enabled: panel.tape.length > 0
          implicitWidth: panel.theme.chipHeight
          implicitHeight: panel.theme.chipHeight
          onClicked: panel.clearTape()
        }
      }
      Rectangle {
        width: parent.width
        height: panel.tape.length ? Math.min(tapeList.contentHeight, panel.theme.detailRowHeight * 4) : empty.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        Shared.EmptyState {
          id: empty
          theme: panel.theme
          visible: panel.tape.length === 0
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          glyph: "󰃬"
          title: "Room to work things out"
          detail: "Press Enter to keep a result here. Select a result to reuse it."
        }
        Shared.SeeleListView {
          id: tapeList
          theme: panel.theme
          anchors.fill: parent
          visible: panel.tape.length > 0
          clip: true
          ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
          model: panel.tape
          delegate: Item {
            id: tapeRow
            required property var modelData
            width: tapeList.width
            height: panel.theme.detailRowHeight
            Shared.HoverWash { theme: panel.theme; hovered: rowHover.hovered }
            HoverHandler { id: rowHover }
            Column {
              anchors { left: parent.left; right: actions.left; verticalCenter: parent.verticalCenter; leftMargin: panel.theme.cardPadding; rightMargin: panel.theme.spaceMedium }
              spacing: panel.theme.spaceTight
              Text { width: parent.width; text: tapeRow.modelData.expression; textFormat: Text.PlainText; elide: Text.ElideRight; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
              Text { width: parent.width; text: "= " + tapeRow.modelData.result; textFormat: Text.PlainText; elide: Text.ElideRight; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textLead }
            }
            Row {
              id: actions
              anchors { right: parent.right; rightMargin: panel.theme.spaceSmall; verticalCenter: parent.verticalCenter }
              Shared.GlyphButton { theme: panel.theme; glyph: "󰁔"; text: "Use result: " + tapeRow.modelData.number; onClicked: panel.insert(tapeRow.modelData.number) }
              Shared.GlyphButton { theme: panel.theme; glyph: "󰆏"; text: "Copy " + tapeRow.modelData.result; onClicked: panel.copyRequested(tapeRow.modelData.result) }
            }
          }
        }
      }
      Shared.SectionRule {
        theme: panel.theme
        width: parent.width
        label: "Unit & expression guide"
        collapsible: true
        expanded: panel.guideOpen
        onToggled: panel.guideOpen = !panel.guideOpen
      }
      Rectangle {
        visible: panel.guideOpen
        width: parent.width
        height: guide.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        Text {
          id: guide
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          text: "+  -  *  /  ^  ( )   ·   % divides by 100\npi · e · ans · sqrt() · abs() · round()\n\nLength   mm cm m km in ft yd mi\nMass     mg g kg oz lb\nTime     ms s min h day\nVolume   mL L\nData     B kB MB GB TB KiB MiB GiB TiB\nHeat     C F K (also °C and °F)\n\nUse a dot for decimals. Unit symbols are case-sensitive.\nExample: (2 + 3) ft to in · ans reuses the last number."
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
    }
  }
}
