import QtQuick
import Quickshell
import Quickshell.Services.Mpris
import "media.js" as Media

ShellRoot {
  QtObject {
    id: player
    property bool canControl: true
    property bool loopSupported: true
    property int loopState: MprisLoopState.None
  }
  function check(value, message) { if (!value) throw new Error(message) }
  Timer {
    interval: 1; running: true
    onTriggered: {
      try {
        check(Media.repeatLabel(player,MprisLoopState)==="Repeat off", "initial enum label")
        check(Media.cycleRepeat(player,MprisLoopState), "first cycle")
        check(player.loopState===MprisLoopState.Playlist, "playlist write on original QObject")
        check(Media.repeatLabel(player,MprisLoopState)==="Repeat playlist", "playlist label")
        check(Media.cycleRepeat(player,MprisLoopState), "second cycle")
        check(player.loopState===MprisLoopState.Track, "track write")
        check(Media.repeatLabel(player,MprisLoopState)==="Repeat one track", "track label")
        check(Media.cycleRepeat(player,MprisLoopState), "third cycle")
        check(player.loopState===MprisLoopState.None, "off write")
        check(Media.nextLoopState(MprisLoopState.None,MprisLoopState)===MprisLoopState.Playlist, "enum projection")
        player.canControl=false
        check(!Media.cycleRepeat(player,MprisLoopState), "read-only player")
        check(player.loopState===MprisLoopState.None, "read-only identity retained")
        player.loopSupported=false
        check(Media.repeatLabel(player,MprisLoopState)==="Repeat unavailable", "unsupported player")
        check(Media.repeatLabel(null,MprisLoopState)==="Repeat unavailable", "missing player")
        console.log("MEDIA_HOST_PASS")
      } catch(error) { console.error("MEDIA_HOST_FAIL: " + error) }
      Qt.quit()
    }
  }
}
