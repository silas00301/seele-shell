import {
  Action,
  ActionPanel,
  Alert,
  closeMainWindow,
  confirmAlert,
  Detail,
  Icon,
  List,
  showToast,
  Toast,
} from "@raycast/api";
import { realpath } from "node:fs/promises";
import React, { useEffect, useMemo, useState } from "react";
import {
  escapeMarkdown,
  formatGenerationDate,
  formatPackageDiff,
  markActiveGenerations,
  parseGenerations,
  runningSystemPath,
  switchGenerationArguments,
} from "./generation-data.mjs";
import { binaries, run, useQuery } from "./runtime";

type Generation = {
  generation: number;
  date: string;
  nixosVersion: string;
  kernelVersion: string;
  configurationRevision: string;
  specialisations: string[];
  profilePath: string;
  storePath?: string;
  runningStorePath: string;
  active: boolean;
};

async function resolve(path: string) {
  try {
    return await realpath(path);
  } catch {
    return undefined;
  }
}

async function loadGenerations(signal?: AbortSignal): Promise<Generation[]> {
  const payload = await run(
    binaries.nixosRebuild,
    ["list-generations", "--json"],
    signal,
  );
  const generations = parseGenerations(payload) as Generation[];
  const runningStorePath = await resolve(runningSystemPath);
  if (!runningStorePath) throw new Error("Running system is unavailable");
  const resolved = await Promise.all(
    generations.map(async (generation) => [
      generation.generation,
      await resolve(generation.profilePath),
    ]),
  );
  const storePaths = new Map<number, string>(
    resolved.filter((entry): entry is [number, string] => Boolean(entry[1])),
  );
  return markActiveGenerations(
    generations,
    runningStorePath,
    storePaths,
  ) as Generation[];
}

function generationMarkdown(generation: Generation, packageDiff: string) {
  const state = generation.active
    ? "Running now"
    : generation.storePath
      ? "Retained rollback target"
      : "No longer retained";
  const revision = generation.configurationRevision
    ? `\n**Configuration revision:** ${escapeMarkdown(generation.configurationRevision)}`
    : "";
  const specialisations = generation.specialisations.length
    ? `\n**Specialisations:** ${generation.specialisations.map(escapeMarkdown).join(", ")}`
    : "";
  return `# Generation ${generation.generation}

**Built:** ${escapeMarkdown(formatGenerationDate(generation.date))}

**Kernel:** ${escapeMarkdown(generation.kernelVersion)}

**NixOS:** ${escapeMarkdown(generation.nixosVersion)}

**State:** ${state}${revision}${specialisations}

## Package diff against the running system

${packageDiff}`;
}

