import QtQuick

// A filled track. Every meter in the shell -- capacity, daily usage, battery,
// the volume OSD, a day's temperature span -- is this one shape: Material 3
// Expressive's linear progress indicator. The filled part and the unfilled
// track are separate pills with a gap between them, and a stop dot marks the
// end of the track so a nearly empty meter still says where full is. The
// unfilled track is the fill's own colour toned down over the surface, so a
// warning meter and a calm one each read as one instrument. A meter that
// tracks something under way -- a track that is playing -- can be `wavy`: the
// filled part becomes Expressive's wave, which flows while `flowing` is set,
// so progress that is moving looks like it.
Item {
  id: meterBar

  required property var theme
  property real ratio: 0
  // Where the fill starts. A level starts at the empty end; a span, such as a
  // day's low to high on the week's scale, starts partway along the track.
  property real from: 0
  property color fill: meterBar.theme.primary
  property bool wavy: false
  property bool flowing: false
  property real wavePhase: 0
  readonly property color track: Qt.tint(meterBar.theme.surfaceContainerHighest, meterBar.theme.alpha(meterBar.fill, 0.22))
  readonly property real start: Math.max(0, Math.min(1, meterBar.from))
  readonly property real end: Math.max(meterBar.start, Math.min(1, meterBar.ratio))
  readonly property real gap: meterBar.end > meterBar.start ? meterBar.theme.meterGap : 0
  // A filled part never shrinks below a dot, so a reading just above nothing
  // is still a reading.
  readonly property real filledWidth: meterBar.end > meterBar.start ? Math.max(height, width * (meterBar.end - meterBar.start)) : 0
  readonly property real filledX: Math.min(width - meterBar.filledWidth, width * meterBar.start)

  implicitHeight: meterBar.theme.meterHeight

  Rectangle {
    visible: meterBar.filledX - meterBar.gap > 0
    width: Math.max(0, meterBar.filledX - meterBar.gap)
    height: parent.height
    radius: height / 2
    color: meterBar.track
    antialiasing: true
  }

  Rectangle {
    visible: !meterBar.wavy
    x: meterBar.filledX
    width: meterBar.filledWidth
    height: parent.height
    radius: height / 2
    color: meterBar.fill
    antialiasing: true
  }

  // The wave is drawn through the strip around the meter, so its crests have
  // room: it is centred on the track and reaches `meterWaveAmplitude` either
  // side of it.
  Canvas {
    id: meterWave

    visible: meterBar.wavy && meterBar.filledWidth > 0
    x: meterBar.filledX
    y: (parent.height - height) / 2
    width: meterBar.filledWidth
    height: meterBar.height + meterBar.theme.meterWaveAmplitude * 2
    antialiasing: true
    onWidthChanged: requestPaint()
    onVisibleChanged: if (visible) requestPaint()
    Connections {
      target: meterBar
      function onWavePhaseChanged() { meterWave.requestPaint() }
      function onFillChanged() { meterWave.requestPaint() }
    }
    onPaint: {
      var context = getContext("2d")
      context.reset()
      var stroke = meterBar.height
      var amplitude = meterBar.theme.meterWaveAmplitude
      var length = meterBar.theme.meterWavelength
      var middle = height / 2
      context.lineWidth = stroke
      context.lineCap = "round"
      context.lineJoin = "round"
      context.strokeStyle = meterBar.fill
      context.beginPath()
      for (var x = stroke / 2; x <= width - stroke / 2; x += 1) {
        // The wave flattens into the track at the leading end, so it starts
        // from the track rather than from a crest.
        var ease = Math.min(1, x / length)
        var y = middle + Math.sin((x / length + meterBar.wavePhase) * Math.PI * 2) * amplitude * ease
        if (x === stroke / 2) context.moveTo(x, y)
        else context.lineTo(x, y)
      }
      context.stroke()
    }
  }

  NumberAnimation on wavePhase {
    from: 0
    to: -1
    duration: meterBar.theme.meterWavePeriod
    loops: Animation.Infinite
    running: meterBar.wavy && meterBar.flowing && meterBar.visible
  }

  Rectangle {
    id: meterRest

    readonly property real startX: meterBar.filledX + meterBar.filledWidth + meterBar.gap
    visible: parent.width - meterRest.startX > 0
    x: meterRest.startX
    width: Math.max(0, parent.width - meterRest.startX)
    height: parent.height
    radius: height / 2
    color: meterBar.track
    antialiasing: true

    Rectangle {
      visible: meterRest.width >= meterBar.height * 2
      anchors.right: parent.right
      anchors.rightMargin: (parent.height - height) / 2
      anchors.verticalCenter: parent.verticalCenter
      width: Math.max(2, parent.height - meterBar.theme.spaceTight / 2)
      height: width
      radius: width / 2
      color: meterBar.fill
      antialiasing: true
    }
  }
}
