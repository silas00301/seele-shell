#!/usr/bin/env bash
set -euo pipefail

agent_state=${1:?agent-state path required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/state/seele-shell"

printf '#!%s\n' "$BASH" >"$work/codexbar"
cat >>"$work/codexbar" <<'SCRIPT'
set -euo pipefail

case $1 in
  usage)
    printf '[]\n'
    ;;
  cost)
    [[ " $* " == *" --days 365 "* ]]
    count_file=$FAKE_CODEXBAR_STATE/cost-count
    count=$(($(cat "$count_file" 2>/dev/null || printf 0) + 1))
    printf '%s\n' "$count" >"$count_file"
    claude_cost=0
    ((count > 1)) && claude_cost=7
    jq -nc --argjson claude_cost "$claude_cost" '[
      {
        provider: "codex",
        daily: [
          {date: "2026-08-29", totalTokens: 20, totalCost: 2, modelBreakdowns: [{modelName: "codex-model", totalTokens: 20, cost: 2}]},
          {date: "2026-08-24", totalTokens: 10, totalCost: 1, modelBreakdowns: [{modelName: "codex-model", totalTokens: 10, cost: 1}]},
          {date: "2026-08-09", totalTokens: 30, totalCost: 3, modelBreakdowns: [{modelName: "codex-model", totalTokens: 30, cost: 3}]},
          {date: "2026-07-10", totalTokens: 40, totalCost: 4, modelBreakdowns: [{modelName: "codex-model", totalTokens: 40, cost: 4}]}
        ],
        last30DaysTokens: 60,
        last30DaysCostUSD: 6
      },
      {
        provider: "claude",
        daily: [
          {date: "2026-08-29", totalTokens: 70, totalCost: $claude_cost, modelBreakdowns: [{modelName: "claude-model", totalTokens: 70, cost: $claude_cost}]},
          {date: "2026-08-24", totalTokens: 20, totalCost: $claude_cost, modelBreakdowns: [{modelName: "claude-model", totalTokens: 20, cost: $claude_cost}]},
          {date: "2026-08-09", totalTokens: 10, totalCost: $claude_cost, modelBreakdowns: [{modelName: "claude-model", totalTokens: 10, cost: $claude_cost}]},
          {date: "2026-07-10", totalTokens: 50, totalCost: $claude_cost, modelBreakdowns: [{modelName: "claude-model", totalTokens: 50, cost: $claude_cost}]}
        ],
        last30DaysTokens: 100,
        last30DaysCostUSD: ($claude_cost * 3)
      }
    ]'
    ;;
esac
SCRIPT
chmod +x "$work/codexbar"

XDG_STATE_HOME="$work/state" \
FAKE_CODEXBAR_STATE="$work" \
SEELE_SHELL_CODEXBAR="$work/codexbar" \
SEELE_SHELL_TODAY="2026-08-29" \
  "$agent_state" >"$work/result.json"

jq -e '
  .local.periods.day.totalTokens == 90
  and .local.periods.day.totalCost == 9
  and .local.periods.week.totalTokens == 120
  and .local.periods.week.totalCost == 17
  and .local.periods.month.totalTokens == 160
  and .local.periods.month.totalCost == 27
  and .local.periods.all.totalTokens == 250
  and .local.periods.all.totalCost == 38
  and any(.local.periods.day.models[]; .name == "claude-model" and .tokens == 70 and .cost == 7)
' "$work/result.json" >/dev/null

test "$(<"$work/cost-count")" -eq 2
printf 'agent state tests passed\n'
