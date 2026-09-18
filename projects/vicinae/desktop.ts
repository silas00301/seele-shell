export type Client = {
  address: string;
  title: string;
  class: string;
  workspace: { id: number; name: string };
  monitor: number;
  focusHistoryID: number;
};
export type Workspace = {
  id: number;
  name: string;
  monitor: string;
  windows: number;
};
export type AudioDevice = {
  id: number;
  kind: string;
  name: string;
  node: string;
  profile: number | null;
  default: boolean;
  selected?: boolean;
};

import { binaries, run } from "./runtime";

// Event-to-native adapters; target validation and Lua construction live in Rust.
export async function focusWindow(address: string) {
  await run(binaries.control, ["vicinae-focus", "window", address]);
}
export async function focusWorkspace(id: number) {
  await run(binaries.control, [
    "vicinae-focus",
    "workspace",
    String(JSON.stringify(id)),
  ]);
}
// The desktop's own application endpoints already validate the address and own
// the compositor request, so window closing is not duplicated here.
export async function closeWindow(address: string) {
  await run(binaries.control, ["application", "quit", address]);
}
export async function forceQuitWindow(address: string) {
  await run(binaries.control, ["application", "force-quit", address]);
}
