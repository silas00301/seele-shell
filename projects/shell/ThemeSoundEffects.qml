import QtQuick
import QtMultimedia

// The theme's cues as Qt sound effects, loaded by ThemeSounds only while the
// user has sounds on and the theme carries some. Each cue is decoded once
// when its file is set, so playing one later does no file or decoder work; a
// cue the theme leaves empty has no source and plays nothing. Registration has
// already refused any cue that is not a WAV file, the one format SoundEffect
// decodes.
Item {
  id: effects

  required property var theme

  // A path escaped segment by segment into a file URL, as the wallpaper and
  // the typeface are, so a name may hold any character a file name can.
  function source(file) {
    return file === "" ? "" : "file://" + file.split("/").map(encodeURIComponent).join("/")
  }
  function play(cue) {
    var effect = ({
      notification: notification,
      notificationUrgent: notificationUrgent,
      focusDone: focusDone,
      panelOpen: panelOpen,
      panelClose: panelClose,
      levelStep: levelStep,
    })[cue]
    if (effect && effect.status === SoundEffect.Ready) effect.play()
  }

  SoundEffect { id: notification; objectName: "notification"; source: effects.source(effects.theme.soundNotification) }
  SoundEffect { id: notificationUrgent; objectName: "notificationUrgent"; source: effects.source(effects.theme.soundNotificationUrgent) }
  SoundEffect { id: focusDone; objectName: "focusDone"; source: effects.source(effects.theme.soundFocusDone) }
  SoundEffect { id: panelOpen; objectName: "panelOpen"; source: effects.source(effects.theme.soundPanelOpen) }
  SoundEffect { id: panelClose; objectName: "panelClose"; source: effects.source(effects.theme.soundPanelClose) }
  SoundEffect { id: levelStep; objectName: "levelStep"; source: effects.source(effects.theme.soundLevelStep) }
}
