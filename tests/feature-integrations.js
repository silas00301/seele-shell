const assert = require('node:assert/strict');
const fs = require('node:fs');

const shell = fs.readFileSync(process.argv[2], 'utf8');
const pkg = fs.readFileSync(process.argv[3], 'utf8');

const shellPatterns = {
  'transfers store': /TransfersStore\s*\{\s*id: transfersStore/,
  'transfers history panel': /TransfersPanel\s*\{[\s\S]*?theme: root[\s\S]*?store: transfersStore/,
  'registered health store': /IntegrationHealthStore\s*\{\s*id: integrationHealth/,
  'system health panel': /SystemHealthPanel\s*\{\s*id:\s*healthContent/,
  'focus panel uses resident timer': /FocusPanel\s*\{\s*id: focusContent\s*theme: root\s*timer: focusTimer/,
  'focus timer store': /FocusTimer\s*\{\s*\n\s*id: focusTimer/,
  'quick AI prompt controller': /AiPrompt\s*\{\s*\n\s*id: aiPrompt/,
  'quick AI prompt IPC': /function togglePrompt\(\): void \{ root\.togglePrompt\(\) \}/,
  'screen colour picker controller': /ColorPicker\s*\{\s*\n\s*id: colorPicker/,
  'screen colour picker IPC': /function toggleColor\(\): void \{ root\.toggleColor\(\) \}/,
  'screen colour picker surface': /namespace: "seele-shell-color"/,
  'Quick Look controller': /QuickLook\s*\{\s*\n\s*id: quickLook/,
  'Quick Look IPC': /function previewFiles\(paths: string\): void \{ root\.toggleQuickLook\(paths\) \}/,
  'Quick Look reaches received transfers': /onPreviewRequested: path => root\.toggleQuickLook\(path\)/,
  'Home Assistant store': /HomeAssistantStore\s*\{\s*id: homeAssistantStore/,
  'GitHub notification worker': /GitHubInboxStore\s*\{\s*\n\s*id: githubInbox/,
  'GitHub notification panel': /GitHubInboxPanel\s*\{/,
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
  'per-player volume level': /component PlayerLevelRow: Item/,
  'per-application volume level': /component ApplicationLevelRow: Row/,
  'per-application volume control': /root\.commitStreamVolume\(/,
  'per-application mixer group': /label: "APPLICATIONS"/,
  'playback speed': /MediaSpeed\.select\(/,
  'playback speed presets': /id: playbackSpeedRule/,
  'shared empty state': /component EmptyState: Shared\.EmptyState \{ theme: root \}/,
  'timed quiet periods': /id: quietMenu/,
  'Caffeinate session store': /CaffeinateStore \{\s*\n\s*id: caffeinateStore/,
  'conditional Caffeinate bar item': /visible: caffeinateStore\.active/,
  'Caffeinate panel shares the session store': /CaffeinatePanel \{ theme: root; store: caffeinateStore/,
  'port inspector store': /PortsStore\s*\{\s*\n\s*id: portsStore/,
  'port inspector panel': /PortsPanel \{ id: portsPanel; theme: root; store: portsStore/,
  'port inspector tile': /label: "Ports"/,
  'port inspector leaves the bar and takes focus only when opened': /namespace: "seele-shell-ports"\s+WlrLayershell.keyboardFocus: visible \? WlrKeyboardFocus.OnDemand/,
  'microphone test store': /MicTestStore\s*\{\s*\n\s*id: micTest/,
  'microphone test card': /MicTestCard \{ theme: root; store: micTest/,
  'Audio panel keyboard access': /namespace: "seele-shell-audio"\s+WlrLayershell.keyboardFocus: visible \? WlrKeyboardFocus.OnDemand/,
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

// A toast maps while the user is typing somewhere else, and Hyprland focuses a
// layer surface that asks for on-demand keyboard interactivity as it maps, so
// any interactivity here swallows the next keystrokes.
const notificationPopup = shell.slice(shell.indexOf('id: notificationPopupWindow'), shell.indexOf('component MediaSlot:'));
assert.ok(notificationPopup.includes('WlrLayershell.keyboardFocus: WlrKeyboardFocus.None'),
  'notification toasts must declare no keyboard focus');
assert.ok(!/WlrKeyboardFocus\.(OnDemand|Exclusive)/.test(notificationPopup),
  'notification toasts must never take keyboard focus away from the focused window');

const packagedSources = [
  'CaffeinateStore.qml',
  'CaffeinatePanel.qml',
  'health.js', 'IntegrationHealthStore.qml', 'SystemHealthPanel.qml',
  'AiPrompt.qml',
  'ai-prompt.js',
  'ColorPicker.qml',
  'color-picker.js',
  'QuickLook.qml',
  'QuickLookMedia.qml',
  'quicklook.js',
  'FocusTimer.qml',
  'FocusPanel.qml',
  'focus.js',
  'HomeAssistantStore.qml',
  'HomeAssistantPanel.qml',
  'GitHubStore.qml',
  'GitHubInboxStore.qml',
  'GitHubInboxPanel.qml',
  'github.js',
  'network.js',
  'player-volume.js',
  'media-speed.js',
  'PortsStore.qml',
  'PortsPanel.qml',
  'MicTestStore.qml',
  'MicTestCard.qml',
  'mic-test.js',
];
for (const source of packagedSources) {
  const reference = new RegExp(`\\$\\{\\./${source.replace('.', '\\.')}\\}`);
  assert.ok(reference.test(pkg), `${source} is not installed by the shell package`);
}

assert.ok(pkg.includes('substituteInPlace "$out/share/seele-shell/FocusPanel.qml"'),
  'Focus panel shared imports use the installed layout');

const focusedTests = [
  'health.js',
  'ai-prompt.js',
  'color-picker.js',
  'color-picker.sh',
  'quicklook.js',
  'quicklook.sh',
  'focus.js',
  'focus-timer.sh',
  'panel-layouts.js',
  'shell-load.sh',
  'home-assistant-store.js',
  'home-assistant-panel.sh',
  'home-assistant-interaction.sh',
  'github.js',
  'network-addresses.js',
  'player-volume.js',
  'media-speed.js',
  'ports.js',
  'audio-streams.js',
  'vicinae-generations.mjs',
  'caffeinate.js',
  'vicinae-caffeinate.cjs',
  'mic-test.js',
  'mic-test.sh',
  'vicinae-views.cjs',
];
for (const test of focusedTests) {
  assert.ok(pkg.includes(`tests/${test}`) || pkg.includes("${tests}/" + test), `${test} is not run by the shell package`);
}

for (const helper of ['integrations', 'prompt', 'runtime']) {
  assert.ok(pkg.includes('${' + helper + '}/bin/seele-'), helper + ' native package is not wired');
}
assert.ok(!pkg.includes('/bin/python3'), 'Python must not be a shell runtime wrapper');

assert.ok(!/notificationSearch|notificationClipboard/.test(shell), 'notification UI must retain the 11:00 behavior');
console.log('production QML, packaged helpers, and focused checks cover the enabled features');

assert.ok(pkg.includes('bash ${tests}/transfers-panel.sh'), 'Transfers keyboard/render fixture is packaged');
assert.ok(pkg.includes('node ${tests}/transfers.js'), 'Transfers native history/store fixture is packaged');
