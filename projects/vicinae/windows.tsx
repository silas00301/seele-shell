import {
  Action,
  ActionPanel,
  Alert,
  Color,
  Icon,
  List,
  confirmAlert,
} from "@raycast/api";
import React, { useState } from "react";
import {
  Client,
  Workspace,
  closeWindow,
  focusWindow,
  focusWorkspace,
  forceQuitWindow,
  moveWindow,
} from "./desktop";
import { binaries, perform, run, useQuery } from "./runtime";
import { RefreshAction, Unavailable, applicationIcon, shortcuts } from "./ui";

async function load(signal: AbortSignal) {
  const snapshot = JSON.parse(
    await run(binaries.control, ["vicinae-desktop"], signal),
  ) as {
    clientGroups: Client[][];
    workspaces: Workspace[];
  };
  return {
    // Preserve the host's exact locale collation for equal-focus addresses.
    clients: snapshot.clientGroups.flatMap((group) =>
      group.sort((a, b) => a.address.localeCompare(b.address)),
    ),
    workspaces: snapshot.workspaces,
  };
}

const everywhere = "all";

export default function Command() {
  const { data, error, loading, refresh } = useQuery(load, 3000);
  const [scope, setScope] = useState(everywhere);
  const workspaces = data?.workspaces ?? [];
  // A workspace that closed while its filter was selected must not hide every
  // remaining window; fall back to the whole desktop instead.
  const selected =
    scope !== everywhere && workspaces.some((w) => String(w.id) === scope)
      ? scope
      : everywhere;
  const clients = (data?.clients ?? []).filter(
    (client) =>
      selected === everywhere || String(client.workspace.id) === selected,
  );
  const reload = <RefreshAction title="Refresh Desktop" onAction={refresh} />;
  const quit = async (client: Client, force: boolean) => {
    if (
      force &&
      !(await confirmAlert({
        title: `Force quit ${client.class}?`,
        message:
          "The application is killed immediately and unsaved work is lost.",
        primaryAction: {
          title: "Force Quit",
          style: Alert.ActionStyle.Destructive,
        },
      }))
    )
      return;
    await perform(force ? "Force quit" : "Close window", async () => {
      await (force
        ? forceQuitWindow(client.address)
        : closeWindow(client.address));
      refresh();
    });
  };

  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search window titles, apps, or workspaces..."
      searchBarAccessory={
        <List.Dropdown
          tooltip="Limit to one workspace"
          value={selected}
          onChange={setScope}
        >
          <List.Dropdown.Item
            title="All workspaces"
            value={everywhere}
            icon={Icon.Layers}
          />
          {workspaces.map((workspace) => (
            <List.Dropdown.Item
              key={workspace.id}
              title={`Workspace ${workspace.name}`}
              value={String(workspace.id)}
              icon={Icon.Desktop}
            />
          ))}
        </List.Dropdown>
      }
    >
      {error && (
        <Unavailable
          title="Desktop unavailable"
          hint="Hyprland did not answer. Refresh once it is running."
          refreshTitle="Refresh Desktop"
          onRefresh={refresh}
        />
      )}
      <List.Section
        title="Windows"
        subtitle={`${clients.length} in focus order`}
      >
        {clients.map((client) => (
          <List.Item
            key={client.address}
            title={client.title || client.class}
            subtitle={client.class}
            icon={applicationIcon(client.class || client.title)}
            keywords={[
              client.class,
              client.workspace.name,
              `workspace ${client.workspace.id}`,
            ]}
            accessories={[
              ...(client.focusHistoryID === 0
                ? [{ tag: { value: "Focused", color: Color.Green } }]
                : []),
              { text: `Workspace ${client.workspace.name}` },
            ]}
            actions={
              <ActionPanel>
                <Action
                  title="Focus Window"
                  icon={Icon.AppWindow}
                  onAction={() =>
                    perform(
                      "Focus window",
                      () => focusWindow(client.address),
                      true,
                    )
                  }
                />
                <Action
                  title="Focus Its Workspace"
                  icon={Icon.Desktop}
                  shortcut={shortcuts.toggle}
                  onAction={() =>
                    perform(
                      "Focus workspace",
                      () => focusWorkspace(client.workspace.id),
                      true,
                    )
                  }
                />
                {(client.moveTargets?.length ?? 0) > 0 && (
                  <ActionPanel.Submenu
                    title="Move to Workspace"
                    icon={Icon.Desktop}
                  >
                    {client.moveTargets?.map((destination) => (
                      <Action
                        key={destination.selector}
                        title={`Workspace ${destination.name}`}
                        icon={Icon.Desktop}
                        onAction={() =>
                          perform("Move window", async () => {
                            await moveWindow(client, destination);
                            refresh();
                          })
                        }
                      />
                    ))}
                  </ActionPanel.Submenu>
                )}
                <Action
                  title="Close Window"
                  icon={Icon.XMarkCircle}
                  style={Action.Style.Destructive}
                  shortcut={shortcuts.close}
                  onAction={() => quit(client, false)}
                />
                <Action
                  title="Force Quit Application"
                  icon={Icon.Trash}
                  style={Action.Style.Destructive}
                  shortcut={shortcuts.forceClose}
                  onAction={() => quit(client, true)}
                />
                <Action.CopyToClipboard
                  title="Copy Window Title"
                  content={client.title}
                  shortcut={shortcuts.copy}
                />
                <Action.CopyToClipboard
                  title="Copy Application Class"
                  content={client.class}
                />
                {reload}
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
      <List.Section title="Workspaces" subtitle={String(workspaces.length)}>
        {workspaces.map((workspace) => (
          <List.Item
            key={workspace.id}
            title={`Workspace ${workspace.name}`}
            subtitle={workspace.monitor}
            icon={Icon.Desktop}
            keywords={["workspace", "monitor", workspace.monitor]}
            accessories={[
              {
                text:
                  workspace.windows === 1
                    ? "1 window"
                    : `${workspace.windows} windows`,
              },
            ]}
            actions={
              <ActionPanel>
                <Action
                  title="Focus Workspace"
                  icon={Icon.Desktop}
                  onAction={() =>
                    perform(
                      "Focus workspace",
                      () => focusWorkspace(workspace.id),
                      true,
                    )
                  }
                />
                <Action
                  title="Show Only This Workspace"
                  icon={Icon.Filter}
                  shortcut={shortcuts.toggle}
                  onAction={() => setScope(String(workspace.id))}
                />
                {reload}
              </ActionPanel>
            }
          />
        ))}
      </List.Section>
      <List.EmptyView
        title="No windows or workspaces"
        description="Open an application, or widen the workspace filter."
      />
    </List>
  );
}
