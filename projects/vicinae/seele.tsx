import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import React from "react";
import { binaries, perform, run, shell } from "./runtime";
import { Status, useStatus } from "./status";
import {
  RefreshAction,
  Unavailable,
  audioIcon,
  batteryColor,
  batteryIcon,
  levelAccessories,
  percent,
  shortcuts,
} from "./ui";
import Audio from "./audio";
import Generations from "./generations";
import Keybindings from "./keybindings";
import Windows from "./windows";

type Entry = {
  title: string;
  subtitle: string;
  icon: Icon;
  args: string[];
  keywords?: string[];
};

const panels: Entry[] = [
  {
    title: "Control Center",
    subtitle: "Network, Bluetooth, camera, sound, and now playing",
    icon: Icon.Gauge,
    args: ["center"],
    keywords: ["center", "settings", "quick"],
  },
  {
    title: "Notifications",
    subtitle: "History and Do Not Disturb",
    icon: Icon.Bell,
    args: ["control", "notifications"],
    keywords: ["alerts", "toasts", "dnd"],
  },
  {
    title: "Now Playing",
    subtitle: "Artwork, transport, and timeline",
    icon: Icon.Music,
    args: ["control", "media"],
    keywords: ["media", "player", "music", "spotify", "mpris"],
  },
  {
    title: "Audio Controls",
    subtitle: "Volume, output, and input devices",
    icon: Icon.SpeakerHigh,
    args: ["control", "audio"],
    keywords: ["sound", "volume", "speakers"],
  },
  {
    title: "Network Controls",
    subtitle: "Connection status, addresses, and speed test",
    icon: Icon.Wifi,
    args: ["control", "network"],
    keywords: ["wifi", "internet", "ethernet", "ssh"],
  },
  {
    title: "VPN",
    subtitle: "Tailscale and Proton VPN",
    icon: Icon.Lock,
    args: ["control", "vpn"],
    keywords: ["tailscale", "proton", "privacy"],
  },
  {
    title: "Bluetooth Controls",
    subtitle: "Devices, pairing, and audio receiver",
    icon: Icon.Bluetooth,
    args: ["control", "bluetooth"],
    keywords: ["pair", "devices"],
  },
  {
    title: "Headphones",
    subtitle: "Nothing or AirPods noise control",
    icon: Icon.Headphones,
    args: ["control", "airpods"],
    keywords: ["airpods", "anc", "transparency", "noise"],
  },
  {
    title: "Batteries",
    subtitle: "This device and connected gear",
    icon: Icon.Battery,
    args: ["control", "battery"],
    keywords: ["power", "charge"],
  },
  {
    title: "Webcam",
    subtitle: "Preview and camera controls",
    icon: Icon.Video,
    args: ["control", "camera"],
    keywords: ["camera", "video", "brio"],
  },
  {
    title: "Calendar",
    subtitle: "Month view and ISO weeks",
    icon: Icon.Calendar,
    args: ["control", "calendar"],
    keywords: ["date", "month", "week"],
  },
  {
    title: "Calculator",
    subtitle: "Arithmetic, unit conversions and a private tape",
    icon: Icon.Calculator,
    args: ["calculator"],
    keywords: ["math", "convert", "units", "tape"],
  },
  {
    title: "Meeting Planner",
    subtitle: "Find working-hour overlap across pinned world clocks",
    icon: Icon.Clock,
    args: ["control", "meeting"],
    keywords: ["meeting", "schedule", "timezone", "overlap"],
  },
  {
    title: "World Clock",
    subtitle: "Search timezones and manage pinned clocks",
    icon: Icon.Clock,
    args: ["control", "clock"],
    keywords: ["time", "timezone", "utc"],
  },
  {
    title: "GitHub Inbox",
    subtitle: "Triaged notifications and pull requests",
    icon: Icon.Hashtag,
    args: ["control", "github"],
    keywords: ["notifications", "reviews", "issues", "pr"],
  },
  {
    title: "Home Assistant",
    subtitle: "Rooms, favorites, and device controls",
    icon: Icon.House,
    args: ["control", "home-assistant"],
    keywords: ["smart", "lights", "sensors", "hass"],
  },
  {
    title: "System Health",
    subtitle: "Integration health and maintenance findings",
    icon: Icon.Heartbeat,
    args: ["control", "system-health"],
    keywords: ["maintenance", "status", "diagnostics"],
  },
  {
    title: "Transfers",
    subtitle: "Send files to personal devices and view received files",
    icon: Icon.Download,
    args: ["transfers"],
    keywords: ["taildrop", "files", "send", "receive"],
  },
];

