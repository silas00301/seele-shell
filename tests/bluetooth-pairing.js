const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const source = fs.readFileSync(process.argv[2], 'utf8');
function method(name) {
  const begin = source.indexOf('  function ' + name + '(');
  assert(begin >= 0, name);
  const brace = source.indexOf('{', begin);
  let end = brace + 1, depth = 1;
  while (depth) {
    if (source[end] === '{') depth++;
    if (source[end] === '}') depth--;
    end++;
  }
  return source.slice(begin, end);
}
const root = { pairingLoadToken: '', pairingRequest: {}, pairingScreen: '', pairingKeyboardToken: '', currentScreen: () => 'fixture-output' };
root.closeOverlays = () => { root.pairingKeyboardToken = ''; };
const read = { running: false, token: '', command: [] };
const answer = { running: false, payload: '', command: ['seele-control', 'bluetooth-pairing-answer-stdin'] };
const context = vm.createContext({root, bluetoothPairingReadProcess: read, bluetoothPairingAnswerProcess: answer, console});
const keyboardBinding = source.match(/readonly property bool keyboardActive: (visible && root\.pairingKeyboardToken[^]*?)\n\s*screen: modelData/)[1];
context.visible = true;
function keyboardActive() { return vm.runInContext(keyboardBinding, context); }
for (const name of ['setBluetoothPairing', 'loadBluetoothPairing', 'receiveBluetoothPairing', 'clearBluetoothPairing', 'answerBluetoothPairing', 'focusBluetoothPairing']) {
  vm.runInContext(method(name), context);
  root[name] = context[name];
}
const first = 'a'.repeat(32), second = 'b'.repeat(32);
root.setBluetoothPairing('invalid-token');
assert.equal(read.running, false);
root.setBluetoothPairing(first);
assert.equal(read.running, true);
assert.deepEqual(Array.from(read.command), ['seele-control', 'bluetooth-pairing-read', first]);
root.pairingRequest = {token: first};
root.setBluetoothPairing(second);
root.answerBluetoothPairing("accept", "123456");
assert.equal(answer.running, false, "a superseded visible prompt cannot answer the newer request");
root.pairingRequest = {};
root.receiveBluetoothPairing(JSON.stringify({token: first, passkey: '123456'}));
assert.equal(root.pairingRequest.token, undefined, 'superseded completion must not reopen a dialog');
read.running = false;
root.loadBluetoothPairing();
root.receiveBluetoothPairing(JSON.stringify({token: second, passkey: '654321'}));
assert.equal(root.pairingRequest.token, second);
assert.equal(root.pairingScreen, 'fixture-output');
assert.equal(root.pairingKeyboardToken, '', 'an arriving request never asks for the keyboard');
assert.equal(keyboardActive(), false);
root.focusBluetoothPairing('chosen-output');
assert.equal(root.pairingKeyboardToken, second, 'explicit entry belongs to the reviewed request');
assert.equal(keyboardActive(), true);
assert.equal(root.pairingScreen, 'chosen-output');
root.answerBluetoothPairing('accept', '654321');
assert.equal(answer.stdinEnabled, true);
assert.equal(answer.running, true);
assert.equal(JSON.parse(answer.payload).value, '654321');
assert.equal(answer.command.join(' ').includes('654321'), false);
assert.equal(root.pairingLoadToken, '');
assert.equal(root.pairingKeyboardToken, '', 'answering drops keyboard intent');
assert.equal(keyboardActive(), false);
root.receiveBluetoothPairing(JSON.stringify({token: second, passkey: '654321'}));
assert.equal(root.pairingRequest.token, undefined, 'dismissed completion must stay dismissed');
root.pairingRequest = {token: first};
root.pairingLoadToken = second;
root.focusBluetoothPairing('another-output');
assert.equal(root.pairingKeyboardToken, '', 'superseded prompts cannot claim the keyboard');
root.pairingKeyboardToken = first;
assert.equal(keyboardActive(), false, 'a newer load revokes keyboard intent before its payload arrives');
root.pairingRequest = {token: second};
assert.equal(keyboardActive(), false, 'a new prompt cannot inherit the previous request intent');

const focusCalls = [];
context.Qt = {callLater: callback => callback()};
context.pairingWindow = {visible: true, keyboardActive: true, kind: 'confirm'};
context.keyboardActive = true;
context.pairingRejectMouse = {forceActiveFocus: () => focusCalls.push('reject')};
context.pairingDismissMouse = {forceActiveFocus: () => focusCalls.push('dismiss')};
context.pairingCodeField = {forceActiveFocus: () => focusCalls.push('code'), selectAll: () => {}};
root.pairingWantsCode = () => ['passkey', 'pincode'].includes(context.pairingWindow.kind);
vm.runInContext(method('focusInitial'), context);
for (const kind of ['confirm', 'authorize', 'display', 'passkey', 'pincode']) {
  context.pairingWindow.kind = kind;
  context.focusInitial();
}
assert.deepEqual(focusCalls, ['reject', 'reject', 'dismiss', 'code', 'code']);
context.keyboardActive = false;
context.pairingWindow.keyboardActive = false;
context.pairingWindow.kind = 'confirm';
context.focusInitial();
assert.equal(focusCalls.length, 5, 'passive confirmation does not move QML focus');
assert.match(source, /panel === "bluetooth" && pairingPrompting[\s\S]*?focusBluetoothPairing\(screen\)/);
assert.match(source, /readonly property bool keyboardActive: visible && root\.pairingKeyboardToken !== ""\s*&& root\.pairingKeyboardToken === String\(\(root\.pairingRequest \|\| \{\}\)\.token \|\| ""\)\s*&& root\.pairingKeyboardToken === root\.pairingLoadToken/);
assert.match(source, /keyboardFocus: keyboardActive \? WlrKeyboardFocus\.Exclusive\s*: visible && root\.pairingWantsCode\(\) \? WlrKeyboardFocus\.OnDemand : WlrKeyboardFocus\.None/);
assert.match(source, /else pairingRejectMouse\.forceActiveFocus\(\)/);
assert(source.includes('command: ["seele-control", "bluetooth-pairing-answer-stdin"]'));
assert(!source.includes('Quickshell.execDetached(["seele-control", "bluetooth-pairing-answer",'));
console.log('Production Bluetooth handlers preserve stale-request guards and keep pairing codes out of argv');
