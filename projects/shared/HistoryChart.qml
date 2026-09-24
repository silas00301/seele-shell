import QtQuick

// A render-only, fixed-window history. Missing observations break the stroke;
// newly opened panels grow from the right instead of stretching a short trace.
Canvas {
  id: chart
  required property var theme
  property var series: []
  property int capacity: 60
  property real maximum: 100
  implicitHeight: theme.controlHeight * 2
  onSeriesChanged: requestPaint()
  onMaximumChanged: requestPaint()
  onCapacityChanged: requestPaint()
  onWidthChanged: requestPaint()
  onHeightChanged: requestPaint()
  onThemeChanged: requestPaint()
  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()
    if (width <= 0 || height <= 0 || capacity < 2 || !isFinite(maximum) || maximum <= 0) return
    ctx.lineWidth = theme.hairline
    ctx.strokeStyle = theme.edgeLight
    for (var grid = 0; grid <= 2; grid++) {
      var gy = theme.hairline + (height - theme.hairline * 2) * grid / 2
      ctx.beginPath(); ctx.moveTo(0, gy); ctx.lineTo(width, gy); ctx.stroke()
    }
    for (var s = 0; s < series.length; s++) {
      var values = (series[s].values || []).slice(-capacity)
      ctx.strokeStyle = series[s].color || theme.accent
      ctx.lineWidth = theme.hairline * 2
      ctx.lineJoin = "round"
      ctx.beginPath()
      var drawing = false
      for (var i = 0; i < values.length; i++) {
        var value = values[i]
        if (typeof value !== "number" || !isFinite(value)) { drawing = false; continue }
        var x = (capacity - values.length + i) / (capacity - 1) * width
        var y = height - theme.hairline - Math.max(0, Math.min(maximum, value)) / maximum * (height - theme.hairline * 2)
        if (drawing) ctx.lineTo(x, y)
        else ctx.moveTo(x, y)
        drawing = true
      }
      ctx.stroke()
    }
  }
}
