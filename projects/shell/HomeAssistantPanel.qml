import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared

Column {
  id: panel
  required property var theme
  required property var store
  property string page: "home"
  property string expanded: ""
  property string editingId: ""
  readonly property bool setupShown: page === "setup" || !store.configured
  spacing: theme.panelSpacing
  signal closeRequested()
  Keys.onEscapePressed: {
    tokenField.clear()
    if (page !== "home" && store.configured) page = "home"
    else closeRequested()
  }
  function opened() {
    tokenField.clear()
    if (!visible) return
    store.discover(page === "devices" && !setupShown)
    if (setupShown) { serverField.text = store.url; serverField.forceActiveFocus(); serverField.selectAll() }
    else if (page === "devices") { search.forceActiveFocus(); search.selectAll() }
    else forceActiveFocus()
  }
  function closed() { tokenField.clear(); store.discover(false) }
  onPageChanged: Qt.callLater(opened)
  Connections {
    target: panel.store
    function onSetupComplete() { panel.page = panel.visible ? "devices" : "home" }
  }

  ListModel { id: homeModel; dynamicRoles: true }
  ListModel { id: deviceModel; dynamicRoles: true }
  function reconcile(model, items) {
    var keys = items.map(function(item) { return item.entity_id || "heading:" + item.heading })
    for (var i = 0; i < items.length; i++) {
      var found = -1
      for (var j = i; j < model.count; j++) if (model.get(j).key === keys[i]) { found = j; break }
      if (found < 0) model.insert(i, {key: keys[i], payload: items[i]})
      else {
        if (found !== i) model.move(found, i, 1)
        if (JSON.stringify(model.get(i).payload) !== JSON.stringify(items[i])) model.setProperty(i, "payload", items[i])
      }
    }
    if (model.count > items.length) model.remove(items.length, model.count - items.length)
  }
  function syncDevices() {
    var query = search.text.toLowerCase()
    var known = store.entities.slice()
    store.catalog.forEach(function(item) { if (!known.some(function(entry) { return entry.entity_id === item.entity_id })) known.push(item) })
    reconcile(deviceModel, known.filter(function(item) { return (item.name + " " + item.entity_id + " " + item.room).toLowerCase().indexOf(query) >= 0 }))
  }
  Connections {
    target: panel.store
    function onEntitiesChanged() { panel.reconcile(homeModel, panel.store.rows()); panel.syncDevices() }
    function onCatalogChanged() { panel.syncDevices() }
  }
  Component.onCompleted: { reconcile(homeModel, store.rows()); syncDevices() }

  component Action: Shared.ActionButton { theme: panel.theme; activeFocusOnTab: true }
  component Field: TextField {
    id: field
    implicitHeight: panel.theme.controlHeight
    color: panel.theme.text
    placeholderTextColor: panel.theme.subtext
    selectionColor: panel.theme.selectedColor
    selectedTextColor: panel.theme.text
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    leftPadding: panel.theme.cardPadding
    rightPadding: panel.theme.cardPadding
    selectByMouse: true
    background: Rectangle {
      color: panel.theme.wellColor
      radius: panel.theme.radiusSmall
      border.color: field.activeFocus ? panel.theme.edgeCrown : panel.theme.cardBorder
    }
  }
  component Label: Text {
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
  }
  component LightSlider: Column {
    id: level
    required property string title
    required property real current
    property real minimum: 1
    property real maximum: 100
    property real step: 1
    property string suffix: "%"
    signal committed(real value)
    spacing: panel.theme.spaceTight
    Label { text: level.title + " · " + Math.round(slider.value) + level.suffix }
    Slider {
      id: slider
      width: parent.width
      height: panel.theme.controlHeight
      from: level.minimum
      to: level.maximum
      stepSize: level.step
      value: level.current
      live: true
      onPressedChanged: if (!pressed) level.committed(value)
      Keys.onReleased: event => {
        if (!event.isAutoRepeat && (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Home || event.key === Qt.Key_End)) level.committed(value)
      }
      background: Shared.MeterBar {
        theme: panel.theme
        x: slider.leftPadding
        y: (slider.height - height) / 2
        width: slider.availableWidth
        height: panel.theme.spaceMedium
        ratio: slider.visualPosition
        fill: panel.theme.accent
      }
      handle: Rectangle {
        x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
        y: (slider.height - height) / 2
        width: panel.theme.spaceLarge
        height: width
        radius: width / 2
        color: slider.enabled ? panel.theme.text : panel.theme.overlay
      }
      HoverHandler { cursorShape: slider.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
    }
  }

  Shared.PanelHeader {
    theme: panel.theme
    width: parent.width
    glyph: "󰋜"
    title: "Home Assistant"
    detail: panel.setupShown ? "Connect your home" : panel.store.connected ? "Live · " + panel.store.entities.length + " selected" : "Offline · last known values"
    detailColor: panel.store.connected || panel.setupShown ? panel.theme.subtext : panel.theme.yellow
    Row {
      spacing: panel.theme.spaceTight
      Action {
        text: panel.page === "home" ? "Devices" : "Back"
        visible: panel.store.configured
        onClicked: panel.page = panel.page === "home" ? "devices" : "home"
      }
      Action { text: "Setup"; visible: !panel.setupShown; onClicked: panel.page = "setup" }
    }
  }
  Label {
    width: parent.width
    visible: text !== ""
    text: panel.store.error
    color: panel.theme.yellow
  }

  Column {
    visible: panel.setupShown
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.SectionRule { theme: panel.theme; width: parent.width; label: "CONNECTION" }
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
        Label { text: "Server URL" }
        Field { id: serverField; width: parent.width; placeholderText: "https://home.example"; enabled: !panel.store.busy }
        Label { text: "Long-lived access token" }
        Field {
          id: tokenField
          width: parent.width
          echoMode: TextInput.Password
          placeholderText: "Paste your token"
          enabled: !panel.store.busy
          inputMethodHints: Qt.ImhHiddenText | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
          onAccepted: connectButton.clicked()
        }
        Label {
          width: parent.width
          text: "Create a long-lived access token in your Home Assistant profile. Your system keyring stores it and may ask you to unlock it."
        }
        Action {
          width: parent.width
          text: "Retry keyring"
          visible: panel.store.error !== "" && !panel.store.configured
          enabled: !panel.store.busy
          onClicked: panel.store.refresh()
        }
        Action {
          id: connectButton
          width: parent.width
          text: panel.store.settingsPending ? "Connecting…" : "Connect and save"
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

  Column {
    visible: !panel.setupShown && panel.page === "devices"
    width: parent.width
    spacing: panel.theme.panelSpacing
    Field { id: search; width: parent.width; placeholderText: "Search devices or rooms"; onTextChanged: panel.syncDevices() }
    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "DEVICES"
      detail: panel.store.settingsPending ? "Saving…" : "Select up to 32"
    }
    Label {
      width: parent.width
      text: "Select devices, star favorites, and choose one menu bar reading. Arrows set the order within Favorites and each room."
    }
    Shared.DeviceListCard {
      theme: panel.theme
      width: parent.width
      listHeight: Math.min(deviceList.contentHeight, panel.theme.rowHeight * 8)
      Shared.SeeleListView {
        theme: panel.theme
        id: deviceList
        anchors.fill: parent
        anchors.margins: panel.theme.cardPadding
        clip: true
        spacing: panel.theme.spaceTight
        model: deviceModel
        delegate: Rectangle {
          id: device
          required property var payload
          readonly property var modelData: payload
          readonly property var preference: panel.store.preference(modelData.entity_id)
          width: ListView.view.width
          height: deviceContent.implicitHeight + panel.theme.cardPadding * 2
          radius: panel.theme.radiusSmall
          color: panel.theme.rowColor
          Shared.HoverWash { theme: panel.theme; hovered: deviceHover.hovered }
          HoverHandler { id: deviceHover }
          Column {
            id: deviceContent
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
            spacing: panel.theme.spaceSmall
            RowLayout {
              width: parent.width
              Label { Layout.fillWidth: true; text: device.modelData.name; color: panel.theme.text; font.pixelSize: panel.theme.textBody; elide: Text.ElideRight; wrapMode: Text.NoWrap }
              Action {
                text: panel.editingId === device.modelData.entity_id ? "Done" : "Edit"
                visible: device.preference !== null
                onClicked: panel.editingId = panel.editingId === device.modelData.entity_id ? "" : device.modelData.entity_id
              }
              Action {
                text: device.preference ? "Remove" : "Add"
                enabled: !panel.store.busy && (device.preference !== null || panel.store.preferences.length < 32)
                onClicked: panel.store.select(device.modelData.entity_id)
              }
            }
            Label { width: parent.width; text: device.modelData.entity_id + " · " + device.modelData.room; elide: Text.ElideRight; wrapMode: Text.NoWrap }
            Column {
              width: parent.width
              visible: device.preference !== null && panel.editingId === device.modelData.entity_id
              spacing: panel.theme.spaceSmall
              Field {
                width: parent.width
                placeholderText: "Display name"
                text: device.preference ? device.preference.name : ""
                enabled: !panel.store.busy
                onEditingFinished: if (device.preference && text !== device.preference.name) panel.store.edit(device.modelData.entity_id, "name", text)
              }
              Field {
                width: parent.width
                placeholderText: "Room · " + device.modelData.room
                text: device.preference ? device.preference.room : ""
                enabled: !panel.store.busy
                onEditingFinished: if (device.preference && text !== device.preference.room) panel.store.edit(device.modelData.entity_id, "room", text)
              }
              RowLayout {
                width: parent.width
                spacing: panel.theme.spaceTight
                Action { text: "★"; Accessible.name: "Favorite"; selected: !!device.preference && device.preference.favorite; enabled: !panel.store.busy; onClicked: panel.store.edit(device.modelData.entity_id, "favorite", !selected) }
                Action { text: "Menu bar"; selected: panel.store.summary === device.modelData.entity_id; enabled: !panel.store.busy; onClicked: panel.store.save(panel.store.preferences, selected ? "" : device.modelData.entity_id) }
                Item { Layout.fillWidth: true }
                Action { text: "↑"; Accessible.name: "Move earlier"; enabled: !panel.store.busy && panel.store.moveTarget(device.modelData.entity_id, -1) >= 0; onClicked: panel.store.move(device.modelData.entity_id, -1) }
                Action { text: "↓"; Accessible.name: "Move later"; enabled: !panel.store.busy && panel.store.moveTarget(device.modelData.entity_id, 1) >= 0; onClicked: panel.store.move(device.modelData.entity_id, 1) }
              }
            }
          }
        }
        ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
      }
    }
  }

  Column {
    visible: !panel.setupShown && panel.page === "home"
    width: parent.width
    spacing: panel.theme.panelSpacing
    Label { width: parent.width; visible: panel.store.entities.length === 0; text: "Choose Devices to add your lights and sensors." }
    Action { visible: !panel.store.connected; text: "Reconnect"; onClicked: panel.store.refresh() }
    Shared.DeviceListCard {
      theme: panel.theme
      width: parent.width
      visible: panel.store.entities.length > 0
      listHeight: Math.min(homeList.contentHeight, panel.theme.rowHeight * 10)
      Shared.SeeleListView {
        theme: panel.theme
        id: homeList
        anchors.fill: parent
        anchors.margins: panel.theme.cardPadding
        clip: true
        spacing: panel.theme.spaceTight
        model: homeModel
        delegate: Item {
          id: homeRow
          required property var payload
          readonly property var modelData: payload
          readonly property bool heading: !!modelData.heading
          readonly property bool actionable: !heading && panel.store.connected && modelData.available && modelData.controllable && !panel.store.pending[modelData.entity_id] && !panel.store.settingsPending
          width: ListView.view.width
          height: heading ? section.height + panel.theme.spaceMedium : rowContent.implicitHeight + panel.theme.cardPadding * 2
          function changeState() { if (actionable) panel.store.setState(modelData, modelData.state === "on" ? "off" : "on") }
          Keys.onPressed: event => {
            if (event.isAutoRepeat || (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
              changeState()
              event.accepted = true
            }
          }
          activeFocusOnTab: actionable
          Shared.SectionRule {
            id: section
            theme: panel.theme
            width: parent.width
            visible: homeRow.heading
            label: (homeRow.modelData.heading || "").toUpperCase()
            detail: homeRow.modelData.detail || ""
          }
          Rectangle {
            anchors.fill: parent
            visible: !homeRow.heading
            color: homeRow.activeFocus ? panel.theme.selectedColor : panel.theme.rowColor
            radius: panel.theme.radiusSmall
            Shared.HoverWash { theme: panel.theme; hovered: homeHover.hovered }
            HoverHandler { id: homeHover }
          }
          Column {
            id: rowContent
            visible: !homeRow.heading
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
            spacing: panel.theme.spaceSmall
            RowLayout {
              width: parent.width
              Column {
                Layout.fillWidth: true
                Label { width: parent.width; text: homeRow.modelData.name || ""; color: panel.theme.text; font.pixelSize: panel.theme.textBody; elide: Text.ElideRight; wrapMode: Text.NoWrap }
                Label { width: parent.width; text: (homeRow.modelData.state || "") + " " + (homeRow.modelData.unit || "") + (!panel.store.connected ? " · stale" : panel.store.pending[homeRow.modelData.entity_id] ? " · updating…" : ""); color: panel.store.connected && homeRow.modelData.available ? panel.theme.subtext : panel.theme.yellow }
              }
              Action {
                text: panel.expanded === homeRow.modelData.entity_id ? "⌃" : "⌄"
                Accessible.name: "Light controls"
                visible: !!homeRow.modelData.dimmable || !!homeRow.modelData.temperature
                onClicked: panel.expanded = panel.expanded === homeRow.modelData.entity_id ? "" : homeRow.modelData.entity_id
              }
              Shared.ControlSwitch {
                theme: panel.theme
                visible: !!homeRow.modelData.controllable
                enabled: homeRow.actionable
                checked: panel.store.pending[homeRow.modelData.entity_id] && panel.store.pending[homeRow.modelData.entity_id].state !== undefined ? panel.store.pending[homeRow.modelData.entity_id].state === "on" : homeRow.modelData.state === "on"
                busy: !!panel.store.pending[homeRow.modelData.entity_id]
                onToggled: homeRow.changeState()
              }
            }
            Column {
              width: parent.width
              visible: panel.expanded === homeRow.modelData.entity_id
              spacing: panel.theme.spaceSmall
              LightSlider {
                width: parent.width
                title: "Brightness"
                current: homeRow.modelData.brightness || 1
                visible: !!homeRow.modelData.dimmable
                enabled: homeRow.actionable
                onCommitted: value => panel.store.setValue(homeRow.modelData, {brightness: Math.round(value)})
              }
              LightSlider {
                width: parent.width
                title: "Warm / cool"
                suffix: " K"
                step: 50
                minimum: homeRow.modelData.min_kelvin || 2000
                maximum: homeRow.modelData.max_kelvin || 6500
                current: homeRow.modelData.kelvin || minimum
                visible: !!homeRow.modelData.temperature
                enabled: homeRow.actionable
                onCommitted: value => panel.store.setValue(homeRow.modelData, {kelvin: Math.round(value)})
              }
            }
          }
        }
        ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panelHover.hovered }
      }
    }
  }
  HoverHandler { id: panelHover }
}
