import { Action, ActionPanel, Color, Icon, Keyboard, List } from "@raycast/api";
import React from "react";

// Shared presentation for every Seele view. These are scalar expressions over
// data the native endpoints already validated; none of them starts a process
// or decides what an action is allowed to do.
export const shortcuts = {
  refresh: { modifiers: ["ctrl"], key: "r" } as Keyboard.Shortcut,
  copy: { modifiers: ["ctrl", "shift"], key: "c" } as Keyboard.Shortcut,
  secondaryCopy: { modifiers: ["shift"], key: "enter" } as Keyboard.Shortcut,
  toggle: { modifiers: ["ctrl"], key: "enter" } as Keyboard.Shortcut,
  raise: { modifiers: ["ctrl"], key: "arrowUp" } as Keyboard.Shortcut,
  lower: { modifiers: ["ctrl"], key: "arrowDown" } as Keyboard.Shortcut,
  close: { modifiers: ["ctrl"], key: "w" } as Keyboard.Shortcut,
  forceClose: { modifiers: ["ctrl", "shift"], key: "w" } as Keyboard.Shortcut,
  panel: { modifiers: ["ctrl"], key: "o" } as Keyboard.Shortcut,
};

export function RefreshAction(props: { title: string; onAction: () => void }) {
  return (
    <Action
      title={props.title}
      icon={Icon.ArrowClockwise}
      shortcut={shortcuts.refresh}
      onAction={props.onAction}
    />
  );
}

// One shape for every unavailable source: say what is missing, say what to do
// about it, and keep the subprocess error itself out of the interface.
export function Unavailable(props: {
  title: string;
  hint: string;
  refreshTitle: string;
  onRefresh: () => void;
}) {
  return (
    <List.Item
      title={props.title}
      subtitle={props.hint}
      icon={{ source: Icon.Warning, tintColor: Color.Orange }}
      actions={
        <ActionPanel>
          <RefreshAction
            title={props.refreshTitle}
            onAction={props.onRefresh}
          />
        </ActionPanel>
      }
    />
  );
}

export function percent(value?: number) {
  return value === undefined ? undefined : `${Math.round(value)}%`;
}

// A level reads as its number, with mute as the exception the eye should find.
export function levelAccessories(value?: number, muted?: boolean) {
  const accessories: List.Item.Accessory[] = [];
  if (muted) accessories.push({ tag: { value: "Muted", color: Color.Red } });
  const text = percent(value);
  if (text) accessories.push({ text });
  return accessories;
}

const applicationIcons: [RegExp, Icon][] = [
  [/ghostty|kitty|alacritty|foot|wezterm|term/i, Icon.Terminal],
  [/zen|firefox|chromium|chrome|brave|epiphany|browser/i, Icon.Compass],
  [/code|zed|neovide|jetbrains|idea/i, Icon.Code],
  [/discord|signal|telegram|slack|element|thunderbird/i, Icon.SpeechBubble],
  [/spotify|mpv|vlc|audacious|music/i, Icon.Music],
  [/steam|lutris|heroic|game/i, Icon.GameController],
  [/obsidian|zathura|evince|okular|notes/i, Icon.Pencil],
  [/nautilus|thunar|dolphin|yazi|files/i, Icon.Folder],
  [/imv|gimp|inkscape|loupe|image/i, Icon.Image],
  [/vicinae|seele/i, Icon.Stars],
];

export function applicationIcon(name: string) {
  return (
    applicationIcons.find(([match]) => match.test(name))?.[1] ?? Icon.AppWindow
  );
}

const audioIcons: [RegExp, Icon][] = [
  [/airpods/i, Icon.Airpods],
  [/headphone|headset|earbud|buds/i, Icon.Headphones],
  [/hdmi|displayport|\bdp\b|monitor/i, Icon.Monitor],
  [/webcam|camera/i, Icon.Video],
];

export function audioIcon(name: string, kind: string) {
  const match = audioIcons.find(([pattern]) => pattern.test(name))?.[1];
  if (match) return match;
  return kind === "input" ? Icon.Microphone : Icon.Speaker;
}

export function batteryIcon(status: string) {
  return /charging|full/i.test(status) ? Icon.BatteryCharging : Icon.Battery;
}

export function batteryColor(percent: number, status: string) {
  if (/charging/i.test(status)) return Color.Green;
  if (percent <= 15) return Color.Red;
  if (percent <= 30) return Color.Orange;
  return Color.SecondaryText;
}
