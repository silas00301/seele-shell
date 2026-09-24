import {
  Action,
  ActionPanel,
  Clipboard,
  Detail,
  Icon,
  showToast,
  Toast,
} from "@raycast/api";
import React, { useEffect, useRef, useState } from "react";
import { Buffer } from "node:buffer";
import { execFile } from "node:child_process";
import { binaries } from "./runtime";

type Preview = {
  original: string;
  cleaned: string;
  removed: string[];
  status: "cleaned" | "unchanged" | "protected" | "ambiguous";
  message: string;
  markdown: string;
};

// Clipboard payloads travel only over stdin. execFile owns the deadline,
// output bounds and abort; the native endpoint starts no descendant process.
export function loadPreview(text: string, signal: AbortSignal): Promise<Preview> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(new Error("Cancelled"));
      return;
    }
    // Bound host-side stdin buffering too; native validation stays authoritative.
    if (Buffer.byteLength(text, "utf8") > 16 * 1024) {
      reject(new Error("Link preview unavailable"));
      return;
    }
    const child = execFile(
      binaries.control,
      ["vicinae-clean-link"],
      { encoding: "utf8", timeout: 5000, maxBuffer: 256 * 1024, signal },
      (error, stdout) => {
        if (error) {
          reject(new Error("Link preview unavailable"));
          return;
        }
        try {
          resolve(JSON.parse(stdout) as Preview);
        } catch {
          reject(new Error("Link preview unavailable"));
        }
      },
    );
    child.stdin?.on("error", () => reject(new Error("Link preview unavailable")));
    child.stdin?.end(text);
  });
}

export default function Command() {
  const [preview, setPreview] = useState<Preview>();
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const active = useRef(false);
  const copying = useRef(false);
  // One host clipboard read per command mount. Re-renders never read or launch.
  useEffect(() => {
    const controller = new AbortController();
    active.current = true;
    void Clipboard.readText()
      .then(async (text) => {
        if (controller.signal.aborted) return;
        if (text === undefined) throw new Error("No text");
        const result = await loadPreview(text, controller.signal);
        if (!controller.signal.aborted) setPreview(result);
      })
      .catch(() => {
        if (!controller.signal.aborted) setFailed(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => {
      active.current = false;
      controller.abort();
    };
  }, []);

  async function copy() {
    if (!preview || !active.current || copying.current) return;
    copying.current = true;
    try {
      await Clipboard.copy(preview.cleaned);
      if (active.current)
        await showToast({ style: Toast.Style.Success, title: "Link copied" });
    } catch {
      if (active.current)
        await showToast({ style: Toast.Style.Failure, title: "Could not copy link" });
    } finally {
      copying.current = false;
    }
  }

  return (
    <Detail
      navigationTitle="Copy Clean Link"
      isLoading={loading}
      markdown={
        preview?.markdown ??
        (failed
          ? "Copy one HTTP or HTTPS link without spaces or control characters (maximum 16 KiB), then reopen this command. The clipboard is unchanged."
          : "Reading clipboard…")
      }
      actions={
        preview ? (
          <ActionPanel>
            <Action
              title={preview.status === "cleaned" ? "Copy Cleaned Link" : "Copy Unchanged Link"}
              icon={Icon.CopyClipboard}
              onAction={copy}
            />
          </ActionPanel>
        ) : undefined
      }
    />
  );
}
