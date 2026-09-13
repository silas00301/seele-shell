// Execute the same Rust functions used by the QML plugin. Existing behavioral
// fixtures keep their assertions; only the host's native-call boundary changes.
const {spawnSync} = require('node:child_process');
const path = require('node:path');
function execute(binary, args, input) {
  const result = spawnSync(binary, args, {
    input: JSON.stringify(input) + '\n', encoding: 'utf8', timeout: 10000, maxBuffer: 17 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw Error('native function process failed');
  const response = JSON.parse(result.stdout);
  if (!response.ok) throw Error(response.error);
  return response.value;
}
function nativeFunctions(binary = process.env.SEELE_QML_FUNCTIONS || path.resolve(__dirname, '../target/debug/seele-qml-functions')) {
  return {Functions: {
    call(operation, args) { return execute(binary, [], {operation, arguments: args}); },
    notificationState(now) {
      if (!Number.isFinite(now)) throw Error('invalid notification timestamp');
      const steps = [];
      // Test adapter only: replay bounded events through the same Rust state
      // methods. Production Qt owns one Rust object and never retains an event log.
      return {call(operation, args) {
        if (steps.length >= 1024) throw Error('notification fixture exceeds its limit');
        const step = {operation, arguments: args};
        const response = execute(binary, ['--notification-fixture'], {now, steps: [...steps, step]});
        if (operation !== 'view' && operation !== 'save') steps.push(step);
        return response;
      }};
    },
  }};
}

function source(text) {
  return text.replace(/^\.pragma[^\n]*$/gm, '').replace(/^\.import[^\n]*$/gm, '');
}
function nativeBridge(binary) {
  const vm = require('node:vm');
  const fs = require('node:fs');
  const filename = process.env.SEELE_QML_BRIDGE || path.resolve(__dirname, '../projects/shared/Native.js');
  const bridge = vm.createContext({Native: nativeFunctions(binary)});
  vm.runInContext(source(fs.readFileSync(filename, 'utf8')), bridge, {filename});
  return bridge;
}
module.exports = {nativeFunctions, nativeBridge, source};
