import { Action, ActionPanel, Clipboard, Icon, List } from "@raycast/api";
import { execFileSync } from "node:child_process";
import React from "react";

const hyprctl = "@HYPRCTL@";

type Binding = {
  modmask?: number;
  key?: string;
  keycode?: number;
  dispatcher?: string;
  arg?: string;
  description?: string;
};

function modifiers(mask = 0) {
  const names: string[] = [];
  if (mask & 64) names.push("Super");
  if (mask & 4) names.push("Ctrl");
  if (mask & 8) names.push("Alt");
  if (mask & 1) names.push("Shift");
  return names;
}

function loadBindings() {
  try {
    const rows = JSON.parse(execFileSync(hyprctl, ["binds", "-j"], { encoding: "utf8" })) as Binding[];
    return rows
      .filter((row) => row.key || row.keycode)
      .map((row, index) => {
        const key = row.key || `code:${row.keycode}`;
        const shortcut = [...modifiers(row.modmask), key].join(" + ");
        const action = [row.dispatcher, row.arg].filter(Boolean).join(" ");
        return { id: `${shortcut}-${action}-${index}`, shortcut, action, description: row.description || "" };
      })
      .sort((a, b) => a.shortcut.localeCompare(b.shortcut));
  } catch {
    return [];
  }
}

export default function Command() {
  const bindings = loadBindings();
  return (
    <List searchBarPlaceholder="Search keys or actions…">
      <List.Section title="Active Hyprland keybindings" subtitle={String(bindings.length)}>
        {bindings.map((binding) => (
          <List.Item
            key={binding.id}
            title={binding.shortcut}
            subtitle={binding.description || binding.action}
            icon={Icon.Keyboard}
            keywords={[binding.action, binding.description]}
            actions={
              <ActionPanel>
                <Action title="Copy Keybinding" icon={Icon.Clipboard} onAction={() => Clipboard.copy(binding.shortcut)} />
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
    </List>
  );
}
