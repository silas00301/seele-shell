const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync(process.argv[2], "utf8");
const media = {};
vm.createContext(media);
vm.runInContext(source, media, { filename: process.argv[2] });

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const spotify = {
  isPlaying: true,
  identity: "Spotify",
  desktopEntry: "spotify",
  dbusName: "org.mpris.MediaPlayer2.spotify",
  trackTitle: "Fixture title",
  trackArtist: "",
  length: 0,
  metadata: { "xesam:artist": ["Fixture artist"], "xesam:title": "Fixture title" },
};
const device = {
  isPlaying: true,
  identity: "Fixture device",
  desktopEntry: "fixture-device",
  dbusName: "org.mpris.MediaPlayer2.fixture",
  trackTitle: "Device title",
  trackArtist: "Device artist",
  length: 0,
  metadata: {},
};
const spotifyMirror = {
  isPlaying: true,
  identity: "Remote device",
  desktopEntry: "remote-device",
  dbusName: "org.mpris.MediaPlayer2.remote",
  trackTitle: "Fixture title",
  trackArtist: "Fixture artist",
  trackArtUrl: "https://example.invalid/cover",
  length: 0,
  metadata: {},
};
// Spotify plays podcasts with an empty artist and mirrors itself onto an embedded Chromium
// service that renames the track to "<title> • <album>".
const spotifyPodcast = {
  isPlaying: true,
  identity: "Spotify",
  desktopEntry: "spotify",
  dbusName: "org.mpris.MediaPlayer2.spotify",
  trackTitle: "Episode title",
  trackArtist: "",
  trackAlbum: "Show name",
  length: 4157.321,
  metadata: { "xesam:artist": [""], "xesam:title": "Episode title", "xesam:album": "Show name" },
};
const spotifyChromium = {
  isPlaying: true,
  identity: "Chromium",
  desktopEntry: "",
  dbusName: "org.mpris.MediaPlayer2.chromium.instance68033",
  trackTitle: "Episode title • Show name",
  trackArtist: "",
  trackAlbum: "",
  length: 4157.321,
  metadata: { "xesam:artist": [""], "xesam:title": "Episode title • Show name" },
};

assert(media.artist(spotify) === "Fixture artist", "raw Spotify artist metadata must be used as a fallback");
assert(media.label(spotify) === "Fixture title · Fixture artist", "labels must read <title> · <artist>");
assert(media.label(spotifyPodcast) === "Episode title · Show name", "podcasts without an artist must fall back to the show name");
assert(media.label({ trackTitle: "Title only", trackArtist: "", metadata: {} }) === "Title only", "title-only labels must not end with a separator");
assert(media.label({ trackTitle: "", trackArtist: "Artist only", metadata: {} }) === "Artist only", "artist-only labels must not start with a separator");
assert(media.spotifyPlayer([spotify]) === spotify, "Spotify must use the Spotify slot");
assert(media.devicePlayer([spotify]) === null, "Spotify must not be duplicated into the device slot");
assert(media.devicePlayer([spotifyMirror, spotify]) === null, "a second MPRIS service mirroring Spotify's track must be deduplicated");
assert(media.devicePlayer([spotifyChromium, spotifyPodcast]) === null, "Spotify's embedded Chromium service must be deduplicated");
assert(media.devicePlayer([{ ...spotifyMirror, trackArtist: "Different artist" }, spotify]) !== null, "same-title tracks with different known artists must remain distinct");
assert(
  media.devicePlayer([{ ...spotifyChromium, length: 180 }, spotifyPodcast]) !== null,
  "players reporting different track lengths must remain distinct",
);
assert(media.spotifyPlayer([device, spotify]) === spotify, "Spotify selection must ignore other players");
assert(media.devicePlayer([spotify, device]) === device, "device selection must ignore Spotify");
assert(media.isSpotify({ dbusName: "org.mpris.MediaPlayer2.spotify.instance" }), "Spotify detection must fall back to its D-Bus name");
assert(media.playerName(spotify) === "Spotify", "the picker must use the player's stable identity");
assert(
  media.playerName({ identity: "", desktopEntry: "", dbusName: "org.mpris.MediaPlayer2.firefox.instance_1" }) === "Firefox",
  "the picker must derive a readable name from the D-Bus name",
);

const paused = {
  isPlaying: false,
  identity: "Fixture paused",
  desktopEntry: "fixture-paused",
  dbusName: "org.mpris.MediaPlayer2.paused",
  trackTitle: "Paused title",
  trackArtist: "Paused artist",
  length: 0,
  metadata: {},
};

