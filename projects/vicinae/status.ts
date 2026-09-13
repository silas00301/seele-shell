import { spawn } from "node:child_process";
import { useEffect, useRef, useState } from "react";
import { AudioDevice } from "./desktop";
import { binaries } from "./runtime";

export type Status = {
  volume?: number;
  muted?: boolean;
  microphoneVolume?: number;
  microphoneMuted?: boolean;
  dnd?: boolean;
  wifiEnabled?: boolean;
  wifiAvailable?: boolean;
  bluetoothAvailable?: boolean;
  bluetoothPowered?: boolean;
  microphoneActive?: boolean;
  cameraActive?: boolean;
  screenRecording?: boolean;
  audioDevices?: AudioDevice[];
  headphones?: { connected: boolean; name: string };
};

const maximumFrame = 256 * 1024;
const booleans = new Set([
  "muted",
  "microphoneMuted",
  "dnd",
  "wifiEnabled",
  "wifiAvailable",
  "bluetoothAvailable",
  "bluetoothPowered",
  "microphoneActive",
  "cameraActive",
  "screenRecording",
]);
const text = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length <= 1024 &&
  !/[\u0000-\u001f\u007f-\u009f]/u.test(value);
const id = (value: unknown): value is number =>
  Number.isSafeInteger(value) && Number(value) >= 0;
function object(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
function patchValue(value: unknown): Status {
  if (!object(value)) throw new Error("Invalid status");
  const result: Record<string, unknown> = {};
  for (const [key, field] of Object.entries(value)) {
    if (booleans.has(key)) {
      if (typeof field !== "boolean") throw new Error("Invalid switch");
      result[key] = field;
    } else if (key === "volume" || key === "microphoneVolume") {
      if (
        typeof field !== "number" ||
        !Number.isFinite(field) ||
        field < 0 ||
        field > 1000
      )
        throw new Error("Invalid volume");
      result[key] = field;
    } else if (key === "headphones") {
      if (
        !object(field) ||
        typeof field.connected !== "boolean" ||
        !text(field.name)
      )
        throw new Error("Invalid headphones");
      result[key] = { connected: field.connected, name: field.name };
    } else if (key === "audioDevices") {
      if (!Array.isArray(field) || field.length > 512)
        throw new Error("Invalid devices");
      result[key] = field.map((device) => {
        if (
          !object(device) ||
          !id(device.id) ||
          !text(device.kind) ||
          !text(device.name) ||
          !text(device.node) ||
          (device.profile !== null && !id(device.profile)) ||
          typeof device.default !== "boolean" ||
          (device.selected !== undefined &&
            typeof device.selected !== "boolean")
        )
          throw new Error("Invalid device");
        return {
          id: device.id,
          kind: device.kind,
          name: device.name,
          node: device.node,
          profile: device.profile,
          default: device.default,
          ...(device.selected === undefined
            ? {}
            : { selected: device.selected }),
        };
      });
    }
    // The native status feed serves other clients too. Ignore their fields
    // instead of retaining an unbounded set of keys in this view.
  }
  return result as Status;
}

// Only the host's React/stream adapter lives here; native watch-status owns all
// device logic. Frame limits apply before decoding or JSON allocation.
export function useStatus() {
  const [data, setData] = useState<Status>({});
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);
  const [generation, setGeneration] = useState(0);
  const request = useRef<() => void>(() => {});
  useEffect(() => {
    let disposed = false;
    let stopped = false;
    let closed = false;
    let pendingRefresh = false;
    let pieces: Buffer[] = [];
    let bytes = 0;
    setLoading(true);
    setError(false);
    setData({});
    const child = spawn(binaries.control, ["watch-status"], {
      stdio: ["pipe", "pipe", "ignore"],
    });
    const shutdown = () => {
      if (stopped) return;
      stopped = true;
      pieces = [];
      bytes = 0;
      request.current = () => {};
      clearTimeout(startup);
      child.stdout.removeListener("data", receive);
      child.stdout.destroy();
      if (closed) return;
      const terminate = setTimeout(() => child.kill("SIGTERM"), 2000);
      const kill = setTimeout(() => child.kill("SIGKILL"), 4000);
      terminate.unref();
      kill.unref();
      child.once("close", () => {
        clearTimeout(terminate);
        clearTimeout(kill);
      });
      child.stdin.end();
    };
    const fail = () => {
      if (!disposed && !stopped) {
        setError(true);
        setLoading(false);
      }
      shutdown();
    };
    const receive = (chunk: Buffer) => {
      if (stopped || disposed) return;
      if (!Buffer.isBuffer(chunk) || chunk.length > maximumFrame * 2) {
        fail();
        return;
      }
      let offset = 0;
      while (offset < chunk.length && !stopped) {
        const end = chunk.indexOf(10, offset);
        const piece = chunk.subarray(offset, end < 0 ? chunk.length : end);
        bytes += piece.length;
        if (bytes > maximumFrame) {
          fail();
          return;
        }
        pieces.push(piece);
        if (end < 0) break;
        try {
          const patch = patchValue(
            JSON.parse(Buffer.concat(pieces, bytes).toString("utf8")),
          );
          pieces = [];
          bytes = 0;
          pendingRefresh = false;
          if (Object.keys(patch).length) {
            setData((previous) => ({ ...previous, ...patch }));
            setLoading(false);
            clearTimeout(startup);
          }
        } catch {
          fail();
          return;
        }
        offset = end + 1;
      }
    };
    const startup = setTimeout(fail, 10000);
    startup.unref();
    child.on("error", fail);
    child.on("close", () => {
      closed = true;
      fail();
    });
    child.stdin.on("error", fail);
    child.stdout.on("error", fail);
    child.stdout.on("data", receive);
    request.current = () => {
      if (!stopped && !pendingRefresh && !child.stdin.destroyed) {
        pendingRefresh = true;
        child.stdin.write("all\n");
      }
    };
    return () => {
      disposed = true;
      shutdown();
    };
  }, [generation]);
  return {
    data,
    error,
    loading,
    refresh: () => (error ? setGeneration((g) => g + 1) : request.current()),
  };
}
