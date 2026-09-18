import {
  Action,
  ActionPanel,
  Alert,
  Color,
  closeMainWindow,
  confirmAlert,
  Detail,
  Icon,
  List,
  showToast,
  Toast,
} from "@raycast/api";
import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  formatGenerationAge,
  formatGenerationDate,
} from "./generation-data.mjs";
import { binaries, run, useQuery } from "./runtime";
import { RefreshAction, Unavailable, shortcuts } from "./ui";

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

function generationState(generation: Generation) {
  if (generation.active) return { text: "Running now", color: Color.Green };
  if (generation.storePath)
    return { text: "Retained rollback target", color: Color.PrimaryText };
  return { text: "No longer retained", color: Color.SecondaryText };
}

// The diff is the whole reason this view exists, so it owns the markdown and
// every fixed field moves into the metadata panel beside it.
function generationMarkdown(generation: Generation, packageDiff: string) {
  return `# Generation ${generation.generation}

## Package changes against the running system

${packageDiff}`;
}

export function GenerationDetail({ generation }: { generation: Generation }) {
  const [packageDiff, setPackageDiff] = useState("_Calculating package diff…_");
  const [diffLoading, setDiffLoading] = useState(!generation.active);
  const [switching, setSwitching] = useState(false);
  const [reviewed, setReviewed] = useState<Generation>();
  const review = useRef<Generation | undefined>(undefined);
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
  const state = generationState(generation);
  const activatable =
    !generation.active &&
    Boolean(generation.storePath) &&
    !diffLoading &&
    reviewed === generation;
  return (
    <Detail
      isLoading={diffLoading || switching}
      navigationTitle={`Generation ${generation.generation}`}
      markdown={markdown}
      metadata={
        <Detail.Metadata>
          <Detail.Metadata.Label
            title="Built"
            icon={Icon.Clock}
            text={formatGenerationDate(generation.date)}
          />
          <Detail.Metadata.Label
            title="Age"
            text={formatGenerationAge(generation.date) || "Unknown"}
          />
          <Detail.Metadata.Separator />
          <Detail.Metadata.Label
            title="Kernel"
            text={generation.kernelVersion}
          />
          <Detail.Metadata.Label title="NixOS" text={generation.nixosVersion} />
          {generation.configurationRevision ? (
            <Detail.Metadata.Label
              title="Revision"
              text={generation.configurationRevision}
            />
          ) : null}
          {generation.specialisations.length ? (
            <Detail.Metadata.TagList title="Specialisations">
              {generation.specialisations.map((name) => (
                <Detail.Metadata.TagList.Item key={name} text={name} />
              ))}
            </Detail.Metadata.TagList>
          ) : null}
          <Detail.Metadata.Separator />
          <Detail.Metadata.Label
            title="State"
            text={{ value: state.text, color: state.color }}
          />
        </Detail.Metadata>
      }
      actions={
        <ActionPanel>
          {activatable ? (
            <Action
              title={`Switch to Generation ${generation.generation}`}
              icon={Icon.ArrowClockwise}
              style={Action.Style.Destructive}
              onAction={switchGeneration}
            />
          ) : null}
          {generation.storePath ? (
            <Action.CopyToClipboard
              title="Copy Store Path"
              content={generation.storePath}
              shortcut={shortcuts.copy}
            />
          ) : null}
          <Action.CopyToClipboard
            title="Copy Generation Number"
            content={String(generation.generation)}
          />
        </ActionPanel>
      }
    />
  );
}

function GenerationItem({ generation }: { generation: Generation }) {
  const state = generationState(generation);
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
        "rollback",
        "generation",
      ]}
      accessories={[
        { text: formatGenerationAge(generation.date) },
        { text: generation.nixosVersion, tooltip: "NixOS version" },
        { text: generation.kernelVersion, tooltip: "Kernel" },
        ...(generation.active
          ? [{ tag: { value: "Running", color: Color.Green } }]
          : generation.storePath
            ? []
            : [{ tag: { value: state.text, color: Color.SecondaryText } }]),
      ]}
      actions={
        <ActionPanel>
          <Action.Push
            title="Review Generation"
            icon={Icon.Eye}
            target={<GenerationDetail generation={generation} />}
          />
          {generation.storePath ? (
            <Action.CopyToClipboard
              title="Copy Store Path"
              content={generation.storePath}
              shortcut={shortcuts.copy}
            />
          ) : null}
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
    <RefreshAction title="Refresh Generations" onAction={refresh} />
  );

  return (
    <List
      isLoading={loading}
      searchBarPlaceholder="Search generation, build date, kernel, or NixOS version…"
    >
      {error && (
        <Unavailable
          title="NixOS generations unavailable"
          hint="The system profile did not answer. Refresh to try again."
          refreshTitle="Refresh Generations"
          onRefresh={refresh}
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