assert(media.activePlayer([paused, spotify]) === spotify, "the Control Center must show the playing player first");
assert(media.activePlayer([paused]) === paused, "the Control Center must fall back to a paused player with a track");
assert(media.activePlayer([{ trackTitle: "", trackArtist: "", metadata: {} }]) === null, "a player without a track must not fill the now playing module");
assert(media.activePlayer([]) === null, "no players must leave the now playing module empty");
const available = media.availablePlayers([spotifyMirror, paused, device, spotify]);
assert(available.length === 3, "the picker must retain distinct players and remove a mirrored service");
assert(available[0] === paused && available[1] === device && available[2] === spotify, "the picker must retain bus order");
assert(media.selectedPlayer([paused, spotify], paused) === paused, "an explicit selection must override the active player");
assert(media.selectedPlayer([spotify], paused) === spotify, "a selection whose client exited must fall back to the active player");
assert(media.selectedPlayer([], paused) === null, "an exited selection without a fallback must leave media empty");
assert(
  media.timelineAvailable({ canSeek: true, positionSupported: true, lengthSupported: true, length: 180, metadata: {} }),
  "a seekable player with position and length support must expose the timeline",
);
assert(
  !media.timelineAvailable({ canSeek: false, positionSupported: true, lengthSupported: true, length: 180, metadata: {} }),
  "a player that cannot seek must not expose an inert timeline",
);
assert(
  !media.timelineAvailable({ canSeek: true, positionSupported: true, lengthSupported: true, length: 0, metadata: {} }),
  "a player without a duration must not expose a timeline",
);
const liveStream = {
  canSeek: true,
  positionSupported: true,
  lengthSupported: true,
  length: 9223372036854,
  metadata: { "mpris:length": 9223372036854000000 },
};
assert(media.liveStream(liveStream), "the Firefox signed 64-bit duration sentinel must identify a live stream");
assert(media.timelineAvailable(liveStream), "a live stream must expose its non-interactive live bar");
assert(!media.liveStream({ length: 86400, metadata: {} }), "ordinary long-form media must keep a seekable timeline");

assert(media.presentPlayer([device, spotify], spotify) === spotify, "a held player still on the bus must keep its bar entry");
assert(media.presentPlayer([device], spotify) === null, "a held player whose client quit must lose its bar entry");
assert(media.presentPlayer([device], null) === null, "an empty hold must not resolve to a player");

console.log("media normalization checks passed");

// Keyboard seeking clamps to real tracks and excludes live/unsupported players.
const seekable = {length:100,position:98,canSeek:true,positionSupported:true,lengthSupported:true};
assert(media.seekTarget(seekable, "forward", false) === 100, "forward clamps to duration");
assert(media.seekTarget(seekable, "back", true) === 68, "Shift seeks thirty seconds");
assert(media.seekTarget({...seekable,position:2}, "back", false) === 0, "back clamps to start");
assert(media.seekTarget(seekable, "start", false) === 0, "Home seeks to start");
assert(media.seekTarget(seekable, "end", false) === 100, "End seeks to duration");
assert(media.seekTarget(seekable, "other", false) === null, "unrelated keys are not consumed");
for (const patch of [{canSeek:false},{positionSupported:false},{lengthSupported:false},{length:0},{length:Infinity},{position:NaN},{length:366*86400}])
  assert(media.seekTarget({...seekable,...patch}, "forward", false) === null, "unsupported/live players cannot be seeked");
assert(media.seekTarget(null, "forward", false) === null, "disappearing player is safe");
const shell = fs.readFileSync(require("node:path").join(require("node:path").dirname(process.argv[2]), "shell.qml"), "utf8");
const handler = shell.match(/  function seekMediaKey\([^]*?\n  }/);
assert(handler, "production key handler exists");
const seekQt = {Key_Left:1,Key_Right:2,Key_Home:3,Key_End:4,ControlModifier:1,AltModifier:2,MetaModifier:4,ShiftModifier:8};
const context = vm.createContext({Media:media,Qt:seekQt});
vm.runInContext(handler[0], context);
let player = {...seekable,position:20}, event = {key:2,modifiers:8,accepted:false};
context.seekMediaKey(player,event);
assert(player.position === 50 && event.accepted, "production handler writes and consumes supported seeking");
event = {key:2,modifiers:1,accepted:false};
context.seekMediaKey(player,event);
assert(player.position === 50 && !event.accepted, "Ctrl chords remain with their original owner");
event = {key:2,modifiers:0,accepted:false};
context.seekMediaKey(null,event);
assert(!event.accepted, "a vanished player does not consume the event");
console.log("Keyboard media seeking checks passed");
