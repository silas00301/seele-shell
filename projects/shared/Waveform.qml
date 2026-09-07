pragma ComponentBehavior: Bound
import QtQuick

Item {
  id: waveform
  required property var theme
  property var samples: []
  property color tint: theme.accent
  readonly property int bars: 40

  function push(level) {
    var value = Number(level)
    var next = samples.slice(-(bars - 1))
    next.push(isFinite(value) ? Math.max(0, Math.min(1, value)) : 0)
    samples = next
  }
  function clear() { samples = [] }

  Row {
    anchors.fill: parent
    spacing: waveform.theme.spaceTight
    Repeater {
      model: waveform.bars
      Rectangle {
        required property int index
        readonly property int sampleIndex: index - (waveform.bars - waveform.samples.length)
        width: Math.max(1, (waveform.width - (waveform.bars - 1) * waveform.theme.spaceTight) / waveform.bars)
        height: Math.max(waveform.theme.spaceTight, waveform.height * (sampleIndex >= 0 ? waveform.samples[sampleIndex] : 0))
        anchors.verticalCenter: parent.verticalCenter
        radius: width / 2
        color: waveform.tint
        opacity: 0.35 + 0.65 * (index + 1) / waveform.bars
        Behavior on height { NumberAnimation { duration: waveform.theme.durationFast } }
      }
    }
  }
}
