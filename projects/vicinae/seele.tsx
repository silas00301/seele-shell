import { Action, ActionPanel, Icon, List, closeMainWindow } from "@raycast/api";
import { spawn } from "node:child_process";
import React from "react";

const ctl = "@SEELE_SHELLCTL@";

function run(args: string[]) {
  closeMainWindow();
  const child = spawn(ctl, args, { detached: true, stdio: "ignore" });
  child.unref();
}

const commands = [
  { title: "Control Center", subtitle: "Network, Bluetooth, camera, AirPods, sound, and now playing", icon: Icon.Gauge, args: ["center"] },
  { title: "AI Cockpit", subtitle: "Usage, limits, and agents", icon: Icon.Stars, args: ["agents"] },
  { title: "Launch Pi", subtitle: "Primary coding agent", icon: Icon.Terminal, args: ["agent", "pi"] },
  { title: "Launch OpenCode", subtitle: "Coding agent", icon: Icon.Code, args: ["agent", "opencode"] },
  { title: "Launch Codex", subtitle: "Coding agent", icon: Icon.CodeBlock, args: ["agent", "codex"] },
  { title: "Launch Claude Code", subtitle: "Coding agent", icon: Icon.Stars, args: ["agent", "claude"] },
  { title: "Audio Controls", subtitle: "Volume, output, and input devices", icon: Icon.SpeakerHigh, args: ["control", "audio"] },
  { title: "Network Controls", subtitle: "Connection status and settings", icon: Icon.Wifi, args: ["control", "network"] },
  { title: "VPN", subtitle: "Tailscale and Proton VPN", icon: Icon.Lock, args: ["control", "vpn"] },
  { title: "Bluetooth Controls", subtitle: "Devices, pairing, and autoconnect", icon: Icon.Bluetooth, args: ["control", "bluetooth"] },
  { title: "AirPods", subtitle: "Noise control and auto play", icon: Icon.Headphones, args: ["control", "airpods"] },
  { title: "Batteries", subtitle: "This device and connected gear", icon: Icon.Battery, args: ["control", "battery"] },
  { title: "Notifications", subtitle: "History and Do Not Disturb", icon: Icon.Bell, args: ["control", "notifications"] },
  { title: "Webcam", subtitle: "Preview and camera controls", icon: Icon.Video, args: ["control", "camera"] },
  { title: "Toggle Dictation", subtitle: "Voxtype speech input", icon: Icon.Microphone, args: ["voxtype"] },
  { title: "Lock", subtitle: "Lock this session", icon: Icon.Lock, args: ["lock"] },
];

export default function Command() {
  return (
    <List searchBarPlaceholder="Search Seele controls…">
      {commands.map((command) => (
        <List.Item
          key={command.title}
          title={command.title}
          subtitle={command.subtitle}
          icon={command.icon}
          actions={
            <ActionPanel>
              <Action title={command.title} onAction={() => run(command.args)} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
