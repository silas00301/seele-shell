import {
  Action,
  ActionPanel,
  Color,
  Icon,
  List,
  showToast,
  Toast,
} from "@raycast/api";
import React, { useRef, useState } from "react";
import { binaries, run, useQuery } from "./runtime";

type Theme = {
  id: string;
  name: string;
  mode: "light" | "dark";
  base: string;
  text: string;
  accent: string;
  red: string;
  green: string;
  yellow: string;
};
type Catalog = { current: string; themes: Theme[] };
const load = async (signal: AbortSignal): Promise<Catalog> =>
  JSON.parse(await run(binaries.theme, ["list"], signal));

export default function Command() {
  const catalog = useQuery(load, 0);
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  async function apply(id: string) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      const result = JSON.parse(await run(binaries.theme, ["set", id]));
      catalog.refresh();
      if (result.pending.length) {
        await showToast({
          style: Toast.Style.Failure,
          title: "Theme saved; some apps need reloading",
          message: result.pending.join(", "),
        });
      }
    } catch {
      await showToast({
        style: Toast.Style.Failure,
        title: "Could not apply theme",
        message: "Reload the catalog and try again.",
      });
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <List
      isLoading={catalog.loading || busy}
      searchBarPlaceholder="Choose a theme…"
      isShowingDetail
    >
      {catalog.error ? (
        <List.EmptyView
          icon={Icon.Warning}
          title="Themes unavailable"
          description="Reload the catalog to try again."
          actions={
            <ActionPanel>
              <Action title="Reload Themes" onAction={catalog.refresh} />
            </ActionPanel>
          }
        />
      ) : null}
      {(!catalog.error ? (catalog.data?.themes ?? []) : []).map((theme) => (
        <List.Item
          key={theme.id}
          id={theme.id}
          title={theme.name}
          icon={{ source: Icon.Circle, tintColor: theme.accent }}
          accessories={[
            {
              tag: {
                value: theme.mode === "light" ? "Light" : "Dark",
                color: Color.SecondaryText,
              },
            },
            ...(catalog.data?.current === theme.id
              ? [
                  {
                    icon: { source: Icon.Checkmark, tintColor: Color.Green },
                    tooltip: "Current theme",
                  },
                ]
              : []),
          ]}
          detail={
            <List.Item.Detail
              markdown={`# ${theme.name}\n\nA coordinated palette for your desktop, terminal and editor.\n\nFish updates at the next prompt. GTK apps may need reopening; Ghostty outside its desktop service uses Reload Configuration.`}
              metadata={
                <List.Item.Detail.Metadata>
                  <List.Item.Detail.Metadata.TagList title="Palette">
                    {[
                      theme.base,
                      theme.text,
                      theme.accent,
                      theme.red,
                      theme.green,
                      theme.yellow,
                    ].map((color, index) => (
                      <List.Item.Detail.Metadata.TagList.Item
                        key={index}
                        text={color}
                        color={color}
                      />
                    ))}
                  </List.Item.Detail.Metadata.TagList>
                </List.Item.Detail.Metadata>
              }
            />
          }
          actions={
            <ActionPanel>
              <Action
                title="Apply Theme"
                icon={Icon.Checkmark}
                onAction={() => apply(theme.id)}
              />
              <Action
                title="Reload Themes"
                icon={Icon.ArrowClockwise}
                shortcut={{ modifiers: ["cmd"], key: "r" }}
                onAction={catalog.refresh}
              />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
