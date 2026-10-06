import QtQuick
import QtTest
import "production" as Production
import "production/shared" as Shared
TestCase {
  name: "FocusProgress"
  width: 420; height: 800; visible: true; when: windowShown
  Shared.Theme { id: theme }
  QtObject {
    id: timer
    property var timerState: ({status: "running", duration: 1500, remaining: 1200, deadline: 2000000})
    property string label: "20:00"
    property bool canExtend: true
    property string extensionHint: ""
    function command(action, minutes) {}
  }
  Production.FocusPanel { id: panel; width: 390; theme: theme; timer: timer }
  function test_progress_caption_pause_done() {
    var caption = findChild(panel, "focusProgressCaption")
    verify(caption !== null)
    compare(panel.progress.ratio, 0.2)
    verify(caption.text.indexOf("05:00 elapsed · Ends at ") === 0)
    timer.timerState = ({status: "paused", duration: 1500, remaining: 1200, deadline: 0})
    compare(caption.text, "05:00 elapsed · Paused")
    timer.timerState = ({status: "done", duration: 1500, remaining: 0, deadline: 0})
    compare(panel.progress.ratio, 1)
    compare(caption.text, "25:00 elapsed")
    timer.timerState = ({status: "idle", duration: 1500, remaining: 1500, deadline: 0})
    compare(caption.visible, false)
  }
}
