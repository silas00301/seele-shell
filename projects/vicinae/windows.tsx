import { Action, ActionPanel, Icon, List } from "@raycast/api";
import React from "react";
import {
  Client,
  Workspace,
  focusWindow,
  focusWorkspace,
  visibleClients,
} from "./desktop";
import { binaries, perform, run, useQuery } from "./runtime";

async function load(signal: AbortSignal) {
  const [clients, workspaces] = await Promise.all([
    run(binaries.hyprctl, ["clients", "-j"], signal),
    run(binaries.hyprctl, ["workspaces", "-j"], signal),
  ]);
  return {
    clients: visibleClients(JSON.parse(clients) as Client[]),
    workspaces: (JSON.parse(workspaces) as Workspace[])
      .filter((w) => w.id > 0)
      .sort((a, b) => a.id - b.id),
  };
}

async function dispatch(expression: string) {
  const result = await run(binaries.hyprctl, ["dispatch", expression]);
  // Hyprland can report a Lua error while returning exit status zero.
  if (result.trim() !== "ok") throw new Error("Hyprland rejected the action");
}

export default function Command() {
  const { data, error, loading, refresh } = useQuery(load, 3000);
  const reload = (
    <Action
      title="Refresh Desktop"
      icon={Icon.ArrowClockwise}
      shortcut={{ modifiers: ["ctrl"], key: "r" }}
      onAction={refresh}
    />
  );
  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search window titles, apps, or workspaces..."
    >
      {error && (
        <List.Item
          title="Desktop unavailable"
          subtitle="Check Hyprland, then refresh"
          icon={Icon.Warning}
          actions={<ActionPanel>{reload}</ActionPanel>}
        />
      )}
      <List.Section
        title="Windows"
        subtitle={String(data?.clients.length ?? 0)}
      >
        {data?.clients.map((client) => (
          <List.Item
            key={client.address}
            title={client.title || client.class}
            subtitle={client.class}
            icon={Icon.AppWindow}
            keywords={[
              client.class,
              client.workspace.name,
              `workspace ${client.workspace.id}`,
            ]}
            accessories={[{ text: `Workspace ${client.workspace.name}` }]}
            actions={
              <ActionPanel>
                <Action
                  title="Focus Window"
                  icon={Icon.AppWindow}
                  onAction={() =>
                    perform(
                      "Focus window",
                      () => dispatch(focusWindow(client.address)),
                      true,
                    )
                  }
                />
                <Action.CopyToClipboard
                  title="Copy Window Title"
                  content={client.title}
                />
                {reload}
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
      <List.Section title="Workspaces">
        {data?.workspaces.map((workspace) => (
          <List.Item
            key={workspace.id}
            title={`Workspace ${workspace.name}`}
            subtitle={workspace.monitor}
            icon={Icon.Desktop}
            accessories={[{ text: `${workspace.windows} windows` }]}
            actions={
              <ActionPanel>
                <Action
                  title="Focus Workspace"
                  icon={Icon.Desktop}
                  onAction={() =>
                    perform(
                      "Focus workspace",
                      () => dispatch(focusWorkspace(workspace.id)),
                      true,
                    )
                  }
                />
                {reload}
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
      <List.EmptyView
        title="No windows or workspaces"
        description="Open an application to see it here."
      />
    </List>
  );
}
