import { spawnSync } from "node:child_process";

declare const SEELE_AGENT_HOOK: string;

// These hosts expose lifecycle events only through their extension APIs. All
// state validation, process identity and atomic publication belong to Rust.
export function statusPublisher(agent: "pi" | "opencode") {
  const binary = typeof SEELE_AGENT_HOOK === "string" ? SEELE_AGENT_HOOK : "seele-agent-hook";
  return (status: "working" | "input" | "end" | "host-event", payload?: unknown) => {
    const result = spawnSync(binary, [agent, status], {
      stdio: ["pipe", "ignore", "ignore"],
      input: payload === undefined ? "" : JSON.stringify(payload),
      timeout: 3000,
      killSignal: "SIGKILL",
      windowsHide: true,
    });
    // Status is ancillary: a failed publication must not break the model turn.
    if (result.error || result.status !== 0) return;
  };
}
