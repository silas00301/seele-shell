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
import React, { useEffect, useMemo, useRef, useState } from "react";
import { formatGenerationDate } from "./generation-data.mjs";
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
  switchArguments?: string[];
  escaped: {
    date: string;
    nixosVersion: string;
    kernelVersion: string;
    configurationRevision: string;
    specialisations: string[];
  };
};

async function loadGenerations(signal?: AbortSignal): Promise<Generation[]> {
  return JSON.parse(
    await run(binaries.control, ["vicinae-generations"], signal),
  );
}

function generationMarkdown(generation: Generation, packageDiff: string) {
  const state = generation.active
    ? "Running now"
    : generation.storePath
      ? "Retained rollback target"
      : "No longer retained";
  const revision = generation.configurationRevision
    ? `\n**Configuration revision:** ${generation.escaped.configurationRevision}`
    : "";
  const specialisations = generation.specialisations.length
    ? `\n**Specialisations:** ${generation.escaped.specialisations.join(", ")}`
    : "";
  return `# Generation ${generation.generation}

**Built:** ${formatGenerationDate(generation.date, generation.escaped.date)}

**Kernel:** ${generation.escaped.kernelVersion}

**NixOS:** ${generation.escaped.nixosVersion}

**State:** ${state}${revision}${specialisations}

## Package diff against the running system

${packageDiff}`;
}

export function GenerationDetail({ generation }: { generation: Generation }) {
  const [packageDiff, setPackageDiff] = useState("_Calculating package diff…_");
  const [diffLoading, setDiffLoading] = useState(!generation.active);
  const [switching, setSwitching] = useState(false);
  const [reviewed, setReviewed] = useState<Generation>();
  const review = useRef<Generation>();
  const switchPending = useRef(false);

  useEffect(() => {
    review.current = undefined;
    setReviewed(undefined);
    if (generation.active) {
      setPackageDiff("_This generation is already running._");
      setDiffLoading(false);
      return;
    }
    if (!generation.storePath || !generation.switchArguments) {
      setPackageDiff("_Package diff unavailable._");
      setDiffLoading(false);
      return;
    }
    const controller = new AbortController();
    setDiffLoading(true);
    void run(
      binaries.control,
      ["vicinae-generation-diff", ...generation.switchArguments],
      controller.signal,
      60_000,
    )
      .then((output) => {
        if (!controller.signal.aborted) {
          const result = JSON.parse(output);
          if (typeof result.diff !== "string")
            throw new Error("Invalid package diff");
          setPackageDiff(result.diff);
          review.current = generation;
          setReviewed(generation);
        }
      })
      .catch(() => {
        if (!controller.signal.aborted)
          setPackageDiff("_Package diff unavailable._");
      })
      .finally(() => {
        if (!controller.signal.aborted) setDiffLoading(false);
      });
    return () => {
      controller.abort();
      review.current = undefined;
    };
  }, [generation]);

  async function switchGeneration() {
    if (review.current !== generation || switchPending.current) return;
    switchPending.current = true;
    let toast: Awaited<ReturnType<typeof showToast>> | undefined;
    try {
      const confirmed = await confirmAlert({
        title: `Switch to generation ${generation.generation}?`,
        message: `Built ${formatGenerationDate(generation.date)} with kernel ${generation.kernelVersion}. This activates the reviewed generation now and makes it the system profile.`,
        primaryAction: {
          title: `Switch to Generation ${generation.generation}`,
          style: Alert.ActionStyle.Destructive,
        },
      });
      if (!confirmed || review.current !== generation) return;

      setSwitching(true);
      await closeMainWindow();
      toast = await showToast({
        style: Toast.Style.Animated,
        title: `Switching to generation ${generation.generation}`,
        message: "Waiting for authorization and system activation…",
      });
      // Native preflight rechecks both reviewed identities before escalation;
      // the privileged helper repeats that check after authorization as well.
      await run(binaries.control, [
        "vicinae-generation-check",
        ...generation.switchArguments!,
      ]);

      await run(
        binaries.run0,
        [binaries.switchGeneration, ...generation.switchArguments!],
        undefined,
        10 * 60_000,
      );
      toast.style = Toast.Style.Success;
      toast.title = `Generation ${generation.generation} is active`;
      toast.message = `Built ${formatGenerationDate(generation.date)}`;
    } catch {
      toast ??= await showToast({
        style: Toast.Style.Failure,
        title: "Generation switch failed",
      });
      toast.style = Toast.Style.Failure;
      toast.title = "Generation switch failed";
      toast.message =
        "Open the picker to review current generations and try again.";
    } finally {
      switchPending.current = false;
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
        !generation.active &&
        generation.storePath &&
        !diffLoading &&
        reviewed === generation ? (
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
