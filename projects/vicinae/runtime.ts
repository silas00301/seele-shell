import { closeMainWindow, showToast, Toast } from "@raycast/api";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { useCallback, useEffect, useRef, useState } from "react";

const execute = promisify(execFile);
export const binaries = {
  shell: "@SEELE_SHELLCTL@",
  control: "@SEELE_CONTROL@",
  hyprctl: "@HYPRCTL@",
  wtype: "@WTYPE@",
  nvd: "@NVD@",
  nixosRebuild: "/run/current-system/sw/bin/nixos-rebuild",
  run0: "/run/current-system/sw/bin/run0",
  switchGeneration: "@SWITCH_GENERATION@",
};

export async function run(
  file: string,
  args: string[],
  signal?: AbortSignal,
  timeout = 15000,
) {
  const { stdout } = await execute(file, args, {
    encoding: "utf8",
    timeout,
    maxBuffer: 4 * 1024 * 1024,
    signal,
  });
  return stdout;
}

// One request at a time, and no child process survives the view that owns it.
export function useQuery<T>(
  load: (signal: AbortSignal) => Promise<T>,
  interval = 5000,
) {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const refreshRef = useRef<() => void>(() => {});
  useEffect(() => {
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    let busy = false;
    async function refresh() {
      if (busy || controller.signal.aborted) return;
      clearTimeout(timer);
      busy = true;
      setLoading(true);
      try {
        const next = await load(controller.signal);
        if (!controller.signal.aborted) {
          setData(next);
          setError(undefined);
        }
      } catch (error) {
        if (!controller.signal.aborted)
          setError(error instanceof Error ? error.message : String(error));
      } finally {
        busy = false;
        if (!controller.signal.aborted) {
          setLoading(false);
          if (interval) timer = setTimeout(refresh, interval);
        }
      }
    }
    refreshRef.current = refresh;
    void refresh();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [load, interval]);
  return {
    data,
    error,
    loading,
    refresh: useCallback(() => refreshRef.current(), []),
  };
}

export async function perform(
  title: string,
  action: () => Promise<unknown>,
  dismiss = false,
) {
  try {
    if (dismiss) await closeMainWindow();
    await action();
  } catch {
    // Child errors can include private command output; keep notifications generic.
    await showToast({
      style: Toast.Style.Failure,
      title: `${title} failed`,
      message: "Check that the desktop service is running and try again.",
    });
  }
}

export function shell(args: string[]) {
  return perform("Seele action", () => run(binaries.shell, args), true);
}
