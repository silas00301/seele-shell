import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
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

// Consume the shell's field patches. Closing the view closes stdin, which also
// stops the worker's D-Bus and PipeWire listeners and its child processes.
export function useStatus() {
  const [data, setData] = useState<Status>({});
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);
  const [generation, setGeneration] = useState(0);
  const request = useRef<() => void>(() => {});
  useEffect(() => {
    let disposed = false;
    setLoading(true);
    setError(false);
    setData({});
    const child = spawn(binaries.control, ["watch-status"], {
      stdio: ["pipe", "pipe", "ignore"],
    });
    const lines = createInterface({ input: child.stdout });
    const fail = () => {
      if (!disposed) {
        setError(true);
        setLoading(false);
      }
    };
    child.on("error", fail);
    child.on("close", fail);
    child.stdin.on("error", fail);
    lines.on("line", (line) => {
      if (disposed) return;
      try {
        const patch = JSON.parse(line);
        if (!patch || typeof patch !== "object" || Array.isArray(patch))
          throw new Error("Invalid patch");
        setData((previous) => ({ ...previous, ...patch }));
        setLoading(false);
      } catch {
        fail();
      }
    });
    request.current = () => {
      if (!child.stdin.destroyed) child.stdin.write("all\n");
    };
    return () => {
      disposed = true;
      request.current = () => {};
      lines.close();
      child.stdin.end();
      // EOF is the normal shutdown protocol; bound cleanup if a probe hangs.
      const timer = setTimeout(() => child.kill("SIGTERM"), 2000);
      timer.unref();
      child.once("close", () => clearTimeout(timer));
    };
  }, [generation]);
  return {
    data,
    error,
    loading,
    refresh: () => (error ? setGeneration((g) => g + 1) : request.current()),
  };
}
