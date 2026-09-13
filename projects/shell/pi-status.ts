import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { statusPublisher } from "./status-hook";

const publish = statusPublisher("pi");

export default function (pi: ExtensionAPI) {
  pi.on("session_start", () => publish("input"));
  pi.on("agent_start", () => publish("working"));
  pi.on("agent_settled", () => publish("input"));
  pi.on("session_shutdown", () => publish("end"));
}
