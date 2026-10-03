import QtQuick

// The Done green, as light on the bezel rather than a frame. Opacity is the
// only thing that moves, and it moves on the cue's strength, so the layer
// surface itself is not animated.
Item {
  id: rim

  required property var theme
  required property real strength

  opacity: strength
  readonly property int depth: Math.max(1, Math.round(Math.min(width, height) * theme.edgeCueReach))
  readonly property color edge: theme.alpha(theme.green, theme.edgeCueAlpha)
  readonly property color mid: theme.alpha(theme.green, theme.edgeCueAlpha * 0.42)
  readonly property color fade: theme.alpha(theme.green, 0)

  Rectangle {
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.top: parent.top
    height: rim.depth
    gradient: Gradient {
      GradientStop { position: 0.0; color: rim.edge }
      GradientStop { position: 0.45; color: rim.mid }
      GradientStop { position: 1.0; color: rim.fade }
    }
  }

  Rectangle {
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    height: rim.depth
    gradient: Gradient {
      GradientStop { position: 0.0; color: rim.fade }
      GradientStop { position: 0.55; color: rim.mid }
      GradientStop { position: 1.0; color: rim.edge }
    }
  }

  Rectangle {
    anchors.left: parent.left
    anchors.top: parent.top
    anchors.bottom: parent.bottom
    width: rim.depth
    gradient: Gradient {
      orientation: Gradient.Horizontal
      GradientStop { position: 0.0; color: rim.edge }
      GradientStop { position: 0.45; color: rim.mid }
      GradientStop { position: 1.0; color: rim.fade }
    }
  }

  Rectangle {
    anchors.right: parent.right
    anchors.top: parent.top
    anchors.bottom: parent.bottom
    width: rim.depth
    gradient: Gradient {
      orientation: Gradient.Horizontal
      GradientStop { position: 0.0; color: rim.fade }
      GradientStop { position: 0.55; color: rim.mid }
      GradientStop { position: 1.0; color: rim.edge }
    }
  }
}
