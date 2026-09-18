import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import React, { useRef, useState } from "react";
import { AudioDevice } from "./desktop";
import { binaries, perform, run, shell } from "./runtime";
import { useStatus } from "./status";
import {
  RefreshAction,
  Unavailable,
  audioIcon,
  levelAccessories,
  shortcuts,
} from "./ui";

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
  const level = (
    kind: "volume" | "microphone",
    action: string,
    title: string,
  ) =>
    perform(title, async () => {
      await run(binaries.shell, [kind, action]);
      refresh();
    });
  const reload = <RefreshAction title="Refresh Audio" onAction={refresh} />;
  const devices = data.audioDevices ?? [];
  const playing = devices.filter(
    (device) =>
      device.kind === "output" &&
      device.node &&
      (device.selected || device.default),
  );

  return (
    <List
      isLoading={loading || busy}
      searchBarPlaceholder="Search speakers, headphones, or microphones..."
    >
      {error && (
        <Unavailable
          title="Audio status unavailable"
          hint="The desktop status feed stopped. Refresh to reconnect."
          refreshTitle="Reconnect Audio"
          onRefresh={refresh}
        />
      )}
      {(["output", "input"] as const).map((kind) => {
        const kindDevices = devices.filter((device) => device.kind === kind);
        const stream = kind === "output" ? "volume" : "microphone";
        const muted = kind === "output" ? data.muted : data.microphoneMuted;
        const value = kind === "output" ? data.volume : data.microphoneVolume;
        return (
          <List.Section
            key={kind}
            title={kind === "output" ? "Output" : "Microphone"}
            subtitle={
              kind === "output" && playing.length > 1
                ? `${playing.length} outputs playing together`
                : String(kindDevices.length)
            }
          >
            {kindDevices.map((device) => {
              const active = device.default || device.selected;
              const shared = Boolean(device.selected) && !device.default;
              const inactiveProfile = device.profile !== null;
              return (
                <List.Item
                  key={`${kind}-${device.id}-${device.profile}`}
                  title={device.name}
                  subtitle={
                    inactiveProfile ? "Activates this card profile" : undefined
                  }
                  icon={audioIcon(device.name, kind)}
                  keywords={[kind, device.node, "device", "sound"]}
                  accessories={[
                    ...(device.default
                      ? [{ tag: { value: "Default", color: Color.Green } }]
                      : shared
                        ? [
                            {
                              tag: { value: "Also playing", color: Color.Blue },
                            },
                          ]
                        : inactiveProfile
                          ? [
                              {
                                tag: {
                                  value: "Profile",
                                  color: Color.SecondaryText,
                                },
                              },
                            ]
                          : []),
                    ...(active ? levelAccessories(value, muted) : []),
                  ]}
                  actions={
                    <ActionPanel>
                      <Action
                        title={
                          playing.length > 1 && active
                            ? "Use Only This Device"
                            : "Use Device"
                        }
                        icon={Icon.Checkmark}
                        onAction={() => select(device)}
                      />
                      {kind === "output" && device.node && (
                        <Action
                          title={
                            shared || device.default
                              ? "Remove from Playback"
                              : "Play Here Too"
                          }
                          icon={
                            shared || device.default ? Icon.Minus : Icon.Plus
                          }
                          shortcut={shortcuts.toggle}
                          onAction={() => select(device, true)}
                        />
                      )}
                      <Action
                        title={muted ? "Unmute" : "Mute"}
                        icon={
                          kind === "output"
                            ? muted
                              ? Icon.SpeakerHigh
                              : Icon.SpeakerOff
                            : muted
                              ? Icon.Microphone
                              : Icon.MicrophoneDisabled
                        }
                        onAction={() => level(stream, "mute", "Toggle mute")}
                      />
                      <Action
                        title="Raise Volume"
                        icon={Icon.Plus}
                        shortcut={shortcuts.raise}
                        onAction={() => level(stream, "up", "Raise volume")}
                      />
                      <Action
                        title="Lower Volume"
                        icon={Icon.Minus}
                        shortcut={shortcuts.lower}
                        onAction={() => level(stream, "down", "Lower volume")}
                      />
                      <Action
                        title="Open Audio Controls"
                        icon={Icon.SpeakerHigh}
                        shortcut={shortcuts.panel}
                        onAction={() => shell(["control", "audio"])}
                      />
                      {reload}
                    </ActionPanel>
                  }
                />
              );
            })}
          </List.Section>
        );
      })}
      <List.EmptyView
        title="No audio devices"
        description="Connect a device, or open the Seele Audio panel."
      />
    </List>
  );
}
