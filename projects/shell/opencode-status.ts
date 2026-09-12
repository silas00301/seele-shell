import type { Plugin } from "@opencode-ai/plugin";
import { statusPublisher } from "./status-hook";

const report = statusPublisher("opencode");
process.once("exit", () => report("end"));

export const SeeleShellStatus: Plugin = async () => {
  report("input");
  return {
    event: async ({ event }) => {
      // Project only host identity/status fields; Rust owns session aggregation
      // and transitions. Permission contents and model text never cross here.
      const properties = event.properties as Record<string, unknown>;
      const session = properties?.session as { id?: unknown } | undefined;
      const status = properties?.status as { type?: unknown } | undefined;
      report("host-event", { event: event.type, sessionID: properties?.sessionID,
        session: session?.id, status: status?.type });
    },
  };
};
