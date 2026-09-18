import QtQuick
import "../shared" as Shared

// The microphone test, compact while idle and unmistakable while it runs. The
// state it draws is derived whole by the shared policy; nothing here decides
// whether a signal clipped, which output a stream reaches, or whether a test
// may start.
Column {
  id: card

  required property var theme
  required property var store
  property bool outputsExpanded: false

  readonly property var view: card.store.presentation
  readonly property var usage: card.store.usage
  readonly property color tone: card.view.tone === "error" || card.view.tone === "recording"
    ? card.theme.red
    : card.view.tone === "playing"
      ? card.theme.green
      : card.view.tone === "live"
        ? card.theme.accent
        : card.theme.overlay

  width: parent.width
  spacing: card.theme.spaceSmall

  Shared.SectionRule {
    theme: card.theme
    width: parent.width
    label: "MICROPHONE TEST"
    detail: card.store.inputName

    Shared.StatusChip {
      anchors.verticalCenter: parent.verticalCenter
      visible: card.view.active
      theme: card.theme
      text: card.view.title
      tint: card.tone
    }
  }

  Rectangle {
    width: parent.width
    implicitHeight: body.implicitHeight + card.theme.cardPadding * 2
    height: implicitHeight
    radius: card.theme.radius
    color: card.theme.cardColor
    antialiasing: true

    Shared.CardEdge { theme: card.theme }

    Column {
      id: body

      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: parent.top
      anchors.margins: card.theme.cardPadding
      spacing: card.theme.spaceSmall

      Text {
        width: parent.width
        text: card.view.detail
        textFormat: Text.PlainText
        color: card.view.tone === "error" ? card.theme.red : card.theme.subtext
        font.family: card.theme.fontFamily
        font.pixelSize: card.theme.textCaption
        wrapMode: Text.Wrap
      }

      // The bar is drawn on a compressed scale so speech is visible along it;
      // clipping is reported beside it from the samples themselves, so the
      // meter's shape can never hide it.
      Item {
        width: parent.width
        height: meter.implicitHeight

        Shared.MeterBar {
          id: meter

          anchors.left: parent.left
          anchors.right: clipping.left
          anchors.rightMargin: clipping.visible ? card.theme.spaceSmall : 0
          anchors.verticalCenter: parent.verticalCenter
          theme: card.theme
          ratio: card.view.level
          fill: card.view.clipped ? card.theme.red : card.tone
        }

        Text {
          id: clipping

          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          visible: card.view.clipped || card.view.sampleClipped
          width: visible ? implicitWidth : 0
          text: "CLIPPING"
          textFormat: Text.PlainText
          color: card.theme.red
          font.family: card.theme.fontFamily
          font.pixelSize: card.theme.textMicro
          font.weight: card.theme.weightMedium
          font.letterSpacing: card.theme.trackingLabel
        }
      }

      // Wrapping rather than a fixed row, so every control stays reachable at
      // a narrower panel instead of the last one leaving the card.
      Flow {
        width: parent.width
        spacing: card.theme.spaceSmall

        Shared.ActionButton {
          theme: card.theme
          text: "Record 5 s"
          enabled: card.store.usageReady && card.view.canRecord
          onClicked: card.store.begin("sample")
        }
        Shared.ActionButton {
          theme: card.theme
          text: "Listen live"
          enabled: card.store.usageReady && card.view.canLive
          onClicked: card.store.begin("live")
        }
        Shared.ActionButton {
          theme: card.theme
          text: "Replay"
          enabled: card.view.canReplay
          onClicked: card.store.replay()
        }
        Shared.ActionButton {
          theme: card.theme
          text: "Stop"
          danger: true
          enabled: card.view.canStop
          onClicked: card.store.stop()
        }
      }

      Text {
        width: parent.width
        visible: !card.view.active
        text: "Use headphones. Playing the microphone through a speaker can feed back into it."
        textFormat: Text.PlainText
        color: card.theme.overlay
        font.family: card.theme.fontFamily
        font.pixelSize: card.theme.textMicro
        wrapMode: Text.Wrap
      }

      // The test's own output. It names one stream and changes no default, so
      // nothing else moves with it. The disclosure is a button rather than a
      // fold arrow, because choosing where the test plays has to be reachable
      // from the keyboard like every other control here.
      Shared.ActionButton {
        theme: card.theme
        width: parent.width
        text: (card.outputsExpanded ? "󰅃  " : "󰅀  ") + "Test output: " + (card.store.outputName || "none")
        onClicked: card.outputsExpanded = !card.outputsExpanded
      }

      Column {
        width: parent.width
        visible: card.outputsExpanded
        spacing: card.theme.spaceTight

        Repeater {
          model: card.store.outputs

          Shared.ActionButton {
            required property var modelData
            theme: card.theme
            width: parent.width
            text: modelData.name
            selected: modelData.node === card.store.output
            onClicked: card.store.chooseOutput(modelData.node)
          }
        }
      }

      Shared.StatusBanner {
        width: parent.width
        visible: card.view.muted
        theme: card.theme
        glyph: "󰍭"
        tint: card.theme.yellow
        title: "This microphone is muted"
        detail: "The test records it as it is; unmute it yourself to hear anything."
      }

      // Microphone use stays on screen for as long as it lasts, because a
      // confirmed test does not make the microphone exclusive.
      Shared.StatusBanner {
        width: parent.width
        visible: card.usage.title !== "" && card.store.confirming === ""
        theme: card.theme
        glyph: card.usage.busy ? "󰍬" : "󰋗"
        tint: card.usage.busy ? card.theme.yellow : card.theme.overlay
        title: card.usage.title
        detail: card.usage.detail
      }

      Shared.StatusBanner {
        width: parent.width
        visible: card.store.notice !== ""
        theme: card.theme
        glyph: "󰀪"
        tint: card.theme.red
        title: card.store.notice
      }

      Shared.StatusBanner {
        id: confirmation

        width: parent.width
        visible: card.store.confirming !== ""
        theme: card.theme
        glyph: "󰍬"
        tint: card.theme.yellow
        title: card.usage.title
        detail: card.usage.detail
      }

      Flow {
        width: parent.width
        visible: confirmation.visible
        spacing: card.theme.spaceSmall

        Shared.ActionButton {
          theme: card.theme
          text: "Test anyway"
          selected: true
          onClicked: card.store.confirm()
        }
        Shared.ActionButton {
          theme: card.theme
          text: "Cancel"
          onClicked: card.store.cancel()
        }
      }
    }
  }
}
