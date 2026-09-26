import QtQuick
import QtQuick.Controls

// A device level is drawn as the Audio panel and the player draw theirs: a
// well carrying its own fill, named inside the track it sets. It stays a
// Slider so the keyboard and accessibility contract is Qt's.
Slider {
  id: level
  required property var theme
  required property string title
  required property real current
  property real minimum: 1
  property real maximum: 100
  property real step: 1
  property string suffix: "%"
  property real draft: current
  // A level that sets a colour rather than an amount, such as a light's
  // temperature, draws that colour: the colours at the track's start, middle
  // and end. The well carries the whole range faintly and the fill runs
  // through it to the colour of the value, so the track shows the setting.
  property var spectrum: []
  readonly property bool hasSpectrum: level.spectrum.length === 3
  // `changing` follows the pointer for a device that can take every step,
  // such as a light being dimmed; `committed` is the value the user let go on.
  signal changing(real value)
  signal committed(real value)
  objectName: level.title
  Accessible.name: level.title
  implicitHeight: level.theme.rowHeight
  padding: 0
  from: level.minimum
  to: level.maximum
  stepSize: level.step
  live: true
  opacity: level.enabled ? 1 : level.theme.disabledOpacity
  Binding on value {
    value: level.current
    when: !level.pressed
    restoreMode: Binding.RestoreNone
  }
  onMoved: {
    level.draft = level.value
    level.changing(level.value)
  }
  // Only user input commits. A state update never sends another request.
  onPressedChanged: {
    if (level.pressed) level.draft = level.value
    else level.committed(level.draft)
  }
  Keys.onReleased: event => {
    if (!event.isAutoRepeat && (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Home || event.key === Qt.Key_End)) level.committed(level.value)
  }
  background: Rectangle {
    radius: level.theme.radius
    color: level.theme.wellColor
    border.width: 1
    border.color: level.activeFocus ? level.theme.accent : level.theme.alpha(level.theme.text, 0.05)
    clip: true
    antialiasing: true
    Rectangle {
      visible: level.hasSpectrum
      anchors.fill: parent
      radius: parent.radius
      opacity: 0.28
      antialiasing: true
      gradient: Gradient {
        orientation: Gradient.Horizontal
        GradientStop { position: 0; color: level.hasSpectrum ? level.spectrum[0] : "black" }
        GradientStop { position: 0.5; color: level.hasSpectrum ? level.spectrum[1] : "black" }
        GradientStop { position: 1; color: level.hasSpectrum ? level.spectrum[2] : "black" }
      }
    }
    Rectangle {
      id: levelFill
      // A colour is never "none": the fill keeps a rounded cap at the start of
      // the range, so the warmest setting still shows its colour.
      width: level.hasSpectrum ? Math.max(parent.height, parent.width * level.visualPosition) : parent.width * level.visualPosition
      height: parent.height
      radius: parent.radius
      color: level.theme.fillColor
      antialiasing: true
      // The fill is only part of the track, so its stops are the track's
      // colours at its own start, at the track's middle, and at its end.
      gradient: level.hasSpectrum ? levelSpectrum : null
      opacity: level.hasSpectrum ? 0.62 : 1
      Gradient {
        id: levelSpectrum
        orientation: Gradient.Horizontal
        GradientStop { position: 0; color: level.hasSpectrum ? level.spectrumAt(0) : "black" }
        GradientStop {
          position: Math.min(1, 0.5 * levelFill.parent.width / Math.max(1, levelFill.width))
          color: level.hasSpectrum ? level.spectrumAt(Math.min(0.5, levelFill.width / Math.max(1, levelFill.parent.width))) : "black"
        }
        GradientStop { position: 1; color: level.hasSpectrum ? level.spectrumAt(levelFill.width / Math.max(1, levelFill.parent.width)) : "black" }
      }
    }
    Text {
      id: levelReadout
      anchors { right: parent.right; rightMargin: level.theme.spaceLarge; verticalCenter: parent.verticalCenter }
      text: Math.round(level.value) + level.suffix
      textFormat: Text.PlainText
      // A colour track is lit to its far end, where the quiet readout would
      // sink into the cool end of the range.
      color: level.hasSpectrum ? level.theme.text : level.theme.subtext
      font.family: level.theme.fontFamily
      font.pixelSize: level.theme.textBody
    }
    Text {
      anchors { left: parent.left; leftMargin: level.theme.spaceLarge; right: levelReadout.left; rightMargin: level.theme.spaceMedium; verticalCenter: parent.verticalCenter }
      text: level.title
      textFormat: Text.PlainText
      elide: Text.ElideRight
      color: level.theme.text
      font.family: level.theme.fontFamily
      font.pixelSize: level.theme.textBody
      font.weight: level.theme.weightStrong
    }
  }
  function mix(from, to, amount) {
    return Qt.rgba(from.r + (to.r - from.r) * amount, from.g + (to.g - from.g) * amount, from.b + (to.b - from.b) * amount, 1)
  }
  // The spectrum's colour at `position` along the track, 0 to 1.
  function spectrumAt(position) {
    var start = Qt.color(level.spectrum[0]), middle = Qt.color(level.spectrum[1]), end = Qt.color(level.spectrum[2])
    return position <= 0.5 ? level.mix(start, middle, position * 2) : level.mix(middle, end, (position - 0.5) * 2)
  }
  handle: Item {}
  HoverHandler { cursorShape: level.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
