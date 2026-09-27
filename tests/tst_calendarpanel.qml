import QtQuick
import QtTest
import "shared" as Shared

// The calendar popup's agenda and settings against a stand-in for the worker's
// published sections. Every state the agenda can be in is a distinct screen
// with its own way out, and the event rows and account steps are driven the
// way a person would drive them.
Rectangle {
  width: 420; height: 900
  color: theme.mantle
  Shared.TestTheme { id: theme }

  readonly property double nine: new Date("2026-09-28T09:00:00").getTime()
  readonly property var events: [
    { key: "work:offsite", title: "Team offsite", calendar: "Work", color: "#f83a22", all_day: true, time: "All day · 27–28 Sep", when: "Sun 27–Mon 28 Sep · 2 days", start: nine - 9 * 3600000, end: nine + 15 * 3600000, rsvp: "", rsvp_label: "", location: "", description: "", join: "", join_label: "", link: "https://www.google.com/calendar/event?eid=offsite" },
    { key: "me:breakfast", title: "Breakfast", calendar: "me@example.com", color: "#4986e7", all_day: false, time: "07:30–08:15", when: "07:30–08:15 · 45 min", start: nine - 90 * 60000, end: nine - 45 * 60000, rsvp: "", rsvp_label: "", location: "", description: "", join: "https://meet.google.com/old", join_label: "Google Meet", link: "" },
    { key: "me:standup", title: "Standup", calendar: "me@example.com", color: "#4986e7", all_day: false, time: "09:10–09:25", when: "09:10–09:25 · 15 min", start: nine + 10 * 60000, end: nine + 25 * 60000, rsvp: "needsAction", rsvp_label: "Not answered", location: "Room 4", description: "Daily sync\n• Blockers", join: "https://meet.google.com/abc-defg-hij", join_label: "Google Meet", link: "https://www.google.com/calendar/event?eid=standup" }
  ]

  QtObject {
    id: store
    property bool ready: true
    property var account: ({ status: "online", configured: true, account: "me@example.com", syncing: false, signing_in: false, error: "", refreshed_at: 0, updated: "08:55", stale: false })
    property var calendars: [
      { id: "me@example.com", name: "me@example.com", color: "#4986e7", role: "Primary", selected: true, loading: false },
      { id: "work", name: "Work", color: "#f83a22", role: "Shared with you", selected: false, loading: false }
    ]
    property var agenda: ({ day: "2026-09-28", covered: true, loading: false, items: [] })
    property var calls: []
    readonly property string status: ready ? account.status : "unavailable"
    readonly property bool signedIn: ["online", "offline", "connecting", "expired"].indexOf(status) >= 0
    readonly property int selectedCount: calendars.filter(function(c) { return c.selected }).length
    function record(call) { calls = calls.concat([call]) }
    function setup(id) { record("setup:" + id) }
    function forgetClient() { record("forget") }
    function signin() { record("signin") }
    function cancelSignin() { record("cancel") }
    function refresh() { record("refresh") }
    function disconnect() { record("disconnect") }
    function choose(id, selected) { record("choose:" + id + ":" + selected) }
  }

  CalendarAgenda {
    id: agenda
    y: 0
    width: 360; height: 320
    theme: theme
    store: store
    day: "2026-09-28"
    today: "2026-09-28"
    now: nine
  }

  CalendarSettings {
    id: settings
    y: 340
    width: 360; height: 520
    theme: theme
    store: store
  }

  TestCase {
    name: "CalendarPanel"
    when: windowShown

    function child(parent, name) {
      var item = findChild(parent, name)
      verify(item !== null, "Missing " + name)
      return item
    }
    function status(value) {
      store.account = Object.assign({}, store.account, { status: value, error: "" })
    }
    function init() {
      failOnWarning(/.?/)
      store.calls = []
      store.ready = true
      status("online")
      store.calendars = store.calendars.map(function(c) { return Object.assign({}, c, { selected: c.id === "me@example.com", loading: false }) })
      store.agenda = { day: "2026-09-28", covered: true, loading: false, items: events }
      agenda.day = "2026-09-28"
      agenda.expandedKey = ""
      settings.confirmDisconnect = false
      wait(20)
    }

    function test_each_state_names_itself_and_offers_its_way_out() {
      var empty = child(agenda, "calendarEmpty")
      status("setup")
      compare(empty.visible, true)
      compare(empty.title, "Connect Google Calendar")
      status("signed-out")
      compare(empty.title, "Sign in to Google Calendar")
      status("signing-in")
      compare(empty.title, "Waiting for Google…")
      status("online")
      store.calendars = store.calendars.map(function(c) { return Object.assign({}, c, { selected: false }) })
      compare(empty.title, "No calendars selected")
      store.calendars = store.calendars.map(function(c) { return Object.assign({}, c, { selected: true }) })
      store.agenda = { day: "2026-09-28", covered: true, loading: false, items: [] }
      compare(empty.title, "Nothing scheduled")
      status("offline")
      store.agenda = { day: "2026-09-28", covered: false, loading: false, items: [] }
      compare(empty.title, "Not available offline")
      compare(empty.visible, true)
    }

    function test_loading_waits_before_it_shows() {
      var empty = child(agenda, "calendarEmpty")
      store.agenda = { day: "2026-09-28", covered: false, loading: true, items: [] }
      compare(agenda.mode, "loading")
      compare(empty.visible, false, "a quick answer never flashes a loading state")
      tryCompare(empty, "visible", true, 1000)
      compare(empty.title, "Loading events")
      // An answer for another day is not this day's agenda.
      store.agenda = { day: "2026-09-29", covered: true, loading: false, items: events }
      compare(agenda.mode, "loading")
    }

    function test_rows_unfold_from_the_keyboard_with_their_actions() {
      var list = child(agenda, "calendarEvents")
      compare(list.visible, true)
      compare(list.count, 3)
      var standup = child(agenda, "calendarEvent_me:standup")
      standup.forceActiveFocus()
      verify(standup.activeFocus)
      keyClick(Qt.Key_Return)
      compare(agenda.expandedKey, "me:standup")
      tryVerify(function() { return findChild(standup, "calendarJoin") !== null })
      compare(child(standup, "calendarJoin").text, "Join Google Meet")
      compare(child(standup, "calendarJoin").selected, true, "a meeting ten minutes away leads with Join")
      compare(child(standup, "calendarOpen").visible, true)
      keyClick(Qt.Key_Space)
      compare(agenda.expandedKey, "")
    }

    function test_join_is_offered_only_before_a_meeting_ends() {
      compare(child(child(agenda, "calendarEvent_me:standup"), "calendarQuickJoin").visible, true)
      compare(child(child(agenda, "calendarEvent_me:breakfast"), "calendarQuickJoin").visible, false)
    }

    function test_reveal_unfolds_an_event_once_its_day_arrives() {
      store.agenda = { day: "2026-09-27", covered: true, loading: false, items: [] }
      agenda.day = "2026-09-29"
      agenda.reveal("me:standup")
      store.agenda = { day: "2026-09-29", covered: true, loading: false, items: events }
      compare(agenda.expandedKey, "me:standup")
      tryCompare(agenda, "pendingReveal", "")
    }

    function test_expired_sign_in_keeps_saved_events_and_offers_sign_in() {
      status("expired")
      compare(child(agenda, "calendarEvents").visible, true)
      compare(agenda.detail, "Offline · synced 08:55")
    }

    function test_setup_saves_a_trimmed_client_id() {
      status("setup")
      compare(child(settings, "calendarSetup").visible, true)
      compare(child(settings, "calendarAccount").visible, false)
      var save = child(settings, "calendarSaveClient")
      compare(save.enabled, false)
      var field = child(settings, "calendarClientId")
      field.text = "  1-abc.apps.googleusercontent.com  "
      compare(save.enabled, true)
      waitForRendering(settings)
      mouseClick(save)
      compare(store.calls, ["setup:1-abc.apps.googleusercontent.com"])
    }

    function test_signed_out_offers_sign_in_or_another_client() {
      status("signed-out")
      compare(child(settings, "calendarSignin").visible, true)
      compare(child(settings, "calendarSetup").visible, false)
    }

    function test_a_calendar_row_toggles_from_anywhere_on_it() {
      compare(child(settings, "calendarAccount").visible, true)
      var work = child(settings, "calendarChoice_work")
      mouseClick(work, 40, work.height / 2)
      compare(store.calls, ["choose:work:true"])
      work.forceActiveFocus()
      keyClick(Qt.Key_Space)
      compare(store.calls, ["choose:work:true", "choose:work:true"])
    }

    function test_disconnect_asks_once_more() {
      mouseClick(child(settings, "calendarDisconnect"))
      compare(store.calls, [])
      compare(settings.confirmDisconnect, true)
      mouseClick(child(settings, "calendarConfirmDisconnect"))
      compare(store.calls, ["disconnect"])
      compare(settings.confirmDisconnect, false)
    }
  }
}
