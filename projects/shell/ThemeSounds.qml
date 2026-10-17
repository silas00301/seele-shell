import QtQuick

// The theme's interface sounds: the cues a theme gives a file to (`sound…`),
// played on the events they are named for. Nothing sounds unless the user has
// asked for sounds (`seele-theme sound on`) and the published theme carries
// some; until then the player is not even loaded, so a theme without sounds,
// or a user who never wants them, costs nothing. Presentation mode holds every
// cue back, as it holds back the toasts.
//
// The player is ThemeSoundEffects.qml, loaded on its own so a QtMultimedia
// backend that will not start costs the cues and nothing else. Qt plays a
// SoundEffect as an `event` stream, which the Audio panel's mixer leaves out,
// so a cue never appears there as an application.
Item {
  id: sounds

  required property var theme
  // Held back entirely while true.
  property bool quiet: false
  // The control panel on screen ("" for none), and whether the floating theme
  // switcher is: a panel appearing or going away plays its cue.
  property string panel: ""
  property bool switcher: false
  readonly property bool active: sounds.theme.soundEnabled && sounds.theme.themeSounds

  // Plays `cue`: notification, notificationUrgent, focusDone, panelOpen,
  // panelClose or levelStep.
  function play(cue) {
    if (!sounds.active || sounds.quiet || player.status !== Loader.Ready) return
    player.item.play(cue)
  }

  // A toast was shown. A new notification plays the theme's cue, a critical
  // one its own; a toast coming back (a reminder, a pin, the end of focus) is
  // not a new arrival, and a sender that asked for silence (`suppress-sound`,
  // as the focus timer's own notification does after its cue) gets none.
  function arrived(entry, fresh) {
    if (!fresh || !entry || entry.silent) return
    sounds.play(entry.urgency === 2 ? "notificationUrgent" : "notification")
  }

  // The panel the last cue spoke for. The shell swaps one panel for another by
  // closing everything first, so a change is judged once the turn it happened
  // in is over: another panel taking the place of one is a panel appearing,
  // and a panel closed and opened again in the same turn is no change at all.
  property string shown: ""
  function settle() {
    if (sounds.panel === sounds.shown) return
    sounds.play(sounds.panel !== "" ? "panelOpen" : "panelClose")
    sounds.shown = sounds.panel
  }
  onPanelChanged: Qt.callLater(sounds.settle)
  onSwitcherChanged: sounds.play(sounds.switcher ? "panelOpen" : "panelClose")

  Loader {
    id: player
    objectName: "player"
    active: sounds.active
    asynchronous: true
    source: ""
    Component.onCompleted: setSource("ThemeSoundEffects.qml", { "theme": sounds.theme })
  }
}
