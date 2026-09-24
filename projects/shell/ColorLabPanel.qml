pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared
import "../shared/Native.js" as Native

// Colours and exports are native data. Qt owns the edit buffer, focus and paint.
FocusScope {
  id: panel
  required property var theme
  property string foreground: String(theme.accent)
  property string background: String(theme.base)
  property string target: "foreground"
  property string format: "hex"
  property string notice: ""
  property string sampledColor: ""
  readonly property var result: Native.call("color_lab.view", [foreground, background])
  readonly property var tonalPalette: Native.call("color_lab.palette", target === "foreground" ? [foreground, background] : [background, foreground])
  signal copyRequested(string payload)
  signal dismissed()
  implicitHeight: body.implicitHeight

  function focusInput() { foregroundField.forceActiveFocus(); foregroundField.selectAll() }
  function swap() { var previous = foreground; foreground = background; background = previous; notice = "" }
  function applyColor(value) {
    if (target === "foreground") foreground = value
    else background = value
    notice = ""
  }
  function copy(requestedFormat) {
    if (!result.valid) return
    notice = ""
    copyRequested(Native.call("color_lab.copy", [foreground, background, requestedFormat || format, target]))
  }
  function handleKey(event) {
    if (event.isAutoRepeat) return
    if (event.key === Qt.Key_Escape) { dismissed(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_S) { swap(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) { copy(event.modifiers & Qt.ShiftModifier ? "css" : format); event.accepted = true }
    else if ((event.modifiers & Qt.AltModifier) && event.key === Qt.Key_1) { foregroundField.forceActiveFocus(); foregroundField.selectAll(); event.accepted = true }
    else if ((event.modifiers & Qt.AltModifier) && event.key === Qt.Key_2) { backgroundField.forceActiveFocus(); backgroundField.selectAll(); event.accepted = true }
  }
  Keys.priority: Keys.BeforeItem
  Keys.onPressed: event => handleKey(event)

  // Window shortcuts reach controls that consume Enter before it bubbles.
  Shortcut { sequences: ["Ctrl+Return", "Ctrl+Enter"]; enabled: panel.visible; onActivated: panel.copy() }
  Shortcut { sequences: ["Ctrl+Shift+Return", "Ctrl+Shift+Enter"]; enabled: panel.visible; onActivated: panel.copy("css") }

  component Caption: Text {
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    textFormat: Text.PlainText
  }
  component Action: Shared.ActionButton {
    theme: panel.theme
  }
  component ColorField: Shared.ValueField {
    id: field
    theme: panel.theme
    property bool valid: true
    height: panel.theme.controlHeight
    maximumLength: 96
    Keys.priority: Keys.BeforeItem
    Keys.onPressed: event => panel.handleKey(event)
    selectByMouse: true
    borderColor: !field.valid ? panel.theme.red : field.activeFocus ? panel.theme.accent : panel.theme.cardBorder
  }

  Column {
    id: body
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.PanelHeader {
      theme: panel.theme; width: parent.width; glyph: "󰏘"; title: "Colour Lab"
      detail: "Contrast, typography & a palette that belongs together"
      Shared.GlyphButton { theme: panel.theme; glyph: "󰅖"; text: "Close Colour Lab"; onClicked: panel.dismissed() }
    }
    Row {
      width: parent.width
      spacing: panel.theme.spaceMedium
      Column {
        width: (parent.width - panel.theme.controlHeight - panel.theme.spaceMedium * 2) / 2
        spacing: panel.theme.spaceTight
        Caption { text: "FOREGROUND · Alt 1" }
        ColorField {
          id: foregroundField; objectName: "foregroundField"; width: parent.width
          text: panel.foreground; valid: panel.result.foregroundValid
          Accessible.name: "Foreground colour"
          onTextEdited: { panel.foreground = text; panel.notice = "" }
          onActiveFocusChanged: if (activeFocus) panel.target = "foreground"
        }
      }
      Shared.GlyphButton {
        theme: panel.theme; glyph: "⇄"; text: "Swap colours · Ctrl S"; objectName: "swapButton"
        anchors.bottom: parent.bottom; onClicked: panel.swap()
      }
      Column {
        width: (parent.width - panel.theme.controlHeight - panel.theme.spaceMedium * 2) / 2
        spacing: panel.theme.spaceTight
        Caption { text: "BACKGROUND · Alt 2" }
        ColorField {
          id: backgroundField; objectName: "backgroundField"; width: parent.width
          text: panel.background; valid: panel.result.backgroundValid
          Accessible.name: "Background colour"
          onTextEdited: { panel.background = text; panel.notice = "" }
          onActiveFocusChanged: if (activeFocus) panel.target = "background"
        }
      }
    }
    Caption {
      width: parent.width; wrapMode: Text.Wrap
      text: panel.result.valid ? "Opaque sRGB · HEX, RGB or HSL · select a field to edit its palette" : panel.result.error
      color: panel.result.valid ? panel.theme.overlay : panel.theme.red
    }
    Rectangle {
      width: parent.width; height: specimen.implicitHeight + panel.theme.panelMargin * 2
      radius: panel.theme.radius; color: panel.result.valid ? panel.result.background.hex : panel.theme.wellColor
      Column {
        id: specimen
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.panelMargin }
        spacing: panel.theme.spaceLarge
        Text {
          text: "The art of being legible."; width: parent.width; wrapMode: Text.Wrap
          color: panel.result.valid ? panel.result.foreground.hex : panel.theme.overlay
          font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCode; font.weight: panel.theme.weightStrong
        }
        Text {
          text: "Good colour makes room for every word.\n0123456789 · Aa Bb Cc · @ # & +"
          color: panel.result.valid ? panel.result.foreground.hex : panel.theme.overlay
          font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody
        }
        Row {
          spacing: panel.theme.spaceMedium
          Rectangle {
            width: panel.theme.controlHeight; height: panel.theme.chipHeight
            radius: panel.theme.radiusSmall; color: panel.result.valid ? panel.result.foreground.hex : panel.theme.overlay
            Text { anchors.centerIn: parent; text: "Aa"; color: panel.result.valid ? panel.result.background.hex : panel.theme.base; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
          }
          Text { anchors.verticalCenter: parent.verticalCenter; text: "Solid colours · no alpha or wallpaper blending"; color: panel.result.valid ? panel.result.foreground.hex : panel.theme.overlay; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
        }
      }
    }
    Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Contrast"; detail: "WCAG 2.2 · exact thresholds" }
    Rectangle {
      width: parent.width; height: contrastBody.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius; color: panel.theme.cardColor
      Shared.CardEdge { theme: panel.theme }
      Row {
        id: contrastBody
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
        spacing: panel.theme.spaceLarge
        Column {
          width: panel.theme.colorLabScoreWidth
          spacing: panel.theme.spaceTight
          Text { text: panel.result.valid ? panel.result.ratioText : "—"; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCode; font.weight: panel.theme.weightLight }
          Caption { text: "contrast ratio" }
          Caption { text: "Large: 18pt or 14pt bold"; font.pixelSize: panel.theme.textMicro }
        }
        Column {
          width: parent.width - panel.theme.colorLabScoreWidth - parent.spacing
          spacing: panel.theme.spaceTight
          Repeater {
            model: panel.result.valid ? panel.result.grades : []
            delegate: Row {
              id: grade
              required property var modelData
              width: parent.width
              Text { width: parent.width - verdict.width; text: grade.modelData.label + " · " + grade.modelData.threshold; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
              Text { id: verdict; text: grade.modelData.pass ? "✓ Pass" : "× Fail"; color: grade.modelData.pass ? panel.theme.green : panel.theme.red; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption; font.weight: panel.theme.weightStrong }
            }
          }
        }
      }
    }
    Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Tonal palette"; detail: panel.target + " · dot = normal AA" }
    Row {
      width: parent.width; spacing: panel.theme.spaceTight
      Repeater {
        model: panel.tonalPalette
        delegate: Button {
          id: swatch
          objectName: "swatch" + modelData.lightness
          required property var modelData
          width: (parent.width - panel.theme.spaceTight * 8) / 9
          height: panel.theme.detailRowHeight
          hoverEnabled: true; focusPolicy: Qt.StrongFocus
          Accessible.name: modelData.hex + ", contrast " + modelData.ratioText
          Keys.onReturnPressed: clicked()
          Keys.onEnterPressed: clicked()
          onClicked: panel.applyColor(modelData.hex)
          background: Rectangle {
            color: swatch.modelData.hex; radius: panel.theme.radiusSmall
            Shared.HoverWash { theme: panel.theme; hovered: swatch.hovered }
            Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: panel.theme.spaceTight } height: panel.theme.spaceTight; radius: height / 2; visible: swatch.modelData.pass; color: panel.theme.green }
            Shared.FocusRing { anchors.fill: parent; theme: panel.theme; shown: swatch.visualFocus; radius: panel.theme.radiusSmall }
          }
          HoverHandler { cursorShape: Qt.PointingHandCursor }
          ToolTip.visible: hovered
          ToolTip.text: modelData.hex + " · " + modelData.ratioText
        }
      }
    }
    Row {
      width: parent.width; spacing: panel.theme.spaceMedium
      Shared.SegmentWell {
        theme: panel.theme; width: parent.width - exportButton.width - panel.theme.spaceMedium; height: panel.theme.controlHeight
        Row {
          width: parent.width; height: parent.height
          Repeater {
            model: ["hex", "rgb", "hsl", "css"]
            delegate: Shared.SegmentChoice {
              id: formatButton
              objectName: "format" + modelData
              required property string modelData
              theme: panel.theme
              width: parent.width / 4; height: parent.height
              text: modelData.toUpperCase()
              selected: panel.format === modelData
              Accessible.name: modelData.toUpperCase() + " export format"
              onClicked: panel.format = modelData
            }
          }
        }
      }
      Action { id: exportButton; objectName: "copyButton"; text: "Copy"; enabled: panel.result.valid; onClicked: panel.copy() }
    }
    Row {
      width: parent.width; spacing: panel.theme.spaceMedium
      Action { objectName: "sampleButton"; visible: panel.sampledColor !== ""; text: "Use sampled colour"; onClicked: panel.applyColor(panel.sampledColor) }
      Caption { anchors.verticalCenter: parent.verticalCenter; text: "Copy " + (panel.format === "css" ? "both colours" : panel.target) + " · Ctrl Enter" }
    }
    Caption { width: parent.width; wrapMode: Text.Wrap; text: panel.notice || "Ctrl Shift Enter copies CSS · Escape closes"; color: panel.notice ? panel.theme.accent : panel.theme.overlay }
  }
}
