import { Action, ActionPanel, Icon, List } from "@raycast/api";
import React, { useRef, useState } from "react";
import { AudioDevice } from "./desktop";
import { binaries, perform, run, shell } from "./runtime";
import { useStatus } from "./status";

export default function Command() {
  const { data, error, loading, refresh } = useStatus();
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  async function select(device: AudioDevice, toggle = false) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    await perform("Switch audio device", async () => {
      // Native selection resolves the reviewed node/profile against current
      // PipeWire state before choosing a device or changing combined playback.
      await run(binaries.control, [
        "vicinae-audio",
        JSON.stringify(device),
        toggle ? "toggle" : "select",
      ]);
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
                accessories={
                  device.selected || device.default
                    ? [{ text: "Selected" }]
                    : []
                }
                actions={
                  <ActionPanel>
                    <Action
                      title="Use Device"
                      icon={Icon.Checkmark}
                      onAction={() => select(device)}
                    />
                    {kind === "output" && device.node && (
                      <Action
                        title={
                          device.selected || device.default
                            ? "Remove from Playback"
                            : "Play Here Too"
                        }
                        icon={Icon.SpeakerHigh}
                        shortcut={{ modifiers: ["ctrl"], key: "enter" }}
                        onAction={() => select(device, true)}
                      />
                    )}
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
