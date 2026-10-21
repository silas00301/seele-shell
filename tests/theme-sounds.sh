#!/usr/bin/env bash
set -euo pipefail

# The theme's interface sounds (SIL-240): ThemeSounds' gating and cues against
# a recording stand-in for the QtMultimedia player. Nothing is played, and the
# real player is not loaded: a build has no audio device, and Qt refuses a
# sound effect without one. tests/theme-sounds.js checks the files themselves.
#
#   bash tests/theme-sounds.sh <shell sources> <Qt QML imports>
sources=${1:?usage: theme-sounds.sh SOURCES QT_IMPORT}
qt_import=${2:?usage: theme-sounds.sh SOURCES QT_IMPORT}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/gated"
cp "$sources/ThemeSounds.qml" "$work/gated/"
# The stand-in records each cue it is asked to play.
cat > "$work/gated/ThemeSoundEffects.qml" <<'QML'
import QtQuick
Item {
  required property var theme
  property var cues: []
  function play(cue) { cues = cues.concat([cue]) }
}
QML
cp "$(dirname "${BASH_SOURCE[0]}")/tst_themesounds.qml" "$work/"

QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner \
  -import "$qt_import" -input "$work/tst_themesounds.qml"
