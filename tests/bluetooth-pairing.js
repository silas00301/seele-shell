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
const root = { pairingLoadToken: '', pairingRequest: {}, pairingScreen: '', currentScreen: () => 'fixture-output' };
const read = { running: false, token: '', command: [] };
const answer = { running: false, payload: '', command: ['seele-control', 'bluetooth-pairing-answer-stdin'] };
const context = vm.createContext({root, bluetoothPairingReadProcess: read, bluetoothPairingAnswerProcess: answer, console});
for (const name of ['setBluetoothPairing', 'loadBluetoothPairing', 'receiveBluetoothPairing', 'clearBluetoothPairing', 'answerBluetoothPairing']) {
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
root.answerBluetoothPairing('accept', '654321');
assert.equal(answer.stdinEnabled, true);
assert.equal(answer.running, true);
assert.equal(JSON.parse(answer.payload).value, '654321');
assert.equal(answer.command.join(' ').includes('654321'), false);
assert.equal(root.pairingLoadToken, '');
root.receiveBluetoothPairing(JSON.stringify({token: second, passkey: '654321'}));
assert.equal(root.pairingRequest.token, undefined, 'dismissed completion must stay dismissed');
assert(source.includes('command: ["seele-control", "bluetooth-pairing-answer-stdin"]'));
assert(!source.includes('Quickshell.execDetached(["seele-control", "bluetooth-pairing-answer",'));
console.log('Production Bluetooth handlers preserve stale-request guards and keep pairing codes out of argv');
