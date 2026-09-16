import {
  Action,
  ActionPanel,
  Clipboard,
  Color,
  Icon,
  List,
} from "@raycast/api";
import React from "react";

import { binaries, perform, run, useQuery } from "./runtime";
import { RefreshAction, Unavailable, shortcuts } from "./ui";

type DisplayBinding = {
  id: string;
  shortcut: string;
  action: string;
  description: string;
  key: string;
  modmask: number;
  modifiers?: string[];
  inputAllowed: boolean;
};
async function executeBinding(binding: DisplayBinding) {
  await perform(
    "Input keybinding",
    () =>
      run(binaries.control, [
        "vicinae-input-keybinding",
        JSON.stringify({ key: binding.key, modmask: binding.modmask }),
      ]),
    true,
  );
}
async function loadBindings(signal: AbortSignal): Promise<DisplayBinding[]> {
  const rows = JSON.parse(
    await run(binaries.control, ["vicinae-keybindings"], signal),
  ) as DisplayBinding[];
  return rows.sort(
    (a, b) =>
      a.shortcut.localeCompare(b.shortcut) ||
      a.description.localeCompare(b.description),
  );
}

const unmodified = "Single keys";

// Native policy names the modifiers; the host only groups and collates them,
// so a cheat sheet reads chord by chord instead of as one flat column.
function chord(binding: DisplayBinding) {
  const modifiers = binding.modifiers ?? [];
  return modifiers.length ? modifiers.join(" + ") : unmodified;
}

function sections(bindings: DisplayBinding[]) {
  const groups = new Map<string, DisplayBinding[]>();
  for (const binding of bindings) {
    const name = chord(binding);
    const group = groups.get(name);
    if (group) group.push(binding);
    else groups.set(name, [binding]);
  }
  return [...groups.entries()].sort(([a], [b]) => {
    if (a === unmodified) return 1;
    if (b === unmodified) return -1;
    const depth = a.split(" + ").length - b.split(" + ").length;
    return depth || a.localeCompare(b);
  });
}

// A command line belongs in the row, but never at a length that pushes the
// shortcut off the screen it is supposed to teach.
function command(action: string) {
  return action.length > 96 ? `${action.slice(0, 95)}…` : action;
}

export default function Command() {
  const {
    data: bindings = [],
    error,
    loading,
    refresh,
  } = useQuery(loadBindings, 0);
  const reload = (
    <RefreshAction title="Refresh Keybindings" onAction={refresh} />
  );

  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search keys, actions, or commands..."
    >
      {error && (
        <Unavailable
          title="Keybindings unavailable"
          hint="Hyprland did not answer. Refresh once it is running."
          refreshTitle="Refresh Keybindings"
          onRefresh={refresh}
        />
      )}
      <List.EmptyView
        title="No matching keybindings"
        description="Search a key, an action, or the command it runs."
      />
      {sections(bindings).map(([name, rows]) => (
        <List.Section key={name} title={name} subtitle={String(rows.length)}>
          {rows.map((binding) => (
            <List.Item
              key={binding.id}
              title={binding.description}
              subtitle={command(binding.action)}
              icon={Icon.Keyboard}
              keywords={[
                binding.shortcut,
                binding.action,
                binding.description,
                binding.key,
              ]}
              accessories={[
                {
                  tag: {
                    value: binding.shortcut,
                    color: binding.inputAllowed
                      ? Color.PrimaryText
                      : Color.SecondaryText,
                  },
                },
              ]}
              actions={
                <ActionPanel>
                  {binding.inputAllowed && (
                    <Action
                      title="Input Keybinding"
                      icon={Icon.Play}
                      onAction={() => executeBinding(binding)}
                    />
                  )}
                  <Action
                    title="Copy Keybinding"
                    icon={Icon.CopyClipboard}
                    shortcut={shortcuts.secondaryCopy}
                    onAction={() => Clipboard.copy(binding.shortcut)}
                  />
                  {binding.action && (
                    <Action
                      title="Copy Command"
                      icon={Icon.Terminal}
                      shortcut={shortcuts.copy}
                      onAction={() => Clipboard.copy(binding.action)}
                    />
                  )}
                  {reload}
                </ActionPanel>
              }
            />
          ))}
        </List.Section>
      ))}
    </List>
  );
}
