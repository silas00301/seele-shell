pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// Local TCP listeners, what owns them, and the two deliberate things that can
// be done about one: reach it, or stop it. Every row is the worker's own
// description; the panel decides nothing about ownership or privilege and
// contacts no listening service by drawing one.
FocusScope {
  id: panel
  required property var theme
  required property var store
  property bool popupHovered: false
  property real maximumHeight: theme.portsMaximumHeight
  readonly property string hint: "J / K moves · Enter unfolds · R refreshes · Escape closes"
  implicitHeight: content.implicitHeight

  // What a binding reaches, said in words rather than in an address.
  function reach(entry) {
    return entry.scopeLabel + " · " + entry.binding
  }
  function toggle(id) {
    store.expand(id)
    if (store.expanded === id) reveal(id)
  }
  function reveal(id) {
    for (var index = 0; index < store.model.count; index++) {
      if (store.model.get(index).entry.id !== id) continue
      listeners.currentIndex = index
      listeners.positionViewAtIndex(index, ListView.Contain)
      return
    }
  }
  function handleKey(event) {
    if (event.modifiers & Qt.ControlModifier) return
    if (event.key === Qt.Key_J || event.key === Qt.Key_Down) { listeners.incrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_K || event.key === Qt.Key_Up) { listeners.decrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_R) { panel.store.refresh(); event.accepted = true }
    else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && listeners.currentIndex >= 0 && listeners.currentIndex < store.model.count) {
      panel.toggle(store.model.get(listeners.currentIndex).entry.id)
      event.accepted = true
    }
  }

  component Caption: Text {
    width: parent ? parent.width : 0
    textFormat: Text.PlainText
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    wrapMode: Text.Wrap
  }
  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing

    Shared.StatusBanner {
      theme: panel.theme
      width: parent.width
      visible: panel.store.error !== ""
      glyph: "󰀦"
      tint: panel.theme.yellow
      title: "The port inspector is unavailable"
      detail: panel.store.error
    }
    Shared.SearchField {
      id: search
      theme: panel.theme
      width: parent.width
      focus: true
      placeholderText: "Port, address, process, service or project"
      text: panel.store.query
      onTextEdited: panel.store.query = text
      Keys.onDownPressed: event => { listeners.incrementCurrentIndex(); event.accepted = true }
      Keys.onUpPressed: event => { listeners.decrementCurrentIndex(); event.accepted = true }
      Keys.onReturnPressed: event => {
        if (listeners.currentIndex >= 0 && listeners.currentIndex < panel.store.model.count)
          panel.toggle(panel.store.model.get(listeners.currentIndex).entry.id)
        event.accepted = true
      }
    }
    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "LISTENING"
      detail: panel.store.model.count === panel.store.total
        ? panel.store.total + " TCP"
        : panel.store.model.count + " of " + panel.store.total + " TCP"
      Shared.ActionButton {
        theme: panel.theme
        text: "Refresh"
        enabled: !panel.store.busy
        onClicked: panel.store.refresh()
      }
    }
    // Listeners owned by another user stay in the list; what is missing is
    // said once, here, rather than guessed at in each row.
    Caption {
      visible: panel.store.limited
      text: "Some listeners belong to other users. Their processes are not readable without authentication."
    }
    Text {
      width: parent.width
      visible: panel.store.actionError !== ""
      text: panel.store.actionError
      textFormat: Text.PlainText
      wrapMode: Text.Wrap
      color: panel.theme.red
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textBody
    }
    Caption {
      visible: panel.store.copied !== ""
      text: "Copied " + panel.store.copied
    }
    Shared.EmptyState {
      theme: panel.theme
      width: parent.width
      visible: panel.store.model.count === 0
      glyph: panel.store.query !== "" ? "󰍉" : "󰛳"
      title: panel.store.query !== "" ? "No matching listener" : "Nothing is listening"
      detail: panel.store.query !== ""
        ? "Search a port, an address such as localhost:3000, a process, a service or a project."
        : "No local TCP port is open in this network namespace."
    }
    Shared.SeeleListView {
      id: listeners
      theme: panel.theme
      width: parent.width
      visible: panel.store.model.count > 0
      height: visible ? Math.min(contentHeight, panel.maximumHeight) : 0
      clip: true
      spacing: panel.theme.spaceSmall
      model: panel.store.model
      currentIndex: count ? 0 : -1
      ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
      delegate: Rectangle {
        id: row
        required property var entry
        required property int index
        readonly property bool open: panel.store.expanded === row.entry.id
        readonly property var target: row.entry.target || ({})
        readonly property var owners: row.entry.owners || []
        readonly property bool current: listeners.currentIndex === row.index
        readonly property string address: panel.store.url(row.entry)
        // A confirmation belongs to the row it was raised on and to no other.
        readonly property var review: panel.store.plan && panel.store.plan.id === row.entry.id ? panel.store.plan : null
        readonly property var outcome: panel.store.outcome && panel.store.outcome.token === row.entry.token ? panel.store.outcome : null
        width: listeners.width - panel.theme.scrollGutter
        height: head.height + body.height
        radius: panel.theme.radius
        color: headMouse.pressed ? panel.theme.pressColor : row.current ? panel.theme.selectedColor : panel.theme.cardColor
        antialiasing: true
        clip: true
        Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
        Shared.CardEdge { theme: panel.theme }

        Item {
          id: head
          width: parent.width
          height: headText.implicitHeight + panel.theme.cardPadding * 2
          HoverHandler { id: headHover }
          Shared.HoverWash { theme: panel.theme; hovered: headHover.hovered }
          Column {
            id: headText
            anchors { left: parent.left; right: chip.left; top: parent.top; leftMargin: panel.theme.cardPadding; rightMargin: panel.theme.spaceMedium; topMargin: panel.theme.cardPadding }
            spacing: panel.theme.spaceTight
            Text {
              width: parent.width
              text: row.entry.binding
              textFormat: Text.PlainText
              color: panel.theme.text
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textBody
              font.weight: panel.theme.weightStrong
              elide: Text.ElideRight
            }
            Caption {
              text: panel.store.summary(row.entry)
              wrapMode: Text.NoWrap
              elide: Text.ElideRight
            }
            Caption {
              visible: row.owners.length > 0 && !!row.owners[0].project
              text: row.owners.length ? row.owners[0].project + " · " + row.owners[0].projectPath : ""
              wrapMode: Text.NoWrap
              elide: Text.ElideRight
            }
          }
          Shared.StatusChip {
            id: chip
            theme: panel.theme
            anchors { right: chevron.left; rightMargin: panel.theme.spaceSmall; top: parent.top; topMargin: panel.theme.cardPadding }
            text: row.entry.scopeLabel
            tint: row.entry.scope === "wildcard" ? panel.theme.yellow : panel.theme.overlay
          }
          Text {
            id: chevron
            anchors { right: parent.right; rightMargin: panel.theme.cardPadding; verticalCenter: chip.verticalCenter }
            text: row.open ? "󰅃" : "󰅀"
            color: panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textBody
          }
          MouseArea {
            id: headMouse
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              listeners.currentIndex = row.index
              panel.toggle(row.entry.id)
            }
          }
        }

        // The row grows where it sits, so the list never moves under the
        // reader while a confirmation is open on it.
        Item {
          id: body
          y: head.height
          width: parent.width
          height: row.open ? detail.implicitHeight + panel.theme.cardPadding : 0
          visible: height > 0
          clip: true
          Behavior on height { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }
          Column {
            id: detail
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: panel.theme.cardPadding; rightMargin: panel.theme.cardPadding }
            spacing: panel.theme.spaceMedium

            // A port is not proof of a protocol, so the address is shown as a
            // proposal with its scheme in the open and changeable.
            Row {
              width: parent.width
              spacing: panel.theme.spaceSmall
              Shared.SegmentWell {
                theme: panel.theme
                width: panel.theme.controlHeight * 3
                Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; text: "http"; selected: panel.store.scheme(row.entry.id) === "http"; onClicked: panel.store.setScheme(row.entry.id, "http") }
                Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; text: "https"; selected: panel.store.scheme(row.entry.id) === "https"; onClicked: panel.store.setScheme(row.entry.id, "https") }
              }
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: row.address !== "" ? row.address : "No address can be proposed for this binding."
                textFormat: Text.PlainText
                color: row.address !== "" ? panel.theme.text : panel.theme.subtext
                font.family: panel.theme.fontFamily
                font.pixelSize: panel.theme.textBody
                elide: Text.ElideRight
                width: parent.width - panel.theme.controlHeight * 3 - panel.theme.spaceSmall
              }
            }
            Caption {
              visible: row.entry.scope === "wildcard"
              text: "Bound to every interface. " + row.entry.destination + " is this machine's own way in; the binding itself is unchanged."
            }
            Flow {
              width: parent.width
              spacing: panel.theme.spaceSmall
              Shared.ActionButton {
                theme: panel.theme
                text: "Copy address"
                enabled: row.address !== ""
                onClicked: panel.store.copy(row.entry)
              }
              Shared.ActionButton {
                theme: panel.theme
                text: "Open in browser"
                enabled: row.address !== ""
                onClicked: panel.store.openUrl(row.entry)
              }
              Shared.ActionButton {
                theme: panel.theme
                visible: !!row.entry.identifiable
                text: "Identify owner"
                enabled: !panel.store.busy
                onClicked: panel.store.identify(row.entry.id)
              }
              Shared.ActionButton {
                theme: panel.theme
                danger: true
                visible: row.target.kind === "service" || row.target.kind === "process"
                text: row.target.privileged ? "Stop…  (authenticates)" : "Stop…"
                enabled: !panel.store.busy && !row.review
                onClicked: panel.store.review(row.entry.id, "graceful")
              }
            }
            // Why Stop is not offered is said in the row, never by leaving an
            // action there that would refuse.
            Caption {
              visible: !!row.target.reason
              text: row.target.reason
            }
            // More than one process holds this socket: stopping one of them is
            // a choice the reader has to make before it can be confirmed.
            Column {
              width: parent.width
              visible: row.owners.length > 1
              spacing: panel.theme.spaceSmall
              Shared.SectionRule { theme: panel.theme; width: parent.width; label: "OWNERS"; detail: row.owners.length + " processes" }
              Repeater {
                model: row.owners
                Shared.ActionButton {
                  required property var modelData
                  theme: panel.theme
                  width: parent.width
                  selected: row.entry.selected === modelData.pid
                  text: modelData.name + " (" + modelData.pid + ")" + (modelData.unit ? " · " + modelData.unit : "")
                  enabled: !panel.store.busy
                  onClicked: panel.store.select(row.entry.id, modelData.pid)
                }
              }
            }

            // The confirmation. It names the exact target, discloses what else
            // stops with it and how it can come back, and sends nothing until
            // Confirm.
            Shared.StatusBanner {
              width: parent.width
              theme: panel.theme
              visible: !!row.review
              glyph: "󰀦"
              tint: panel.theme.yellow
              title: row.review
                ? (row.review.mode === "force" ? "Force stop " : "Stop ")
                  + (row.review.target.kind === "service" ? row.review.target.unit : row.review.target.name + " (" + row.review.target.pid + ")")
                  + " on " + row.review.binding
                : ""
              detail: row.review ? (row.review.disclosure || []).join("\n") : ""
              Shared.ActionButton {
                theme: panel.theme
                danger: true
                text: row.review && row.review.mode === "force" ? "Force stop" : "Stop"
                onClicked: panel.store.confirm()
              }
              Shared.ActionButton {
                theme: panel.theme
                text: "Cancel"
                onClicked: panel.store.dismiss()
              }
            }

            // What actually happened. A listener that is still there is never
            // reported as removed, and Force appears only here, after a
            // graceful attempt, as its own confirmed decision.
            Shared.StatusBanner {
              width: parent.width
              theme: panel.theme
              visible: !!row.outcome
              glyph: row.outcome && row.outcome.ok ? "󰄬" : "󰀦"
              tint: row.outcome && row.outcome.ok ? panel.theme.green : panel.theme.red
              title: row.outcome
                ? row.outcome.ok
                  ? "Stopped · the port is free"
                  : row.outcome.remaining && !row.outcome.error
                    ? "Still listening"
                    : "Nothing was stopped"
                : ""
              detail: row.outcome
                ? row.outcome.ok
                  ? ""
                  : row.outcome.error
                    ? panel.store.failure(row.outcome.error)
                    : "The graceful stop was accepted but " + row.entry.binding + " is still bound."
                : ""
              Shared.ActionButton {
                theme: panel.theme
                danger: true
                visible: !!(row.outcome && row.outcome.canForce)
                text: "Force stop…"
                enabled: !panel.store.busy && !row.review
                onClicked: panel.store.review(row.entry.id, "force")
              }
            }
          }
        }
      }
    }
  }
  Keys.onPressed: event => panel.handleKey(event)
}
