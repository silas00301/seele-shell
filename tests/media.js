const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync(process.argv[2], "utf8");
const media = {Bridge: nativeBridge()};
vm.createContext(media);
vm.runInContext(nativeSource(source), media, { filename: process.argv[2] });

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

const loopStates = {None: 10, Track: 20, Playlist: 30};
const controllable = {canControl:true, shuffleSupported:true, loopSupported:true, shuffle:false, loopState:loopStates.None};
assert(media.toggleShuffle(controllable), 'supported writable shuffle must be actionable');
assert(controllable.shuffle, 'shuffle turns on');
media.toggleShuffle(controllable);
assert(!controllable.shuffle, 'shuffle turns off');
for (const expected of [loopStates.Playlist, loopStates.Track, loopStates.None]) {
  assert(media.cycleRepeat(controllable, loopStates), 'repeat must accept a writable player');
  assert(controllable.loopState === expected, 'repeat cycles off, playlist, track, off using supplied enum');
}
for (const player of [null, {}, {...controllable,canControl:false}, {...controllable,shuffleSupported:false,loopSupported:false}]) {
  assert(!media.toggleShuffle(player), 'unsupported or read-only player must never receive shuffle writes');
  assert(!media.cycleRepeat(player,loopStates), 'unsupported or read-only player must never receive loop writes');
}
const second = {...controllable,trackTitle:'Second'};
const chosen = media.selectedPlayer([controllable,second],second);
media.toggleShuffle(chosen);
assert(second.shuffle && !controllable.shuffle, 'mode changes address only the selected player');
assert(media.repeatLabel({...controllable,loopState:loopStates.Track},loopStates) === 'Repeat one track','repeat-one label distinguishes the mode');
assert(media.repeatLabel(null,loopStates) === 'Repeat unavailable','missing player is safe');
console.log('media shuffle/repeat capability and selection checks passed');

const qml = fs.readFileSync(require("node:path").join(require("node:path").dirname(process.argv[2]), "shell.qml"), "utf8");
const button = qml.split("  component MediaButton:")[1].split("  component MediaTimeline:")[0];
const keyBody = button.match(/Keys.onPressed: event => \{([\s\S]*?)^    \}/m)[1];
let presses = 0;
const keys = vm.createContext({mediaButton:{activated(){presses++}}, Qt:{Key_Return:13,Key_Enter:14,Key_Space:32,ControlModifier:1,AltModifier:2,MetaModifier:4}});
vm.runInContext("function press(event) {" + keyBody + "}", keys);
keys.press({key:32,modifiers:0,isAutoRepeat:false});
keys.press({key:32,modifiers:0,isAutoRepeat:true});
keys.press({key:13,modifiers:1,isAutoRepeat:false});
keys.press({key:14,modifiers:0,isAutoRepeat:false});
assert(presses === 2, "media keys ignore held-key repeats and modified shortcuts");
console.log("media keyboard intent checks passed");

const seekable={canControl:true,canSeek:true,positionSupported:true,lengthSupported:true,length:180,position:20};
assert(media.seekTarget(seekable,"back",false)===15,"left seeks five seconds");
assert(media.seekTarget(seekable,"forward",true)===50,"shift seeks thirty seconds");
assert(media.seekTarget(seekable,"start",false)===0 && media.seekTarget(seekable,"end",false)===180,"home/end reach bounded endpoints");
assert(media.seekTarget({...seekable,position:178},"forward",true)===180,"seek clamps to duration");
for(const player of [null,{}, {...seekable,canSeek:false},{...seekable,canControl:false},{...seekable,position:Infinity},{...seekable,length:9223372036854}]) assert(media.seekTarget(player,"forward",false)===null,"unsupported, invalid and live players cannot seek");
assert(media.seekTarget(seekable,"invalid",false)===null,"unrelated key never moves playback");
const timingOnly={...seekable};Object.defineProperty(timingOnly,"trackArtist",{get(){throw new Error("timing copied track metadata")}});
assert(media.timelineAvailable(timingOnly),"timeline query does not touch unrelated track fields");
console.log("media bounded native keyboard seeking and narrow snapshot checks passed");
