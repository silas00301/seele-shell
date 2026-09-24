import QtQuick
import QtTest
import "production" as Production
import "production/shared" as Shared

TestCase {
  id: test
  name: "MeetingPlanner"
  when: windowShown
  width: 480; height: 650
  visible: true
  Shared.Theme { id: theme }
  Production.MeetingPlanner {
    id: planner
    theme: theme
    width: test.width
    maximumHeight: test.height
  }
  SignalSpy { id: requests; target: planner; signalName: "requested" }
  SignalSpy { id: copies; target: planner; signalName: "copyRequested" }
  SignalSpy { id: closes; target: planner; signalName: "closeRequested" }

  function init() {
    findChild(planner, "meetingDate").focus = false
    findChild(planner, "meetingTimeline").focus = false
    planner.forceActiveFocus()
    planner.plan = {date: "2026-09-22", minute: 720, duration: 60, time: "12:00", end: "2026-09-22 13:00 UTC",
      summary: "Meeting · 2026-09-22 12:00 UTC", allWorking: true, overlap: [], rows: []}
    planner.pending = false
    planner.copyPending = false
    planner.error = ""
    requests.clear(); copies.clear(); closes.clear()
    planner.forceActiveFocus()
  }
  function test_absolute_time_keyboard() {
    keyClick(Qt.Key_Right)
    compare(requests.count, 1)
    compare(requests.signalArguments[0][0].minute, 735)
    compare(requests.signalArguments[0][0].date, "2026-09-22")
    keyClick(Qt.Key_PageDown)
    compare(requests.signalArguments[1][0].shift, 1)
    keyClick(Qt.Key_PageUp)
    compare(requests.signalArguments[2][0].shift, -1)
    keyClick(Qt.Key_Escape)
    compare(closes.count, 1)
  }
  function test_timeline_clamps_without_local_time_parsing() {
    planner.choose(-15, 60)
    compare(requests.signalArguments[0][0].minute, 0)
    planner.choose(1500, 90)
    compare(requests.signalArguments[1][0].minute, 1439)
    compare(requests.signalArguments[1][0].duration, 90)
    var timeline = findChild(planner, "meetingTimeline")
    verify(timeline)
    timeline.forceActiveFocus()
    keyClick(Qt.Key_Left)
    compare(requests.signalArguments[2][0].minute, 1424)
  }
  function test_date_validation_belongs_to_worker() {
    var field = findChild(planner, "meetingDate")
    field.forceActiveFocus()
    field.text = "2026-02-30"
    keyClick(Qt.Key_Return)
    compare(requests.count, 1)
    compare(requests.signalArguments[0][0].date, "2026-02-30")
    planner.error = "That calendar date does not exist"
    planner.copy()
    compare(copies.count, 0)
  }
  function test_unsubmitted_date_cannot_copy_old_meeting() {
    var field = findChild(planner, "meetingDate")
    field.forceActiveFocus()
    field.text = "2026-10-25"
    verify(planner.dateDirty)
    verify(!findChild(planner, "meetingCopy").enabled)
    planner.copy()
    compare(copies.count, 0)
  }
  function test_now_and_copy_buttons_use_current_projection() {
    var copy = findChild(planner, "meetingCopy")
    verify(copy.enabled)
    mouseClick(copy)
    compare(copies.count, 1)
    compare(copies.signalArguments[0][0], planner.plan.summary)
    planner.pending = true
    verify(!copy.enabled)
    keyClick(Qt.Key_C, Qt.ControlModifier)
    compare(copies.count, 1)
    planner.pending = false
    planner.copyPending = true
    verify(!copy.enabled)
    planner.copyPending = false
    var now = findChild(planner, "meetingNow")
    mouseClick(now)
    compare(requests.count, 1)
    compare(requests.signalArguments[0][0].date, "")
  }
  function test_shared_start_strip_paints_the_native_intersection() {
    var overlap = []
    for (var i = 0; i < 96; i++) overlap.push(i >= 52 && i <= 56)
    planner.plan = {date: "2026-09-22", minute: 840, duration: 60, time: "14:00", end: "15:00 UTC", rows: [], overlap: overlap}
    wait(20)
    var strip = findChild(planner, "meetingOverlap")
    verify(strip)
    compare(findChild(strip, "meetingSlot51").color, theme.wellColor)
    compare(findChild(strip, "meetingSlot52").color, theme.fillColor)
    compare(findChild(strip, "meetingSlot56").color, theme.fillColor)
    compare(findChild(strip, "meetingSlot57").color, theme.wellColor)
    verify(findChild(strip, "meetingSlot52").width > 0)
  }
  function test_rows_scroll_inside_output_bound() {
    var rows = []
    for (var i = 0; i < 30; i++) rows.push({id: "UTC", label: "Zone " + i, time: "12:00", date: "2026-09-22", abbreviation: "UTC", offset: "UTC+00:00", working: true, slots: []})
    planner.plan = {date: "2026-09-22", minute: 720, duration: 60, time: "12:00", end: "13:00 UTC", rows: rows, overlap: []}
    wait(50)
    verify(planner.implicitHeight <= planner.maximumHeight)
    var list = findChild(planner, "meetingZones")
    verify(list.contentHeight > list.height)
    list.positionViewAtEnd()
    verify(list.contentY > 0)
  }
}
