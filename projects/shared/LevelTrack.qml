pragma ComponentBehavior: Bound
import QtQuick

// The drawing of a level the pointer sets, as Material 3 Expressive's large
// slider: the filled part of the track and the rest of it are separate pills
// on either side of an upright handle, with a gap between each, and the level
// is named inside the track it sets. The name and the reading change colour
// where the fill passes under them, so each stays legible on whichever part
// it sits. It draws and nothing more: the control that owns it decides what a
// press, a drag or a key does, so the Audio panel's batched volume, a Home
// Assistant light and a stream's own level all look like one instrument.
Item {
  id: track

  required property var theme
  property real ratio: 0
  property string title: ""
  property string value: ""
  // A glyph leading the title, drawn in the same ink as the title.
  property string glyph: ""
  // Space kept clear at the leading end for a control the owner lays over
  // the track, such as a mute button or an application's icon.
  property real leadingInset: 0
  property bool muted: false
  property bool dimmed: false
  property bool hovered: false
  property bool pressed: false
  property bool focused: false
  // A level that sets a colour rather than an amount, such as a light's
  // temperature: the colours at the track's start, middle and end. The rest
  // of the track carries the whole range faintly and the fill runs through
  // it to the colour of the value.
  property var spectrum: []
  readonly property bool hasSpectrum: track.spectrum.length === 3
  readonly property real gap: track.theme.trackHandleGap
  readonly property real handleWidth: track.pressed ? track.theme.trackHead / 2 : track.theme.trackHead
  // The handle's centre runs between the two ends of the track, held far
  // enough in that neither part ever has a negative width.
  readonly property real edge: track.gap + track.theme.trackHead / 2
  readonly property real handleCentre: track.edge + Math.max(0, Math.min(1, track.ratio)) * (track.width - track.edge * 2)
  readonly property real fillWidth: Math.max(0, track.handleCentre - track.edge)
  readonly property real restX: track.handleCentre + track.edge
  readonly property real partHeight: track.height - track.gap * 2
  readonly property color fillColor: track.muted ? track.theme.error : track.theme.primary
  readonly property color fillInk: track.hasSpectrum ? track.theme.inkOn(track.spectrumAt(0.5))
    : track.muted ? track.theme.textOnError
    : track.theme.textOnPrimary
  readonly property color restColor: track.hasSpectrum ? track.theme.surfaceContainerHighest : track.theme.secondaryContainer
  readonly property color restInk: track.hasSpectrum ? track.theme.text : track.theme.textOnSecondaryContainer

  function mix(from, to, amount) {
    return Qt.rgba(from.r + (to.r - from.r) * amount, from.g + (to.g - from.g) * amount, from.b + (to.b - from.b) * amount, 1)
  }

  // The spectrum's colour at `position` along the track, 0 to 1.
  function spectrumAt(position) {
    if (!track.hasSpectrum) return track.theme.primary
    var start = Qt.color(track.spectrum[0]), middle = Qt.color(track.spectrum[1]), end = Qt.color(track.spectrum[2])
    return position <= 0.5 ? track.mix(start, middle, position * 2) : track.mix(middle, end, (position - 0.5) * 2)
  }

  opacity: track.dimmed ? track.theme.disabledOpacity : 1

  // The labels once more under both parts, in the surface's own ink, so a
  // letter that falls in the gap around the handle is still drawn and a name
  // or a reading the handle passes through reads as one word.
  Loader {
    anchors.fill: parent
    sourceComponent: trackLabels
    onLoaded: item.ink = Qt.binding(() => track.theme.text)
  }

  Rectangle {
    id: trackRest

    x: track.restX
    width: Math.max(0, parent.width - track.restX)
    height: track.partHeight
    anchors.verticalCenter: parent.verticalCenter
    radius: track.theme.shapeMedium
    topLeftRadius: track.theme.shapeExtraSmall
    bottomLeftRadius: track.theme.shapeExtraSmall
    color: track.restColor
    clip: true
    antialiasing: true

    Rectangle {
      visible: track.hasSpectrum
      x: -trackRest.x
      width: track.width
      height: parent.height
      opacity: 0.32
      gradient: Gradient {
        orientation: Gradient.Horizontal
        GradientStop { position: 0; color: track.hasSpectrum ? track.spectrum[0] : "black" }
        GradientStop { position: 0.5; color: track.hasSpectrum ? track.spectrum[1] : "black" }
        GradientStop { position: 1; color: track.hasSpectrum ? track.spectrum[2] : "black" }
      }
    }

    HoverWash { theme: track.theme; hovered: track.hovered; pressed: track.pressed; tint: track.theme.alpha(track.restInk, track.theme.stateHover) }

    Loader {
      x: -trackRest.x
      y: -track.gap
      width: track.width
      height: track.height
      sourceComponent: trackLabels
      onLoaded: item.ink = Qt.binding(() => track.restInk)
    }
  }

  Rectangle {
    id: trackFill

    visible: width > 0
    width: track.fillWidth
    height: track.partHeight
    anchors.verticalCenter: parent.verticalCenter
    radius: track.theme.shapeMedium
    topRightRadius: track.theme.shapeExtraSmall
    bottomRightRadius: track.theme.shapeExtraSmall
    color: track.fillColor
    clip: true
    antialiasing: true
    gradient: track.hasSpectrum ? trackSpectrum : null

    Behavior on color { ColorAnimation { duration: track.theme.durationFast } }

    // The fill is only part of the track, so its stops are the track's
    // colours at its own start, at the track's middle, and at its end.
    Gradient {
      id: trackSpectrum
      orientation: Gradient.Horizontal
      GradientStop { position: 0; color: track.spectrumAt(0) }
      GradientStop {
        position: Math.min(1, 0.5 * track.width / Math.max(1, trackFill.width))
        color: track.spectrumAt(Math.min(0.5, trackFill.width / Math.max(1, track.width)))
      }
      GradientStop { position: 1; color: track.spectrumAt(trackFill.width / Math.max(1, track.width)) }
    }

    HoverWash { theme: track.theme; hovered: track.hovered; pressed: track.pressed; tint: track.theme.alpha(track.fillInk, track.theme.stateHover) }

    Loader {
      y: -track.gap
      width: track.width
      height: track.height
      sourceComponent: trackLabels
      onLoaded: item.ink = Qt.binding(() => track.fillInk)
    }
  }

  // The handle: an upright bar the full height of the strip, which thins
  // while it is held so the value under it shows.
  Rectangle {
    width: track.handleWidth
    height: parent.height
    x: track.handleCentre - width / 2
    radius: width / 2
    color: track.hasSpectrum ? track.theme.text : track.fillColor
    antialiasing: true
    Behavior on width { NumberAnimation { duration: track.theme.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: track.theme.springFastSpatial } }
  }

  FocusRing { theme: track.theme; shown: track.focused; baseRadius: track.theme.shapeMedium }

  Component {
    id: trackLabels

    Item {
      property color ink: track.theme.text

      // The reading sits at the far end of the track, and moves to the end of
      // the filled part once the rest of the track is too short to hold it,
      // so the handle never runs through it.
      Text {
        id: trackValue
        readonly property bool inFill: track.width - track.restX < track.theme.levelValueWidth + track.theme.spaceLarge
        anchors.verticalCenter: parent.verticalCenter
        x: (trackValue.inFill ? track.fillWidth - track.theme.spaceMedium : track.width - track.theme.spaceLarge) - width
        width: track.theme.levelValueWidth
        horizontalAlignment: Text.AlignRight
        text: track.value
        textFormat: Text.PlainText
        color: parent.ink
        font.family: track.theme.fontFamily
        font.pixelSize: track.theme.textLabel
        font.weight: track.theme.weightStrong
      }

      Text {
        anchors { left: parent.left; leftMargin: track.theme.spaceLarge + track.leadingInset; right: parent.right; rightMargin: track.theme.spaceLarge + track.theme.levelValueWidth + track.theme.spaceMedium; verticalCenter: parent.verticalCenter }
        text: track.glyph !== "" ? track.glyph + "  " + track.title : track.title
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: parent.ink
        font.family: track.theme.fontFamily
        font.pixelSize: track.theme.textBody
        font.weight: track.theme.weightStrong
      }
    }
  }
}
