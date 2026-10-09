pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// Local weather in the clock popup: one quiet line that unfolds in place into
// the coming hours, the week and the place it is for, with the place search as
// a step inside it. The worker sends every label, glyph and ratio already
// decided; this view only draws them.
FocusScope {
  id: card
  required property var theme
  required property var store
  property bool popupHovered: false
  property bool expanded: false
  // The place search, a step inside the unfolded card.
  property bool searching: false
  // The most height the popup can give the unfolded card.
  property real available: 0

  readonly property var current: store.current
  readonly property var status: store.status
  readonly property var place: store.place
  readonly property var search: store.search
  readonly property string mode: store.mode
  readonly property bool placeStep: expanded && (searching || mode === "no-place")
  readonly property real foldedHeight: theme.rowHeight
  readonly property real unfoldedHeight: Math.min(card.available, foldedHeight + body.implicitHeight + theme.cardPadding)

  function unfold() {
    expanded = true
    if (mode === "no-place") openSearch()
  }
  function fold() {
    // Focus follows the fold only when it was inside the card to begin with.
    var focused = card.activeFocus
    closeSearch()
    expanded = false
    if (focused) headline.forceActiveFocus()
  }
  // The popup opens on the folded line, or unfolded when Integration Health
  // asked for the weather's settings.
  function reset(unfolded) {
    closeSearch()
    expanded = false
    if (unfolded) unfold()
  }
  function backToForecast() {
    closeSearch()
    headline.forceActiveFocus()
  }
  function toggle() { if (expanded) fold(); else unfold() }
  function openSearch() {
    searching = true
    Qt.callLater(function() { placeField.forceActiveFocus(); placeField.selectAll() })
  }
  function closeSearch() {
    if (!searching) return
    searching = false
    placeField.text = ""
    store.find("")
  }
  function choose(id) {
    store.choose(id)
    closeSearch()
    headline.forceActiveFocus()
  }

  visible: mode !== "unavailable"
  implicitHeight: visible ? (expanded ? unfoldedHeight : foldedHeight) : 0
  height: implicitHeight
  Behavior on height { NumberAnimation { duration: card.theme.durationDefaultSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: card.theme.springDefaultSpatial } }

  // Escape steps back out one level: the search, then the unfolded card.
  Keys.onPressed: event => {
    if (event.key !== Qt.Key_Escape || !expanded) return
    if (searching && mode !== "no-place") backToForecast()
    else fold()
    event.accepted = true
  }

  Rectangle {
    id: surface
    anchors.fill: parent
    radius: card.theme.radius
    color: card.theme.cardColor
    clip: true

    // The line itself is the control that folds the card.
    Rectangle {
      id: headline
      objectName: "weatherHeadline"
      width: parent.width
      height: card.foldedHeight
      radius: card.theme.radius
      color: card.theme.clearColor
      activeFocusOnTab: true
      Accessible.role: Accessible.Button
      Accessible.name: (card.current ? card.current.temperature + ", " + card.current.condition + ", " : "")
        + (card.place.name ? card.place.name + ". " : "") + (card.expanded ? "Hide the forecast" : "Show the forecast")
      Keys.onPressed: event => {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space) return
        card.toggle()
        event.accepted = true
      }

      HoverHandler { id: headlineHover; cursorShape: Qt.PointingHandCursor }
      Shared.HoverWash { theme: card.theme; hovered: headlineHover.hovered }
      Shared.FocusRing { theme: card.theme; shown: headline.activeFocus }
      MouseArea {
        anchors.fill: parent
        onClicked: { headline.forceActiveFocus(); card.toggle() }
      }

      Shared.CenteredGlyph {
        id: headlineGlyph
        x: card.theme.cardPadding
        width: card.theme.textDisplay
        height: parent.height
        visible: !headlineSpinner.visible
        text: card.current ? card.current.glyph : card.mode === "no-place" ? "󰍎" : "󰖐"
        color: card.current && card.current.tone === "sun" ? card.theme.yellow
          : card.current ? card.theme.text : card.theme.overlay
        font.family: card.theme.fontFamily
        font.pixelSize: card.theme.textCard
      }
      Shared.RefreshGlyph {
        id: headlineSpinner
        theme: card.theme
        anchors.centerIn: headlineGlyph
        width: card.theme.textIcon
        height: width
        visible: !card.current && card.mode === "connecting"
        spinning: visible
        color: card.theme.overlay
      }

      Row {
        id: headlineText
        anchors.left: headlineGlyph.right
        anchors.leftMargin: card.theme.spaceMedium
        anchors.right: headlineTrailing.left
        anchors.rightMargin: card.theme.spaceMedium
        anchors.verticalCenter: parent.verticalCenter
        spacing: card.theme.spaceMedium

        Text {
          id: headlineTemperature
          visible: !!card.current
          anchors.baseline: headlineSummary.baseline
          text: card.current ? card.current.temperature : ""
          textFormat: Text.PlainText
          color: card.theme.text
          font.family: card.theme.fontFamily
          font.pixelSize: card.theme.textLead
          font.weight: card.theme.weightStrong
        }
        Text {
          id: headlineSummary
          width: headlineText.width - (headlineTemperature.visible ? headlineTemperature.width + headlineText.spacing : 0)
          anchors.verticalCenter: parent.verticalCenter
          text: card.current ? card.current.condition + (card.place.name ? " · " + card.place.name : "")
            : card.mode === "no-place" ? "Weather · choose a place"
            : card.mode === "offline" ? "Weather unavailable offline"
            : "Weather · " + card.status.label
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: card.current ? card.theme.subtext : card.theme.overlay
          font.family: card.theme.fontFamily
          font.pixelSize: card.theme.textBody
        }
      }

      Row {
        id: headlineTrailing
        anchors.right: parent.right
        anchors.rightMargin: card.theme.cardPadding
        anchors.verticalCenter: parent.verticalCenter
        spacing: card.theme.spaceSmall

        // A forecast the worker cannot vouch for says so in one word; one it
        // can shows the day's range instead.
        Text {
          objectName: "weatherBadge"
          anchors.verticalCenter: parent.verticalCenter
          visible: text !== ""
          text: card.status.badge || (card.current && card.current.high ? card.current.high + " / " + card.current.low : "")
          textFormat: Text.PlainText
          color: card.status.badge ? card.theme.yellow : card.theme.overlay
          font.family: card.theme.fontFamily
          font.pixelSize: card.theme.textCaption
          font.weight: card.status.badge ? card.theme.weightMedium : card.theme.weightRegular
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: card.expanded ? "󰅃" : "󰅀"
          color: headlineHover.hovered || headline.activeFocus ? card.theme.text : card.theme.overlay
          font.family: card.theme.fontFamily
          font.pixelSize: card.theme.textBody
        }
      }
    }

    Shared.SeeleFlickable {
      id: bodyView
      theme: card.theme
      y: card.foldedHeight
      width: parent.width
      height: Math.max(0, parent.height - y)
      visible: card.expanded
      clip: true
      contentHeight: body.implicitHeight + card.theme.cardPadding
      boundsBehavior: contentHeight > height ? Flickable.DragAndOvershootBounds : Flickable.StopAtBounds
      ScrollBar.vertical: Shared.SlimScrollBar { theme: card.theme; popupHovered: card.popupHovered }

      Column {
        id: body
        x: card.theme.cardPadding
        width: bodyView.width - card.theme.cardPadding * 2
        spacing: card.theme.spaceMedium

        // --- The forecast --------------------------------------------------
        Flow {
          id: facts
          objectName: "weatherFacts"
          width: parent.width
          visible: !card.placeStep && !!card.current && card.current.facts.length > 0
          spacing: card.theme.spaceLarge
          Repeater {
            model: card.current ? card.current.facts : []
            Row {
              id: fact
              required property var modelData
              spacing: card.theme.spaceTight
              Accessible.role: Accessible.StaticText
              Accessible.name: fact.modelData.name
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: fact.modelData.glyph
                color: card.theme.overlay
                font.family: card.theme.fontFamily
                font.pixelSize: card.theme.textBody
              }
              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: fact.modelData.text
                textFormat: Text.PlainText
                color: card.theme.subtext
                font.family: card.theme.fontFamily
                font.pixelSize: card.theme.textCaption
              }
            }
          }
        }

        Shared.SectionRule {
          theme: card.theme
          width: parent.width
          visible: hourStrip.visible
          label: "Next hours"
        }
        Rectangle {
          id: hourStrip
          objectName: "weatherHours"
          width: parent.width
          height: hourColumns.implicitHeight + card.theme.spaceMedium * 2
          visible: !card.placeStep && card.store.hours.length > 0
          radius: card.theme.radiusRow
          color: card.theme.rowColor

          Row {
            id: hourColumns
            anchors.centerIn: parent
            width: parent.width - card.theme.spaceSmall * 2
            Repeater {
              model: card.store.hours
              Column {
                id: hour
                required property var modelData
                required property int index
                width: hourColumns.width / Math.max(1, card.store.hours.length)
                spacing: card.theme.spaceTight
                Accessible.role: Accessible.StaticText
                Accessible.name: hour.modelData.label + ", " + hour.modelData.condition + ", " + hour.modelData.temperature
                  + (hour.modelData.rain ? ", " + hour.modelData.rain + " chance of rain" : "")
                Text {
                  width: parent.width
                  horizontalAlignment: Text.AlignHCenter
                  text: hour.modelData.label
                  textFormat: Text.PlainText
                  color: hour.index === 0 ? card.theme.text : card.theme.overlay
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textCaption
                  font.weight: hour.index === 0 ? card.theme.weightStrong : card.theme.weightRegular
                }
                Shared.CenteredGlyph {
                  width: parent.width
                  height: card.theme.textSubhead + card.theme.spaceTight
                  text: hour.modelData.glyph
                  color: hour.modelData.tone === "sun" ? card.theme.yellow : card.theme.subtext
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textSubhead
                }
                Text {
                  width: parent.width
                  horizontalAlignment: Text.AlignHCenter
                  text: hour.modelData.temperature
                  textFormat: Text.PlainText
                  color: card.theme.text
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textBody
                  font.weight: card.theme.weightMedium
                }
                // Held even when empty, so every column keeps one height.
                Text {
                  width: parent.width
                  horizontalAlignment: Text.AlignHCenter
                  text: hour.modelData.rain || " "
                  textFormat: Text.PlainText
                  color: card.theme.subtext
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textMicro
                }
              }
            }
          }
        }

        Shared.SectionRule {
          theme: card.theme
          width: parent.width
          visible: dayList.visible
          label: "This week"
        }
        Rectangle {
          id: dayList
          objectName: "weatherDays"
          width: parent.width
          height: dayRows.implicitHeight + card.theme.spaceTight * 2
          visible: !card.placeStep && card.store.days.length > 0
          radius: card.theme.radiusRow
          color: card.theme.rowColor

          // The widest weekday label and reading, so the columns stay straight.
          TextMetrics { id: dayLabelWidth; font.family: card.theme.fontFamily; font.pixelSize: card.theme.textBody; text: "Today" }
          TextMetrics { id: readingWidth; font.family: card.theme.fontFamily; font.pixelSize: card.theme.textCaption; text: "−888°" }
          TextMetrics { id: rainWidth; font.family: card.theme.fontFamily; font.pixelSize: card.theme.textCaption; text: "100%" }

          Column {
            id: dayRows
            anchors.centerIn: parent
            width: parent.width - card.theme.spaceMedium * 2
            Repeater {
              model: card.store.days
              Item {
                id: day
                required property var modelData
                required property int index
                width: dayRows.width
                height: card.theme.chipHeight
                Accessible.role: Accessible.StaticText
                Accessible.name: day.modelData.name + ", " + day.modelData.condition + ", "
                  + day.modelData.low + " to " + day.modelData.high
                  + (day.modelData.rain ? ", " + day.modelData.rain + " chance of rain" : "")

                Text {
                  id: dayLabel
                  width: dayLabelWidth.advanceWidth
                  anchors.verticalCenter: parent.verticalCenter
                  text: day.modelData.label
                  textFormat: Text.PlainText
                  color: card.theme.text
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textBody
                  font.weight: day.index === 0 ? card.theme.weightStrong : card.theme.weightRegular
                }
                Shared.CenteredGlyph {
                  id: dayGlyph
                  anchors.left: dayLabel.right
                  anchors.leftMargin: card.theme.spaceMedium
                  width: card.theme.textCard
                  height: parent.height
                  text: day.modelData.glyph
                  color: day.modelData.tone === "sun" ? card.theme.yellow : card.theme.subtext
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textIcon
                }
                Text {
                  id: dayRain
                  anchors.left: dayGlyph.right
                  anchors.leftMargin: card.theme.spaceTight
                  anchors.verticalCenter: parent.verticalCenter
                  width: rainWidth.advanceWidth
                  text: day.modelData.rain
                  textFormat: Text.PlainText
                  color: card.theme.subtext
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textCaption
                }
                Text {
                  id: dayLow
                  anchors.left: dayRain.right
                  anchors.leftMargin: card.theme.spaceSmall
                  anchors.verticalCenter: parent.verticalCenter
                  width: readingWidth.advanceWidth
                  horizontalAlignment: Text.AlignRight
                  text: day.modelData.low
                  textFormat: Text.PlainText
                  color: card.theme.overlay
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textCaption
                }
                Shared.MeterBar {
                  theme: card.theme
                  anchors.left: dayLow.right
                  anchors.leftMargin: card.theme.spaceMedium
                  anchors.right: dayHigh.left
                  anchors.rightMargin: card.theme.spaceMedium
                  anchors.verticalCenter: parent.verticalCenter
                  from: day.modelData.from
                  ratio: day.modelData.to
                }
                Text {
                  id: dayHigh
                  anchors.right: parent.right
                  anchors.verticalCenter: parent.verticalCenter
                  width: readingWidth.advanceWidth
                  horizontalAlignment: Text.AlignRight
                  text: day.modelData.high
                  textFormat: Text.PlainText
                  color: card.theme.text
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textCaption
                  font.weight: card.theme.weightMedium
                }
              }
            }
          }
        }

        // No forecast to draw yet, or none that could be fetched.
        Shared.EmptyState {
          objectName: "weatherEmpty"
          width: parent.width
          height: implicitHeight + card.theme.spaceLarge * 2
          visible: !card.placeStep && !card.current
          theme: card.theme
          glyph: card.mode === "offline" ? "󰼯" : ""
          tint: card.theme.yellow
          title: card.mode === "offline" ? "Weather unavailable offline" : "Loading the forecast…"
          detail: card.mode === "offline" ? "The forecast appears once Open-Meteo can be reached." : ""
          Shared.ActionButton {
            objectName: "weatherRetry"
            theme: card.theme
            visible: card.mode === "offline"
            text: "Retry"
            onClicked: card.store.refresh()
          }
        }

        // --- The place -----------------------------------------------------
        Shared.SectionRule {
          theme: card.theme
          width: parent.width
          visible: card.placeStep
          label: "Place"
        }

        Item {
          id: placeRow
          objectName: "weatherPlace"
          width: parent.width
          height: card.theme.controlHeight
          visible: !card.placeStep

          Shared.CenteredGlyph {
            id: placeMark
            width: card.theme.textIcon
            height: parent.height
            text: "󰍎"
            color: card.theme.overlay
            font.family: card.theme.fontFamily
            font.pixelSize: card.theme.textBody
          }
          Text {
            id: placeName
            anchors.left: placeMark.right
            anchors.leftMargin: card.theme.spaceSmall
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, (placeActions.x - x) / 2)
            text: card.place.name
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: card.theme.text
            font.family: card.theme.fontFamily
            font.pixelSize: card.theme.textBody
            font.weight: card.theme.weightStrong
          }
          Text {
            anchors.left: placeName.right
            anchors.leftMargin: card.theme.spaceSmall
            anchors.right: placeActions.left
            anchors.rightMargin: card.theme.spaceSmall
            anchors.baseline: placeName.baseline
            text: card.place.detail
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: card.theme.overlay
            font.family: card.theme.fontFamily
            font.pixelSize: card.theme.textCaption
          }
          Row {
            id: placeActions
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: card.theme.spaceSmall
            Shared.ActionButton {
              id: useCity
              objectName: "weatherUseCity"
              theme: card.theme
              implicitHeight: card.theme.chipHeight
              visible: card.place.chosen
              text: "Use timezone city"
              onClicked: card.store.reset()
            }
            Shared.GlyphButton {
              objectName: "weatherSearchPlace"
              theme: card.theme
              implicitWidth: card.theme.chipHeight
              implicitHeight: card.theme.chipHeight
              glyph: "󰍉"
              text: "Search for a place"
              onClicked: card.openSearch()
            }
          }
        }

        Shared.SearchField {
          id: placeField
          objectName: "weatherPlaceField"
          theme: card.theme
          width: parent.width
          visible: card.placeStep
          placeholderText: "Search for a city…"
          onTextEdited: card.store.find(text)
          Keys.onPressed: event => {
            if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && card.search.results.length > 0) {
              card.choose(card.search.results[0].id)
              event.accepted = true
            } else if (event.key === Qt.Key_Down && placeResults.count > 0) {
              placeResults.itemAt(0).forceActiveFocus()
              event.accepted = true
            }
          }
        }

        Column {
          width: parent.width
          spacing: card.theme.spaceTight
          visible: card.placeStep

          Repeater {
            id: placeResults
            model: card.placeStep ? card.search.results : []
            Rectangle {
              id: result
              required property var modelData
              required property int index
              objectName: "weatherResult_" + modelData.id
              width: parent.width
              height: card.theme.detailRowHeight - card.theme.spaceMedium
              radius: card.theme.radiusRow
              color: card.theme.rowColor
              activeFocusOnTab: true
              Accessible.role: Accessible.Button
              Accessible.name: "Use " + result.modelData.name + (result.modelData.detail ? ", " + result.modelData.detail : "")
              Keys.onPressed: event => {
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) card.choose(result.modelData.id)
                else if (event.key === Qt.Key_Down && result.index + 1 < placeResults.count) placeResults.itemAt(result.index + 1).forceActiveFocus()
                else if (event.key === Qt.Key_Up) (result.index > 0 ? placeResults.itemAt(result.index - 1) : placeField).forceActiveFocus()
                else return
                event.accepted = true
              }

              HoverHandler { id: resultHover; cursorShape: Qt.PointingHandCursor }
              Shared.HoverWash { theme: card.theme; hovered: resultHover.hovered }
              Shared.FocusRing { theme: card.theme; shown: result.activeFocus }
              MouseArea { anchors.fill: parent; onClicked: card.choose(result.modelData.id) }

              Column {
                anchors.left: parent.left
                anchors.leftMargin: card.theme.cardPadding
                anchors.right: parent.right
                anchors.rightMargin: card.theme.cardPadding
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                  width: parent.width
                  text: result.modelData.name
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: card.theme.text
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textBody
                  font.weight: card.theme.weightStrong
                }
                Text {
                  width: parent.width
                  visible: text !== ""
                  text: result.modelData.detail
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: card.theme.subtext
                  font.family: card.theme.fontFamily
                  font.pixelSize: card.theme.textCaption
                }
              }
            }
          }

          // What the search is doing when it has no places to offer.
          Row {
            objectName: "weatherSearchState"
            width: parent.width
            height: card.theme.chipHeight
            spacing: card.theme.spaceSmall
            visible: card.search.results.length === 0 && (card.search.busy || card.search.error !== "" || card.search.query.length >= 2)
            Shared.RefreshGlyph {
              theme: card.theme
              anchors.verticalCenter: parent.verticalCenter
              width: card.theme.textBody
              height: width
              visible: card.search.busy
              spinning: visible
              color: card.theme.overlay
            }
            Text {
              anchors.verticalCenter: parent.verticalCenter
              text: card.search.busy ? "Searching…" : card.search.error || "No places found"
              textFormat: Text.PlainText
              color: card.search.error ? card.theme.yellow : card.theme.overlay
              font.family: card.theme.fontFamily
              font.pixelSize: card.theme.textCaption
            }
          }

          Flow {
            width: parent.width
            spacing: card.theme.spaceSmall
            topPadding: card.theme.spaceTight
            Shared.ActionButton {
              theme: card.theme
              visible: card.place.chosen
              text: "Use timezone city" + (card.place.city ? " (" + card.place.city + ")" : "")
              onClicked: { card.store.reset(); card.backToForecast() }
            }
            Shared.ActionButton {
              theme: card.theme
              visible: card.mode !== "no-place"
              text: "Cancel"
              onClicked: card.backToForecast()
            }
          }
        }

        // --- Where it comes from -------------------------------------------
        Item {
          width: parent.width
          height: card.theme.chipHeight
          visible: !card.placeStep

          Text {
            objectName: "weatherStatus"
            anchors.left: parent.left
            anchors.right: weatherRefresh.left
            anchors.rightMargin: card.theme.spaceSmall
            anchors.verticalCenter: parent.verticalCenter
            text: "Open-Meteo" + (card.status.label ? " · " + card.status.label : "")
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: card.status.stale && card.current ? card.theme.yellow : card.theme.overlay
            font.family: card.theme.fontFamily
            font.pixelSize: card.theme.textCaption
          }
          Shared.GlyphButton {
            id: weatherRefresh
            theme: card.theme
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: card.theme.chipHeight - 4
            implicitHeight: card.theme.chipHeight - 4
            visible: card.mode !== "no-place"
            enabled: !card.status.fetching
            glyph: card.status.fetching ? "" : "󰑐"
            text: card.status.fetching ? "Refreshing the forecast" : "Refresh the forecast"
            onClicked: card.store.refresh()
            Shared.RefreshGlyph {
              theme: card.theme
              anchors.centerIn: parent
              width: card.theme.textIcon
              height: width
              visible: card.status.fetching
              spinning: visible
              color: card.theme.subtext
              font.pixelSize: card.theme.textBody
            }
          }
        }
      }
    }
  }
}
