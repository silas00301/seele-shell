#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/seele-shell
pin_file=$state_dir/timezone
zoneinfo_dir=${TZDIR:-/etc/zoneinfo}
unit_separator=$'\x1f'

# Every zone comes from tzdata itself: zone1970.tab lists the canonical zones
# with the countries that use them, and iso3166.tab names those countries. The
# flag is the country code rendered as regional indicator letters, so no zone,
# label, or flag is maintained by hand here.
zones() {
  awk -F '\t' -v OFS='|' '
    function flag(code,   first, second) {
      if (length(code) != 2) return ""
      first = index("ABCDEFGHIJKLMNOPQRSTUVWXYZ", substr(code, 1, 1)) - 1
      second = index("ABCDEFGHIJKLMNOPQRSTUVWXYZ", substr(code, 2, 1)) - 1
      if (first < 0 || second < 0) return ""
      return sprintf("%c%c%c%c%c%c%c%c", 240, 159, 135, 166 + first, 240, 159, 135, 166 + second)
    }

    FILENAME ~ /iso3166/ {
      if ($0 ~ /^#/ || NF < 2) next
      country[$1] = $2
      next
    }

    $0 ~ /^#/ || NF < 3 { next }
    {
      zone = $3
      split($1, codes, ",")

      label = zone
      sub(/^.*\//, "", label)
      gsub(/_/, " ", label)

      names = ""
      for (i = 1; i in codes; i++) {
        if (codes[i] in country) names = names " " country[codes[i]]
      }

      region = zone
      gsub(/[_\/]/, " ", region)

      print zone, zone, label, flag(codes[1]), region names " " $4, "city"
    }
  ' "$zoneinfo_dir/iso3166.tab" "$zoneinfo_dir/zone1970.tab" | sort -t '|' -k3,3

  printf '%s\n' 'UTC|UTC|Coordinated Universal Time||UTC GMT Zulu Coordinated Universal Time|abbreviation'

  # tzdata's Etc/GMT files are fixed whole-hour offsets. POSIX reverses their
  # signs (Etc/GMT-1 is UTC+1), so expose human-facing UTC/GMT spellings while
  # still deriving every available entry from the packaged database.
  local path zone suffix magnitude hours display_sign padded
  for path in "$zoneinfo_dir"/Etc/GMT[+-]*; do
    [[ -e $path ]] || continue
    zone=${path#"$zoneinfo_dir"/}
    suffix=${zone#Etc/GMT}
    magnitude=${suffix#?}
    hours=$((10#$magnitude))
    if [[ $suffix == +* ]]; then
      display_sign=-
    else
      display_sign=+
    fi
    printf -v padded '%02d' "$hours"
    printf 'UTC%s%d|%s|UTC%s%d||UTC%s%d UTC%s%s UTC%s%d:00 UTC%s%s:00 GMT%s%d GMT%s%s GMT%s%d:00 GMT%s%s:00 %s%s00 %s%s:00|offset\n' \
      "$display_sign" "$hours" "$zone" "$display_sign" "$hours" \
      "$display_sign" "$hours" "$display_sign" "$padded" \
      "$display_sign" "$hours" "$display_sign" "$padded" \
      "$display_sign" "$hours" "$display_sign" "$padded" \
      "$display_sign" "$hours" "$display_sign" "$padded" \
      "$display_sign" "$padded" "$display_sign" "$padded"
  done
}

resolve_id() {
  local wanted=${1:-}
  # The whole list is read even after a match: leaving the pipe early would
  # kill the generator with SIGPIPE, which pipefail then reports as failure.
  zones | awk -F '|' -v wanted="$wanted" '
    BEGIN { wanted = tolower(wanted) }
    tolower($1) == wanted && exact == "" { exact = $1 }
    tolower($2) == wanted && fallback == "" { fallback = $1 }
    END {
      if (exact != "") print exact
      else if (fallback != "") print fallback
    }
  '
}

pinned_ids() {
  local legacy
  if [[ ! -f $pin_file ]]; then
    printf '[]'
  elif jq -e 'type == "array"' "$pin_file" >/dev/null 2>&1; then
    jq -c 'reduce (.[] | strings) as $id ([]; if index($id) then . else . + [$id] end)' "$pin_file"
  else
    legacy=$(resolve_id "$(<"$pin_file")")
    jq -cn --arg id "$legacy" 'if $id == "" then [] else [$id] end'
  fi
}

write_pins() {
  local pins=$1 temp
  if [[ $(jq 'length' <<<"$pins") == 0 ]]; then
    rm -f "$pin_file"
    return
  fi
  mkdir -p "$state_dir"
  temp=$(mktemp "${pin_file}.XXXXXX")
  printf '%s\n' "$pins" >"$temp"
  mv "$temp" "$pin_file"
}

list() {
  local pinned year winter summer
  pinned=$(pinned_ids)

  # Both halves of the year are sampled so a zone stays searchable by either of
  # its abbreviations, whichever one happens to be in force today.
  printf -v year '%(%Y)T' -1
  winter=$(date -d "$year-01-15 12:00" +%s)
  summer=$(date -d "$year-07-15 12:00" +%s)

  zones_json=$(
    while IFS='|' read -r id zone label flag aliases kind; do
      local timezone clock day abbreviation offset standard daylight
      if [[ $zone == */* ]]; then
        [[ -f $zoneinfo_dir/$zone ]] || continue
        timezone=:$zoneinfo_dir/$zone
      else
        timezone=$zone
      fi

      # printf's time format is a shell builtin, so a few hundred zones cost no
      # processes at all.
      TZ=$timezone printf -v clock '%(%H:%M)T' -1
      TZ=$timezone printf -v day '%(%a %d %b)T' -1
      TZ=$timezone printf -v abbreviation '%(%Z)T' -1
      TZ=$timezone printf -v offset '%(%z)T' -1
      TZ=$timezone printf -v standard '%(%Z)T' "$winter"
      TZ=$timezone printf -v daylight '%(%Z)T' "$summer"

      printf '%s\n' "$id$unit_separator$zone$unit_separator$label$unit_separator$flag$unit_separator$aliases $standard $daylight$unit_separator$kind$unit_separator$clock$unit_separator$day$unit_separator$abbreviation$unit_separator$offset"
    done < <(zones) | jq -R -s --arg separator "$unit_separator" '
      split("\n")
      | map(select(length > 0) | split($separator))
      | map({
          id: .[0],
          zone: .[1],
          label: .[2],
          flag: .[3],
          aliases: (
            .[4]
            | [splits("\\s+")]
            | reduce .[] as $word ([]; if any(.[]; ascii_downcase == ($word | ascii_downcase)) then . else . + [$word] end)
            | join(" ")
          ),
          kind: .[5],
          time: .[6],
          day: .[7],
          abbreviation: .[8],
          offset: .[9]
        })'
  )

  jq -cn --argjson pinned "$pinned" --argjson zones "$zones_json" '{pinned:$pinned, zones:$zones}'
}

pin() {
  local id pins
  id=$(resolve_id "${1:-}")
  if [[ -z $id ]]; then
    printf 'Unknown timezone: %s\n' "${1:-}" >&2
    exit 2
  fi
  pins=$(pinned_ids)
  write_pins "$(jq -c --arg id "$id" 'if index($id) then . else . + [$id] end' <<<"$pins")"
}

unpin() {
  local id pins
  id=$(resolve_id "${1:-}")
  if [[ -z $id ]]; then
    printf 'Unknown timezone: %s\n' "${1:-}" >&2
    exit 2
  fi
  pins=$(pinned_ids)
  write_pins "$(jq -c --arg id "$id" 'map(select(. != $id))' <<<"$pins")"
}

case ${1:-list} in
  list) list ;;
  pin) pin "${2:-}" ;;
  unpin) unpin "${2:-}" ;;
  *) printf 'Usage: seele-clock [list|pin TIMEZONE|unpin TIMEZONE]\n' >&2; exit 2 ;;
esac
