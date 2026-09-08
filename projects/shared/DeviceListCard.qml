import QtQuick

// A list of choices sits in a card the way every other group on a panel does,
// so the rows inside it can take the lighter tint the elevation ramp gives
// them instead of floating on the panel's own material.
Rectangle {
  id: deviceListCard

  required property var theme
  property real listHeight: 0

  // Stated as an implicit height so the card sizes itself both under a Column,
  // where an item falls back to it, and inside a Layout, which reads it as the
  // preferred height rather than overwriting a height the card set itself.
  implicitHeight: deviceListCard.listHeight + deviceListCard.theme.cardPadding * 2
  radius: deviceListCard.theme.radius
  color: deviceListCard.theme.cardColor
  antialiasing: true

  CardEdge { theme: deviceListCard.theme }
}
