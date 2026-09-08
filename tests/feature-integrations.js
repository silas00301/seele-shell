const assert = require('node:assert/strict');
const fs = require('node:fs');

const shell = fs.readFileSync(process.argv[2], 'utf8');
const pkg = fs.readFileSync(process.argv[3], 'utf8');

const shellPatterns = {
  'focus timer store': /FocusTimer\s*\{\s*\n\s*id: focusTimer/,
  'Home Assistant store': /HomeAssistantStore\s*\{\s*id: homeAssistantStore\s*\}/,
  'GitHub store': /GitHubStore\s*\{\s*\n\s*id: githubStore/,
  'GitHub leaves bar and click-away input available': /namespace: "seele-shell-github"\s+WlrLayershell.keyboardFocus: visible \? WlrKeyboardFocus.OnDemand/,
  'collapsed network addresses': /property bool addressesExpanded: false/,
  'native notification cards': /component NotificationCard: Rectangle/,
  'in-place app stacks': /height: notificationGroupColumn\.implicitHeight \+ notificationGroup\.stackReach/,
  'media keyboard seeking': /function seekMediaKey\(/,
  'shuffle control': /Media\.toggleShuffle\(/,
  'repeat control': /Media\.cycleRepeat\(/,
  'calendar copy': /Time\.calendarCopyDate\(/,
  'world-clock copy': /copyClockTimestamp\(/,
  'network address rows': /Network\.addresses\(/,
  'per-player volume': /PlayerVolume\.adjust\(/,
  'playback speed': /MediaSpeed\.cycle\(/,
  'timed quiet periods': /id: quietPresets/,
};

for (const [feature, pattern] of Object.entries(shellPatterns)) {
  assert.ok(pattern.test(shell), `${feature} is not wired into production shell.qml`);
}

const packagedSources = [
  'FocusTimer.qml',
  'focus.js',
  'HomeAssistantStore.qml',
  'HomeAssistantPanel.qml',
  'GitHubStore.qml',
  'github.js',
  'network.js',
  'player-volume.js',
  'media-speed.js',
];
for (const source of packagedSources) {
  const reference = new RegExp(`\\$\\{\\./${source.replace('.', '\\.')}\\}`);
  assert.ok(reference.test(pkg), `${source} is not installed by the shell package`);
}

const focusedTests = [
  'focus.js',
  'focus-timer.sh',
  'panel-layouts.js',
  'shell-load.sh',
  'home-assistant-store.js',
  'home-assistant.py',
  'home-assistant-live.py',
  'home-assistant-panel.sh',
  'github.js',
  'github.py',
  'network-addresses.js',
  'player-volume.js',
  'media-speed.js',
];
for (const test of focusedTests) {
  assert.ok(pkg.includes(`tests/${test}`), `${test} is not run by the shell package`);
}

assert.ok(!/notificationSearch|notificationClipboard/.test(shell), 'notification UI must retain the 11:00 behavior');
console.log('production QML, packaged helpers, and focused checks cover the enabled features');
