import QtQuick
import QtMultimedia

Item {
  id: playback
  property alias source: player.source
  readonly property bool playing: player.playbackState === MediaPlayer.PlayingState
  readonly property real position: player.position
  readonly property real duration: player.duration
  readonly property string error: player.errorString
  function play() { player.play() }
  function pause() { player.pause() }
  function stop() { player.stop() }
  function seek(value) { if (player.seekable) player.position = value }
  MediaPlayer { id: player; audioOutput: AudioOutput {} }
}
