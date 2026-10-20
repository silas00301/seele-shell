.pragma library
// One device player mid-track, without artwork, so nothing outside the
// repository reaches a pixel.
var player = {
  identity: "mpv", desktopEntry: "mpv", dbusName: "org.mpris.MediaPlayer2.mpv",
  isPlaying: true, playbackState: 1, canControl: true, canPlay: true, canPause: true, canTogglePlaying: true,
  canGoNext: true, canGoPrevious: true, canSeek: true,
  trackTitle: "Komm, süsser Tod", trackArtist: "Arianne", trackAlbumArtist: "Arianne",
  trackAlbum: "The End of Evangelion", trackArtUrl: "",
  length: 459, position: 172, positionSupported: true, lengthSupported: true,
  volume: 0.8, volumeSupported: true, canSetVolume: true,
  rate: 1, minRate: 1, maxRate: 1,
  loopState: 0, loopSupported: true, shuffle: false, shuffleSupported: true,
  metadata: {
    "xesam:title": "Komm, süsser Tod", "xesam:artist": ["Arianne"],
    "xesam:albumArtist": ["Arianne"], "xesam:album": "The End of Evangelion",
    "mpris:length": 459000000
  },
  togglePlaying: function() {}, play: function() {}, pause: function() {},
  next: function() {}, previous: function() {}
}
var players = { values: [player] }
