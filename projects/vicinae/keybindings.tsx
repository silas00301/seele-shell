import { Action, ActionPanel, Clipboard, Icon, List } from "@raycast/api";
import React from "react";

import { binaries, perform, run, useQuery } from "./runtime";

type DisplayBinding = {
  id: string;
  shortcut: string;
  action: string;
  description: string;
  key: string;
  modmask: number;
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

export default function Command() {
  const {
    data: bindings = [],
    error,
    loading,
    refresh,
  } = useQuery(loadBindings, 0);

  return (
    <List isLoading={loading} searchBarPlaceholder="Search keys or actions...">
      {error && (
        <List.Item
          title="Keybindings unavailable"
          subtitle="Check Hyprland, then refresh"
          icon={Icon.Warning}
          actions={
            <ActionPanel>
              <Action
                title="Refresh Keybindings"
                icon={Icon.ArrowClockwise}
                onAction={refresh}
              />
            </ActionPanel>
          }
        />
      )}
      <List.EmptyView title="No matching keybindings" />
      <List.Section
        title="Active Hyprland keybindings"
        subtitle={String(bindings.length)}
      >
        {bindings.map((binding) => (
          <List.Item
            key={binding.id}
            title={binding.description}
            subtitle={binding.shortcut}
            icon={Icon.Keyboard}
            keywords={[binding.shortcut, binding.action, binding.description]}
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
                  shortcut={{ modifiers: ["shift"], key: "enter" }}
                  onAction={() => Clipboard.copy(binding.shortcut)}
                />
                <Action
                  title="Refresh Keybindings"
                  icon={Icon.ArrowClockwise}
                  shortcut={{ modifiers: ["ctrl"], key: "r" }}
                  onAction={refresh}
                />
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
    </List>
  );
}
