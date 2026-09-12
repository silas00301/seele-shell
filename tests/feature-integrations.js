const assert = require('node:assert/strict');
const fs = require('node:fs');

const shell = fs.readFileSync(process.argv[2], 'utf8');
const pkg = fs.readFileSync(process.argv[3], 'utf8');

const shellPatterns = {
  'registered health store': /IntegrationHealthStore\s*\{\s*id: integrationHealth/,
  'system health panel': /SystemHealthPanel\s*\{\s*id:\s*healthContent/,
  'focus timer store': /FocusTimer\s*\{\s*\n\s*id: focusTimer/,
  'quick AI prompt controller': /AiPrompt\s*\{\s*\n\s*id: aiPrompt/,
  'quick AI prompt IPC': /function togglePrompt\(\): void \{ root\.togglePrompt\(\) \}/,
  'Home Assistant store': /HomeAssistantStore\s*\{\s*id: homeAssistantStore/,
  'GitHub store': /GitHubStore\s*\{\s*\n\s*id: githubStore/,
  'GitHub leaves bar and click-away input available': /namespace: "seele-shell-github"\s+WlrLayershell.keyboardFocus: visible \? WlrKeyboardFocus.OnDemand/,
  'collapsed network addresses': /property bool addressesExpanded: false/,
  'native notification cards': /component NotificationCard: Rectangle/,
  'in-place app stacks': /height: notificationGroupColumn\.implicitHeight \+ notificationGroup\.stackReach/,
  'notification profile icons': /id: notificationProfileImage/,
  'notification application badges': /id: notificationAppBadge/,
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

// SlimScrollBar declares popupHovered required, and Qt fails the whole
// enclosing object when a required property is left unset -- the System Health
// panel's scrollable, and so the panel itself, silently stopped being created.
// qmllint does not catch it and the load test compiles without instantiating,
// so the shape is guarded here.
// The `Shared.` form is the inline alias declaration, which defers the
// property to its callers; every other occurrence is a real instantiation.
const looseScrollBars = shell.match(/(?<!Shared\.)SlimScrollBar\s*\{(?![^}]*popupHovered)[^}]*\}/g) || [];
assert.deepEqual(looseScrollBars, [],
  'every SlimScrollBar must initialize its required popupHovered property');

const notificationCard = shell.slice(shell.indexOf('component NotificationCard:'), shell.indexOf('component NotificationList:'));
assert.ok(!/id: notificationImage\b/.test(notificationCard),
  'notification profile images must not return to the expandable body');

const packagedSources = [
  'health.js', 'IntegrationHealthStore.qml', 'SystemHealthPanel.qml',
  'AiPrompt.qml',
  'ai-prompt.js',
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
  'health.js',
  'ai-prompt.js',
  'ai-prompt.py',
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
  'vicinae-generations.mjs',
];
for (const test of focusedTests) {
  assert.ok(pkg.includes(`tests/${test}`), `${test} is not run by the shell package`);
}

assert.ok(!/notificationSearch|notificationClipboard/.test(shell), 'notification UI must retain the 11:00 behavior');
console.log('production QML, packaged helpers, and focused checks cover the enabled features');
