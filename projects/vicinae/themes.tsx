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
import { RefreshAction, Unavailable, shortcuts } from "./ui";

type Theme = {
  id: string;
  name: string;
  mode: "light" | "dark";
  palette: Record<string, string>;
  base: string;
  surface: string;
  text: string;
  accent: string;
  red: string;
  green: string;
  yellow: string;
};
type Catalog = { current: string; themes: Theme[] };
const load = async (signal: AbortSignal): Promise<Catalog> =>
  JSON.parse(await run(binaries.theme, ["list"], signal));

// Row presentation. Every expression here reads a palette the native helper
// already validated; none of them starts a process or decides an action.
const modeIcon = (theme: Theme) =>
  theme.mode === "light" ? Icon.Sun : Icon.Moon;
// A preset is found by the words its own ID is made of, so "mocha", "dawn" or
// "light" reach it without the catalog carrying a keyword list.
const keywords = (theme: Theme) => [
  ...theme.id.split("-"),
  theme.mode,
  "theme",
  "palette",
  "appearance",
  "colors",
];
// What a selection does to the session, said per theme rather than as one
// paragraph the reader has to map onto their own desktop.
const markdown = (theme: Theme, current: boolean) => `# ${theme.name}

${
  current
    ? "Applied now. Choosing it again republishes its files and asks the running applications to re-read them."
    : "Apply it to recolor the desktop, the terminal and the editor together."
}

**Immediately** — Seele Shell, notifications, Notes, the lock screen, Hyprland
borders, this launcher, tmux, and every terminal opened from here on.

**Needs a nudge** — Fish recolors at its next prompt, GTK applications may need
reopening, and a Ghostty started outside its desktop service uses Reload
Configuration.`;

export default function Command() {
  const catalog = useQuery(load, 0);
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  async function apply(theme: Theme) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    // Publishing a theme writes a generation of files and asks running
    // applications to re-read them, so the wait is reported rather than left
    // to a list that has simply stopped answering.
    const toast = await showToast({
      style: Toast.Style.Animated,
      title: `Applying ${theme.name}…`,
    });
    try {
      const result = JSON.parse(await run(binaries.theme, ["set", theme.id]));
      catalog.refresh();
      toast.style = Toast.Style.Success;
      toast.title = theme.name;
      // The theme is saved either way. Whatever kept its old colors is named,
      // because an application that did not reload reads as a failed switch.
      toast.message = result.pending.length
        ? `Applied; still to reload: ${result.pending.join(", ")}`
        : "Applied";
    } catch {
      toast.style = Toast.Style.Failure;
      toast.title = "Could not apply theme";
      toast.message = "Reload the catalog and try again.";
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }

  const themes = catalog.error ? [] : (catalog.data?.themes ?? []);
  const current = catalog.data?.current ?? "";
  const reload = (
    <RefreshAction title="Refresh Themes" onAction={catalog.refresh} />
  );
  // The current theme leads, and the rest are grouped by what they are, so a
  // light palette is never applied by a reader who wanted the next dark one.
  const section = (title: string, rows: Theme[]) =>
    rows.length ? (
      <List.Section title={title} subtitle={String(rows.length)}>
        {rows.map((theme) => (
          <ThemeItem
            key={theme.id}
            theme={theme}
            current={theme.id === current}
            onApply={apply}
            onRefresh={catalog.refresh}
          />
        ))}
      </List.Section>
    ) : null;

  return (
    <List
      isLoading={catalog.loading || busy}
      searchBarPlaceholder="Search a palette: Catppuccin, Rosé Pine, Gruvbox, Nord…"
      isShowingDetail={themes.length > 0}
    >
      {catalog.error && (
        <Unavailable
          title="Themes unavailable"
          hint="The theme catalog did not answer. Refresh to try again."
          refreshTitle="Refresh Themes"
          onRefresh={catalog.refresh}
        />
      )}
      {section(
        "Current",
        themes.filter((theme) => theme.id === current),
      )}
      {section(
        "Dark",
        themes.filter((theme) => theme.id !== current && theme.mode === "dark"),
      )}
      {section(
        "Light",
        themes.filter(
          (theme) => theme.id !== current && theme.mode === "light",
        ),
      )}
      <List.EmptyView
        icon={Icon.Swatch}
        title="No matching theme"
        description="Search a palette by name, or by whether it is light or dark."
        actions={<ActionPanel>{reload}</ActionPanel>}
      />
    </List>
  );
}

function ThemeItem(props: {
  theme: Theme;
  current: boolean;
  onApply: (theme: Theme) => void;
  onRefresh: () => void;
}) {
  const { theme, current } = props;
  return (
    <List.Item
      id={theme.id}
      title={theme.name}
      icon={{ source: Icon.CircleFilled, tintColor: theme.accent }}
      keywords={keywords(theme)}
      accessories={
        current
          ? [{ tag: { value: "Current", color: Color.Green } }]
          : [
              {
                icon: modeIcon(theme),
                tooltip: theme.mode === "light" ? "Light theme" : "Dark theme",
              },
            ]
      }
      detail={
        <List.Item.Detail
          markdown={markdown(theme, current)}
          metadata={
            <List.Item.Detail.Metadata>
              <List.Item.Detail.Metadata.Label
                title="Mode"
                icon={modeIcon(theme)}
                text={theme.mode === "light" ? "Light" : "Dark"}
              />
              <List.Item.Detail.Metadata.Separator />
              {/* The ends of a palette are read as values: a tag tinted with
                  its own background or foreground is the one tag a light or a
                  dark launcher renders illegibly. */}
              <List.Item.Detail.Metadata.Label
                title="Background"
                text={theme.base}
              />
              <List.Item.Detail.Metadata.Label
                title="Surface"
                text={theme.surface}
              />
              <List.Item.Detail.Metadata.Label
                title="Foreground"
                text={theme.text}
              />
              <List.Item.Detail.Metadata.Separator />
              {/* These four state themselves: each tag is drawn in the color it
                  carries, and they are the mid-tones that stay legible either
                  way round. */}
              <List.Item.Detail.Metadata.TagList title="Accents">
                {[theme.accent, theme.red, theme.green, theme.yellow].map(
                  (color, index) => (
                    <List.Item.Detail.Metadata.TagList.Item
                      key={index}
                      text={color}
                      color={color}
                    />
                  ),
                )}
              </List.Item.Detail.Metadata.TagList>
            </List.Item.Detail.Metadata>
          }
        />
      }
      actions={
        <ActionPanel>
          <Action
            title={current ? "Reapply Theme" : "Apply Theme"}
            icon={current ? Icon.ArrowClockwise : Icon.Checkmark}
            onAction={() => props.onApply(theme)}
          />
          <Action.CopyToClipboard
            title="Copy Base16 Palette"
            icon={Icon.CopyClipboard}
            shortcut={shortcuts.copy}
            content={JSON.stringify(theme.palette, null, 2)}
          />
          <RefreshAction title="Refresh Themes" onAction={props.onRefresh} />
        </ActionPanel>
      }
    />
  );
}
