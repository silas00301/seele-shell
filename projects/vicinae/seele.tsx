import { Action, ActionPanel, Icon, List } from "@raycast/api";
import React from "react";
import { binaries, perform, run, shell } from "./runtime";
import { useStatus } from "./status";
import Audio from "./audio";
import Windows from "./windows";

const commands = [
  {
    title: "Notes and Voice Memos",
    subtitle: "Write, record, and keep ideas locally",
    icon: Icon.Document,
    args: ["notes"],
  },
  {
    title: "Control Center",
    subtitle: "Network, Bluetooth, camera, headphones, sound, and now playing",
    icon: Icon.Gauge,
    args: ["center"],
  },
  {
    title: "AI Cockpit",
    subtitle: "Usage, limits, and agents",
    icon: Icon.Stars,
    args: ["agents"],
  },
  {
    title: "Launch Pi",
    subtitle: "Primary coding agent",
    icon: Icon.Terminal,
    args: ["agent", "pi"],
  },
  {
    title: "Launch OpenCode",
    subtitle: "Coding agent",
    icon: Icon.Code,
    args: ["agent", "opencode"],
  },
  {
    title: "Launch Codex",
    subtitle: "Coding agent",
    icon: Icon.CodeBlock,
    args: ["agent", "codex"],
  },
  {
    title: "Launch Claude Code",
    subtitle: "Coding agent",
    icon: Icon.Stars,
    args: ["agent", "claude"],
  },
  {
    title: "Audio Controls",
    subtitle: "Volume, output, and input devices",
    icon: Icon.SpeakerHigh,
    args: ["control", "audio"],
  },
  {
    title: "Network Controls",
    subtitle: "Connection status and settings",
    icon: Icon.Wifi,
    args: ["control", "network"],
  },
  {
    title: "VPN",
    subtitle: "Tailscale and Proton VPN",
    icon: Icon.Lock,
    args: ["control", "vpn"],
  },
  {
    title: "Bluetooth Controls",
    subtitle: "Devices, pairing, and autoconnect",
    icon: Icon.Bluetooth,
    args: ["control", "bluetooth"],
  },
  {
    title: "Headphones",
    subtitle: "Nothing or AirPods noise control",
    icon: Icon.Headphones,
    args: ["control", "airpods"],
  },
  {
    title: "Batteries",
    subtitle: "This device and connected gear",
    icon: Icon.Battery,
    args: ["control", "battery"],
  },
  {
    title: "Notifications",
    subtitle: "History and Do Not Disturb",
    icon: Icon.Bell,
    args: ["control", "notifications"],
  },
  {
    title: "Webcam",
    subtitle: "Preview and camera controls",
    icon: Icon.Video,
    args: ["control", "camera"],
  },
  {
    title: "Toggle Dictation",
    subtitle: "Voxtype speech input",
    icon: Icon.Microphone,
    args: ["voxtype"],
  },
  {
    title: "Pick Screen Links and Codes",
    subtitle:
      "Freeze screens, then open or copy a numbered URI, QR code, or barcode",
    icon: Icon.Link,
    args: ["uris"],
  },
  {
    title: "Session Controls",
    subtitle: "Lock, suspend, restart, and shutdown",
    icon: Icon.Desktop,
    args: ["controls"],
  },
  {
    title: "Calendar",
    subtitle: "Month view and dates",
    icon: Icon.Calendar,
    args: ["control", "calendar"],
  },
  {
    title: "World Clock",
    subtitle: "Search timezones and manage pinned clocks",
    icon: Icon.Clock,
    args: ["control", "clock"],
  },
  {
    title: "Lock",
    subtitle: "Lock this session",
    icon: Icon.Lock,
    args: ["lock"],
  },
];

