import QtQuick
import QtTest

// A card that reports the pointer has to ask the card, not the pointer area
// covering it. Qt delivers a hover event to one item: a child that accepts it
// takes it away from the parent underneath, so a card whose fill reads an
// inner `MouseArea.containsMouse` goes cold the moment the pointer reaches one
// of the controls the card carries, and lights again when it leaves. That is
// the flicker a notification card, a Bluetooth row and a time zone row each
// had. `HoverHandler` is not delivered that way and stays hovered for the
// whole card, which is what those three surfaces now read.
TestCase {
  name: "CardHover"
  when: windowShown
  width: 300; height: 120
  visible: true

  // A card carrying a control on top of its own pointer area, as the Control
  // Center's audio card carries its two level tracks, a notification entry
  // carries dismiss, and a Bluetooth row carries Auto. The control does not
  // have to be written inline: the audio card's came from a `ControlLevel`
  // instantiation, which is how this shape hid the longest.
  Rectangle {
    id: card

    readonly property bool hovered: cardHover.hovered

    width: 300; height: 50

    HoverHandler { id: cardHover }

    MouseArea { id: cardMouse; anchors.fill: parent; hoverEnabled: true }

    // Sized like a level track rather than a button: nearly the whole card, so
    // the card would be cold almost everywhere a pointer could land on it.
    Rectangle {
      width: 280; height: 30; x: 10; y: 10
      MouseArea { id: actionMouse; anchors.fill: parent; hoverEnabled: true }
    }
  }

  // A row whose trailing button was never inside its pointer area at all, as a
  // time zone row's Pin button was not.
  Rectangle {
    id: row

    readonly property bool hovered: rowHover.hovered

    y: 60; width: 300; height: 40

    HoverHandler { id: rowHover }

    Rectangle { width: 40; height: 40; x: 250 }
  }

  function test_card_keeps_the_pointer_over_its_own_button() {
    mouseMove(card, 150, 45)
    verify(card.hovered, "the card reports a pointer on the card")
    verify(cardMouse.containsMouse, "and so does the area under it, so far")

    mouseMove(card, 150, 20)
    verify(actionMouse.containsMouse, "the pointer is on the button")
    compare(cardMouse.containsMouse, false,
      "the button has taken the hover from the area below it")
    verify(card.hovered, "the card is still hovered, because it is")
  }

  function test_row_keeps_the_pointer_over_a_button_outside_its_area() {
    mouseMove(row, 20, 20)
    verify(row.hovered, "the row reports a pointer on the row")
    mouseMove(row, 270, 20)
    verify(row.hovered, "and keeps reporting it over the trailing button")
  }

  function test_hover_still_ends_at_the_edge() {
    mouseMove(card, 150, 45)
    verify(card.hovered)
    mouseMove(row, 20, 20)
    verify(!card.hovered, "the card lets go once the pointer is off it")
  }
}
