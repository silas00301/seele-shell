pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

FocusScope {
  id: panel
  required property var theme
  required property var store
  property bool popupHovered: false
  // The host owns keyboard focus, so it says when the current row is the one
  // the keys act on. A panel used on its own answers for itself.
  property bool keyboardActive: panel.activeFocus
  property real maximumHeight: theme.githubInboxMaximumHeight
  readonly property var snapshot: store.snapshot
  readonly property var selected: snapshot.detail
  readonly property bool detailOpen: !!selected
  readonly property string selectedId: detailOpen ? snapshot.selected || "" : ""
  readonly property bool failed: !!(store.connectionError || snapshot.error)
  readonly property string subjectUrl: detailOpen ? (selected.thread && selected.thread.url) || (selected.detail && selected.detail.url) || "" : ""
  // The host draws one hint line under the whole panel; this is what it says
  // while the inbox is being read.
  readonly property string hint: detailOpen
    ? "Enter or Escape folds · O opens · D marks Done · R retries triage"
    : "Ctrl+Tab switches · J / K moves · Enter unfolds · R refreshes"
  implicitHeight: content.implicitHeight

  // A priority is named in a word short enough for a chip and graded in colour
  // from quiet to urgent. Until the analysis exists the chip says where it is.
  function chip(entry) {
    if (entry.pendingDone) return { text: "Marking Done…", tint: theme.overlay }
    var priority = entry.triage ? entry.triage.priority : ""
    if (priority === "Immediate Action required") return { text: "Act now", tint: theme.red }
    if (priority === "Action required soon") return { text: "Act soon", tint: theme.yellow }
    if (priority === "Action required sometime") return { text: "Sometime", tint: theme.accent }
    if (priority === "Informational") return { text: "FYI", tint: theme.overlay }
    if (entry.state === "failed") return { text: "Triage failed", tint: theme.overlay }
    return { text: "Triaging…", tint: theme.overlay }
  }
  // One thread is open at a time, and it opens where it already sits.
  function toggle(id) { store.send(selectedId === id ? "back" : "select", id) }
  function reveal() {
    for (var index = 0; index < store.model.count; index++) {
      if (store.model.get(index).entry.id !== selectedId) continue
      inbox.currentIndex = index
      inbox.positionViewAtIndex(index, ListView.Beginning)
      return
    }
  }
  onSelectedIdChanged: if (selectedId !== "") Qt.callLater(reveal)

  function handleKey(event) {
    if (event.modifiers & Qt.ControlModifier) return
    if (event.key === Qt.Key_Escape && detailOpen) { store.send("back"); event.accepted = true }
    else if (event.key === Qt.Key_R) { store.send(detailOpen ? "retry" : "refresh", snapshot.selected); event.accepted = true }
    else if (event.key === Qt.Key_O && detailOpen) { store.send("open", snapshot.selected); event.accepted = true }
    else if (event.key === Qt.Key_D && detailOpen) { store.send("done", snapshot.selected); event.accepted = true }
    else if (event.key === Qt.Key_J || event.key === Qt.Key_Down) { inbox.incrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_K || event.key === Qt.Key_Up) { inbox.decrementCurrentIndex(); event.accepted = true }
    else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && inbox.currentIndex >= 0 && inbox.currentIndex < store.model.count) { toggle(store.model.get(inbox.currentIndex).entry.id); event.accepted = true }
  }
  Keys.onPressed: event => handleKey(event)

  component Caption: Text {
    width: parent ? parent.width : 0
    textFormat: Text.PlainText
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    wrapMode: Text.Wrap
  }
  component Body: TextEdit {
    width: parent ? parent.width : 0
    textFormat: TextEdit.PlainText
    readOnly: true
    selectByMouse: true
    color: panel.theme.text
    selectionColor: panel.theme.selectedColor
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    wrapMode: TextEdit.Wrap
  }

  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.StatusBanner {
      theme: panel.theme
      width: parent.width
      visible: panel.failed
      glyph: "󰀦"
      tint: panel.theme.yellow
      title: "GitHub inbox needs attention"
      detail: panel.store.connectionError || panel.snapshot.error
      Shared.ActionButton { theme: panel.theme; text: "Retry"; onClicked: panel.store.send("refresh") }
    }
    Caption {
      visible: !!panel.snapshot.notice
      text: panel.snapshot.notice
    }
    // What the inbox holds rides on its rule, as the pull request tabs carry
    // theirs. The rule stays one line of text; the hand-off to GitHub's own
    // inbox is a panel action and sits in the header beside refresh.
    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "INBOX"
      detail: panel.snapshot.count + " unread" + (panel.snapshot.complete ? "" : " so far · still loading")
    }
    Shared.EmptyState {
      theme: panel.theme
      width: parent.width
      visible: panel.store.model.count === 0
      glyph: panel.snapshot.refreshing ? "󰂚" : panel.snapshot.complete ? "󰄬" : "󰀦"
      title: panel.snapshot.refreshing ? "Loading notifications…" : panel.snapshot.complete ? "You're all caught up" : "Inbox unavailable"
      detail: panel.snapshot.refreshing ? "" : panel.snapshot.complete ? "Threads leave once they are read or marked Done on GitHub." : "Refresh to load them again."
      Shared.ActionButton {
        theme: panel.theme
        visible: !panel.snapshot.refreshing && !panel.snapshot.complete && !panel.failed
        text: "Refresh"
        onClicked: panel.store.send("refresh")
      }
    }
    Shared.SeeleListView {
      id: inbox
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
        readonly property bool open: panel.selectedId !== "" && panel.selectedId === row.entry.id
        readonly property var view: row.open ? panel.selected : null
        readonly property bool current: panel.keyboardActive && inbox.currentIndex === row.index
        readonly property var status: panel.chip(row.entry)
        width: inbox.width - panel.theme.scrollGutter
        height: rowHead.height + rowBody.height
        radius: panel.theme.radius
        color: rowMouse.pressed ? panel.theme.pressColor : row.current ? panel.theme.selectedColor : panel.theme.cardColor
        antialiasing: true
        clip: true
        Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
        Shared.CardEdge { theme: panel.theme }

        Item {
          id: rowHead
          width: parent.width
          height: rowHeadText.implicitHeight + panel.theme.cardPadding * 2
          HoverHandler { id: rowHover }
          Shared.HoverWash { theme: panel.theme; hovered: rowHover.hovered }
          Column {
            id: rowHeadText
            anchors { left: parent.left; right: rowChip.left; top: parent.top; leftMargin: panel.theme.cardPadding; rightMargin: panel.theme.spaceMedium; topMargin: panel.theme.cardPadding }
            spacing: panel.theme.spaceTight
            Text {
              width: parent.width; text: row.entry.title; textFormat: Text.PlainText
              color: panel.theme.text; font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textBody; font.weight: panel.theme.weightStrong
              wrapMode: Text.Wrap; maximumLineCount: row.open ? 6 : 2; elide: Text.ElideRight
            }
            Caption {
              text: row.entry.repository + " · " + row.entry.kind + " · " + row.entry.reason
              wrapMode: Text.NoWrap
              elide: Text.ElideRight
            }
            Caption {
              visible: !!row.entry.triage
              text: row.entry.triage ? row.entry.triage.summary : ""
              maximumLineCount: row.open ? 12 : 2
              elide: Text.ElideRight
            }
          }
          Shared.StatusChip {
            id: rowChip
            theme: panel.theme
            anchors { right: rowChevron.left; rightMargin: panel.theme.spaceSmall; top: parent.top; topMargin: panel.theme.cardPadding }
            text: row.status.text
            tint: row.status.tint
          }
          Text {
            id: rowChevron
            anchors { right: parent.right; rightMargin: panel.theme.cardPadding; verticalCenter: rowChip.verticalCenter }
            text: row.open ? "󰅃" : "󰅀"
            color: panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textBody
          }
          MouseArea {
            id: rowMouse
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              inbox.currentIndex = row.index
              panel.toggle(row.entry.id)
            }
          }
        }

        // The open thread grows the row it belongs to rather than replacing
        // the list, so the inbox stays where the reader left it.
        Item {
          id: rowBody
          y: rowHead.height
          width: parent.width
          height: row.open ? rowDetail.implicitHeight + panel.theme.cardPadding : 0
          visible: height > 0
          clip: true
          Behavior on height { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }
          Column {
            id: rowDetail
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: panel.theme.cardPadding; rightMargin: panel.theme.cardPadding }
            spacing: panel.theme.spaceMedium
            Flow {
              width: parent.width
              spacing: panel.theme.spaceSmall
              Shared.ActionButton {
                theme: panel.theme
                text: row.view && row.view.pendingDone ? "Marking Done…" : "Mark Done"
                enabled: !!row.view && !row.view.pendingDone && !panel.store.connectionError
                onClicked: panel.store.send("done", row.entry.id)
              }
              Shared.ActionButton {
                theme: panel.theme
                text: panel.subjectUrl !== "" ? "Open on GitHub" : "GitHub inbox"
                onClicked: panel.store.send(panel.subjectUrl !== "" ? "open" : "inbox", row.entry.id)
              }
              Shared.ActionButton {
                theme: panel.theme
                visible: !!row.view && (row.view.state === "failed" || row.view.state === "ready")
                text: row.view && row.view.state === "failed" ? "Retry triage" : "Recheck"
                onClicked: panel.store.send("retry", row.entry.id)
              }
            }
            Caption {
              visible: !!row.view && row.view.state !== "ready"
              text: !row.view ? "" : row.view.state === "failed"
                ? (row.view.error || "Triage failed.") + " The original thread is below."
                : row.view.triage ? "Rechecking the analysis…" : "Triage is running. The original thread is below."
              color: row.view && row.view.state === "failed" ? panel.theme.yellow : panel.theme.subtext
            }
            Rectangle {
              width: parent.width
              visible: !!row.view && (!!row.view.triage || !!row.view.previous)
              height: visible ? analysis.implicitHeight + panel.theme.spaceMedium * 2 : 0
              radius: panel.theme.radiusSmall
              color: panel.theme.rowColor
              Column {
                id: analysis
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.spaceMedium }
                spacing: panel.theme.spaceMedium
                Repeater {
                  model: {
                    if (!row.view) return []
                    var sections = []
                    var triage = row.view.triage
                    if (triage) sections.push(
                      { label: "NEEDS YOUR ATTENTION", body: triage.attention },
                      { label: "SUGGESTED NEXT ACTION", body: triage.nextAction },
                      { label: "WHAT CHANGED", body: triage.changes },
                      { label: "WHY YOU WERE NOTIFIED", body: triage.reason })
                    var previous = row.view.previous
                    if (previous) sections.push({ label: "EARLIER ANALYSIS · " + panel.chip({ triage: previous }).text.toUpperCase(), body: previous.summary })
                    return sections
                  }
                  Column {
                    id: section
                    required property var modelData
                    width: analysis.width
                    spacing: panel.theme.spaceTight
                    Shared.SectionLabel { theme: panel.theme; text: section.modelData.label }
                    Body { text: section.modelData.body }
                  }
                }
              }
            }
            Shared.SectionLabel { theme: panel.theme; text: "THREAD" }
            Repeater {
              model: row.open ? panel.store.detailModel : null
              Rectangle {
                id: detailBlock
                required property var block
                width: rowDetail.width
                height: blockText.implicitHeight + panel.theme.spaceMedium * 2
                radius: panel.theme.radiusSmall
                color: panel.theme.rowColor
                Column {
                  id: blockText
                  anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.spaceMedium }
                  spacing: panel.theme.spaceTight
                  Text {
                    width: parent.width
                    text: detailBlock.block.label
                    textFormat: Text.PlainText
                    color: panel.theme.text
                    font.family: panel.theme.fontFamily
                    font.pixelSize: panel.theme.textCaption
                    font.weight: panel.theme.weightStrong
                  }
                  Caption { visible: text !== ""; text: detailBlock.block.meta || "" }
                  Body { visible: text !== ""; text: detailBlock.block.body || "" }
                }
              }
            }
            Text {
              width: parent.width
              text: "Saving and restoring Done stay in GitHub's web inbox."
              textFormat: Text.PlainText
              color: panel.theme.overlay
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
              wrapMode: Text.Wrap
            }
          }
        }
      }
    }
  }
}
