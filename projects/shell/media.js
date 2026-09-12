.import "../shared/Native.js" as Bridge

// QObject properties and identity belong to Qt. Rust receives plain snapshots
// and returns selected indexes; these adapters return the original live object.
function metadata(player, key) { return player && player.metadata ? player.metadata[key] : null }
function snapshot(player, kind) {
  if (!player) return null
  var result = {}
  if (!kind || kind === "identity") {
    result.identity = player.identity
    result.desktopEntry = player.desktopEntry
    result.dbusName = player.dbusName
    result.isPlaying = player.isPlaying
  }
  var data = player.metadata || {}
  var metadata = {}
  if (!kind || kind === "track") {
    result.trackTitle = player.trackTitle
    result.trackArtist = player.trackArtist
    result.trackAlbumArtist = player.trackAlbumArtist
    result.trackAlbum = player.trackAlbum
    metadata["xesam:title"] = data["xesam:title"]
    metadata["xesam:artist"] = data["xesam:artist"]
    metadata["xesam:albumArtist"] = data["xesam:albumArtist"]
    metadata["xesam:album"] = data["xesam:album"]
  }
  if (!kind || kind === "timing") {
    result.length = Bridge.number(player.length)
    result.canSeek = player.canSeek
    result.positionSupported = player.positionSupported
    result.lengthSupported = player.lengthSupported
    metadata["mpris:length"] = Bridge.number(data["mpris:length"])
  }
  result.metadata = metadata
  return result
}

function snapshots(players) {
  var result = []
  for (var i = 0; i < (players || []).length; i++) result.push(snapshot(players[i]))
  return result
}
function identityIndex(players, player) {
  if (!player) return -1
  for (var i = 0; i < (players || []).length; i++) if (players[i] === player) return i
  return -1
}
function selected(name, players, extra) {
  players = players || []
  var index = Bridge.call("media." + name, [snapshots(players), extra])
  return index === null ? null : players[index]
}
function modes(player) {
  return player ? {canControl: player.canControl, shuffleSupported: player.shuffleSupported,
    loopSupported: player.loopSupported, shuffle: player.shuffle, loopState: player.loopState} : null
}
function clean(value) { return Bridge.call("media.clean", [value]) }
function listText(value) { return Bridge.call("media.listText", [value]) }
function isSpotify(player) { return Bridge.call("media.isSpotify", [snapshot(player, "identity")]) }
function title(player) { return Bridge.call("media.title", [snapshot(player, "track")]) }
function artist(player) { return Bridge.call("media.artist", [snapshot(player, "track")]) }
function album(player) { return Bridge.call("media.album", [snapshot(player, "track")]) }
function subtitle(player) { return Bridge.call("media.subtitle", [snapshot(player, "track")]) }
function label(player) { return Bridge.call("media.label", [snapshot(player, "track")]) }
function playerName(player) { return Bridge.call("media.playerName", [snapshot(player, "identity")]) }
function lengthSeconds(player) { return Bridge.call("media.lengthSeconds", [snapshot(player, "timing")]) }
function liveStream(player) { return Bridge.call("media.liveStream", [snapshot(player, "timing")]) }
function timelineAvailable(player) { return Bridge.call("media.timelineAvailable", [snapshot(player, "timing")]) }
function titleKey(player) { return Bridge.call("media.titleKey", [snapshot(player, "track")]) }
function sameTrack(left, right) { return Bridge.call("media.sameTrack", [snapshot(left), snapshot(right)]) }
function spotifyPlayer(players) { return selected("spotifyPlayer", players, null) }
function devicePlayer(players) { return selected("devicePlayer", players, null) }
function activePlayer(players) { return selected("activePlayer", players, null) }
function availablePlayers(players) {
  players = players || []
  return Bridge.call("media.availablePlayers", [snapshots(players)]).map(function(index) { return players[index] })
}
function selectedPlayer(players, player) { return selected("selectedPlayer", players, identityIndex(players, player)) }
function presentPlayer(players, player) { var index = identityIndex(players, player); return index < 0 ? null : players[index] }
function canShuffle(player) { return Bridge.call("media.canShuffle", [modes(player)]) }
function canRepeat(player) { return Bridge.call("media.canRepeat", [modes(player)]) }
function toggleShuffle(player) {
  var next = Bridge.call("media.nextShuffle", [modes(player)])
  if (next === null) return false
  player.shuffle = next
  return true
}
// MprisLoopState is a Qt singleton, not a JSON object. Only its enum values
// cross the native boundary; the host retains the actual player instance.
function loopStates(states) {
  return states ? {None: Number(states.None), Track: Number(states.Track), Playlist: Number(states.Playlist)} : null
}
function nextLoopState(current, states) { return Bridge.call("media.nextLoopState", [current, loopStates(states)]) }
function cycleRepeat(player, states) {
  var next = Bridge.call("media.nextRepeat", [modes(player), loopStates(states)])
  if (next === null) return false
  player.loopState = next
  return true
}
function repeatLabel(player, states) { return Bridge.call("media.repeatLabel", [modes(player), loopStates(states)]) }

function seekTarget(player, command, largeStep) {
  var value = snapshot(player, "timing")
  if (value) { value.position = Bridge.number(player.position); value.canControl = player.canControl }
  return Bridge.call("media.seekTarget", [value, command, !!largeStep])
}
