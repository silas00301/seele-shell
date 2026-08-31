#!/usr/bin/env bash
set -euo pipefail

codexbar=${SEELE_SHELL_CODEXBAR:-codexbar}
providers=${SEELE_SHELL_CODEXBAR_PROVIDERS:-both}
state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/seele-shell
cache=$state_dir/agents.json
work_dir=$(mktemp -d)
mkdir -p "$state_dir"
output_file=$(mktemp "$state_dir/.agents.XXXXXX")
trap 'rm -rf "$work_dir"; rm -f "$output_file"' EXIT

# CodexBar reports one record per configured subscription. Query every provider
# selector the user asked for; "both" covers Codex and Claude in a single call.
collect() {
  local command=$1 provider index=0 file
  local -a args
  for provider in $providers; do
    file=$work_dir/$command.$index
    args=("$command" --provider "$provider" --json)
    [[ $command == cost ]] && args+=(--days 365)
    timeout 40s "$codexbar" "${args[@]}" >"$file" 2>/dev/null ||
      printf '[]\n' >"$file"
    index=$((index + 1))
  done
  jq -sc 'map(if type == "array" then .[] else . end) | map(select(type == "object"))' "$work_dir/$command."* 2>/dev/null ||
    printf '[]'
}

cost_incomplete() {
  jq -e '
    any(.[];
      ((.provider // .source // "") | ascii_downcase) == "claude"
      and (.last30DaysTokens // .totals.totalTokens // ([.daily[]?.totalTokens // 0] | add) // 0) > 0
      and (
        (.last30DaysCostUSD // .totals.totalCost // ([.daily[]?.totalCost // 0] | add) // 0) <= 0
        or any(.daily[]?.modelBreakdowns[]?; (.totalTokens // 0) > 0 and (.cost // 0) <= 0)
      )
    )
  ' "$1" >/dev/null
}

# Usage and cost are independent network-backed queries. Run them together so
# an explicit cockpit refresh waits for the slower query, not both in series.
collect usage >"$work_dir/usage.json" &
usage_pid=$!
collect cost >"$work_dir/cost.json" &
cost_pid=$!
wait "$usage_pid"
wait "$cost_pid"

# CodexBar occasionally returns Claude token totals with every cost set to zero.
# A direct retry immediately resolves the pricing data, so reject that partial
# response instead of replacing a valid cockpit estimate with it.
for attempt in 1 2; do
  cost_incomplete "$work_dir/cost.json" || break
  rm "$work_dir/cost.json"
  collect cost >"$work_dir/retry.json"
  mv "$work_dir/retry.json" "$work_dir/cost.json"
done

usage=$(<"$work_dir/usage.json")
cost=$(<"$work_dir/cost.json")

jq -n \
  --argjson usage "$usage" \
  --argjson cost "$cost" \
  --arg pi "${SEELE_SHELL_PI:-pi}" \
  --arg opencode "${SEELE_SHELL_OPENCODE:-opencode}" \
  --arg codex "${SEELE_SHELL_CODEX:-codex}" \
  --arg claude "${SEELE_SHELL_CLAUDE:-claude}" \
  --arg today "${SEELE_SHELL_TODAY:-$(date +%F)}" '
  def displayName($id):
    {
      codex: "Codex",
      claude: "Claude",
      openai: "OpenAI",
      copilot: "Copilot",
      cursor: "Cursor",
      gemini: "Gemini",
      opencode: "OpenCode"
    }[$id] // ($id | ascii_upcase);
  def window($name; $value):
    if ($value | type) == "object" and ($value.usedPercent? != null) then {
      name: $name,
      usedPercent: ($value.usedPercent | tonumber),
      resetsAt: ($value.resetsAt // ""),
      resetDescription: ($value.resetDescription // "")
    } else empty end;
  def modelRows($days):
    [$days[]?.modelBreakdowns[]?]
    | group_by(.modelName)
    | map({
        name: .[0].modelName,
        tokens: (map(.totalTokens // 0) | add),
        cost: (map(.cost // 0) | add)
      })
    | sort_by(-.tokens)
    | .[0:5];
  def period($days):
    {
      totalTokens: ([$days[]?.totalTokens // 0] | add // 0),
      totalCost: ([$days[]?.totalCost // 0] | add // 0),
      models: modelRows($days)
    };

  ([$cost[].daily[]?] | map(select(.date != null))) as $days
  | ($today + "T00:00:00Z" | fromdateiso8601) as $todayEpoch
  | (($todayEpoch - 6 * 86400) | strftime("%Y-%m-%d")) as $weekStart
  | (($todayEpoch - 29 * 86400) | strftime("%Y-%m-%d")) as $monthStart
  | ($days | map(select(.date == $today))) as $dayDays
  | ($days | map(select(.date >= $weekStart and .date <= $today))) as $weekDays
  | ($days | map(select(.date >= $monthStart and .date <= $today))) as $monthDays
  | ($days
     | group_by(.date)
     | map({
         date: .[0].date,
         totalTokens: (map(.totalTokens // 0) | add),
         cost: (map(.cost // 0) | add)
       })
     | sort_by(.date)) as $daily
  | {
      generatedAt: (now | todateiso8601),
      subscriptions: [
        $usage[]
        | (.provider // .source // "unknown") as $id
        | {
            id: $id,
            name: displayName($id),
            plan: (.usage.loginMethod // .plan // ""),
            source: (.source // "unavailable"),
            limits: [
              window("Session"; .usage.primary),
              window("Weekly"; .usage.secondary),
              window("Additional"; .usage.tertiary)
            ],
            credits: (.credits.remaining // null)
          }
      ],
      local: {
        today: ($daily | map(select(.date == $today)) | first // {}),
        daily: ($daily | .[-7:]),
        periods: {
          day: period($dayDays),
          week: period($weekDays),
          month: period($monthDays),
          all: period($days)
        },
        models: modelRows($days),
        totalTokens: ([$cost[] | .last30DaysTokens // .totals.totalTokens // 0] | add // 0),
        totalCost: ([$cost[] | .last30DaysCostUSD // .totals.totalCost // 0] | add // 0),
        currency: ([$cost[].currencyCode // empty] | first // "USD")
      },
      launchers: [
        {id: "pi", name: "Pi", command: $pi, description: "Primary Seele coding agent"},
        {id: "opencode", name: "OpenCode", command: $opencode, description: "Provider-flexible coding agent"},
        {id: "codex", name: "Codex", command: $codex, description: "OpenAI Codex CLI"},
        {id: "claude", name: "Claude Code", command: $claude, description: "Anthropic Claude Code CLI"}
      ]
    }
' >"$output_file"

mv "$output_file" "$cache"
cat "$cache"