export default function Command() {
  const { data, error, loading, refresh } = useStatus();
  const airpodsConnected =
    !!data.headphones?.connected && /airpods/i.test(data.headphones.name);
  const level = (value?: number, muted?: boolean) =>
    value === undefined ? "Loading..." : `${value}%${muted ? " · Muted" : ""}`;
  const refreshAction = (
    <Action
      title="Refresh Status"
      icon={Icon.ArrowClockwise}
      shortcut={{ modifiers: ["ctrl"], key: "r" }}
      onAction={refresh}
    />
  );
  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search Seele controls, devices, or windows..."
    >
      {error && (
        <List.Item
          title="Live status unavailable"
          subtitle="Refresh to reconnect"
          icon={Icon.Warning}
          actions={<ActionPanel>{refreshAction}</ActionPanel>}
        />
      )}
      <List.Section title="Desktop">
        <List.Item
          title="Windows and Workspaces"
          subtitle="Switch by title, application, or workspace"
          icon={Icon.AppWindow}
          actions={
            <ActionPanel>
              <Action.Push
                title="Browse Windows"
                icon={Icon.AppWindow}
                target={<Windows />}
              />
            </ActionPanel>
          }
        />
        <List.Item
          title="Audio Devices"
          subtitle="Choose speakers, headphones, or a microphone"
          icon={Icon.Headphones}
          actions={
            <ActionPanel>
              <Action.Push
                title="Choose Audio Device"
                icon={Icon.Headphones}
                target={<Audio />}
              />
            </ActionPanel>
          }
        />
      </List.Section>
      <List.Section title="Live controls">
        {(["volume", "microphone"] as const).map((kind) => (
          <List.Item
            key={kind}
            title={kind === "volume" ? "Output Volume" : "Microphone Volume"}
            subtitle={
              kind === "volume"
                ? level(data.volume, data.muted)
                : level(data.microphoneVolume, data.microphoneMuted)
            }
            icon={kind === "volume" ? Icon.SpeakerHigh : Icon.Microphone}
            actions={
              <ActionPanel>
                <Action
                  title="Toggle Mute"
                  icon={Icon.SpeakerHigh}
                  onAction={() =>
                    perform("Toggle mute", async () => {
                      await run(binaries.shell, [kind, "mute"]);
                      refresh();
                    })
                  }
                />
                <Action
                  title="Raise Volume"
                  icon={Icon.Plus}
                  shortcut={{ modifiers: ["ctrl"], key: "arrowUp" }}
                  onAction={() =>
                    perform("Raise volume", async () => {
                      await run(binaries.shell, [kind, "up"]);
                      refresh();
                    })
                  }
                />
                <Action
                  title="Lower Volume"
                  icon={Icon.Minus}
                  shortcut={{ modifiers: ["ctrl"], key: "arrowDown" }}
                  onAction={() =>
                    perform("Lower volume", async () => {
                      await run(binaries.shell, [kind, "down"]);
                      refresh();
                    })
                  }
                />
                <Action
                  title="Open Audio Controls"
                  icon={Icon.SpeakerHigh}
                  onAction={() => shell(["control", "audio"])}
                />
                {refreshAction}
              </ActionPanel>
            }
          />
        ))}
        <List.Item
          title="Do Not Disturb"
          subtitle={
            data.dnd === undefined ? "Loading..." : data.dnd ? "On" : "Off"
          }
          icon={Icon.Bell}
          actions={
            <ActionPanel>
              <Action
                title="Toggle Do Not Disturb"
                icon={Icon.Bell}
                onAction={() =>
                  perform("Do Not Disturb", async () => {
                    await run(binaries.control, ["dnd"]);
                    refresh();
                  })
                }
              />
              <Action
                title="Open Notifications"
                icon={Icon.Bell}
                onAction={() => shell(["control", "notifications"])}
              />
              {refreshAction}
            </ActionPanel>
          }
        />
        {(data.microphoneActive ||
          data.cameraActive ||
          data.screenRecording) && (
          <List.Item
            title="Privacy Activity"
            subtitle={[
              data.microphoneActive && "Microphone in use",
              data.cameraActive && "Camera in use",
              data.screenRecording && "Screen recording",
            ]
              .filter(Boolean)
              .join(" · ")}
            icon={Icon.Eye}
            actions={
              <ActionPanel>
                <Action
                  title="Open Control Center"
                  icon={Icon.Gauge}
                  onAction={() => shell(["center"])}
                />
              </ActionPanel>
            }
          />
        )}
      </List.Section>
      <List.Section title="Seele panels and actions">
        {commands.map((command) => (
          <List.Item
            key={command.title}
            title={
              command.title === "Headphones" && airpodsConnected
                ? "AirPods"
                : command.title
            }
            subtitle={command.subtitle}
            icon={
              command.title === "Headphones" && airpodsConnected
                ? Icon.Airpods
                : command.icon
            }
            actions={
              <ActionPanel>
                <Action
                  title={
                    command.title === "Headphones" && airpodsConnected
                      ? "AirPods"
                      : command.title
                  }
                  icon={
                    command.title === "Headphones" && airpodsConnected
                      ? Icon.Airpods
                      : command.icon
                  }
                  onAction={() => shell(command.args)}
                />
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
    </List>
  );
}
