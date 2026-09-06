function clean(value) {
  if (value === undefined || value === null) return ""
  return String(value).trim()
}

function listText(value) {
  if (value === undefined || value === null) return ""
  if (typeof value === "string") return clean(value)
  if (typeof value.length === "number") {
    var values = []
    for (var i = 0; i < value.length; i++) {
      var item = clean(value[i])
      if (item !== "") values.push(item)
    }
    return values.join(", ")
  }
  return clean(value)
}

function metadata(player, key) {
  return player && player.metadata ? player.metadata[key] : null
}

function isSpotify(player) {
  if (!player) return false
  var identity = [player.identity, player.desktopEntry, player.dbusName].map(clean).join(" ").toLowerCase()
  return identity.indexOf("spotify") >= 0
}

function title(player) {
  if (!player) return ""
  return clean(player.trackTitle) || clean(metadata(player, "xesam:title"))
}

function artist(player) {
  if (!player) return ""
  return clean(player.trackArtist)
    || listText(metadata(player, "xesam:artist"))
    || clean(player.trackAlbumArtist)
    || listText(metadata(player, "xesam:albumArtist"))
}

function album(player) {
  if (!player) return ""
  return clean(player.trackAlbum) || clean(metadata(player, "xesam:album"))
}

// Podcast episodes carry an empty xesam:artist and name their show in xesam:album.
function subtitle(player) {
  return artist(player) || album(player)
}

function label(player) {
  var trackTitle = title(player)
  var trackSubtitle = subtitle(player)
  if (trackTitle !== "" && trackSubtitle !== "") return trackTitle + " · " + trackSubtitle
  return trackTitle || trackSubtitle
}

function playerName(player) {
  if (!player) return ""
  var name = clean(player.identity) || clean(player.desktopEntry)
  if (name !== "") return name

  var dbusName = clean(player.dbusName).replace(/^org\.mpris\.MediaPlayer2\./, "")
  var segment = dbusName.split(".")[0].replace(/[-_]+/g, " ").trim()
  return segment === "" ? "Media player" : segment.charAt(0).toUpperCase() + segment.slice(1)
}

function lengthSeconds(player) {
  if (!player) return 0
  var direct = Number(player.length)
  if (isFinite(direct) && direct > 0) return direct
  var raw = Number(metadata(player, "mpris:length"))
  return isFinite(raw) && raw > 0 ? raw / 1000000 : 0
}

// Firefox exposes a live Twitch stream with the signed 64-bit duration sentinel.
// A one-year floor is well beyond ordinary media while remaining below that value.
function liveStream(player) {
  return lengthSeconds(player) >= 365 * 24 * 60 * 60
}

function timelineAvailable(player) {
  if (!player || lengthSeconds(player) <= 0) return false
  if (liveStream(player)) return true
  return !!player.canSeek && !!player.positionSupported && !!player.lengthSupported
}

// Chromium-embedded players append " • <album>" to the title, so compare the leading segment only.
function titleKey(player) {
  return title(player).split(/\s+[•·—–|]\s+/)[0].trim().toLowerCase().replace(/\s+/g, " ")
}

function sameTrack(left, right) {
  if (!left || !right) return false

  var leftArtist = artist(left).toLowerCase()
  var rightArtist = artist(right).toLowerCase()
  if (leftArtist !== "" && rightArtist !== "" && leftArtist !== rightArtist) return false

  var leftLength = lengthSeconds(left)
  var rightLength = lengthSeconds(right)
  if (leftLength > 0 && rightLength > 0) return Math.abs(leftLength - rightLength) <= 1

  var leftTitle = titleKey(left)
  return leftTitle !== "" && leftTitle === titleKey(right)
}

function spotifyPlayer(players) {
  players = players || []
  for (var i = 0; i < players.length; i++) {
    if (players[i].isPlaying && isSpotify(players[i])) return players[i]
  }
  return null
}

function devicePlayer(players) {
  players = players || []
  var spotify = spotifyPlayer(players)
  for (var i = 0; i < players.length; i++) {
    if (!players[i].isPlaying || isSpotify(players[i])) continue
    if (spotify && sameTrack(players[i], spotify)) continue
    return players[i]
  }
  return null
}

// MPRIS may expose one track through both its native player and an embedded
// Chromium service. Keep that mirror out of the picker just as the menu bar
// keeps it out of the device slot, while retaining every distinct resumable
// player rather than only the first one currently playing.
function availablePlayers(players) {
  players = players || []
  var spotify = null
  for (var i = 0; i < players.length; i++) {
    if (isSpotify(players[i]) && (title(players[i]) !== "" || subtitle(players[i]) !== "")) {
      spotify = players[i]
      if (players[i].isPlaying) break
    }
  }

  var available = []
  for (var j = 0; j < players.length; j++) {
    var player = players[j]
    if (title(player) === "" && subtitle(player) === "") continue
    if (spotify && player !== spotify && !isSpotify(player) && sameTrack(player, spotify)) continue
    available.push(player)
  }
  return available
}

// The Control Center carries a single now-playing module where the bar keeps
// Spotify and the device player apart, so it falls back to whichever player
// still has a track to resume once nothing is playing.
function activePlayer(players) {
  players = players || []
  var playing = spotifyPlayer(players) || devicePlayer(players)
  if (playing) return playing
  for (var i = 0; i < players.length; i++) {
    if (title(players[i]) !== "" || subtitle(players[i]) !== "") return players[i]
  }
  return null
}

function selectedPlayer(players, selected) {
  players = availablePlayers(players)
  var present = presentPlayer(players, selected)
  return present || activePlayer(players)
}

// The bar holds an entry through a pause, and a client that quits takes its
// player object with it, so a held reference is only worth showing while the
// player it names is still on the bus.
function presentPlayer(players, player) {
  players = players || []
  if (!player) return null
  for (var i = 0; i < players.length; i++) {
    if (players[i] === player) return player
  }
  return null
}
