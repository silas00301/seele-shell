#!/usr/bin/env bash
set -euo pipefail

clock=$1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export XDG_STATE_HOME=$work/state

result=$($clock list)
# The list is derived from tzdata rather than curated, so it must carry the
# whole database, name each zone's countries, and still reach a zone by either
# of its abbreviations.
jq -e '
  .pinned == []
  and (.zones | length) > 200
  and any(.zones[]; .id == "UTC" and .zone == "UTC")
  and any(.zones[]; .id == "Europe/London" and .flag == "🇬🇧" and .kind == "city")
  and any(.zones[]; .id == "Asia/Kathmandu" and .flag == "🇳🇵" and .label == "Kathmandu")
  and any(.zones[]; .id == "America/Argentina/Buenos_Aires" and .label == "Buenos Aires")
  and any(.zones[]; .id == "America/Los_Angeles"
    and (.aliases | contains("PST")) and (.aliases | contains("PDT"))
    and (.aliases | contains("United States")))
  and any(.zones[];
    ([.id, .zone, .label, .aliases] | join(" ") | ascii_downcase | contains("utc+1")))
  and all(.zones[]; .time != "" and .offset != "")
' <<<"$result" >/dev/null

$clock pin Europe/London
$clock pin UTC
$clock pin Europe/London
jq -e '. == ["Europe/London", "UTC"]' "$XDG_STATE_HOME/seele-shell/timezone" >/dev/null
jq -e '.pinned == ["Europe/London", "UTC"]' < <($clock list) >/dev/null

$clock unpin Europe/London
jq -e '.pinned == ["UTC"]' < <($clock list) >/dev/null
$clock unpin UTC
[[ ! -e $XDG_STATE_HOME/seele-shell/timezone ]]

mkdir -p "$XDG_STATE_HOME/seele-shell"
printf 'Europe/London\n' >"$XDG_STATE_HOME/seele-shell/timezone"
jq -e '.pinned == ["Europe/London"]' < <($clock list) >/dev/null

if $clock pin Invalid/Timezone 2>/dev/null; then
  printf 'invalid timezones must not be pinned\n' >&2
  exit 1
fi
