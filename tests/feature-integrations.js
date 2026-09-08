const assert = require('node:assert/strict');
const fs = require('node:fs');

const shell = fs.readFileSync(process.argv[2], 'utf8');
const pkg = fs.readFileSync(process.argv[3], 'utf8');

const shellPatterns = {
  'focus timer store': /FocusTimer\s*\{\s*\n\s*id: focusTimer/,
  'Home Assistant store': /HomeAssistantStore\s*\{\s*id: homeAssistantStore\s*\}/,
  'GitHub store': /GitHubStore\s*\{\s*\n\s*id: githubStore/,
  'notification clipboard': /NotificationClipboard\s*\{\s*id: notificationClipboard\s*\}/,
  'media keyboard seeking': /function seekMediaKey\(/,
  'shuffle control': /Media\.toggleShuffle\(/,
  'repeat control': /Media\.cycleRepeat\(/,
  'calendar copy': /Time\.calendarCopyDate\(/,
  'world-clock copy': /copyClockTimestamp\(/,
  'network address rows': /Network\.addresses\(/,
  'per-player volume': /PlayerVolume\.adjust\(/,
  'playback speed': /MediaSpeed\.cycle\(/,
  'notification search': /NotificationSearch\.filter\(/,
  'notification text copy': /notificationClipboard\.copy\(/,
  'timed DND': /notificationStore\.controller\.snooze\(/,
};

for (const [feature, pattern] of Object.entries(shellPatterns)) {
  assert.ok(pattern.test(shell), `${feature} is not wired into production shell.qml`);
}

const packagedSources = [
  'FocusTimer.qml',
  'focus.js',
  'HomeAssistantStore.qml',
  'GitHubStore.qml',
  'github.js',
  'network.js',
  'player-volume.js',
  'media-speed.js',
  'notification-search.js',
  'NotificationClipboard.qml',
  'notification-copy.js',
];
for (const source of packagedSources) {
  const reference = new RegExp(`\\$\\{\\./${source.replace('.', '\\.')}\\}`);
  assert.ok(reference.test(pkg), `${source} is not installed by the shell package`);
}

const focusedTests = [
  'focus.js',
  'home-assistant-store.js',
  'home-assistant.py',
  'github.js',
  'github.py',
  'network-addresses.js',
  'player-volume.js',
  'media-speed.js',
  'notification-search.js',
  'notification-copy.js',
];
for (const test of focusedTests) {
  assert.ok(pkg.includes(`tests/${test}`), `${test} is not run by the shell package`);
}

console.log('production QML, packaged helpers, and focused checks cover every restored PR feature');
