const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const { nativeBridge } = require("./native-functions.cjs");

// Exercise the actual QML callback without starting a second desktop shell.
const source = fs.readFileSync(process.argv[2], "utf8");
const start = source.indexOf("  function parseSystemData(output) {");
const end = source.indexOf("  function reconcileBluetoothScanIntent", start);
assert(start >= 0 && end > start);
let osds = 0, scans = 0, receivers = 0;
const root = {
  statusInitialized: false, volumeDrag: 75, microphoneDrag: 30,
  systemData: { volume: 50, microphoneVolume: 20, headphones: { connected: false }, notifications: { items: [] }, dnd: false },
  currentScreen: () => "fixture-output",
  showTimedOsd: () => osds++,
  reconcileBluetoothScanIntent: () => scans++,
  reconcileBluetoothReceiverIntent: () => receivers++,
};
root.systemData.apply = patch => {
  Object.assign(root.systemData, patch);
};
const context = vm.createContext({ root, console: { warn: (...args) => { throw Error(args.join(" ")); } } });
vm.runInContext(source.slice(start, end), context);
const update = patch => context.parseSystemData(JSON.stringify(patch));
update({ connection: "Fixture" });
assert.equal(root.statusInitialized, false);
assert.equal(root.volumeDrag, 75);
assert.equal(root.microphoneDrag, 30);
assert.equal(osds + scans + receivers, 0);
update({ headphones: { connected: true, name: "Headphones" } });
assert.equal(osds, 0, "startup discovery should not emit a connection OSD");
update({ notifications: { items: [{ id: 1 }] } });
assert.equal(root.systemData.headphones.connected, true);
assert.equal(scans + receivers + osds, 0);
update({ dnd: true });
update({ notifications: { items: [{ id: 2 }] } });
assert.equal(root.systemData.dnd, true, "unrelated patches preserve native DND state");
update({ volume: 75 });
assert.equal(root.volumeDrag, -1);
assert.equal(root.microphoneDrag, 30);
update({ microphoneVolume: 30, bluetoothScanning: false, bluetoothReceiver: false });
assert.equal(root.microphoneDrag, -1);
assert.equal(scans, 1);
assert.equal(receivers, 1);
update({ headphones: { connected: false } });
assert.equal(osds, 1);
assert.equal(root.systemData.notifications.items[0].id, 2, "device updates preserve native notifications");
console.log("partial status patch checks passed");

// Keep the actual production derived binding: its policy now lives in Rust.
context.Bridge = nativeBridge();
context.systemData = root.systemData;
const batteryProjection = source.match(/readonly property var batteryProjection: ([^\n]+)/);
assert(batteryProjection, "the native battery projection must be wired");
Object.defineProperty(context, "batteryProjection", {
  get: () => vm.runInContext(batteryProjection[1], context),
});

const headphonesStart = source.indexOf("  function headphonesIconKind() {");
const headphonesEnd = source.indexOf("  function headphonesDetail()", headphonesStart);
vm.runInContext(source.slice(headphonesStart, headphonesEnd), context);
root.headphonesIconKind = context.headphonesIconKind;
for (const [name, connected, icon, label] of [
  ["Nothing Headphone (1)", true, "headphones", "Nothing Headphone (1)"],
  ["Silas AirPods", true, "airpods", "Silas AirPods"],
  ["Silas AirPods", false, "headphones", "Headphones"],
  ["Beats", true, "headphones", "Beats"],
]) {
  root.systemData.headphones = { name, connected };
  assert.equal(context.headphonesIconKind(), icon);
  assert.equal(context.headphonesLabel(), label);
}
console.log("headphone identity checks passed");
