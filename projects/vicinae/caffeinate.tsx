import {
  Action,
  ActionPanel,
  Icon,
  List,
  showToast,
  Toast,
} from "@raycast/api";
import React, { useRef, useState } from "react";
import { binaries, run, useQuery } from "./runtime";

type Task = {
  key: string;
  kind: string;
  label: string;
  project: string | null;
  pid: number | null;
};
type Display = {
  active: boolean;
  headline: string;
  detail: string;
  barText: string;
  hoverText: string;
  task: Task | null;
};

// The service and the shared projection own every string below; this command
// chooses none of them and computes no remaining time of its own.
const PRESETS = ["15m", "30m", "1h", "2h", "4h"];
const GLYPHS: Record<string, Icon> = {
  build: Icon.Hammer,
  transfer: Icon.Upload,
  process: Icon.Terminal,
};

async function native(args: string[], signal?: AbortSignal) {
  return JSON.parse(
    await run(binaries.control, ["vicinae-caffeinate", ...args], signal),
  );
}

// A service that cannot answer is an unavailable row, not an empty session.
async function loadSession(signal: AbortSignal): Promise<Display> {
  const reply = await native(["snapshot"], signal);
  if (!reply.ok) throw new Error(String(reply.message ?? ""));
  return reply.display as Display;
}

async function loadTasks(signal: AbortSignal): Promise<Task[]> {
  const reply = await native(["tasks"], signal);
  if (!reply.ok) throw new Error(String(reply.message ?? ""));
  return (reply.tasks ?? []) as Task[];
}

export default function Command() {
  const session = useQuery(loadSession, 3000);
  const tasks = useQuery(loadTasks, 0);
  const [text, setText] = useState("");
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);

  async function act(args: string[]) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      const reply = await native(args);
      if (!reply.ok) {
        // The message is composed natively, so a refused duration or a task
        // that ended first explains itself in the same words everywhere.
        await showToast({
          style: Toast.Style.Failure,
          title: "Caffeinate",
          message: String(reply.message ?? ""),
        });
      } else {
        session.refresh();
      }
    } catch {
      // Child errors can include private command output; keep this generic.
      await showToast({
        style: Toast.Style.Failure,
        title: "Caffeinate failed",
        message: "Check that the desktop service is running and try again.",
      });
    }
    pending.current = false;
    setBusy(false);
  }
  const start = (request: Record<string, string>) =>
    act(["start", JSON.stringify(request)]);

  const reload = (
    <Action
      title="Refresh Tasks"
      icon={Icon.ArrowClockwise}
      shortcut={{ modifiers: ["ctrl"], key: "r" }}
      onAction={() => {
        session.refresh();
        tasks.refresh();
      }}
    />
  );
  const stop = (
    <Action
      title="Stop Caffeinate"
      icon={Icon.Stop}
      onAction={() => act(["stop"])}
    />
  );
  const display = session.data;
  const rows = tasks.data ?? [];
  const tracked = rows.filter((task) => task.kind !== "process");
  const others = rows.filter((task) => task.kind === "process");
  const describe = (task: Task) =>
    [
      task.project ? `Project ${task.project}` : "",
      task.pid ? `PID ${task.pid}` : "",
    ]
      .filter((part) => part !== "")
      .join(" · ");
  const taskItem = (task: Task) => (
    <List.Item
      key={task.key}
      title={task.label}
      subtitle={describe(task)}
      icon={GLYPHS[task.kind] ?? Icon.Terminal}
      keywords={[task.kind, task.project ?? ""]}
      accessories={task.kind === "build" ? [{ text: "Build" }] : []}
      actions={
        <ActionPanel>
          <Action
            title="Keep Awake Until This Ends"
            icon={Icon.Bolt}
            onAction={() => start({ mode: "task", task: task.key })}
          />
          {display?.active ? stop : null}
          {reload}
        </ActionPanel>
      }
    />
  );

  return (
    <List
      isLoading={session.loading || tasks.loading || busy}
      filtering={true}
      searchText={text}
      onSearchTextChange={setText}
      searchBarPlaceholder="Search a running task, or type a duration like 1h30..."
    >
      {session.error && (
        <List.Item
          title="Caffeinate unavailable"
          subtitle="Check its user service, then refresh"
          icon={Icon.Warning}
          actions={<ActionPanel>{reload}</ActionPanel>}
        />
      )}
      {display?.active && (
        <List.Section title="Current session">
          <List.Item
            title={display.headline}
            subtitle={display.detail}
            icon={Icon.Bolt}
            keywords={["caffeinate", "stop", "session"]}
            accessories={display.task ? [{ text: display.task.label }] : []}
            actions={
              <ActionPanel>
                {stop}
                {reload}
              </ActionPanel>
            }
          />
        </List.Section>
      )}
      <List.Section title="Start a session">
        <List.Item
          title="Until stopped"
          subtitle="Stays awake until you stop it"
          icon={Icon.Eye}
          keywords={["manual", "indefinite", "forever"]}
          actions={
            <ActionPanel>
              <Action
                title="Keep Awake Until Stopped"
                icon={Icon.Bolt}
                onAction={() => start({ mode: "manual" })}
              />
              {display?.active ? stop : null}
              {reload}
            </ActionPanel>
          }
        />
        {PRESETS.map((preset) => (
          <List.Item
            key={preset}
            title={`For ${preset}`}
            icon={Icon.Clock}
            keywords={["duration", "timer", preset]}
            actions={
              <ActionPanel>
                <Action
                  title={`Keep Awake for ${preset}`}
                  icon={Icon.Bolt}
                  onAction={() => start({ mode: "duration", duration: preset })}
                />
                {display?.active ? stop : null}
                {reload}
              </ActionPanel>
            }
          />
        ))}
        {text !== "" && (
          <List.Item
            key="custom"
            title={`For ${text}`}
            subtitle="Custom duration"
            icon={Icon.Stopwatch}
            keywords={[text, "duration"]}
            actions={
              <ActionPanel>
                <Action
                  title="Keep Awake for This Duration"
                  icon={Icon.Bolt}
                  onAction={() => start({ mode: "duration", duration: text })}
                />
                {reload}
              </ActionPanel>
            }
          />
        )}
      </List.Section>
      <List.Section
        title="Builds and transfers"
        subtitle={String(tracked.length)}
      >
        {tracked.map(taskItem)}
      </List.Section>
      <List.Section title="Other processes" subtitle={String(others.length)}>
        {others.map(taskItem)}
      </List.Section>
    </List>
  );
}