const agents: Entry[] = [
  {
    title: "AI Cockpit",
    subtitle: "Usage, limits, and running sessions",
    icon: Icon.Stars,
    args: ["agents"],
    keywords: ["usage", "limits", "tokens"],
  },
  {
    title: "Quick AI Prompt",
    subtitle: "Ask about the screen, selection, or focused window",
    icon: Icon.Wand,
    args: ["prompt"],
    keywords: ["ask", "codex", "prompt"],
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
];

const actions: Entry[] = [
  {
    title: "Notes and Voice Memos",
    subtitle: "Write, record, and keep ideas locally",
    icon: Icon.Pencil,
    args: ["notes"],
    keywords: ["obsidian", "capture", "memo", "dictate"],
  },
  {
    title: "Toggle Dictation",
    subtitle: "Voxtype speech input",
    icon: Icon.Microphone,
    args: ["voxtype"],
    keywords: ["voice", "speech", "transcribe"],
  },
  {
    title: "Pick Screen Links and Codes",
    subtitle: "Freeze screens, then open or copy a URI, QR code, or barcode",
    icon: Icon.Link,
    args: ["uris"],
    keywords: ["qr", "barcode", "url", "ocr"],
  },
  {
    title: "Session Controls",
    subtitle: "Lock, suspend, restart, and shutdown",
    icon: Icon.Power,
    args: ["controls"],
    keywords: ["logout", "reboot", "suspend"],
  },
  {
    title: "Lock Session",
    subtitle: "Lock this session now",
    icon: Icon.Lock,
    args: ["lock"],
    keywords: ["screen", "away"],
  },
];

// The headphones entry names what is actually connected, so the row the eye
// looks for after putting AirPods in is the one it finds.
function headphoneEntry(entry: Entry, status: Status): Entry {
  if (entry.title !== "Headphones" || !status.headphones?.connected)
    return entry;
  const airpods = /airpods/i.test(status.headphones.name);
  return {
    ...entry,
    title: airpods ? "AirPods" : entry.title,
    subtitle: status.headphones.name || entry.subtitle,
    icon: airpods ? Icon.Airpods : Icon.Headphones,
  };
}

function EntryItem({ entry, status }: { entry: Entry; status: Status }) {
  const item = headphoneEntry(entry, status);
  return (
    <List.Item
      title={item.title}
      subtitle={item.subtitle}
      icon={item.icon}
      keywords={[
        ...(item.keywords ?? []),
        // "control" is how every panel is opened, so it says nothing here.
        ...item.args.filter((argument) => argument !== "control"),
      ]}
      actions={
        <ActionPanel>
          <Action
            title={`Open ${item.title}`}
            icon={item.icon}
            onAction={() => shell(item.args)}
          />
        </ActionPanel>
      }
    />
  );
}

function networkSummary(status: Status) {
  if (status.connection === undefined) return undefined;
  const wireless = /wireless|wifi/i.test(status.connectionType ?? "");
  const limited =
    status.connectivity && !/^full$/i.test(status.connectivity)
      ? ` · ${status.connectivity}`
      : "";
  return `${status.connection}${wireless ? " · Wi-Fi" : ""}${limited}`;
}

export default function Command() {
  const { data, error, loading, refresh } = useStatus();
  const act = (title: string, args: string[]) =>
    perform(title, async () => {
      await run(binaries.control, args);
      refresh();
    });
  const audioAct = (title: string, args: string[]) =>
    perform(title, async () => {
      await run(binaries.shell, args);
      refresh();
    });
  const refreshAction = (
    <RefreshAction title="Refresh Status" onAction={refresh} />
  );
  const volumeActions = (kind: "volume" | "microphone", muted?: boolean) => (
    <ActionPanel>
      <Action
        title={muted ? "Unmute" : "Mute"}
        icon={
          kind === "volume"
            ? muted
              ? Icon.SpeakerHigh
              : Icon.SpeakerOff
            : muted
              ? Icon.Microphone
              : Icon.MicrophoneDisabled
        }
        onAction={() => audioAct("Toggle mute", [kind, "mute"])}
      />
      <Action
        title="Raise Volume"
        icon={Icon.Plus}
        shortcut={shortcuts.raise}
        onAction={() => audioAct("Raise volume", [kind, "up"])}
      />
      <Action
        title="Lower Volume"
        icon={Icon.Minus}
        shortcut={shortcuts.lower}
        onAction={() => audioAct("Lower volume", [kind, "down"])}
      />
      <ActionPanel.Submenu title="Set Level" icon={Icon.Gauge}>
        {[0, 25, 50, 75, 100].map((level) => (
          <Action
            key={level}
            title={`${level}%`}
            icon={Icon.Gauge}
            onAction={() => audioAct("Set level", [kind, String(level)])}
          />
        ))}
      </ActionPanel.Submenu>
      <Action
        title="Open Audio Controls"
        icon={Icon.SpeakerHigh}
        shortcut={shortcuts.panel}
        onAction={() => shell(["control", "audio"])}
      />
      {refreshAction}
    </ActionPanel>
  );
  const network = networkSummary(data);
  const batteries = data.batteries ?? [];

  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search Seele controls, panels, devices, or windows..."
    >
      {error && (
        <Unavailable
          title="Live status unavailable"
          hint="The desktop status feed stopped. Refresh to reconnect."
          refreshTitle="Reconnect Status"
          onRefresh={refresh}
        />
      )}
      <List.Section title="Live controls">
        <List.Item
          title="Output Volume"
          subtitle={data.muted ? "Muted" : undefined}
          icon={data.muted ? Icon.SpeakerOff : Icon.SpeakerHigh}
          keywords={["sound", "speaker", "mute", "volume"]}
          accessories={levelAccessories(data.volume, data.muted)}
          actions={volumeActions("volume", data.muted)}
        />
        <List.Item
          title="Microphone Volume"
          subtitle={data.microphoneMuted ? "Muted" : undefined}
          icon={
            data.microphoneMuted ? Icon.MicrophoneDisabled : Icon.Microphone
          }
          keywords={["mic", "input", "mute"]}
          accessories={levelAccessories(
            data.microphoneVolume,
            data.microphoneMuted,
          )}
          actions={volumeActions("microphone", data.microphoneMuted)}
        />
        <List.Item
          title="Do Not Disturb"
          icon={data.dnd ? Icon.BellDisabled : Icon.Bell}
          keywords={["dnd", "quiet", "silence", "notifications"]}
          accessories={
            data.dnd === undefined
              ? []
              : [
                  {
                    tag: {
                      value: data.dnd ? "On" : "Off",
                      color: data.dnd ? Color.Orange : Color.SecondaryText,
                    },
                  },
                ]
          }
          actions={
            <ActionPanel>
              <Action
                title={
                  data.dnd
                    ? "Turn off Do Not Disturb"
                    : "Turn on Do Not Disturb"
                }
                icon={data.dnd ? Icon.Bell : Icon.BellDisabled}
                onAction={() => act("Do Not Disturb", ["dnd"])}
              />
              <ActionPanel.Submenu title="Quiet For" icon={Icon.Moon}>
                {[
                  ["15 Minutes", 15],
                  ["1 Hour", 60],
                  ["4 Hours", 240],
                ].map(([title, minutes]) => (
                  <Action
                    key={String(minutes)}
                    title={String(title)}
                    icon={Icon.Moon}
                    onAction={() =>
                      audioAct("Quiet period", [
                        "notification",
                        "snooze",
                        String(minutes),
                      ])
                    }
                  />
                ))}
              </ActionPanel.Submenu>
              <Action
                title="Open Notifications"
                icon={Icon.Bell}
                shortcut={shortcuts.panel}
                onAction={() => shell(["control", "notifications"])}
              />
              {refreshAction}
            </ActionPanel>
          }
        />
        {data.wifiAvailable && (
          <List.Item
            title="Wi-Fi"
            subtitle={network}
            icon={data.wifiEnabled ? Icon.Wifi : Icon.WifiDisabled}
            keywords={["network", "internet", "wireless", "ssid"]}
            accessories={[
              {
                tag: {
                  value: data.wifiEnabled ? "On" : "Off",
                  color: data.wifiEnabled ? Color.Green : Color.SecondaryText,
                },
              },
            ]}
            actions={
              <ActionPanel>
                <Action
                  title={data.wifiEnabled ? "Turn off Wi-Fi" : "Turn on Wi-Fi"}
                  icon={data.wifiEnabled ? Icon.WifiDisabled : Icon.Wifi}
                  onAction={() => act("Wi-Fi", ["wifi", "toggle"])}
                />
                <Action
                  title="Open Network Controls"
                  icon={Icon.Wifi}
                  shortcut={shortcuts.panel}
                  onAction={() => shell(["control", "network"])}
                />
                {refreshAction}
              </ActionPanel>
            }
          />
        )}
        {data.bluetoothAvailable && (
          <List.Item
            title="Bluetooth"
            subtitle={
              data.bluetoothConnected
                ? `${data.bluetoothConnected} connected`
                : undefined
            }
            icon={Icon.Bluetooth}
            keywords={["pair", "headphones", "devices"]}
            accessories={[
              {
                tag: {
                  value: data.bluetoothPowered ? "On" : "Off",
                  color: data.bluetoothPowered
                    ? Color.Blue
                    : Color.SecondaryText,
                },
              },
            ]}
            actions={
              <ActionPanel>
                <Action
                  title={
                    data.bluetoothPowered
                      ? "Turn off Bluetooth"
                      : "Turn on Bluetooth"
                  }
                  icon={Icon.Bluetooth}
                  onAction={() => act("Bluetooth", ["bluetooth", "toggle"])}
                />
                <Action
                  title="Open Bluetooth Controls"
                  icon={Icon.Bluetooth}
                  shortcut={shortcuts.panel}
                  onAction={() => shell(["control", "bluetooth"])}
                />
                {refreshAction}
              </ActionPanel>
            }
          />
        )}
        {data.tailscale?.available && (
          <List.Item
            title="Tailscale"
            subtitle={
              data.tailscale.connected
                ? `${data.tailscale.name}${data.tailscale.tailnet ? ` · ${data.tailscale.tailnet}` : ""}`
                : data.tailscale.needsLogin
                  ? "Sign-in required"
                  : "Disconnected"
            }
            icon={Icon.Network}
            keywords={["vpn", "mesh", "tailnet", "peers"]}
            accessories={
              data.tailscale.connected
                ? [
                    {
                      text: `${data.tailscale.onlinePeers}/${data.tailscale.peers} online`,
                    },
                    { tag: { value: "Connected", color: Color.Green } },
                  ]
                : [{ tag: { value: "Off", color: Color.SecondaryText } }]
            }
            actions={
              <ActionPanel>
                <Action
                  title="Open VPN Panel"
                  icon={Icon.Lock}
                  onAction={() => shell(["control", "vpn"])}
                />
                {refreshAction}
              </ActionPanel>
            }
          />
        )}
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
            icon={{ source: Icon.Eye, tintColor: Color.Orange }}
            keywords={["camera", "microphone", "recording", "privacy"]}
            actions={
              <ActionPanel>
                <Action
                  title="Open Control Center"
                  icon={Icon.Gauge}
                  onAction={() => shell(["center"])}
                />
                {refreshAction}
              </ActionPanel>
            }
          />
        )}
      </List.Section>
      {batteries.length > 0 && (
        <List.Section title="Batteries" subtitle={String(batteries.length)}>
          {batteries.map((battery) => (
            <List.Item
              key={`${battery.kind}-${battery.name}`}
              title={battery.name}
              subtitle={battery.status || undefined}
              icon={batteryIcon(battery.status)}
              keywords={["battery", "charge", "power", battery.kind]}
              accessories={[
                {
                  tag: {
                    value: percent(battery.percent) ?? "",
                    color: batteryColor(battery.percent, battery.status),
                  },
                },
              ]}
              actions={
                <ActionPanel>
                  <Action
                    title="Open Batteries"
                    icon={Icon.Battery}
                    onAction={() => shell(["control", "battery"])}
                  />
                  {refreshAction}
                </ActionPanel>
              }
            />
          ))}
        </List.Section>
      )}
      <List.Section title="Browse">
        <List.Item
          title="Windows and Workspaces"
          subtitle="Switch by title, application, or workspace"
          icon={Icon.AppWindow}
          keywords={["window", "workspace", "switch", "focus", "alt tab"]}
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
          icon={audioIcon(data.headphones?.name ?? "", "output")}
          keywords={["output", "input", "speaker", "microphone", "device"]}
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
        <List.Item
          title="Keybindings"
          subtitle="Search and replay active Hyprland shortcuts"
          icon={Icon.Keyboard}
          keywords={["shortcut", "hyprland", "keys", "cheatsheet"]}
          actions={
            <ActionPanel>
              <Action.Push
                title="Search Keybindings"
                icon={Icon.Keyboard}
                target={<Keybindings />}
              />
            </ActionPanel>
          }
        />
        <List.Item
          title="NixOS Generations"
          subtitle="Review a retained generation and roll back"
          icon={Icon.Layers}
          keywords={["nixos", "rollback", "generation", "system"]}
          actions={
            <ActionPanel>
              <Action.Push
                title="Review Generations"
                icon={Icon.Layers}
                target={<Generations />}
              />
            </ActionPanel>
          }
        />
      </List.Section>
      <List.Section title="Panels">
        {panels.map((entry) => (
          <EntryItem key={entry.title} entry={entry} status={data} />
        ))}
      </List.Section>
      <List.Section title="AI">
        {agents.map((entry) => (
          <EntryItem key={entry.title} entry={entry} status={data} />
        ))}
      </List.Section>
      <List.Section title="Actions">
        {actions.map((entry) => (
          <EntryItem key={entry.title} entry={entry} status={data} />
        ))}
      </List.Section>
      <List.EmptyView
        title="No matching Seele control"
        description="Try a device, panel, or window name."
      />
    </List>
  );
}
