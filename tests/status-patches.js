const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

// Exercise the actual QML callback without starting a second desktop shell.
const source = fs.readFileSync(process.argv[2], "utf8");
const start = source.indexOf("  function parseSystemData(output) {");
const end = source.indexOf("  function reconcileBluetoothScanIntent", start);
assert(start >= 0 && end > start);
let osds = 0, prunes = 0, retired = 0, scans = 0, receivers = 0;
const root = {
  statusInitialized: false, volumeDrag: 75, microphoneDrag: 30,
  systemData: { volume: 50, microphoneVolume: 20, headphones: { connected: false }, notifications: { items: [] }, dnd: false },
  currentScreen: () => "fixture-output",
  notificationPopupClock: () => 1000,
  showTimedOsd: () => osds++,
  pruneNotificationPopups: value => { assert(value); prunes++; },
  retireNotificationPopupsForDnd: value => { assert(value); retired++; },
  reconcileBluetoothScanIntent: () => scans++,
  reconcileBluetoothReceiverIntent: () => receivers++,
};
root.systemData.apply = patch => {
  if (patch.notifications) assert.equal(root.notificationNow, 1000);
  Object.assign(root.systemData, patch);
};
const context = vm.createContext({ root, console: { warn: (...args) => { throw Error(args.join(" ")); } } });
vm.runInContext(source.slice(start, end), context);
const update = patch => context.parseSystemData(JSON.stringify(patch));
update({ connection: "Fixture" });
assert.equal(root.statusInitialized, false);
assert.equal(root.volumeDrag, 75);
assert.equal(root.microphoneDrag, 30);
assert.equal(prunes + osds + scans + receivers, 0);
update({ headphones: { connected: true, name: "Headphones" } });
assert.equal(osds, 0, "startup discovery should not emit a connection OSD");
update({ notifications: { items: [{ id: 1 }] } });
assert.equal(root.systemData.headphones.connected, true);
assert.equal(root.notificationPopupScreen, "fixture-output");
assert.equal(prunes, 1);
assert.equal(scans + receivers + osds, 0);
update({ dnd: true });
assert.equal(retired, 1);
update({ notifications: { items: [{ id: 2 }] } });
assert.equal(retired, 2, "new notifications respect an unchanged DND setting");
update({ volume: 75 });
assert.equal(root.volumeDrag, -1);
assert.equal(root.microphoneDrag, 30);
update({ microphoneVolume: 30, bluetoothScanning: false, bluetoothReceiver: false });
assert.equal(root.microphoneDrag, -1);
assert.equal(scans, 1);
assert.equal(receivers, 1);
update({ headphones: { connected: false } });
assert.equal(osds, 1);
console.log("partial status patch checks passed");
