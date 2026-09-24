import QtQuick
import QtTest

TestCase {
  id: testCase
  name: "UriOverlay"
  when: windowShown
  width: 1000; height: 600
  visible: true
  Overlay { id: overlay }
  SignalSpy { id: activated; target: overlay; signalName: "activated" }
  readonly property string destination: "https://example.org/docs/chapter?view=full&page=2"
  function init() {
    overlay.links.clear()
    overlay.digits = ""
    overlay.links.append({number:1,uri:destination,text:destination,code:false,output:"DP-1",
      x0:0.12,y0:0.2,w:0.5,h:0.04,
      regions:[{x0:0.12,y0:0.2,w:0.5,h:0.04},{x0:0.12,y0:0.27,w:0.35,h:0.04}]})
    wait(30)
    activated.clear()
  }
  function test_continuation_click_hover_and_clear() {
    var first = findChild(overlay,"uriRegion_1_0")
    var second = findChild(overlay,"uriRegion_1_1")
    verify(first !== null && second !== null)
    verify(second.y > first.y + first.height)
    mouseMove(second,second.width/2,second.height/2)
    tryCompare(overlay,"hoveredUri",destination)
    mouseClick(second,second.width/2,second.height/2,Qt.LeftButton,Qt.ControlModifier)
    compare(activated.count,1)
    compare(activated.signalArguments[0][0],destination)
    compare(activated.signalArguments[0][1],true)
    // The space between lines must not act like a large union rectangle.
    mouseClick(overlay,200,150)
    compare(activated.count,1)
    mouseClick(first,first.width/2,first.height/2)
    compare(activated.count,2)
    compare(activated.signalArguments[1][0],destination)
    overlay.links.clear()
    wait(30)
    compare(findChild(overlay,"uriRegion_1_0"),null)
  }
  function test_single_badge_and_filter() {
    var badge = findChild(overlay,"uriBadge_1")
    verify(badge !== null)
    mouseClick(badge,badge.width/2,badge.height/2)
    compare(activated.count,1)
    compare(activated.signalArguments[0][0],destination)
    overlay.digits = "2"
    compare(badge.parent.opacity,0.25)
    overlay.digits = "1"
    compare(badge.parent.opacity,1)
  }
}
