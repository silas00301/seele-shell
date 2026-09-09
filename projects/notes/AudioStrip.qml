pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared
import "notes.js" as Notes

// Recording and playback. The section draws nothing at all when a note has no
// audio and none is being captured, so a written note keeps the whole writing
// area.
ColumnLayout {
  id: strip

  required property var theme
  required property var store
  required property var player
  property string playing: ""
  property bool popupHovered: false

  readonly property var tracks: strip.store.audio
  readonly property real span: strip.player ? Math.max(1, strip.player.duration) : 1

  signal removeRequested(string name)

  spacing: strip.theme.spaceMedium
  visible: strip.store.recording || strip.tracks.length > 0

  function clear() { waveform.clear() }
  function push(level) { waveform.push(level) }

  function toggle(track) {
    if (!strip.player || !track.path || strip.store.recording) return
    if (strip.playing === track.name) {
      if (strip.player.playing) strip.player.pause()
      else strip.player.play()
      return
    }
    strip.player.stop()
    strip.playing = track.name
    strip.player.source = "file://" + track.path.split("/").map(encodeURIComponent).join("/")
    strip.player.play()
  }

  function toggleCurrent() {
    if (!strip.tracks.length) return
    var track = strip.tracks.find(function(item) { return item.name === strip.playing }) || strip.tracks[0]
    strip.toggle(track)
  }

  function seekBy(milliseconds) {
    if (strip.player && strip.playing) strip.player.seek(Math.max(0, strip.player.position + milliseconds))
  }

  Shared.SectionRule {
    Layout.fillWidth: true
    theme: strip.theme
    label: strip.store.recording ? "RECORDING" : "AUDIO"
    detail: strip.store.recording
      ? Notes.duration(strip.store.recordingDuration)
      : String(strip.tracks.length)
    detailColor: strip.store.recording ? strip.theme.red : strip.theme.overlay
  }

  Rectangle {
    Layout.fillWidth: true
    Layout.preferredHeight: strip.theme.rowHeight
    visible: strip.store.recording
    radius: strip.theme.radiusSmall
    color: strip.theme.wellColor
    border.width: 1
    border.color: strip.theme.cardBorder
    antialiasing: true

    Shared.Waveform {
      id: waveform

      theme: strip.theme
      anchors.fill: parent
      anchors.margins: strip.theme.spaceSmall
      tint: strip.theme.red
    }
  }

  Shared.DeviceListCard {
    Layout.fillWidth: true
    theme: strip.theme
    visible: strip.tracks.length > 0
    listHeight: Math.min(strip.theme.notesMemoListHeight, tracks.contentHeight)

    Shared.SeeleListView {
      id: tracks

      theme: strip.theme
      anchors.fill: parent
      anchors.margins: strip.theme.cardPadding
      clip: true
      spacing: strip.theme.spaceTight
      model: strip.tracks

      delegate: Rectangle {
        id: track

        required property var modelData
        readonly property bool current: strip.playing === track.modelData.name
        readonly property bool active: track.current && !!strip.player && strip.player.playing
        readonly property bool available: !!track.modelData.path

        width: tracks.width
        height: strip.theme.rowHeight
        radius: strip.theme.radiusSmall
        color: track.current ? strip.theme.selectedColor : strip.theme.rowColor
        antialiasing: true

        Behavior on color { ColorAnimation { duration: strip.theme.durationFast } }

        Shared.HoverWash { theme: strip.theme; hovered: trackHover.hovered }
        HoverHandler { id: trackHover }

        RowLayout {
          anchors.fill: parent
          anchors.leftMargin: strip.theme.spaceSmall
          anchors.rightMargin: strip.theme.spaceSmall
          spacing: strip.theme.spaceMedium

          Shared.IconButton {
            theme: strip.theme
            Layout.preferredWidth: strip.theme.chipHeight
            Layout.preferredHeight: strip.theme.chipHeight
            active: track.active
            hovered: playMouse.containsMouse
            pressed: playMouse.pressed
            opacity: playMouse.enabled ? 1 : 0.45

            Shared.CenteredGlyph {
              anchors.fill: parent
              text: track.available ? (track.active ? "󰏤" : "󰐊") : "󰝛"
              color: track.active ? strip.theme.accent : track.available ? strip.theme.text : strip.theme.red
              font.family: strip.theme.fontFamily
              font.pixelSize: strip.theme.textStrong
            }

            MouseArea {
              id: playMouse

              anchors.fill: parent
              enabled: track.available && strip.player !== null && !strip.store.recording
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: strip.toggle(track.modelData)
            }
          }

          ColumnLayout {
            Layout.fillWidth: true
            spacing: 0

            Text {
              Layout.fillWidth: true
              text: track.modelData.name
              elide: Text.ElideMiddle
              color: track.available ? strip.theme.text : strip.theme.red
              font.family: strip.theme.fontFamily
              font.pixelSize: strip.theme.textBody
              font.weight: track.current ? strip.theme.weightStrong : strip.theme.weightMedium
            }

            Text {
              Layout.fillWidth: true
              visible: !track.available
              text: "Not found in this vault"
              color: strip.theme.overlay
              font.family: strip.theme.fontFamily
              font.pixelSize: strip.theme.textCaption
            }
          }

          Text {
            visible: track.available
            text: Notes.duration(track.modelData.duration)
            color: strip.theme.subtext
            font.family: strip.theme.fontFamily
            font.pixelSize: strip.theme.textCaption
          }

          // Removing an embed edits the note. The recording stays in the
          // vault, because another note may still be pointing at it.
          Shared.IconButton {
            theme: strip.theme
            Layout.preferredWidth: strip.theme.chipHeight
            Layout.preferredHeight: strip.theme.chipHeight
            tint: strip.theme.red
            hovered: removeMouse.containsMouse
            pressed: removeMouse.pressed
            enabled: !strip.store.recording

            Shared.CenteredGlyph {
              anchors.fill: parent
              text: "󰅖"
              color: removeMouse.containsMouse ? strip.theme.red : strip.theme.overlay
              font.family: strip.theme.fontFamily
              font.pixelSize: strip.theme.textBody
            }

            MouseArea {
              id: removeMouse

              anchors.fill: parent
              enabled: !strip.store.recording
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: strip.removeRequested(track.modelData.name)
            }
          }
        }
      }

      ScrollBar.vertical: Shared.SlimScrollBar { theme: strip.theme; popupHovered: strip.popupHovered }
    }
  }

  RowLayout {
    id: scrub

    Layout.fillWidth: true
    visible: strip.playing !== "" && !!strip.player
    spacing: strip.theme.spaceMedium

    Text {
      text: Notes.duration(strip.player ? strip.player.position : 0)
      color: strip.theme.subtext
      font.family: strip.theme.fontFamily
      font.pixelSize: strip.theme.textCaption
    }

    // The track is thin, so the grab is the row around it rather than the bar
    // itself; a meter that has to be hit exactly is a meter that gets missed.
    Item {
      Layout.fillWidth: true
      Layout.preferredHeight: strip.theme.chipHeight

      Shared.MeterBar {
        id: scrubTrack

        theme: strip.theme
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        ratio: strip.player ? strip.player.position / strip.span : 0
      }

      MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onPressed: mouse => strip.player.seek(Math.max(0, Math.min(1, mouse.x / scrubTrack.width)) * strip.span)
        onPositionChanged: mouse => {
          if (pressed) strip.player.seek(Math.max(0, Math.min(1, mouse.x / scrubTrack.width)) * strip.span)
        }
      }
    }

    Text {
      text: Notes.duration(strip.span)
      color: strip.theme.subtext
      font.family: strip.theme.fontFamily
      font.pixelSize: strip.theme.textCaption
    }
  }
}
