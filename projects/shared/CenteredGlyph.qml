import QtQuick

// QML centres text by its advance width, but icon glyphs can draw outside that
// width. Offset the line box by the ink bounds so the visible mark, rather than
// its cursor position, lands in the middle of the item.
Item {
  id: centeredGlyph

  property alias text: glyphText.text
  property alias color: glyphText.color
  property alias font: glyphText.font
  property alias contentItem: glyphText

  readonly property rect inkBounds: glyphMetrics.tightBoundingRect(glyphText.text)
  readonly property real horizontalCorrection: glyphText.text === "" ? 0 : inkBounds.x + inkBounds.width / 2 - glyphMetrics.advanceWidth(glyphText.text) / 2
  readonly property real verticalCorrection: glyphText.text === "" ? 0 : glyphMetrics.ascent + inkBounds.y + inkBounds.height / 2 - glyphMetrics.height / 2

  FontMetrics {
    id: glyphMetrics
    font: glyphText.font
  }

  Text {
    id: glyphText

    x: -centeredGlyph.horizontalCorrection
    y: -centeredGlyph.verticalCorrection
    width: parent.width
    height: parent.height
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
  }
}
