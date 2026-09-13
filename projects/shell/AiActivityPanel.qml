import QtQuick
import QtQuick.Controls
import "../shared" as Shared
import "ai-activity.js" as Activity

Column {
  id: panel
  required property var theme
  required property var store
  spacing: theme.panelSpacing
  // A job's state is the one thing on its card worth colour, so it is a chip
  // and the caption beside it keeps the consumer and the elapsed time.
  function stateChip(state) {
    return ({
      queued: { text: "Queued", tint: theme.overlay },
      running: { text: "Running", tint: theme.accent },
      retrying: { text: "Retrying", tint: theme.yellow },
      succeeded: { text: "Succeeded", tint: theme.green },
      failed: { text: "Failed", tint: theme.red },
      cancelled: { text: "Cancelled", tint: theme.overlay },
      superseded: { text: "Superseded", tint: theme.overlay }
    })[state] || { text: String(state), tint: theme.overlay }
  }
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
        Item {
          width: parent.width
          height: Math.max(jobState.height, jobCaption.implicitHeight)
          Text {
            id: jobCaption
            anchors { left: parent.left; right: jobState.left; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
            text: card.modelData.consumer + " · "
              + Math.max(0, Math.floor((['succeeded','failed','cancelled','superseded'].indexOf(card.modelData.state) >= 0 ? card.modelData.updated : panel.store.now) - card.modelData.created)) + "s"
            textFormat: Text.PlainText
            color: panel.theme.subtext
            font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption
            elide: Text.ElideRight
          }
          Shared.StatusChip {
            id: jobState
            theme: panel.theme
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            text: panel.stateChip(card.modelData.state).text
            tint: panel.stateChip(card.modelData.state).tint
          }
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
