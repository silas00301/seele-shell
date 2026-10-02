// Fix me selection and the argv it is allowed to build. The native worker
// decides which checks are drifted and is the only thing that can change the
// machine; these guards keep a switch from turning into a command.
const fs = require('node:fs');
const assert = require('node:assert/strict');

const source = fs.readFileSync(process.argv[2], 'utf8');
const store = fs.readFileSync(process.argv[3], 'utf8');
const panel = fs.readFileSync(process.argv[4], 'utf8');
const shell = fs.readFileSync(process.argv[5], 'utf8');

const api = new Function(`${source}\nreturn {chosenIds, argumentsFor};`)();
const checks = [
  {id: 'quad9-dot', drifted: true, unavailable: false},
  {id: 'podman-rootless', drifted: true, unavailable: false},
  {id: 'remote-shell', drifted: false, unavailable: false},
  {id: 'podman-rootless', drifted: true, unavailable: true},
];

assert.deepEqual(api.chosenIds(checks, []), ['quad9-dot', 'podman-rootless']);
assert.deepEqual(api.chosenIds(checks, ['quad9-dot']), ['podman-rootless']);
assert.deepEqual(api.chosenIds(checks, ['quad9-dot', 'podman-rootless']), []);
assert.deepEqual(api.argumentsFor('diff', ['ignored']), ['seele-drift', 'diff']);
assert.deepEqual(
  api.argumentsFor('apply', ['podman-rootless']),
  ['seele-drift', 'apply', 'podman-rootless'],
);
assert.equal(api.argumentsFor('apply', []), null);
assert.equal(api.argumentsFor('apply', ['Podman']), null);
assert.equal(api.argumentsFor('apply', ['nope;reboot']), null);
assert.equal(api.argumentsFor('restore', ['quad9-dot']), null);

assert.match(store, /Drift\.argumentsFor\(/, 'the store builds argv in drift.js');
assert.match(store, /value\.action === "diff" && value\.mutated/, 'a diff that mutated is refused');
assert.doesNotMatch(store, /systemctl|resolvectl|run0/, 'the store names no system command');
assert.match(panel, /modelData\.before/, 'the panel shows the live line');
assert.match(panel, /modelData\.after/, 'the panel shows the flake line');
assert.match(panel, /text: "Now"/, 'the live caption is a sentence');
assert.match(panel, /text: "Flake"/, 'the flake caption is a sentence');
assert.doesNotMatch(panel, /systemctl|resolvectl|run0|trackingLabel/, 'the panel names no command and shouts no caption');
assert.match(shell, /DriftStore \{\s*\n\s*id: driftStore/, 'the store is instantiated');
assert.match(shell, /DriftPanel \{ id: driftPanel; theme: root; store: driftStore/, 'the panel shares that store');
assert.match(shell, /label: "Fix me"/, 'the Control Center carries the tile');
assert.match(shell, /namespace: "seele-shell-drift"/);
assert.match(shell, /height: devicesY \+ smallTileHeight \* 10 \+ gap \* 9/, 'the grid has a row for the tile');
assert.match(
  shell,
  /smallTileHeight \* 9 \+ controlGrid\.gap \* 9[\s\S]*?label: "Fix me"/,
  'Fix me sits on the new row',
);

console.log('drift selection, argv and production wiring passed');
