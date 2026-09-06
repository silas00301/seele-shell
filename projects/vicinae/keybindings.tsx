import { Action, ActionPanel, Clipboard, Icon, List } from "@raycast/api";
import { execFileSync } from "node:child_process";
import React from "react";

const hyprctl = "@HYPRCTL@";
const wtype = "@WTYPE@";

type Binding = {
  modmask?: number;
  key?: string;
  keycode?: number;
  dispatcher?: string;
  arg?: string;
  description?: string;
  release?: boolean;
};

type DisplayBinding = Binding & {
  id: string;
  shortcut: string;
  action: string;
  description: string;
};

const keyNames: Record<string, string> = {
  RETURN: "Enter",
  ESCAPE: "Escape",
  XF86AudioRaiseVolume: "Volume Up",
  XF86AudioLowerVolume: "Volume Down",
  XF86AudioMute: "Mute",
  XF86AudioMicMute: "Mic Mute",
  XF86AudioPlay: "Play/Pause",
  XF86AudioPause: "Pause",
  XF86AudioNext: "Next Track",
  XF86AudioPrev: "Previous Track",
  "mouse:272": "Mouse Left",
  "mouse:273": "Mouse Right",
};

const fallbackDescriptions: Record<string, string> = {
  "Super + Mouse Left": "Move the focused window",
  "Super + Mouse Right": "Resize the focused window",
  "Super + Alt + Mouse Left": "Resize the focused window",
  "Super + Q": "Close the focused window",
  "Super + W": "Open a terminal",
  "Super + Enter": "Open a terminal",
  "Super + B": "Open the web browser",
  "Super + L": "Lock the session",
  "Super + S": "Capture a screenshot",
  "Super + Shift + S": "Capture and annotate a screenshot",
  "Super + Shift + Space": "Toggle floating for the focused window",
  "Super + F": "Toggle maximized for the focused window",
  "Super + Shift + F": "Toggle fullscreen for the focused window",
  "Super + Ctrl + Alt + Shift + Q": "Open the shutdown menu",
  "Super + Ctrl + F12": "Lock the session",
  "Ctrl + Shift + H": "Swap with the window to the left",
  "Ctrl + Shift + J": "Swap with the window below",
  "Ctrl + Shift + K": "Swap with the window above",
  "Ctrl + Shift + L": "Swap with the window to the right",
  "Ctrl + H": "Focus the window to the left",
  "Ctrl + J": "Focus the window below",
  "Ctrl + K": "Focus the window above",
  "Ctrl + L": "Focus the window to the right",
  "Super + Shift + H": "Shrink the focused window horizontally",
  "Super + Shift + J": "Grow the focused window vertically",
  "Super + Shift + K": "Shrink the focused window vertically",
  "Super + Shift + L": "Grow the focused window horizontally",
  "Super + Tab": "Move the workspace to the next monitor",
  "Super + Shift + Tab": "Move the workspace to the previous monitor",
  "XF86AudioRaiseVolume": "Raise the volume",
  "XF86AudioLowerVolume": "Lower the volume",
  "XF86AudioMute": "Toggle mute",
  "XF86AudioMicMute": "Toggle microphone mute",
  "XF86AudioPlay": "Toggle media playback",
  "XF86AudioPause": "Toggle media playback",
  "XF86AudioNext": "Play the next track",
  "XF86AudioPrev": "Play the previous track",
  "Alt + Space": "Open the Vicinae application launcher",
  "Super + A": "Open the AI cockpit",
  "Super + Shift + A": "Launch Pi",
  "Super + C": "Open the Control Center",
  "Super + N": "Open notifications",
  "Super + Escape": "Open session controls",
  "Super + K": "Search Hyprland keybindings",
};

for (let workspace = 1; workspace <= 4; workspace += 1) {
  fallbackDescriptions[`Super + ${workspace}`] = `Focus workspace ${workspace}`;
  fallbackDescriptions[`Super + Shift + ${workspace}`] = `Move the focused window to workspace ${workspace}`;
}
for (let workspace = 5; workspace <= 8; workspace += 1) {
  const key = workspace - 4;
  fallbackDescriptions[`Super + Alt + ${key}`] = `Focus workspace ${workspace}`;
  fallbackDescriptions[`Super + Alt + Shift + ${key}`] = `Move the focused window to workspace ${workspace}`;
}
for (let workspace = 9; workspace <= 12; workspace += 1) {
  const key = workspace - 8;
  fallbackDescriptions[`Super + Ctrl + ${key}`] = `Focus workspace ${workspace}`;
  fallbackDescriptions[`Super + Ctrl + Shift + ${key}`] = `Move the focused window to workspace ${workspace}`;
}

