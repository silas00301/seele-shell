import QtQuick
import QtQuick.Controls
import "../shared" as Shared
import "ai-activity.js" as Activity

Column {
  id: panel
  required property var theme
  required property var store
  spacing: theme.panelSpacing
  Text {
    width: parent.width
    visible: panel.store.error !== "" || panel.store.jobs.length === 0
    text: panel.store.error || "No integration work needs attention"
    color: panel.store.error ? panel.theme.red : panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    wrapMode: Text.Wrap
  }
  Repeater {
    model: panel.store.model
    Rectangle {
      id: card
      required property var modelData
      readonly property bool expanded: !!panel.store.expandedIds[modelData.id]
      width: panel.width
      height: content.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius
      color: panel.theme.cardColor
      Shared.CardEdge { theme: panel.theme }
      HoverHandler { id: hover }
      Shared.HoverWash { theme: panel.theme; hovered: hover.hovered }
      Column {
        id: content
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        anchors.margins: panel.theme.cardPadding
        spacing: panel.theme.spaceMedium
        Shared.ActionButton {
          theme: panel.theme
          width: parent.width
          id: labelButton
          contentItem: Text { text: labelButton.text; textFormat: Text.PlainText; elide: Text.ElideRight; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody; verticalAlignment: Text.AlignVCenter }
          text: card.modelData.label + (card.expanded ? "  ▴" : "  ▾")
          onClicked: { var ids = Object.assign({}, panel.store.expandedIds); ids[card.modelData.id] = !card.expanded; panel.store.expandedIds = ids }
        }
        Text {
          width: parent.width
          text: card.modelData.consumer + " · " + card.modelData.state + " · "
            + Math.max(0, Math.floor((['succeeded','failed','cancelled','superseded'].indexOf(card.modelData.state) >= 0 ? card.modelData.updated : panel.store.now) - card.modelData.created)) + "s"
          textFormat: Text.PlainText
          color: card.modelData.state === "failed" ? panel.theme.red : panel.theme.subtext
          font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption
          wrapMode: Text.Wrap
        }
        Text {
          width: parent.width; visible: card.expanded
          text: card.modelData.model + " · attempt " + card.modelData.attempts
            + "\n" + card.modelData.tokens.input + " input · " + card.modelData.tokens.output + " output tokens"
            + "\nQueued " + Math.floor(card.modelData.queueDuration) + "s · " + card.modelData.id
            + (card.modelData.error ? "\n" + card.modelData.error : "")
          textFormat: Text.PlainText
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption
          wrapMode: Text.WrapAnywhere
        }
        Row {
          spacing: panel.theme.spaceTight
          Repeater {
            model: Activity.actions(card.modelData.state)
            Shared.ActionButton {
              required property var modelData
              theme: panel.theme
              text: panel.store.pendingId === card.modelData.id ? "Working…" : modelData.label
              enabled: panel.store.pendingId === ""
              onClicked: panel.store.act(card.modelData, modelData.op)
            }
          }
        }
        Text {
          visible: panel.store.actionErrorId === card.modelData.id
          width: parent.width; text: panel.store.actionError
          color: panel.theme.red; font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption; wrapMode: Text.Wrap
        }
      }
    }
  }
}
