pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared
import "../shared/ListModels.js" as Models
import "../shared/Native.js" as Bridge

Column {
  id: panel
  required property var theme
  required property var store
  property real maximumHeight: theme.homeAssistantMaximumHeight
  property string page: "home"
  property string expanded: ""
  property string editingId: ""
  property bool selectedOnly: true
  readonly property bool setupShown: page === "setup" || !store.configured
  readonly property real bodyHeight: Math.max(theme.rowHeight, maximumHeight - header.height - spacing - (banner.visible ? banner.height + spacing : 0))
  spacing: theme.panelSpacing
  signal closeRequested()

  Keys.onEscapePressed: event => {
    tokenField.clear()
    if (editingId !== "") editingId = ""
    else if (page === "home" && expanded !== "") expanded = ""
    else if (page !== "home" && store.configured) page = "home"
    else closeRequested()
    event.accepted = true
  }
  function opened() {
    tokenField.clear()
    if (!visible) return
    store.discover(page === "devices" && !setupShown)
    if (setupShown) {
      serverField.text = store.url
      serverField.forceActiveFocus()
      serverField.selectAll()
    } else if (page === "devices") {
      search.forceActiveFocus()
      search.selectAll()
    } else forceActiveFocus()
  }
  function closed() {
    tokenField.clear()
    editingId = ""
    store.discover(false)
  }
  onPageChanged: Qt.callLater(opened)
  onSelectedOnlyChanged: syncDevices()
  Connections {
    target: panel.store
    function onSetupComplete() {
      panel.selectedOnly = false
      panel.page = panel.visible ? "devices" : "home"
    }
    function onEntitiesChanged() { panel.syncHome(); panel.syncDevices() }
    function onCatalogChanged() { panel.syncDevices() }
  }
  ListModel { id: homeModel; dynamicRoles: true }
  ListModel { id: deviceModel; dynamicRoles: true }
  function reconcile(model, items) {
    Models.reconcile(model, items, "payload", function(item) { return item.entity_id || item.key }, "key")
  }
  // Focus and scroll position belong to Qt geometry, not the data projection.
  function reveal(item, viewport) {
    var top = item.mapToItem(viewport.contentItem, 0, 0).y
    var target = viewport.contentY
    if (top < target) target = top
    else if (top + item.height > target + viewport.height) target = top + item.height - viewport.height
    viewport.contentY = Math.max(0, Math.min(target, viewport.contentHeight - viewport.height))
  }
  function syncHome() {
    reconcile(homeModel, store.groups())
  }
  function syncDevices() {
    reconcile(deviceModel, Bridge.call("home_assistant.devices", [store.entities, store.catalog, search.text, selectedOnly]))
  }
  Component.onCompleted: { syncHome(); syncDevices() }

  component Action: Shared.ActionButton { theme: panel.theme }
  component GlyphAction: Shared.GlyphButton { theme: panel.theme }
  component Label: Text {
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
  }
  component Field: TextField {
    id: field
    implicitHeight: panel.theme.controlHeight
    color: panel.theme.text
    placeholderTextColor: panel.theme.overlay
    selectionColor: panel.theme.selectedColor
    selectedTextColor: panel.theme.text
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    leftPadding: panel.theme.cardPadding
    rightPadding: panel.theme.cardPadding
    verticalAlignment: TextInput.AlignVCenter
    selectByMouse: true
    // The same well SearchField is cut into, so every input in the shell
    // reports focus with one ring.
    background: Rectangle {
      color: panel.theme.wellColor
      radius: panel.theme.radius
      border.width: 1
      border.color: field.activeFocus ? panel.theme.accent : panel.theme.cardBorder
      antialiasing: true
      Behavior on border.color { ColorAnimation { duration: panel.theme.durationFast } }
    }
  }
  component Mark: Shared.CenteredGlyph {
    width: panel.theme.controlHeight
    height: width
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textIcon
  }
  component DeviceSlider: Shared.DeviceSlider {
    id: deviceLevel
    theme: panel.theme
    onActiveFocusChanged: if (activeFocus) panel.reveal(deviceLevel, homeViewport)
  }

  Shared.PanelHeader {
    id: header
    theme: panel.theme
    width: parent.width
    glyph: "󰋜"
    title: "Home Assistant"
    detail: panel.setupShown ? "Connection" : panel.page === "devices" ? "Devices · " + panel.store.preferences.length + " of 32 selected" : panel.store.connected ? "Connected · " + panel.store.entities.length + " devices" : "Offline · last known values"
    detailColor: panel.store.connected || panel.setupShown ? panel.theme.subtext : panel.theme.yellow
    Row {
      spacing: panel.theme.spaceTight
      Action {
        objectName: "homeNavigation"
        visible: panel.store.configured
        text: panel.page === "home" ? "Devices" : "Back"
        onClicked: panel.page = panel.page === "home" ? "devices" : "home"
      }
      GlyphAction {
        objectName: "connectionSettings"
        glyph: "󰢻"
        text: "Connection settings"
        visible: !panel.setupShown
        onClicked: panel.page = "setup"
      }
    }
  }
  Shared.StatusBanner {
    id: banner
    theme: panel.theme
    width: parent.width
    visible: panel.store.error !== "" || (!panel.setupShown && !panel.store.connected)
    glyph: "󰀦"
    tint: panel.theme.yellow
    title: panel.store.error !== "" ? "Home Assistant needs attention" : "Connection lost"
    detail: panel.store.error || "Showing the last known values. Controls return when your home reconnects."
    Action {
      text: "Retry"
      enabled: !panel.store.settingsPending
      onClicked: panel.store.refresh()
    }
  }

  Shared.SeeleFlickable {
    id: setupViewport
    theme: panel.theme
    visible: panel.setupShown
    width: parent.width
    height: Math.min(contentHeight, panel.bodyHeight)
    contentHeight: setupContent.implicitHeight
    clip: true
    Column {
      id: setupContent
      width: parent.width
      spacing: panel.theme.panelSpacing
      Shared.SectionRule { theme: panel.theme; width: parent.width; label: "CONNECT YOUR HOME" }
      Rectangle {
        width: parent.width
        height: setupFields.implicitHeight + panel.theme.cardPadding * 2
        color: panel.theme.cardColor
        radius: panel.theme.radius
        Shared.CardEdge { theme: panel.theme }
        Column {
          id: setupFields
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          spacing: panel.theme.spaceMedium
          Label { text: "Server address"; color: panel.theme.text; font.pixelSize: panel.theme.textBody }
          Field {
            id: serverField
            onActiveFocusChanged: if (activeFocus) panel.reveal(serverField, setupViewport)
            objectName: "serverAddress"
            Accessible.name: "Server address"
            width: parent.width
            placeholderText: "https://home.example"
            enabled: !panel.store.busy
            onAccepted: tokenField.forceActiveFocus()
          }
          Label { text: "Access token"; color: panel.theme.text; font.pixelSize: panel.theme.textBody }
          Field {
            id: tokenField
            onActiveFocusChanged: if (activeFocus) panel.reveal(tokenField, setupViewport)
            objectName: "accessToken"
            Accessible.name: "Long-lived access token"
            width: parent.width
            echoMode: TextInput.Password
            placeholderText: panel.store.configured ? "Paste a replacement token" : "Paste your long-lived access token"
            enabled: !panel.store.busy
            inputMethodHints: Qt.ImhHiddenText | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
            onAccepted: connectButton.clicked()
          }
          Label {
            width: parent.width
            text: "Create a long-lived access token in your Home Assistant profile. It is saved securely in your system keyring."
          }
          Action {
            id: connectButton
            onActiveFocusChanged: if (activeFocus) panel.reveal(connectButton, setupViewport)
            objectName: "connectHome"
            width: parent.width
            text: panel.store.settingsPending ? "Connecting…" : panel.store.configured ? "Update connection" : "Connect"
            selected: true
            enabled: !panel.store.busy && serverField.text.trim() !== "" && tokenField.text !== ""
            onClicked: {
              if (!enabled) return
              panel.store.setup(serverField.text, tokenField.text)
              tokenField.clear()
            }
          }
        }
      }
    }
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
  }

  Column {
    id: devicesPage
    visible: !panel.setupShown && panel.page === "devices"
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.SearchField {
      id: search
      objectName: "deviceSearch"
      theme: panel.theme
      width: parent.width
      placeholderText: "Search devices or rooms"
      Accessible.name: placeholderText
      onTextChanged: panel.syncDevices()
    }
    Shared.SegmentWell {
      id: deviceFilter
      theme: panel.theme
      width: parent.width
      Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; text: "Your devices"; selected: panel.selectedOnly; onClicked: panel.selectedOnly = true }
      Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; text: "Add devices"; selected: !panel.selectedOnly; onClicked: panel.selectedOnly = false }
    }
    Shared.DeviceListCard {
      id: deviceCard
      theme: panel.theme
      width: parent.width
      listHeight: deviceModel.count ? Math.min(deviceList.contentHeight, Math.max(panel.theme.rowHeight, panel.bodyHeight - search.height - deviceFilter.height - devicesPage.spacing * 2 - panel.theme.cardPadding * 2)) : pickerEmpty.implicitHeight + panel.theme.cardPadding * 2
      Shared.EmptyState {
        id: pickerEmpty
        theme: panel.theme
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
        visible: deviceModel.count === 0
        glyph: search.text ? "󰍉" : "󰋜"
        title: search.text ? "No matching devices" : panel.selectedOnly ? "Make this space yours" : "No devices available"
        detail: search.text ? "Try a device name or a room." : panel.selectedOnly ? "Add the lights, switches and readings you use most." : "Devices appear here when Home Assistant reports them."
        Action {
          text: search.text ? "Clear search" : panel.selectedOnly ? "Add devices" : "Refresh"
          onClicked: {
            if (search.text) search.clear()
            else if (panel.selectedOnly) panel.selectedOnly = false
            else panel.store.refresh()
          }
        }
      }
      Shared.SeeleListView {
        id: deviceList
        theme: panel.theme
        anchors.fill: parent
        anchors.margins: panel.theme.cardPadding
        visible: deviceModel.count > 0
        clip: true
        spacing: panel.theme.spaceTight
        model: deviceModel
        delegate: Rectangle {
          id: device
          required property var payload
          readonly property var preference: panel.store.preference(payload.entity_id)
          readonly property bool editing: panel.editingId === payload.entity_id && preference !== null
          objectName: "device:" + payload.entity_id
          width: ListView.view.width
          height: deviceHeader.height + editor.height
          radius: panel.theme.radiusSmall
          color: panel.theme.rowColor
          clip: true
          Shared.HoverWash { theme: panel.theme; hovered: deviceHover.hovered }
          HoverHandler { id: deviceHover }
          onEditingChanged: if (editing) {
            nameField.text = preference.name
            roomField.text = preference.room
          }
          RowLayout {
            id: deviceHeader
            width: parent.width
            height: panel.theme.detailRowHeight
            spacing: panel.theme.spaceSmall
            Mark { text: device.payload.glyph; color: device.preference ? panel.theme.accent : panel.theme.subtext }
            Column {
              Layout.fillWidth: true
              spacing: panel.theme.spaceTight
              Label { width: parent.width; text: device.payload.name; color: panel.theme.text; font.pixelSize: panel.theme.textBody; elide: Text.ElideRight; wrapMode: Text.NoWrap }
              Label { width: parent.width; text: (device.payload.room || "Other") + " · " + device.payload.state_label + (device.payload.available ? device.payload.unit || "" : ""); elide: Text.ElideRight; wrapMode: Text.NoWrap }
            }
            GlyphAction {
              glyph: device.preference && device.preference.favorite ? "󰓎" : "󰓏"
              text: device.preference && device.preference.favorite ? "Remove from favorites" : "Add to favorites"
              selected: !!device.preference && device.preference.favorite
              onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList)
              visible: device.preference !== null
              enabled: !panel.store.busy
              onClicked: panel.store.edit(device.payload.entity_id, "favorite", !selected)
            }
            GlyphAction {
              glyph: device.editing ? "󰅃" : "󰒓"
              text: device.editing ? "Close device settings" : "Name, room and menu bar"
              selected: device.editing
              onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList)
              visible: device.preference !== null
              onClicked: panel.editingId = device.editing ? "" : device.payload.entity_id
            }
            GlyphAction {
              glyph: device.preference ? "󰄬" : "󰐕"
              text: device.preference ? "Remove from your home" : panel.store.preferences.length >= 32 ? "32 devices selected" : "Add to your home"
              selected: device.preference !== null
              onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList)
              enabled: !panel.store.busy && (device.preference !== null || panel.store.preferences.length < 32)
              onClicked: panel.store.select(device.payload.entity_id)
            }
          }
          Item {
            id: editor
            y: deviceHeader.height
            width: parent.width
            height: device.editing ? editorContent.implicitHeight + panel.theme.cardPadding * 2 : 0
            visible: height > 0
            clip: true
            Behavior on height { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }
            Column {
              id: editorContent
              anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
              spacing: panel.theme.spaceMedium
              Label { width: parent.width; text: device.payload.entity_id; elide: Text.ElideRight; wrapMode: Text.NoWrap }
              Label { text: "Display name" }
              Field { id: nameField; onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList); Accessible.name: "Display name"; width: parent.width; placeholderText: "Use Home Assistant name"; enabled: !panel.store.busy }
              Label { text: "Room" }
              Field { id: roomField; onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList); Accessible.name: "Room"; width: parent.width; placeholderText: "Use Home Assistant room"; enabled: !panel.store.busy }
              RowLayout {
                width: parent.width
                spacing: panel.theme.spaceSmall
                Action {
                  text: "Menu bar"
                  onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList)
                  selected: panel.store.summary === device.payload.entity_id
                  enabled: !panel.store.busy
                  onClicked: panel.store.save(panel.store.preferences, selected ? "" : device.payload.entity_id)
                }
                GlyphAction { glyph: "󰁝"; text: "Move earlier in group"; enabled: !panel.store.busy && panel.store.moveTarget(device.payload.entity_id, -1) >= 0; onClicked: panel.store.move(device.payload.entity_id, -1) }
                GlyphAction { glyph: "󰁅"; text: "Move later in group"; enabled: !panel.store.busy && panel.store.moveTarget(device.payload.entity_id, 1) >= 0; onClicked: panel.store.move(device.payload.entity_id, 1) }
                Item { Layout.fillWidth: true }
                Action {
                  objectName: "save:" + device.payload.entity_id
                  onActiveFocusChanged: if (activeFocus) panel.reveal(this, deviceList)
                  text: "Save"
                  selected: true
                  enabled: !panel.store.busy && !!device.preference && (nameField.text !== device.preference.name || roomField.text !== device.preference.room)
                  onClicked: {
                    var entries = Bridge.call("home_assistant.edit", [panel.store.preferences, device.payload.entity_id, "name", nameField.text])
                    entries = Bridge.call("home_assistant.edit", [entries, device.payload.entity_id, "room", roomField.text])
                    panel.store.save(entries, panel.store.summary)
                  }
                }
              }
            }
          }
        }
        ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
      }
    }
  }

  Shared.SeeleFlickable {
    id: homeViewport
    theme: panel.theme
    visible: !panel.setupShown && panel.page === "home"
    width: parent.width
    height: Math.min(contentHeight, panel.bodyHeight)
    contentHeight: homeGroups.implicitHeight
    clip: true
    Column {
      id: homeGroups
      width: parent.width
      spacing: panel.theme.panelSpacing
      Shared.EmptyState {
        theme: panel.theme
        width: parent.width
        height: implicitHeight + panel.theme.cardPadding * 2
        visible: homeModel.count === 0
        glyph: "󰋜"
        title: "Your home starts here"
        detail: "Keep your everyday controls and readings close."
        Action { text: "Choose devices"; selected: true; onClicked: { panel.selectedOnly = false; panel.page = "devices" } }
      }
      Repeater {
        model: homeModel
        Column {
          id: group
          required property var payload
          objectName: payload.key
          width: homeGroups.width
          spacing: panel.theme.spaceSmall
          ListModel { id: readings; dynamicRoles: true }
          ListModel { id: controls; dynamicRoles: true }
          function sync() {
            panel.reconcile(readings, payload.readings)
            panel.reconcile(controls, payload.controls)
          }
          onPayloadChanged: sync()
          Component.onCompleted: sync()
          TextMetrics {
            id: headingMetrics
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textLabel
            font.letterSpacing: panel.theme.trackingLabel
            text: group.payload.heading.toUpperCase()
            elide: Text.ElideRight
            elideWidth: group.width
          }
          Shared.SectionRule { theme: panel.theme; width: parent.width; label: headingMetrics.elidedText }
          Rectangle {
            width: parent.width
            height: groupContent.implicitHeight + panel.theme.cardPadding * 2
            color: panel.theme.cardColor
            radius: panel.theme.radius
            Shared.CardEdge { theme: panel.theme }
            Column {
              id: groupContent
              anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
              spacing: panel.theme.spaceSmall
              Grid {
                id: readingGrid
                width: parent.width
                columns: readings.count === 1 ? 1 : 2
                spacing: panel.theme.spaceSmall
                visible: readings.count > 0
                Repeater {
                  model: readings
                  Rectangle {
                    id: reading
                    required property var payload
                    objectName: "reading:" + payload.entity_id
                    width: (readingGrid.width - (readingGrid.columns - 1) * readingGrid.spacing) / readingGrid.columns
                    height: readingContent.implicitHeight + panel.theme.cardPadding * 2
                    radius: panel.theme.radiusSmall
                    color: panel.theme.rowColor
                    Accessible.role: Accessible.StaticText
                    Accessible.name: payload.name + ": " + payload.state_label + (payload.unit || "") + (!panel.store.connected ? ", last known value" : "")
                    Column {
                      id: readingContent
                      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
                      spacing: panel.theme.spaceSmall
                      RowLayout {
                        width: parent.width
                        spacing: panel.theme.spaceSmall
                        Shared.CenteredGlyph { text: reading.payload.glyph; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody; color: panel.theme.subtext; Layout.preferredWidth: panel.theme.textIcon; Layout.preferredHeight: panel.theme.textIcon }
                        Label { Layout.fillWidth: true; text: reading.payload.name + (group.payload.key === "favorites" && reading.payload.room ? " · " + reading.payload.room : ""); elide: Text.ElideRight; wrapMode: Text.NoWrap }
                      }
                      FontMetrics { id: readingValueMetrics; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textDisplay }
                      Label {
                        width: parent.width
                        text: reading.payload.state_label + (reading.payload.available ? reading.payload.unit || "" : "")
                        height: readingValueMetrics.height
                        font.pixelSize: reading.payload.available ? panel.theme.textDisplay : panel.theme.textBody
                        font.weight: reading.payload.available ? panel.theme.weightLight : panel.theme.weightMedium
                        color: !panel.store.connected || !reading.payload.available ? panel.theme.yellow : panel.theme.text
                        elide: Text.ElideRight
                        wrapMode: Text.NoWrap
                      }
                    }
                  }
                }
              }
              Repeater {
                model: controls
                Rectangle {
                  id: homeRow
                  required property var payload
                  readonly property var pending: panel.store.pending[payload.entity_id]
                  readonly property bool actionable: panel.store.connected && payload.available && payload.controllable && !pending && !panel.store.settingsPending
                  readonly property bool adjustable: !!payload.dimmable || !!payload.temperature || !!payload.speed_control
                  readonly property bool unfolded: adjustable && panel.expanded === payload.entity_id
                  readonly property bool isOn: pending && pending.state !== undefined ? pending.state === "on" : payload.state === "on"
                  objectName: "control:" + payload.entity_id
                  width: groupContent.width
                  height: controlHeader.height + levels.height
                  radius: panel.theme.radiusSmall
                  color: panel.theme.rowColor
                  clip: true
                  function changeState() { if (actionable) panel.store.setState(payload, isOn ? "off" : "on") }
                  Shared.HoverWash { theme: panel.theme; hovered: controlHover.hovered }
                  HoverHandler { id: controlHover }
                  Item {
                    id: controlHeader
                    width: parent.width
                    height: panel.theme.detailRowHeight
                    Button {
                      id: rowButton
                      onActiveFocusChanged: if (activeFocus) panel.reveal(rowButton, homeViewport)
                      anchors { left: parent.left; top: parent.top; bottom: parent.bottom; right: powerButton.left; rightMargin: panel.theme.spaceSmall }
                      enabled: homeRow.adjustable || homeRow.actionable
                      focusPolicy: Qt.StrongFocus
                      Accessible.name: homeRow.payload.name + (homeRow.adjustable ? ", controls" : homeRow.isOn ? ", turn off" : ", turn on")
                      padding: 0
                      Keys.onPressed: event => {
                        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
                        if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) clicked()
                        event.accepted = true
                      }
                      onClicked: {
                        if (homeRow.adjustable) panel.expanded = homeRow.unfolded ? "" : homeRow.payload.entity_id
                        else homeRow.changeState()
                      }
                      background: Rectangle { color: rowButton.activeFocus ? panel.theme.selectedColor : panel.theme.clearColor; radius: panel.theme.radiusSmall }
                      contentItem: RowLayout {
                        spacing: panel.theme.spaceSmall
                        Mark { text: homeRow.payload.glyph; color: homeRow.payload.available && homeRow.isOn ? panel.theme.accent : panel.theme.subtext }
                        Column {
                          Layout.fillWidth: true
                          spacing: panel.theme.spaceTight
                          Label { width: parent.width; text: homeRow.payload.name; color: panel.theme.text; font.pixelSize: panel.theme.textBody; elide: Text.ElideRight; wrapMode: Text.NoWrap }
                          Label {
                            width: parent.width
                            text: homeRow.pending ? "Updating…" : !homeRow.payload.available ? "Unavailable" : (group.payload.key === "favorites" && homeRow.payload.room ? homeRow.payload.room + " · " : "") + homeRow.payload.state_label + (homeRow.isOn && homeRow.payload.dimmable ? " · " + homeRow.payload.brightness + "%" : homeRow.isOn && homeRow.payload.speed_control ? " · " + homeRow.payload.percentage + "%" : "")
                            color: !homeRow.payload.available || !panel.store.connected ? panel.theme.yellow : panel.theme.subtext
                            elide: Text.ElideRight
                            wrapMode: Text.NoWrap
                          }
                        }
                        Mark { visible: homeRow.adjustable; text: homeRow.unfolded ? "󰅃" : "󰅀"; Layout.preferredWidth: panel.theme.textIcon; font.pixelSize: panel.theme.textBody }
                      }
                      HoverHandler { cursorShape: rowButton.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    }
                    Button {
                      id: powerButton
                      onActiveFocusChanged: if (activeFocus) panel.reveal(powerButton, homeViewport)
                      objectName: "power:" + homeRow.payload.entity_id
                      anchors { right: parent.right; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
                      width: powerSwitch.implicitWidth
                      height: panel.theme.rowHeight
                      enabled: homeRow.actionable
                      focusPolicy: Qt.StrongFocus
                      Accessible.name: homeRow.payload.name + (homeRow.isOn ? ", turn off" : ", turn on")
                      Keys.onPressed: event => {
                        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
                        if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) homeRow.changeState()
                        event.accepted = true
                      }
                      onClicked: homeRow.changeState()
                      background: Rectangle { color: powerButton.activeFocus ? panel.theme.selectedColor : panel.theme.clearColor; radius: panel.theme.radiusSmall }
                      Shared.ControlSwitch {
                        id: powerSwitch
                        theme: panel.theme
                        anchors.centerIn: parent
                        checked: homeRow.isOn
                        busy: !!homeRow.pending
                        onToggled: homeRow.changeState()
                      }
                      HoverHandler { cursorShape: powerButton.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    }
                  }
                  Item {
                    id: levels
                    y: controlHeader.height
                    width: parent.width
                    height: homeRow.unfolded ? levelContent.implicitHeight + panel.theme.cardPadding * 2 : 0
                    visible: height > 0
                    clip: true
                    Behavior on height { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }
                    Column {
                      id: levelContent
                      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
                      spacing: panel.theme.spaceSmall
                      DeviceSlider {
                        width: parent.width
                        title: "Fan speed"
                        minimum: 0
                        current: homeRow.pending && homeRow.pending.percentage !== undefined ? homeRow.pending.percentage : homeRow.payload.percentage || 0
                        step: homeRow.payload.percentage_step || 1
                        visible: !!homeRow.payload.speed_control
                        enabled: homeRow.actionable
                        onCommitted: value => panel.store.setValue(homeRow.payload, {percentage: Math.round(value)})
                      }
                      DeviceSlider {
                        width: parent.width
                        title: "Brightness"
                        current: homeRow.pending && homeRow.pending.brightness !== undefined ? homeRow.pending.brightness : homeRow.payload.brightness || 1
                        visible: !!homeRow.payload.dimmable
                        enabled: homeRow.actionable
                        onCommitted: value => panel.store.setValue(homeRow.payload, {brightness: Math.round(value)})
                      }
                      DeviceSlider {
                        width: parent.width
                        title: "Color temperature"
                        suffix: " K"
                        spectrum: panel.theme.temperatureSpectrum
                        step: 50
                        minimum: homeRow.payload.min_kelvin || 2000
                        maximum: homeRow.payload.max_kelvin || 6500
                        current: homeRow.pending && homeRow.pending.kelvin !== undefined ? homeRow.pending.kelvin : homeRow.payload.kelvin || minimum
                        visible: !!homeRow.payload.temperature
                        enabled: homeRow.actionable
                        onCommitted: value => panel.store.setValue(homeRow.payload, {kelvin: Math.round(value)})
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
  }
  HoverHandler { id: panelHover }
}