function GenerationDetail({ generation }: { generation: Generation }) {
  const [packageDiff, setPackageDiff] = useState("_Calculating package diff…_");
  const [diffLoading, setDiffLoading] = useState(!generation.active);
  const [switching, setSwitching] = useState(false);

  useEffect(() => {
    if (generation.active) {
      setPackageDiff("_This generation is already running._");
      setDiffLoading(false);
      return;
    }
    const controller = new AbortController();
    setDiffLoading(true);
    void run(
      binaries.nvd,
      ["diff", runningSystemPath, generation.profilePath],
      controller.signal,
      60_000,
    )
      .then((output) => setPackageDiff(formatPackageDiff(output)))
      .catch(() => {
        if (!controller.signal.aborted)
          setPackageDiff("_Package diff unavailable._");
      })
      .finally(() => {
        if (!controller.signal.aborted) setDiffLoading(false);
      });
    return () => controller.abort();
  }, [generation]);

  async function switchGeneration() {
    const confirmed = await confirmAlert({
      title: `Switch to generation ${generation.generation}?`,
      message: `Built ${formatGenerationDate(generation.date)} with kernel ${generation.kernelVersion}. This activates the reviewed generation now and makes it the system profile.`,
      primaryAction: {
        title: `Switch to Generation ${generation.generation}`,
        style: Alert.ActionStyle.Destructive,
      },
    });
    if (!confirmed || switching) return;

    setSwitching(true);
    await closeMainWindow();
    const toast = await showToast({
      style: Toast.Style.Animated,
      title: `Switching to generation ${generation.generation}`,
      message: "Waiting for authorization and system activation…",
    });
    try {
      // The view may be stale after cleanup or another switch. Re-resolve the
      // selected generation and ensure it still names the reviewed closure.
      const fresh = await loadGenerations();
      const selected = fresh.find(
        (candidate) => candidate.generation === generation.generation,
      );
      if (
        !selected ||
        selected.active ||
        !selected.storePath ||
        selected.storePath !== generation.storePath ||
        selected.runningStorePath !== generation.runningStorePath
      )
        throw new Error("Generation changed while the picker was open");

      await run(
        binaries.run0,
        [
          binaries.switchGeneration,
          ...switchGenerationArguments(generation.generation),
        ],
        undefined,
        10 * 60_000,
      );
      toast.style = Toast.Style.Success;
      toast.title = `Generation ${generation.generation} is active`;
      toast.message = `Built ${formatGenerationDate(generation.date)}`;
    } catch {
      toast.style = Toast.Style.Failure;
      toast.title = "Generation switch failed";
      toast.message =
        "Open the picker to review current generations and try again.";
    } finally {
      setSwitching(false);
    }
  }

  const markdown = useMemo(
    () => generationMarkdown(generation, packageDiff),
    [generation, packageDiff],
  );
  return (
    <Detail
      isLoading={diffLoading || switching}
      navigationTitle={`Generation ${generation.generation}`}
      markdown={markdown}
      actions={
        !generation.active && generation.storePath && !diffLoading ? (
          <ActionPanel>
            <Action
              title={`Switch to Generation ${generation.generation}`}
              icon={Icon.ArrowClockwise}
              style={Action.Style.Destructive}
              onAction={switchGeneration}
            />
          </ActionPanel>
        ) : undefined
      }
    />
  );
}

function GenerationItem({ generation }: { generation: Generation }) {
  return (
    <List.Item
      title={`Generation ${generation.generation}`}
      subtitle={formatGenerationDate(generation.date)}
      icon={generation.active ? Icon.CheckCircle : Icon.Clock}
      keywords={[
        generation.date,
        generation.kernelVersion,
        generation.nixosVersion,
        generation.configurationRevision,
      ]}
      accessories={[
        { text: generation.kernelVersion },
        ...(generation.active ? [{ tag: "Running" }] : []),
      ]}
      actions={
        <ActionPanel>
          <Action.Push
            title="Review Generation"
            icon={Icon.Eye}
            target={<GenerationDetail generation={generation} />}
          />
        </ActionPanel>
      }
    />
  );
}

export default function Command() {
  const { data, error, loading, refresh } = useQuery(loadGenerations, 0);
  const active = data?.filter((generation) => generation.active) ?? [];
  const retained = data?.filter((generation) => !generation.active) ?? [];
  const reload = (
    <Action
      title="Refresh Generations"
      icon={Icon.ArrowClockwise}
      shortcut={{ modifiers: ["ctrl"], key: "r" }}
      onAction={refresh}
    />
  );

  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search generation, build date, kernel, or NixOS version…"
    >
      {error && (
        <List.Item
          title="NixOS generations unavailable"
          subtitle="Refresh after the system profile is available"
          icon={Icon.Warning}
          actions={<ActionPanel>{reload}</ActionPanel>}
        />
      )}
      {active.length > 0 && (
        <List.Section title="Running">
          {active.map((generation) => (
            <GenerationItem
              key={generation.generation}
              generation={generation}
            />
          ))}
        </List.Section>
      )}
      <List.Section
        title="Retained generations"
        subtitle={String(retained.length)}
      >
        {retained.map((generation) => (
          <GenerationItem key={generation.generation} generation={generation} />
        ))}
      </List.Section>
      <List.EmptyView
        title="No retained NixOS generations"
        description="The existing nh cleanup policy decides which generations remain available."
        actions={<ActionPanel>{reload}</ActionPanel>}
      />
    </List>
  );
}
