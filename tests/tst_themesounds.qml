import QtQuick
import QtTest
import "gated" as Gated

// The theme's interface sounds (SIL-240). ThemeSounds loads no player at all
// until the user has asked for sounds and the theme carries some, holds every
// cue back while presenting, and turns a panel appearing or going away into
// its cue.
//
//   bash tests/theme-sounds.sh <shell sources> <Qt QML imports>
TestCase {
  id: testCase
  name: "ThemeSounds"
  when: windowShown

  // Theme.qml's sound slots and the two preferences that gate them.
  QtObject {
    id: theme
    property bool soundEnabled: false
    property string soundNotification: ""
    property string soundNotificationUrgent: ""
    property string soundFocusDone: ""
    property string soundPanelOpen: ""
    property string soundPanelClose: ""
    property string soundLevelStep: ""
    readonly property bool themeSounds: [soundNotification, soundNotificationUrgent, soundFocusDone,
      soundPanelOpen, soundPanelClose, soundLevelStep].some(function(file) { return file !== "" })
  }
  Gated.ThemeSounds { id: gated; theme: theme }

  function player() { return findChild(gated, "player") }
  function cues() { return player().item ? player().item.cues : [] }
  function carry() {
    theme.soundNotification = "/themes/cyberpunk/sounds/notification.wav"
    theme.soundPanelOpen = "/themes/cyberpunk/sounds/panel-open.wav"
  }
  function init() { failOnWarning(/.?/) }
  function cleanup() {
    gated.quiet = false
    gated.panel = ""
    gated.switcher = false
    theme.soundEnabled = false
    for (var slot of ["soundNotification", "soundNotificationUrgent", "soundFocusDone", "soundPanelOpen", "soundPanelClose", "soundLevelStep"])
      theme[slot] = ""
  }
  function enabled() {
    carry()
    theme.soundEnabled = true
    tryVerify(function() { return player().status === Loader.Ready }, 2000, "the player loads")
    player().item.cues = []
  }

  function test_silent_by_default_and_for_a_theme_without_sounds() {
    compare(player().status, Loader.Null, "nothing is loaded until sound is asked for")
    gated.play("notification")
    carry()
    wait(50)
    compare(player().status, Loader.Null, "a theme's sounds alone load nothing")
    theme.soundNotification = ""
    theme.soundPanelOpen = ""
    theme.soundEnabled = true
    wait(50)
    compare(player().status, Loader.Null, "nor does asking, for a theme without sounds")
  }

  function test_cues_play_once_asked_for() {
    enabled()
    gated.play("notification")
    gated.play("levelStep")
    compare(cues(), ["notification", "levelStep"])
    // Turning sound off unloads the player again.
    theme.soundEnabled = false
    tryVerify(function() { return player().status === Loader.Null }, 1000, "the player goes away")
    gated.play("notification")
  }

  function test_presenting_holds_every_cue_back() {
    enabled()
    gated.quiet = true
    gated.play("notification")
    gated.panel = "audio"
    compare(cues(), [])
    gated.quiet = false
    gated.play("notification")
    compare(cues(), ["notification"])
  }

  function test_panels_appearing_and_going_away() {
    enabled()
    gated.panel = "control-center"
    wait(0)
    gated.panel = "audio"
    wait(0)
    gated.panel = ""
    wait(0)
    gated.switcher = true
    gated.switcher = false
    compare(cues(), ["panelOpen", "panelOpen", "panelClose", "panelOpen", "panelClose"],
      "a panel taking another's place appears; closing the last one goes away")
  }

  // The shell swaps one panel for another by closing everything first and
  // then opening the next, in one turn: that is one panel appearing.
  function test_a_swap_through_no_panel_is_one_cue() {
    enabled()
    gated.panel = "audio"
    wait(0)
    player().item.cues = []
    gated.panel = ""
    gated.panel = "media"
    wait(0)
    compare(cues(), ["panelOpen"], "a swap plays the open cue alone")
    gated.panel = ""
    gated.panel = "media"
    wait(0)
    compare(cues(), ["panelOpen"], "closing and reopening the same panel in one turn plays nothing")
    gated.panel = ""
    wait(0)
    compare(cues(), ["panelOpen", "panelClose"])
  }

  function test_only_a_new_toast_sounds_and_only_if_its_sender_allows() {
    enabled()
    gated.arrived({ urgency: 1 }, true)
    gated.arrived({ urgency: 2 }, true)
    gated.arrived({ urgency: 1 }, false)
    gated.arrived({ urgency: 2, silent: true }, true)
    gated.arrived(null, true)
    compare(cues(), ["notification", "notificationUrgent"],
      "a reminder or a pin coming back, and a sender asking for silence, get no cue")
  }
}
