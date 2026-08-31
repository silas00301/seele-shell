import { mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Plugin } from "@opencode-ai/plugin";

const stateHome = process.env.XDG_STATE_HOME ?? join(process.env.HOME ?? ".", ".local", "state");
const stateDir = join(stateHome, "seele-shell", "agents");
const stateFile = join(stateDir, `opencode-native-${process.pid}.json`);
const startedAt = new Date().toISOString();

process.once("exit", () => rmSync(stateFile, { force: true }));

export const SeeleShellStatus: Plugin = async () => {
  const busy = new Set<string>();
  const waiting = new Set<string>();

  const publish = () => {
    const status = waiting.size > 0 ? "input" : busy.size > 0 ? "working" : "input";
    mkdirSync(stateDir, { recursive: true });
    const temporary = `${stateFile}.${process.pid}.tmp`;
    writeFileSync(
      temporary,
      JSON.stringify({
        agent: "opencode",
        status,
        source: "native",
        pid: process.pid,
        startedAt,
        updatedAt: new Date().toISOString(),
      }),
      { mode: 0o600 },
    );
    renameSync(temporary, stateFile);
  };

  publish();

  return {
    event: async ({ event }) => {
      const properties = event.properties as Record<string, unknown>;
      const session = properties.session as { id?: unknown } | undefined;
      const sessionID = String(properties.sessionID ?? session?.id ?? "");

      if (event.type === "session.status") {
        const status = properties.status as { type?: unknown } | undefined;
        if (status?.type === "busy" || status?.type === "retry") busy.add(sessionID);
        if (status?.type === "idle") {
          busy.delete(sessionID);
          waiting.delete(sessionID);
        }
      } else if (event.type === "session.idle" || event.type === "session.deleted") {
        busy.delete(sessionID);
        waiting.delete(sessionID);
      } else if (
        event.type === "permission.asked" ||
        event.type === "permission.v2.asked" ||
        event.type === "question.asked" ||
        event.type === "question.v2.asked" ||
        event.type === "session.error"
      ) {
        waiting.add(sessionID);
      } else if (
        event.type === "permission.replied" ||
        event.type === "permission.v2.replied" ||
        event.type === "question.replied" ||
        event.type === "question.rejected" ||
        event.type === "question.v2.replied" ||
        event.type === "question.v2.rejected"
      ) {
        waiting.delete(sessionID);
      } else {
        return;
      }

      publish();
    },
  };
};
