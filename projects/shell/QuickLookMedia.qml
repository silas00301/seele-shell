import QtQuick
import QtMultimedia
import "../shared" as Shared
import "quicklook.js" as QuickLook

// The sound and moving-picture body of Quick Look, loaded in isolation from
// the rest of the shell so a QtMultimedia backend that will not start costs
// this one preview rather than the whole surface.
//
// Nothing plays until it is asked to. A preview is opened to find out what a
// file is, and a folder walked through with the arrow keys would otherwise
// become a sequence of noises.
Item {
  id: media

  required property var theme
  property string path: ""
  property bool video: false
  property bool playing: false
  readonly property bool failed: player.error !== MediaPlayer.NoError

  signal toggled()
  signal finished()

  onPathChanged: player.stop()

  onPlayingChanged: {
    if (media.playing) player.play()
    else player.pause()
  }

  Component.onDestruction: player.stop()

  MediaPlayer {
    id: player
    source: media.path !== "" ? QuickLook.url(media.path) : ""
    videoOutput: output
    audioOutput: AudioOutput { }
    onMediaStatusChanged: if (mediaStatus === MediaPlayer.EndOfMedia) media.finished()
  }

  VideoOutput {
    id: output
    anchors.fill: parent
    anchors.margins: media.theme.cardPadding
    anchors.bottomMargin: transport.height + media.theme.cardPadding * 2
    visible: media.video
    fillMode: VideoOutput.PreserveAspectFit
  }

  Text {
    anchors.centerIn: parent
    visible: !media.video
    text: media.video ? "󰎁" : "󰎆"
    color: media.theme.overlay
    font.family: media.theme.fontFamily
    font.pixelSize: media.theme.textHero
  }

  Text {
    anchors.centerIn: parent
    visible: media.failed
    text: "This file cannot be played"
    textFormat: Text.PlainText
    color: media.theme.subtext
    font.family: media.theme.fontFamily
    font.pixelSize: media.theme.textBody
  }

  Row {
    id: transport
    visible: !media.failed
    anchors.bottom: parent.bottom
    anchors.horizontalCenter: parent.horizontalCenter
    anchors.bottomMargin: media.theme.cardPadding
    spacing: media.theme.spaceSmall

    Shared.GlyphButton {
      theme: media.theme
      glyph: media.playing ? "󰏤" : "󰐊"
      text: media.playing ? "Pause" : "Play"
      selected: media.playing
      onClicked: media.toggled()
    }

    // The bar reports position and is the only way to seek. Left and Right
    // move by five seconds while it owns focus; otherwise they move between
    // highlighted files in the surrounding preview.
    FocusScope {
      id: timeline
      anchors.verticalCenter: parent.verticalCenter
      width: Math.min(media.width - media.theme.controlHeight * 6, media.theme.quickLookTimelineWidth)
      height: media.theme.controlHeight
      activeFocusOnTab: player.seekable
      Accessible.role: Accessible.Slider
      Accessible.name: "Playback position"
      Accessible.value: player.position
      Accessible.maximumValue: player.duration
      Keys.onPressed: event => {
        if (event.key !== Qt.Key_Left && event.key !== Qt.Key_Right) return
        // A rejected variant still belongs to this focused control. Letting it
        // bubble would unexpectedly navigate to another file.
        event.accepted = true
        if (!player.seekable || event.isAutoRepeat || event.modifiers !== Qt.NoModifier) return
        var offset = event.key === Qt.Key_Left ? -5000
          : 5000
        player.setPosition(Math.max(0, Math.min(player.duration, player.position + offset)))
      }

      Shared.MeterBar {
        id: positionMeter
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        theme: media.theme
        ratio: player.duration > 0 ? player.position / player.duration : 0
      }

      MouseArea {
        anchors.fill: parent
        enabled: player.seekable
        cursorShape: Qt.PointingHandCursor
        // `position` is read-only; seeking goes through the player's own call.
        onPressed: mouse => {
          timeline.forceActiveFocus()
          player.setPosition(Math.round(
            player.duration * Math.max(0, Math.min(1, mouse.x / Math.max(1, width)))))
        }
      }

      Shared.FocusRing {
        theme: media.theme
        shown: timeline.activeFocus
      }
    }

    Text {
      anchors.verticalCenter: parent.verticalCenter
      text: QuickLook.duration(player.position) + " / " + QuickLook.duration(player.duration)
      textFormat: Text.PlainText
      color: media.theme.subtext
      font.family: media.theme.fontFamily
      font.pixelSize: media.theme.textCaption
    }
  }
}
