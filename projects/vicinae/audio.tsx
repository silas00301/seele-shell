import { Action, ActionPanel, Icon, List } from "@raycast/api";
import React, { useRef, useState } from "react";
import { AudioDevice, audioArguments } from "./desktop";
import { binaries, perform, run, shell } from "./runtime";
import { useStatus } from "./status";

export default function Command() {
  const { data, error, loading, refresh } = useStatus();
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  async function select(device: AudioDevice) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    await perform("Switch audio device", async () => {
      // Numeric PipeWire IDs are recycled. Resolve the stable node/profile
      // against a fresh snapshot before changing the system default.
      const current = JSON.parse(await run(binaries.control, ["status"])) as {
        audioDevices: AudioDevice[];
      };
      const match = current.audioDevices.find(
        (candidate) =>
          candidate.kind === device.kind &&
          (device.node
            ? candidate.node === device.node
            : candidate.id === device.id &&
              candidate.profile === device.profile &&
              candidate.name === device.name),
      );
      if (!match) throw new Error("Device disconnected");
      await run(binaries.control, audioArguments(match));
      refresh();
    });
    pending.current = false;
    setBusy(false);
  }
  const reload = (
    <Action
      title="Refresh Audio"
      icon={Icon.ArrowClockwise}
      shortcut={{ modifiers: ["ctrl"], key: "r" }}
      onAction={refresh}
    />
  );
  return (
    <List
      isLoading={loading || busy}
      searchBarPlaceholder="Search speakers, headphones, or microphones..."
    >
      {error && (
        <List.Item
          title="Audio status unavailable"
          subtitle="Refresh to reconnect"
          icon={Icon.Warning}
          actions={<ActionPanel>{reload}</ActionPanel>}
        />
      )}
      {(["output", "input"] as const).map((kind) => (
        <List.Section
          key={kind}
          title={kind === "output" ? "Output" : "Microphone"}
        >
          {(data.audioDevices ?? [])
            .filter((device) => device.kind === kind)
            .map((device) => (
              <List.Item
                key={`${kind}-${device.id}-${device.profile}`}
                title={device.name}
                subtitle={
                  device.default
                    ? "Default"
                    : device.profile !== null
                      ? "Activate output profile"
                      : undefined
                }
                icon={kind === "output" ? Icon.SpeakerHigh : Icon.Microphone}
                accessories={device.default ? [{ text: "Selected" }] : []}
                actions={
                  <ActionPanel>
                    <Action
                      title="Use Device"
                      icon={Icon.Checkmark}
                      onAction={() => select(device)}
                    />
                    <Action
                      title="Open Audio Controls"
                      icon={Icon.SpeakerHigh}
                      onAction={() => shell(["control", "audio"])}
                    />
                    {reload}
                  </ActionPanel>
                }
              />
            ))}
        </List.Section>
      ))}
      <List.EmptyView
        title="No audio devices"
        description="Connect a device or open Seele Audio Controls."
      />
    </List>
  );
}
