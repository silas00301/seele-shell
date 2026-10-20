import QtQuick

Item {
  id: icon

  required property color tint
  property string kind: "headphones"
  // theme-contract: artwork -- the headphone and AirPods silhouettes, drawn on their own 16-unit grid and scaled to the size the caller gives
  implicitWidth: 16
  implicitHeight: 16

  Item {
    anchors.centerIn: parent
    width: 16
    height: 16
    scale: Math.min(icon.width, icon.height) / 16

    Item {
      anchors.fill: parent
      visible: icon.kind === "airpods"
      Repeater {
        model: 2
        Item {
          required property int index
          x: index === 0 ? 1 : 10
          y: 2
          width: 5
          height: 12
          Rectangle { width: 5; height: 5; radius: 2.5; color: icon.tint }
          Rectangle { x: index === 0 ? 2.5 : 0.5; y: 3; width: 2; height: 9; radius: 1; color: icon.tint }
        }
      }
    }

    Item {
      anchors.fill: parent
      visible: icon.kind !== "airpods"
      Item {
        x: 1; y: 1; width: 14; height: 9; clip: true
        Rectangle { width: 14; height: 14; radius: 7; color: "transparent"; border.width: 2; border.color: icon.tint }
      }
      Rectangle { x: 1; y: 8; width: 4; height: 7; radius: 1.5; color: icon.tint }
      Rectangle { x: 11; y: 8; width: 4; height: 7; radius: 1.5; color: icon.tint }
    }
  }
  // theme-contract: end
}
