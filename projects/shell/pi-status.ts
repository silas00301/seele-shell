import { mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const stateHome = process.env.XDG_STATE_HOME ?? join(process.env.HOME ?? ".", ".local", "state");
const stateDir = join(stateHome, "seele-shell", "agents");
const stateFile = join(stateDir, `pi-native-${process.pid}.json`);
const startedAt = new Date().toISOString();

function publish(status: "working" | "input") {
  mkdirSync(stateDir, { recursive: true });
  const temporary = `${stateFile}.${process.pid}.tmp`;
  writeFileSync(
    temporary,
    JSON.stringify({
      agent: "pi",
      status,
      source: "native",
      pid: process.pid,
      startedAt,
      updatedAt: new Date().toISOString(),
    }),
    { mode: 0o600 },
  );
  renameSync(temporary, stateFile);
}

export default function (pi: ExtensionAPI) {
  pi.on("session_start", () => publish("input"));
  pi.on("agent_start", () => publish("working"));
  pi.on("agent_settled", () => publish("input"));
  pi.on("session_shutdown", () => rmSync(stateFile, { force: true }));
}
