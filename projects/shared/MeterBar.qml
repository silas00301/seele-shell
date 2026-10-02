import QtQuick

// A filled track. Every meter in the shell -- capacity, daily usage, battery,
// the volume OSD, a day's temperature span -- is this one shape, rounded on
// its own height rather than on a literal that had outgrown the bar it was
// drawn in.
Rectangle {
  id: meterBar

  required property var theme
  property real ratio: 0
  // Where the fill starts. A level starts at the empty end; a span, such as a
  // day's low to high on the week's scale, starts partway along the track.
  property real from: 0
  property color fill: meterBar.theme.accent

  implicitHeight: 7
  radius: height / 2
  color: meterBar.theme.wellColor
  border.width: 1
  border.color: meterBar.theme.alpha(meterBar.theme.text, 0.05)
  antialiasing: true

  // The filled part is graded along its length rather than laid down flat,
  // so a full meter still reads as a lit instrument instead of a block of
  // colour.
  Rectangle {
    readonly property real start: Math.max(0, Math.min(1, meterBar.from))
    x: parent.width * start
    width: parent.width * Math.max(0, Math.min(1, meterBar.ratio) - start)
    height: parent.height
    radius: parent.radius
    antialiasing: true

    gradient: Gradient {
      orientation: Gradient.Horizontal
      GradientStop { position: 0.0; color: meterBar.theme.alpha(meterBar.fill, 0.62) }
      GradientStop { position: 1.0; color: meterBar.fill }
    }
  }
}
