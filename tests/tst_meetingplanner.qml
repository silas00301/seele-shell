import QtQuick
import QtTest
import "production" as Production
import "production/shared" as Shared

TestCase {
  id: test
  name: "MeetingPlanner"
  when: windowShown
  width: 480; height: 720
  visible: true
  Shared.Theme { id: theme }
  // Tuesday 2026-09-29 in a UTC+2 zone: the axis starts 22:00 UTC the day before.
  readonly property int dayStart: 1790632800
  readonly property int hour: 3600
  QtObject {
    id: calendar
    property bool configured: true
    property bool connected: true
    property bool stale: false
    property var blocks: []
    function hasDay(date) { return true }
    function busy(from, to) { return blocks.filter(block => block.end > from && block.start < to) }
  }
  Production.MeetingPlanner {
    id: planner
    theme: theme
    calendar: calendar
    width: test.width
    maximumHeight: test.height
  }
  SignalSpy { id: requests; target: planner; signalName: "requested" }
  SignalSpy { id: copies; target: planner; signalName: "copyRequested" }
  SignalSpy { id: opens; target: planner; signalName: "openRequested" }
  SignalSpy { id: closes; target: planner; signalName: "closeRequested" }
  SignalSpy { id: pins; target: planner; signalName: "managePins" }

  function cells() {
    var result = []
    for (var hour = 0; hour < 24; hour++)
      result.push({from: hour * 60, to: hour * 60 + 60, label: (hour < 10 ? "0" : "") + hour, day: "",
        kind: hour >= 9 && hour < 17 ? "work" : hour === 8 || hour === 17 ? "edge" : "off"})
    return result
  }
  function zone(id, label, extra) {
    return Object.assign({id: id, label: label, home: false, range: "10:00–11:00", caption: "Tue 29 Sep · EDT UTC-4",
      fit: "work", note: "", cells: cells()}, extra || {})
  }
  function plan(extra) {
    return Object.assign({start: dayStart + 16 * hour, end: dayStart + 17 * hour, duration: 60, now: dayStart - 86400,
      day: {date: "2026-09-29", label: "Tue 29 Sep 2026", start: dayStart, end: dayStart + 24 * hour, minutes: 1440, today: false, past: false},
      range: "16:00–17:00", zone: "CEST UTC+2", utc: "14:00–15:00 UTC", fit: "edge", status: "Early in Los Angeles",
      conflicts: [], rows: [zone("Europe/Berlin", "Berlin", {home: true}), zone("America/New_York", "New York")],
      suggestions: [{start: dayStart + 15 * hour, range: "15:00–16:00", fit: "work", busy: false, caption: "Everyone"},
        {start: dayStart + 17 * hour, range: "17:00–18:00", fit: "edge", busy: false, caption: "Late in Berlin"}],
      next: {start: dayStart + 39 * hour, label: "Wed 30 Sep 15:00", fit: "work"},
      summary: "Tue 29 Sep 2026 · 1 h\n- Berlin: 16:00–17:00 CEST UTC+2", calendarUrl: "https://calendar.google.com/calendar/render?action=TEMPLATE"}, extra || {})
  }
  function last() { return requests.signalArguments[requests.count - 1][0] }

  function init() {
    calendar.configured = true
    calendar.blocks = []
    planner.pending = false
    planner.copyPending = false
    planner.error = ""
    planner.plan = plan()
    requests.clear(); copies.clear(); opens.clear(); closes.clear(); pins.clear()
    planner.forceActiveFocus()
  }

  function test_keyboard_moves_the_start_on_the_quarter_hour_grid() {
    keyClick(Qt.Key_Right)
    compare(last().start, dayStart + 16 * hour + 900)
    compare(last().duration, 60)
    keyClick(Qt.Key_Right, Qt.ShiftModifier)
    compare(last().start, dayStart + 17 * hour + 900, "Shift moves an hour from where the selection already is")
    keyClick(Qt.Key_H)
    compare(last().start, dayStart + 17 * hour)
    keyClick(Qt.Key_L, Qt.ShiftModifier)
    compare(last().start, dayStart + 18 * hour)
    keyClick(Qt.Key_Escape)
    compare(closes.count, 1)
  }

  function test_the_last_quarter_hour_steps_into_the_next_day() {
    planner.plan = plan({start: dayStart + 24 * hour - 900})
    keyClick(Qt.Key_Right)
    compare(last().start, dayStart + 24 * hour, "no clamp at midnight; the worker opens the next day")
    planner.plan = plan({start: dayStart})
    keyClick(Qt.Key_Left)
    compare(last().start, dayStart - 900)
  }

  function test_day_moves_add_up_while_one_is_pending() {
    keyClick(Qt.Key_PageDown)
    compare(last().start, planner.plan.start)
    compare(last().days, 1)
    planner.pending = true
    keyClick(Qt.Key_PageDown)
    compare(last().days, 2, "two presses before the reply move two days")
    compare(last().start, planner.plan.start)
    planner.pending = false
    planner.plan = plan({start: dayStart + 64 * hour})
    keyClick(Qt.Key_PageUp)
    compare(last().start, dayStart + 64 * hour)
    compare(last().days, -1, "a reply starts the next move afresh")
  }

  function test_now_next_fit_suggestions_and_lengths() {
    keyClick(Qt.Key_N)
    compare(last().start, undefined)
    compare(last().duration, 60)
    keyClick(Qt.Key_F)
    compare(last().start, planner.plan.next.start)
    keyClick(Qt.Key_2)
    compare(last().start, dayStart + 17 * hour)
    var count = requests.count
    keyClick(Qt.Key_3)
    compare(requests.count, count, "a missing suggestion does nothing")
    keyClick(Qt.Key_Minus)
    compare(last().duration, 45)
    keyClick(Qt.Key_Equal)
    keyClick(Qt.Key_Equal)
    compare(last().duration, 90)
    mouseClick(findChild(planner, "meetingDuration120"))
    compare(last().duration, 120)
    compare(last().start, dayStart + 17 * hour)
  }

  function test_copy_and_google_calendar_follow_the_answered_plan() {
    var copy = findChild(planner, "meetingCopy")
    var open = findChild(planner, "meetingCalendarOpen")
    verify(copy.enabled && open.visible && open.enabled)
    keyClick(Qt.Key_C, Qt.ControlModifier)
    compare(copies.count, 1)
    compare(copies.signalArguments[0][0], planner.plan.summary)
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(opens.count, 1)
    compare(opens.signalArguments[0][0], planner.plan.calendarUrl)
    planner.pending = true
    verify(!copy.enabled && !open.enabled)
    keyClick(Qt.Key_C, Qt.ControlModifier)
    compare(copies.count, 1, "an unanswered selection cannot be copied")
    planner.pending = false
    planner.error = "That calendar date does not exist"
    verify(!copy.enabled)
    planner.error = ""
    planner.copyPending = true
    verify(!copy.enabled)
    planner.copyPending = false
    calendar.configured = false
    verify(!open.visible)
    keyClick(Qt.Key_Return, Qt.ControlModifier)
    compare(opens.count, 1, "without Google Calendar there is nothing to open")
  }

  function test_the_date_field_edits_iso_and_shows_the_day() {
    var field = findChild(planner, "meetingDate")
    compare(field.text, "Tue 29 Sep 2026")
    keyClick(Qt.Key_D)
    verify(field.activeFocus)
    compare(field.text, "2026-09-29")
    field.text = "2026-12-24"
    verify(field.dirty)
    verify(!findChild(planner, "meetingCopy").enabled, "an unsubmitted date cannot copy the old meeting")
    keyClick(Qt.Key_Return)
    compare(last().date, "2026-12-24")
    compare(last().start, planner.plan.start)
    verify(planner.activeFocus)
    compare(field.text, "Tue 29 Sep 2026")
    var count = requests.count
    keyClick(Qt.Key_D)
    field.text = "2027-01-01"
    keyClick(Qt.Key_Escape)
    compare(requests.count, count, "Escape leaves the field without moving")
    compare(closes.count, 0, "and without closing the planner")
    compare(field.text, "Tue 29 Sep 2026")
  }

  function test_the_pointer_centres_the_meeting_where_it_lands() {
    var grid = findChild(planner, "meetingGrid")
    verify(grid)
    // 10:30 on a 24-hour axis centres a one-hour meeting on 10:00–11:00.
    mouseClick(grid, grid.width * 10.5 / 24, 10)
    compare(last().start, dayStart + 10 * hour)
    mousePress(grid, grid.width * 12.5 / 24, 10)
    mouseMove(grid, grid.width * 14.5 / 24, 10)
    compare(last().start, dayStart + 14 * hour, "dragging moves the meeting")
    mouseRelease(grid, grid.width * 14.5 / 24, 10)
    verify(!planner.dragging)
    mouseClick(grid, 1, 10)
    compare(last().start, dayStart, "the first slot is as far left as it goes")
    mouseClick(grid, grid.width - 1, 10)
    compare(last().start, dayStart + 23 * hour, "and the pointer keeps the meeting inside the day")
  }

  function test_suggestions_are_chosen_by_click() {
    var chip = findChild(planner, "meetingSuggestion0")
    verify(chip)
    mouseClick(chip)
    compare(last().start, dayStart + 15 * hour)
    planner.plan = plan({start: dayStart + 15 * hour, suggestions: []})
    verify(!findChild(planner, "meetingSuggestion0"))
    mouseClick(findChild(planner, "meetingNextFit"))
    compare(last().start, planner.plan.next.start)
    planner.plan = plan({next: null})
    verify(!findChild(planner, "meetingNextFit").visible)
  }

  function test_busy_time_names_its_conflicts_and_hides_without_a_calendar() {
    calendar.blocks = [{start: dayStart + 16 * hour, end: dayStart + 17 * hour, title: "Design review", color: "#33b679"},
      {start: dayStart + 9 * hour, end: dayStart + 10 * hour, title: "Standup", color: "#7986cb"}]
    planner.plan = plan({conflicts: [[dayStart + 16 * hour, dayStart + 17 * hour]]})
    var conflict = findChild(planner, "meetingConflict")
    verify(conflict.visible)
    compare(conflict.text, "󰃰  Overlaps Design review")
    var row = findChild(planner, "meetingCalendar")
    verify(row.visible)
    compare(row.range, "Busy")
    planner.plan = plan()
    verify(!conflict.visible)
    compare(row.range, "Free")
    calendar.configured = false
    verify(!row.visible)
    compare(planner.busyBlocks.length, 0)
  }

  function test_rows_scroll_inside_the_output_bound() {
    var rows = []
    for (var i = 0; i < 30; i++) rows.push(zone("UTC", "Zone " + i))
    planner.plan = plan({rows: rows})
    wait(50)
    verify(planner.implicitHeight <= planner.maximumHeight)
    var list = findChild(planner, "meetingZones")
    verify(list.contentHeight > list.height)
    list.contentY = list.contentHeight - list.height
    verify(list.contentY > 0)
  }
}
