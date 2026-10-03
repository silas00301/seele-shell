import QtQuick
import QtTest
import "shared" as Shared

// The clock popup's weather card against a stand-in for the worker's
// published sections. The folded line, the unfolded forecast, the place
// search and every state without a forecast are driven the way a person
// would drive them, with the pointer and from the keyboard.
Rectangle {
  width: 420; height: 760
  color: theme.mantle
  Shared.TestTheme { id: theme }

  readonly property var forecast: ({
    glyph: "󰖐", tone: "plain", condition: "Overcast", temperature: "15°", high: "18°", low: "14°",
    facts: [
      { glyph: "󰔏", text: "Feels 14°", name: "Feels like 14°" },
      { glyph: "󰖝", text: "7 km/h SE", name: "Wind 7 km/h SE" },
      { glyph: "󰖎", text: "67%", name: "Humidity 67%" },
      { glyph: "󰖜", text: "07:09", name: "Sunrise 07:09" },
      { glyph: "󰖛", text: "18:41", name: "Sunset 18:41" }
    ]
  })
  readonly property var hours: [0, 1, 2, 3, 4, 5, 6, 7].map(function(i) {
    return { label: i === 0 ? "Now" : (14 + i) + ":00", glyph: i < 3 ? "󰖙" : "󰖗", tone: i < 3 ? "sun" : "plain",
             condition: i < 3 ? "Clear" : "Light rain", temperature: (15 + i) + "°", rain: i < 3 ? "" : (20 * i) + "%" }
  })
  readonly property var days: ["Today", "Sat", "Sun", "Mon", "Tue", "Wed", "Thu"].map(function(label, i) {
    return { label: label, name: label + " " + (2 + i) + " October", glyph: "󰖐", tone: "plain", condition: "Overcast",
             high: (18 + i) + "°", low: (10 + i) + "°", rain: i === 5 ? "34%" : "", from: i / 10, to: 0.4 + i / 12 }
  })
  readonly property var online: ({ state: "online", fetching: false, stale: false, label: "Updated 14:05", badge: "", error: "" })

  QtObject {
    id: store
    property bool ready: true
    property var place: ({ name: "Berlin", detail: "Timezone city · Europe/Berlin", chosen: false, city: "Berlin" })
    property var status: online
    property var current: forecast
    property var hours: []
    property var days: []
    property var search: ({ query: "", busy: false, error: "", results: [] })
    property var calls: []
    readonly property string mode: ready ? status.state : "unavailable"
    function record(call) { calls = calls.concat([call]) }
    function refresh() { record("refresh") }
    function retry(token) { record("retry:" + token) }
    function find(query) { record("find:" + query) }
    function choose(id) { record("choose:" + id) }
    function reset() { record("reset") }
  }

  WeatherCard {
    id: card
    width: 360
    available: 600
    theme: theme
    store: store
  }

  TestCase {
    name: "WeatherCard"
    when: windowShown

    function child(parent, name) {
      var item = findChild(parent, name)
      verify(item !== null, "Missing " + name)
      return item
    }
    function texts(item) {
      var found = []
      if (item.text !== undefined && item.visible && typeof item.text === "string") found.push(item.text)
      for (var i = 0; i < item.children.length; i++) found = found.concat(texts(item.children[i]))
      return found
    }
    function shows(item, text) { return texts(item).indexOf(text) >= 0 }
    // Unfolds the card and waits for it to finish growing, so the pointer
    // lands on what is drawn rather than on a card still opening.
    function unfold() {
      card.unfold()
      tryVerify(function() { return card.height === card.unfoldedHeight && card.height > theme.rowHeight })
      waitForRendering(card)
    }
    function init() {
      failOnWarning(/.?/)
      store.ready = true
      store.calls = []
      store.status = online
      store.current = forecast
      store.hours = hours
      store.days = days
      store.place = { name: "Berlin", detail: "Timezone city · Europe/Berlin", chosen: false, city: "Berlin" }
      store.search = { query: "", busy: false, error: "", results: [] }
      card.reset(false)
      wait(card.theme.durationNormal + 40)
    }

    function test_the_folded_line_is_one_quiet_row() {
      compare(card.visible, true)
      compare(card.height, theme.rowHeight)
      var headline = child(card, "weatherHeadline")
      verify(shows(headline, "15°"))
      verify(shows(headline, "Overcast · Berlin"))
      compare(child(card, "weatherBadge").text, "18° / 14°")
      compare(child(card, "weatherHours").visible, false, "the forecast waits until it is asked for")
      store.ready = false
      compare(card.visible, false, "a restarting worker takes the line away rather than drawing a dead one")
      compare(card.implicitHeight, 0)
    }

    function test_a_forecast_it_cannot_vouch_for_says_so() {
      store.status = { state: "offline", fetching: false, stale: true, label: "Offline · forecast from 14:05", badge: "Offline", error: "" }
      var badge = child(card, "weatherBadge")
      compare(badge.text, "Offline")
      compare(badge.color, theme.yellow)
      card.unfold()
      compare(child(card, "weatherStatus").text, "Open-Meteo · Offline · forecast from 14:05")
      compare(child(card, "weatherStatus").color, theme.yellow)
      compare(child(card, "weatherHours").visible, true, "the last forecast stays on screen")
    }

    function test_the_line_unfolds_in_place_with_the_pointer_and_the_keyboard() {
      var headline = child(card, "weatherHeadline")
      mouseClick(headline)
      compare(card.expanded, true)
      tryVerify(function() { return card.height === card.unfoldedHeight })
      verify(card.height > theme.rowHeight * 6 && card.height <= card.available)
      compare(child(card, "weatherFacts").visible, true)
      var hourStrip = child(card, "weatherHours")
      compare(hourStrip.visible, true)
      verify(shows(hourStrip, "Now") && shows(hourStrip, "80%"))
      var week = child(card, "weatherDays")
      verify(shows(week, "Today") && shows(week, "Thu") && shows(week, "34%"))
      compare(child(card, "weatherUseCity").visible, false, "the timezone city is already in use")
      verify(headline.activeFocus)
      keyClick(Qt.Key_Escape)
      compare(card.expanded, false)
      keyClick(Qt.Key_Space)
      compare(card.expanded, true)
      keyClick(Qt.Key_Return)
      compare(card.expanded, false)
      tryCompare(card, "height", theme.rowHeight)
    }

    function test_an_unfolded_card_never_outgrows_the_popup() {
      card.available = 260
      card.unfold()
      tryCompare(card, "height", 260)
      card.available = 600
    }

    function test_a_place_is_searched_for_and_chosen_by_its_id() {
      unfold()
      mouseClick(child(card, "weatherSearchPlace"))
      compare(card.searching, true)
      var field = child(card, "weatherPlaceField")
      tryVerify(function() { return field.activeFocus })
      keyClick(Qt.Key_H)
      keyClick(Qt.Key_A)
      compare(store.calls, ["find:h", "find:ha"])
      store.search = { query: "ha", busy: true, error: "", results: [] }
      verify(shows(child(card, "weatherSearchState"), "Searching…"))
      store.search = { query: "ha", busy: false, error: "", results: [
        { id: 2911298, name: "Hamburg", detail: "Free and Hanseatic City of Hamburg, Germany" },
        { id: 2950159, name: "Hanover", detail: "Lower Saxony, Germany" }
      ] }
      compare(child(card, "weatherSearchState").visible, false)
      compare(child(card, "weatherHours").visible, false, "the search is a step of its own")
      keyClick(Qt.Key_Down)
      verify(child(card, "weatherResult_2911298").activeFocus)
      keyClick(Qt.Key_Down)
      keyClick(Qt.Key_Return)
      compare(store.calls.slice(-2), ["choose:2950159", "find:"])
      compare(card.searching, false)
      compare(card.expanded, true, "the forecast for the new place follows in the same card")
    }

    function test_enter_takes_the_first_place_and_escape_steps_back() {
      card.unfold()
      card.openSearch()
      store.search = { query: "Hamb", busy: false, error: "", results: [{ id: 2911298, name: "Hamburg", detail: "Germany" }] }
      var field = child(card, "weatherPlaceField")
      tryVerify(function() { return field.activeFocus })
      keyClick(Qt.Key_Return)
      compare(store.calls, ["choose:2911298", "find:"])
      card.openSearch()
      tryVerify(function() { return field.activeFocus })
      keyClick(Qt.Key_Escape)
      compare(card.searching, false, "Escape leaves the search first")
      compare(card.expanded, true)
      keyClick(Qt.Key_Escape)
      compare(card.expanded, false, "and then the card")
    }

    function test_searching_says_what_it_found_or_why_not() {
      card.unfold()
      card.openSearch()
      var state = child(card, "weatherSearchState")
      store.search = { query: "Xq", busy: false, error: "", results: [] }
      verify(shows(state, "No places found"))
      store.search = { query: "Xq", busy: false, error: "Place search needs a connection.", results: [] }
      verify(shows(state, "Place search needs a connection."))
      store.search = { query: "X", busy: false, error: "", results: [] }
      compare(state.visible, false, "a single letter is not a search yet")
    }

    function test_a_chosen_place_offers_the_timezone_city_back() {
      store.place = { name: "Hamburg", detail: "Germany", chosen: true, city: "Berlin" }
      unfold()
      var useCity = child(card, "weatherUseCity")
      compare(useCity.visible, true)
      mouseClick(useCity)
      compare(store.calls, ["reset"])
    }

    function test_without_a_place_the_card_opens_on_the_search() {
      store.place = { name: "", detail: "", chosen: false, city: "" }
      store.status = { state: "no-place", fetching: false, stale: true, label: "Choose a place for the forecast", badge: "", error: "" }
      store.current = null
      store.hours = []
      store.days = []
      verify(shows(child(card, "weatherHeadline"), "Weather · choose a place"))
      mouseClick(child(card, "weatherHeadline"))
      compare(card.placeStep, true)
      tryVerify(function() { return child(card, "weatherPlaceField").activeFocus })
      compare(child(card, "weatherEmpty").visible, false)
      keyClick(Qt.Key_Escape)
      compare(card.expanded, false, "with nothing to step back to, Escape folds the card")
    }

    function test_no_forecast_offline_offers_a_retry() {
      store.status = { state: "offline", fetching: false, stale: true, label: "Offline", badge: "", error: "" }
      store.current = null
      store.hours = []
      store.days = []
      verify(shows(child(card, "weatherHeadline"), "Weather unavailable offline"))
      compare(child(card, "weatherBadge").visible, false)
      card.unfold()
      var empty = child(card, "weatherEmpty")
      compare(empty.visible, true)
      compare(empty.title, "Weather unavailable offline")
      compare(child(card, "weatherHours").visible, false)
      var retry = child(card, "weatherRetry")
      compare(retry.visible, true)
      unfold()
      mouseClick(retry)
      compare(store.calls, ["refresh"])
    }

    function test_the_first_fetch_is_a_spinner_not_an_error() {
      store.status = { state: "connecting", fetching: true, stale: true, label: "Loading the forecast…", badge: "", error: "" }
      store.current = null
      verify(shows(child(card, "weatherHeadline"), "Weather · Loading the forecast…"))
      card.unfold()
      compare(child(card, "weatherEmpty").title, "Loading the forecast…")
    }
  }
}
