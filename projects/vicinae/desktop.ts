export type Client = {
  address: string;
  title: string;
  class: string;
  mapped: boolean;
  hidden: boolean;
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
};

export function visibleClients(clients: Client[]) {
  return clients
    .filter(
      (client) =>
        client.mapped &&
        !client.hidden &&
        client.class.toLowerCase() !== "vicinae",
    )
    .sort(
      (a, b) =>
        a.focusHistoryID - b.focusHistoryID ||
        a.address.localeCompare(b.address),
    );
}

export function focusWindow(address: string) {
  if (!/^0x[0-9a-f]+$/i.test(address))
    throw new Error("Invalid window address");
  return `hl.dsp.focus({ window = "address:${address}" })`;
}

export function focusWorkspace(id: number) {
  if (!Number.isSafeInteger(id) || id <= 0)
    throw new Error("Invalid workspace");
  return `hl.dsp.focus({ workspace = ${id} })`;
}

export function audioArguments(device: AudioDevice) {
  if (
    !Number.isSafeInteger(device.id) ||
    device.id < 0 ||
    (device.profile !== null &&
      (!Number.isSafeInteger(device.profile) || device.profile < 0))
  )
    throw new Error("Invalid audio device");
  return [
    "audio-device",
    String(device.id),
    ...(device.profile === null ? [] : [String(device.profile)]),
  ];
}