function modifiers(mask = 0) {
  const names: string[] = [];
  if (mask & 64) names.push("Super");
  if (mask & 4) names.push("Ctrl");
  if (mask & 8) names.push("Alt");
  if (mask & 1) names.push("Shift");
  return names;
}

function formatKey(key: string) {
  return keyNames[key] || key;
}

function isGeneratedLuaDescription(description: string) {
  return /^(?:__)?lua\s+\d+$/i.test(description);
}

const inputKeyNames: Record<string, string> = {
  RETURN: "Return",
  ESCAPE: "Escape",
  SPACE: "space",
  TAB: "Tab",
  F12: "F12",
  XF86AudioRaiseVolume: "XF86AudioRaiseVolume",
  XF86AudioLowerVolume: "XF86AudioLowerVolume",
  XF86AudioMute: "XF86AudioMute",
  XF86AudioMicMute: "XF86AudioMicMute",
  XF86AudioPlay: "XF86AudioPlay",
  XF86AudioPause: "XF86AudioPause",
  XF86AudioNext: "XF86AudioNext",
  XF86AudioPrev: "XF86AudioPrev",
};

const inputModifiers: Record<string, string> = {
  Super: "logo",
  Ctrl: "ctrl",
  Alt: "alt",
  Shift: "shift",
};

function executeBinding(binding: DisplayBinding) {
  const key = binding.key;
  if (!key || key.startsWith("mouse:") || key.startsWith("code:")) return;

  const argumentsList = modifiers(binding.modmask).flatMap((modifier) => ["-M", inputModifiers[modifier]]);
  argumentsList.push("-k", inputKeyNames[key] || key.toLowerCase());
  execFileSync(wtype, argumentsList, { stdio: "ignore" });
}

function loadBindings(): DisplayBinding[] {
  try {
    const rows = JSON.parse(execFileSync(hyprctl, ["binds", "-j"], { encoding: "utf8" })) as Binding[];
    return rows
      .filter((row) => row.key && !row.key.startsWith("mouse:"))
      .map((row, index) => {
        const rawKey = row.key || `code:${row.keycode}`;
        const shortcut = [...modifiers(row.modmask), formatKey(rawKey)].join(" + ");
        const action = [row.dispatcher, row.arg].filter(Boolean).join(" ");
        const configuredDescription = row.description?.trim() || "";
        const description =
          configuredDescription && !isGeneratedLuaDescription(configuredDescription)
            ? configuredDescription
            : fallbackDescriptions[shortcut] || "Hyprland keybinding";
        return { ...row, id: `${shortcut}-${action}-${index}`, shortcut, action, description };
      })
      .sort((a, b) => a.shortcut.localeCompare(b.shortcut) || a.description.localeCompare(b.description));
  } catch {
    return [];
  }
}

export default function Command() {
  const [bindings, setBindings] = React.useState<DisplayBinding[]>(() => loadBindings());

  return (
    <List searchBarPlaceholder="Search keys or actions…">
      <List.Section title="Active Hyprland keybindings" subtitle={String(bindings.length)}>
        {bindings.map((binding) => (
          <List.Item
            key={binding.id}
            title={binding.description}
            subtitle={binding.shortcut}
            icon={Icon.Keyboard}
            keywords={[binding.shortcut, binding.action, binding.description]}
            actions={
              <ActionPanel>
                <Action title="Input Keybinding" icon={Icon.Play} onAction={() => executeBinding(binding)} />
                <Action
                  title="Copy Keybinding"
                  icon={Icon.Clipboard}
                  shortcut={{ modifiers: ["shift"], key: "enter" }}
                  onAction={() => Clipboard.copy(binding.shortcut)}
                />
                <Action
                  title="Refresh Keybindings"
                  icon={Icon.ArrowClockwise}
                  shortcut={{ modifiers: ["ctrl"], key: "r" }}
                  onAction={() => setBindings(loadBindings())}
                />
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
    </List>
  );
}
