#!/usr/bin/env bash
set -euo pipefail

seele_config_dir=${XDG_CONFIG_HOME:-$HOME/.config}/seele-shell
tray_config=$seele_config_dir/tray.json
bar_config=$seele_config_dir/bar.json
librepods_config=${XDG_CONFIG_HOME:-$HOME/.config}/AirPodsTrayApp/AirPodsTrayApp.conf
bluetooth_scan_pidfile=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/bluetooth-scan.pid
# One window for both directions of discovery, the way a phone's Bluetooth
# screen behaves: while it is open the machine is looking and answering at
# once, so the scan, the adapter's discoverability, and the pairing agent all
# share this lifetime and expire together.
bluetooth_discovery_timeout=120
bluetooth_receiver_pidfile=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/bluetooth-receiver.pid
bluetooth_agent_pidfile=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/bluetooth-agent.pid
bluetooth_pairing_request=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/bluetooth-pairing.json
bluetooth_pairing_answer=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/bluetooth-pairing.answer
bluetooth_agent_log=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/bluetooth-agent.log
bluetooth_discoverable_timeout=180
agent_state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/seele-shell/agents
openlogi_battery_cache=${XDG_RUNTIME_DIR:-/tmp}/seele-shell/openlogi-batteries.json

timestamp() {
  date -u +%Y-%m-%dT%H:%M:%SZ
}

percent_for() {
  awk '{ printf "%d", $2 * 100 }' <<<"$1"
}

audio_devices() {
  # A card whose profile is `Off` has no sink node at all, so listing sinks
  # alone hides every output that is merely not switched on -- onboard analog
  # and HDMI, typically. Those are offered as profile entries instead, but only
  # where the card has no sink already and the profile reports `available: yes`,
  # which keeps unplugged jacks and the Pro Audio variants out of the list.
  jq -c '
    . as $all
    | [$all[] | select(.type == "PipeWire:Interface:Metadata" and .props["metadata.name"] == "default") | .metadata[]?] as $meta
    | (($meta | map(select(.key == "default.audio.sink")) | first).value.name // "") as $sink
    | (($meta | map(select(.key == "default.audio.source")) | first).value.name // "") as $source
    | [ $all[]
        | select(.info.props["media.class"] == "Audio/Sink" or .info.props["media.class"] == "Audio/Source")
        | {
            id: .id,
            kind: (if .info.props["media.class"] == "Audio/Sink" then "output" else "input" end),
            name: (.info.props["node.description"] // .info.props["node.nick"] // .info.props["node.name"] // ""),
            node: (.info.props["node.name"] // ""),
            profile: null
          }
      ] as $nodes
    | [ $all[]
        | select(.type == "PipeWire:Interface:Node" and .info.props["media.class"] == "Audio/Sink")
        | .info.props["device.id"]
      ] as $sinkDevices
    | [ $all[]
        | select(.type == "PipeWire:Interface:Device" and .info.props["media.class"] == "Audio/Device")
        | select(([.id] | inside($sinkDevices)) | not)
        | . as $device
        | (.info.params.EnumProfile // [])
        | map(select(.available == "yes" and ((.classes // []) | tostring | contains("Audio/Sink"))))
        | .[]
        | {
            id: $device.id,
            kind: "output",
            name: (($device.info.props["device.description"] // $device.info.props["device.name"] // "") + " · " + .description),
            node: "",
            profile: .index
          }
      ] as $profiles
    | ($nodes + $profiles)
    | map(. + { default: (if .profile != null then false elif .kind == "output" then .node == $sink else .node == $source end) })
    | sort_by([.kind, (.name | ascii_downcase)])
  ' <<<"$1"
}

agent_root_pids() {
  # Reading /proc through awk keeps this in the tens of milliseconds; a shell
  # loop over every pid made each control action feel sluggish.
  {
    awk 'FNR == 1 {
      split(FILENAME, path, "/")
      name = $0
      sub(/^\./, "", name)
      sub(/-wrapp(ed)?$/, "", name)
      if (name ~ /^(pi|opencode|codex|claude)$/) print path[3], name
    }' /proc/[0-9]*/comm 2>/dev/null
    awk 'BEGIN { RS = "\0" }
      FNR == 1 {
        split(FILENAME, path, "/")
        name = $0
        sub(/.*\//, "", name)
        sub(/^\./, "", name)
        sub(/-wrapp(ed)?$/, "", name)
        if (name ~ /^(pi|opencode|codex|claude)$/) print path[3], name
        nextfile
      }' /proc/[0-9]*/cmdline 2>/dev/null
  } | sort -u
}

# Harnesses without a lifecycle integration still reveal whether they are
# thinking or waiting for the user: a session that burns no CPU across several
# refreshes is waiting. Measure whole subtrees, because Claude Code and Pi do
# their work in helper processes.
agent_cpu_records() {
  local roots subtrees sample_file previous now records
  roots=$(agent_root_pids)
  if [[ -z $roots ]]; then
    printf '[]'
    return
  fi

  subtrees=$(awk -v roots="$roots" '
    BEGIN {
      total = split(roots, lines, "\n")
      for (i = 1; i <= total; i++) {
        split(lines[i], entry, " ")
        if (entry[1] != "") owner[entry[1]] = entry[2]
      }
    }
    {
      pid = $1
      match($0, /\(.*\)/)
      rest = substr($0, RSTART + RLENGTH + 2)
      split(rest, fields, " ")
      parent[pid] = fields[2]
      ticks[pid] = fields[12] + fields[13]
      pids[++count] = pid
    }
    END {
      for (root in owner) {
        delete member
        member[root] = 1
        changed = 1
        while (changed) {
          changed = 0
          for (i = 1; i <= count; i++) {
            if (!(pids[i] in member) && (parent[pids[i]] in member)) {
              member[pids[i]] = 1
              changed = 1
            }
          }
        }
        total = 0
        for (i = 1; i <= count; i++) {
          if (pids[i] in member) total += ticks[pids[i]]
        }
        print root, owner[root], total
      }
    }
  ' /proc/[0-9]*/stat 2>/dev/null)

  sample_file=$agent_state_dir/.cpu-sample.json
  previous='{}'
  [[ -r $sample_file ]] && previous=$(jq -c 'if type == "object" then . else {} end' "$sample_file" 2>/dev/null || printf '{}')
  now=$(date +%s)

  records=$(jq -Rsc \
    --argjson previous "$previous" \
    --argjson now "$now" \
    --arg updatedAt "$(timestamp)" '
    split("\n")
    | map(select(length > 0) | split(" ") | {pid: (.[0] | tonumber), agent: .[1], ticks: (.[2] | tonumber)})
    | map(. as $sample
        | ($previous[$sample.pid | tostring] // null) as $last
        | (if $last == null then 0 else ($now - ($last.at // $now)) end) as $elapsed
        | (if $last == null then 0 else ($sample.ticks - ($last.ticks // $sample.ticks)) end) as $burnt
        | (if $last == null or $elapsed <= 0 or $burnt > (2 * $elapsed) then 0
           else (($last.idle // 0) + $elapsed) end) as $quiet
        | $sample + {
            source: "cpu",
            at: $now,
            idle: $quiet,
            updatedAt: $updatedAt,
            status: (if $quiet >= 20 then "input" else "working" end)
          })
  ' <<<"$subtrees")

  mkdir -p "$agent_state_dir"
  local tmp
  tmp=$(mktemp "$agent_state_dir/.cpu-sample.XXXXXX")
  if jq -c 'map({key: (.pid | tostring), value: {ticks, at, idle}}) | from_entries' <<<"$records" >"$tmp"; then
    mv "$tmp" "$sample_file"
  else
    rm -f "$tmp"
  fi

  printf '%s' "$records"
}

screen_recording() {
  if jq -e 'any(.[]; (.info.props["media.class"] // "") == "Stream/Output/Video" and (.info.state // "") == "running")' <<<"$1" >/dev/null 2>&1; then
    printf 'true'
    return
  fi
  if awk 'FNR == 1 {
      name = $0
      sub(/^\./, "", name)
      if (name ~ /^(wf-recorder|wl-screenrec|obs|gpu-screen|kooha|simplescreenr)/) { found = 1; exit }
    }
    END { exit !found }' /proc/[0-9]*/comm 2>/dev/null; then
    printf 'true'
  else
    printf 'false'
  fi
}

# A record only describes a session while its process is alive. Runs that ended
# stay visible briefly so a completed launch can report itself, then their files
# are removed instead of pinning a stale status to the cockpit forever.
agent_records() {
  local files=() records pid live now
  if [[ -d $agent_state_dir ]]; then
    shopt -s nullglob
    files=("$agent_state_dir"/*.json)
    shopt -u nullglob
  fi
  if ((${#files[@]} == 0)); then
    printf '[]'
    return
  fi

  records=$(jq -c '. + {file: input_filename}' "${files[@]}" 2>/dev/null |
    jq -sc 'map(select(type == "object" and (.agent | type) == "string"))' 2>/dev/null || printf '[]')

  live=$(while IFS= read -r pid; do
    [[ $pid =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null && printf '%s\n' "$pid"
  done < <(jq -r '.[].pid // empty' <<<"$records") |
    jq -Rsc 'split("\n") | map(select(length > 0) | tonumber) | unique')
  now=$(date +%s)

  records=$(jq -c --argjson live "$live" --argjson now "$now" '
    map(. + {
      live: (((.pid // -1) as $pid | $live | index($pid)) != null),
      age: ($now - ((.updatedAt // .endedAt // .startedAt // "") | fromdateiso8601? // 0))
    })' <<<"$records")

  while IFS= read -r file; do
    [[ -n $file ]] && rm -f "$file"
  done < <(jq -r '.[] | select((.live | not) and .age >= 300) | .file // empty' <<<"$records")

  jq -c 'map(select(.live or .age < 300))' <<<"$records"
}

agent_states() {
  local records sampled
  records=$(agent_records)
  sampled=$(agent_cpu_records)
  jq -nc --argjson records "$records" --argjson sampled "$sampled" '
    def ordered: sort_by(.updatedAt // .endedAt // .startedAt // "");
    def status($items):
      if any($items[]; .status == "input") then "input"
      elif any($items[]; .status == "working") then "working"
      else ($items | ordered | last | .status // "running")
      end;
    reduce (([$records[].agent, $sampled[].agent] | unique)[]) as $agent ({};
      ($records | map(select(.agent == $agent)) | ordered) as $saved
      | ($saved | map(select(.live and .source == "native"))) as $native
      | ($saved | map(select(.live and .source != "native"))) as $heuristic
      | ($sampled | map(select(.agent == $agent))) as $running
      | ((($native + $heuristic + $running) | length) > 0) as $active
      | (($native | last) // ($heuristic | last) // ($running | last) // ($saved | last) // {agent: $agent}) as $base
      | . + { ($agent): (
          $base + {
            agent: $agent,
            active: $active,
            status: (
              if ($native | length) > 0 then status($native)
              elif ($heuristic | length) > 0 then status($heuristic)
              elif ($running | length) > 0 then status($running)
              elif ($saved | length) > 0 then ($saved | last | .status // "idle")
              else "idle"
              end
            )
          }
          | del(.file, .live, .age, .ticks, .at, .idle)
        ) }
    )
  '
}

notification_state() {
  local active history times now state_file result temp
  state_file=${XDG_STATE_HOME:-$HOME/.local/state}/seele-shell/notification-times.json
  active=$(makoctl list -j 2>/dev/null || printf '[]')
  history=$(makoctl history -j 2>/dev/null || printf '[]')
  now=$(date +%s)
  times='{}'
  if [[ -r $state_file ]]; then
    times=$(jq -cs 'if length == 1 and (.[0] | type == "object") then .[0] else {} end' "$state_file" 2>/dev/null) || times='{}'
  fi
  # mako does not timestamp notifications, so remember when each id first
  # appeared and use that to build the 24 hour history view.
  result=$(jq -nc \
    --argjson active "$active" \
    --argjson history "$history" \
    --argjson times "$times" \
    --argjson now "$now" '
    def stamp($entries; $seen): reduce $entries[] as $entry ($seen;
      if has(($entry.id | tostring)) then . else . + { ($entry.id | tostring): $now } end);
    ($times | with_entries(select(.value > ($now - 86400)))) as $kept
    | stamp($active + $history; $kept) as $stamps
    | def at($entry): $stamps[($entry.id | tostring)] // $now;
      {
        stamps: $stamps,
        count: ($active | length),
        items: ($active | map(. + { time: at(.) }) | reverse),
        history: (
          $history
          | map(select(($stamps[(.id | tostring)] // 0) > ($now - 86400)))
          | map(. + { time: at(.) })
          | reverse
          | .[:40]
        )
      }')
  mkdir -p "${state_file%/*}"
  temp=$(mktemp "${state_file}.XXXXXX")
  if jq -c '.stamps' <<<"$result" >"$temp"; then
    mv "$temp" "$state_file"
  else
    rm -f "$temp"
  fi
  jq -c 'del(.stamps)' <<<"$result"
}

speedtest_result() {
  speedtest \
    --accept-license \
    --accept-gdpr \
    --format=jsonl \
    --progress=yes \
    --progress-update-interval=250 \
    | jq --unbuffered -ce '
        if .type == "ping" then
          {phase:"ping", ping:(.ping.latency // 0), jitter:(.ping.jitter // 0)}
        elif .type == "download" then
          {phase:"download", download:((.download.bandwidth // 0) * 8 / 1000000)}
        elif .type == "upload" then
          {phase:"upload", upload:((.upload.bandwidth // 0) * 8 / 1000000)}
        elif .type == "result" then
          {
            ping:(.ping.latency // 0),
            jitter:(.ping.jitter // 0),
            download:((.download.bandwidth // 0) * 8 / 1000000),
            upload:((.upload.bandwidth // 0) * 8 / 1000000),
            server:(.server.name // "Ookla Speedtest")
          }
        else empty
        end
      '
}

launcher_toggle() {
  if vicinae state open; then
    vicinae close
  else
    vicinae open
  fi
}

bluetooth_scan_active() {
  local pid
  [[ -r $bluetooth_scan_pidfile ]] || return 1
  pid=$(<"$bluetooth_scan_pidfile")
  [[ -n $pid && -r /proc/$pid/comm && $(</proc/"$pid"/comm) == bluetoothctl ]]
}

bluetooth_state() {
  local objects scanning=false receiver=false
  if bluetooth_scan_active; then scanning=true; fi
  if bluetooth_receiver_active; then receiver=true; fi
  objects=$(busctl --json=short call org.bluez / org.freedesktop.DBus.ObjectManager GetManagedObjects 2>/dev/null) || objects=""
  if [[ -z $objects ]]; then
    printf '{"available":false,"powered":false,"scanning":false,"receiver":false,"discoverable":false,"connected":0,"devices":[],"airpodsConnected":false,"airpodsName":""}'
    return
  fi
  jq -c --argjson scanning "$scanning" --argjson receiver "$receiver" '
    (.data[0] // {}) as $objects
    | [$objects[]["org.bluez.Adapter1"] // empty] as $adapters
    | ([$objects[]
        | .["org.bluez.MediaTransport1"] // empty
        | { device: (.Device.data // ""), state: (.State.data // "") }
      ]) as $transports
    | ([$objects | to_entries[]
        | select(.value["org.bluez.Device1"])
        | .key as $path
        | .value as $interfaces
        | $interfaces["org.bluez.Device1"] as $device
        | {
            address: ($device.Address.data // ""),
            name: ($device.Alias.data // $device.Name.data // $device.Address.data // ""),
            icon: ($device.Icon.data // ""),
            paired: ($device.Paired.data // false),
            trusted: ($device.Trusted.data // false),
            connected: ($device.Connected.data // false),
            rssi: ($device.RSSI.data // null),
            # A phone plays into this machine through the A2DP Audio Source
            # role, which is what separates a device the receiver can carry
            # from a headset that only ever receives.
            source: (($device.UUIDs.data // []) | any(ascii_downcase | startswith("0000110a"))),
            streaming: (any($transports[]; .device == $path and .state == "active")),
            battery: ($interfaces["org.bluez.Battery1"].Percentage.data // null)
          }
        | select(.address != "" and .name != "")
        | select(.paired or .connected or (.name | ascii_downcase) != (.address | ascii_downcase | gsub(":"; "-")))
      # Deliberately not sorted on connection state. That changes on its own --
      # a headset reconnecting, a phone dropping -- and would pull rows out from
      # under whichever one is being aimed at. Pairing changes only when the
      # user asks for it, so it is safe to order on.
      ] | sort_by([(if .paired then 0 else 1 end), (.name | ascii_downcase), .address])) as $devices
    | ([$devices[] | select(.connected and (.name | test("airpods|beats"; "i")))] | first) as $airpods
    | {
        available: ($adapters | length > 0),
        powered: ([$adapters[] | select(.Powered.data == true)] | length > 0),
        scanning: $scanning,
        receiver: $receiver,
        discoverable: ([$adapters[] | select(.Discoverable.data == true)] | length > 0),
        connected: ([$devices[] | select(.connected)] | length),
        devices: $devices,
        airpodsConnected: ($airpods != null),
        airpodsName: ($airpods.name // "")
      }
  ' <<<"$objects"
}

bluetooth_scan_stop() {
  local pid=""
  if [[ -r $bluetooth_scan_pidfile ]]; then
    pid=$(<"$bluetooth_scan_pidfile")
    rm -f "$bluetooth_scan_pidfile"
  fi
  if [[ -n $pid && -r /proc/$pid/comm && $(</proc/"$pid"/comm) == bluetoothctl ]]; then
    kill "$pid" 2>/dev/null || true
  fi
  timeout 5 bluetoothctl scan off >/dev/null 2>&1 || true
}

bluetooth_scan_start() {
  bluetooth_scan_stop
  bluetoothctl power on >/dev/null 2>&1 || true
  mkdir -p "${bluetooth_scan_pidfile%/*}"
  setsid bash -c 'echo $$ >"$1"; shift; exec "$@"' seele-bluetooth-scan "$bluetooth_scan_pidfile" \
    bluetoothctl --timeout "$bluetooth_discovery_timeout" scan on >/dev/null 2>&1 &
  for _ in $(seq 1 20); do
    if bluetooth_scan_active; then break; fi
    sleep 0.05
  done
}

bluetooth_receiver_active() {
  local pid
  [[ -r $bluetooth_receiver_pidfile ]] || return 1
  pid=$(<"$bluetooth_receiver_pidfile")
  [[ -n $pid && -r /proc/$pid/cmdline ]] || return 1
  grep -qa bt-receiver "/proc/$pid/cmdline"
}

bluetooth_receiver_stop() {
  local pid=""
  if [[ -r $bluetooth_receiver_pidfile ]]; then
    pid=$(<"$bluetooth_receiver_pidfile")
    rm -f "$bluetooth_receiver_pidfile"
  fi
  # `setsid` made the supervisor its own process group leader, so signalling the
  # group retires every loopback it started without waiting for its next poll.
  if [[ -n $pid && -r /proc/$pid/cmdline ]] && grep -qa bt-receiver "/proc/$pid/cmdline"; then
    kill -TERM -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  fi
}

bluetooth_receiver_start() {
  bluetooth_receiver_stop
  # Receiver mode deliberately leaves discoverability alone. A device this
  # machine has already paired reaches it over ordinary page scan, so nothing
  # here needs the adapter to answer inquiries from strangers.
  bluetoothctl power on >/dev/null 2>&1 || true
  mkdir -p "${bluetooth_receiver_pidfile%/*}"
  setsid bash -c 'echo $$ >"$1"; shift; exec "$@"' seele-bluetooth-receiver "$bluetooth_receiver_pidfile" \
    seele-bt-receiver >/dev/null 2>&1 &
  for _ in $(seq 1 20); do
    if bluetooth_receiver_active; then break; fi
    sleep 0.05
  done
}

bluetooth_agent_active() {
  local pid
  [[ -r $bluetooth_agent_pidfile ]] || return 1
  pid=$(<"$bluetooth_agent_pidfile")
  [[ -n $pid && -r /proc/$pid/cmdline ]] || return 1
  grep -qa bt-agent "/proc/$pid/cmdline"
}

bluetooth_agent_stop() {
  local pid=""
  if [[ -r $bluetooth_agent_pidfile ]]; then
    pid=$(<"$bluetooth_agent_pidfile")
    rm -f "$bluetooth_agent_pidfile"
  fi
  if [[ -n $pid && -r /proc/$pid/cmdline ]] && grep -qa bt-agent "/proc/$pid/cmdline"; then
    kill -TERM -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  fi
  rm -f "$bluetooth_pairing_request" "$bluetooth_pairing_answer"
  seele-shellctl -q bluetooth-pairing-dismiss >/dev/null 2>&1 || true
}

bluetooth_agent_start() {
  bluetooth_agent_stop
  mkdir -p "${bluetooth_agent_pidfile%/*}"
  # Keep the agent's own account of what BlueZ asked it. Which association
  # model a remote picks is the first thing worth knowing when a pairing fails,
  # and it is invisible from anywhere else.
  SEELE_BLUETOOTH_PAIRING_WINDOW=$bluetooth_discovery_timeout \
  SEELE_BLUETOOTH_DISCOVERABLE_TIMEOUT=$bluetooth_discoverable_timeout \
  setsid bash -c 'echo $$ >"$1"; shift; exec "$@"' seele-bluetooth-agent "$bluetooth_agent_pidfile" \
    seele-bt-agent >"$bluetooth_agent_log" 2>&1 &
  for _ in $(seq 1 20); do
    if bluetooth_agent_active; then break; fi
    sleep 0.05
  done
}

bluetooth_agent_ensure() {
  bluetooth_agent_active || bluetooth_agent_start
}

bluetooth_pairing_close() {
  bluetooth_agent_stop
  bluetoothctl discoverable off >/dev/null 2>&1 || true
  bluetoothctl pairable off >/dev/null 2>&1 || true
  bluetoothctl discoverable-timeout "$bluetooth_discoverable_timeout" >/dev/null 2>&1 || true
}

bluetooth_pairing_open() {
  bluetoothctl power on >/dev/null 2>&1 || true
  # The agent has to be listening before the adapter answers anyone, because
  # BlueZ rejects a pairing request outright when none is registered.
  bluetooth_agent_start
  # BlueZ retires discoverability itself when the timeout expires, so the
  # window closes on its own even if nothing else closes it.
  bluetoothctl discoverable-timeout "$bluetooth_discovery_timeout" >/dev/null 2>&1 || true
  bluetoothctl pairable on >/dev/null 2>&1 || true
  bluetoothctl discoverable on >/dev/null 2>&1 || true
}

# Discovery is symmetric. Searching without answering finds a speaker but
# leaves a phone unable to reach this machine at all, and the two halves must
# expire together or the spinner stops while the adapter is still open.
bluetooth_discovery_start() {
  bluetooth_scan_start
  bluetooth_pairing_open
}

bluetooth_discovery_stop() {
  bluetooth_scan_stop
  bluetooth_pairing_close
}

bluetooth_pairing_answer() {
  local token=$1 verdict=$2 value=${3:-}
  case $verdict in
    accept|reject) ;;
    *) return 2 ;;
  esac
  mkdir -p "${bluetooth_pairing_answer%/*}"
  printf '%s %s %s\n' "$token" "$verdict" "$value" >"$bluetooth_pairing_answer.new"
  mv "$bluetooth_pairing_answer.new" "$bluetooth_pairing_answer"
}

bluetooth_paired() {
  jq -r --arg address "$1" 'any(.devices[]; .address == $address and .paired)' <<<"$(bluetooth_state)"
}

# Pairing this machine started is the same exchange as pairing it answered, so
# it runs through the same agent and the same prompt. A terminal is the
# fallback for a step the shell cannot draw, and it can draw this one.
bluetooth_pair_device() {
  local address=$1
  bluetooth_scan_stop
  bluetooth_agent_ensure
  # Detached, because the exchange waits on a person: holding control.sh open
  # for it would block every other action in the panel behind it.
  setsid -f bash -c '
    timeout 90 bluetoothctl pair "$1" >/dev/null 2>&1 || true
    timeout 10 bluetoothctl trust "$1" >/dev/null 2>&1 || true
    timeout 20 bluetoothctl connect "$1" >/dev/null 2>&1 || true
  ' seele-bluetooth-pair "$address" >/dev/null 2>&1
}

tray_hidden() {
  if [[ -r $tray_config ]]; then
    jq -c '[(.hidden // [])[] | select(type == "string")]' "$tray_config" 2>/dev/null || printf '[]'
  else
    printf '[]'
  fi
}

tray_set_hidden() {
  local id=$1 action=$2 hidden
  hidden=$(tray_hidden)
  mkdir -p "$seele_config_dir"
  jq -nc --argjson hidden "$hidden" --arg id "$id" --arg action "$action" '
    (($hidden | index($id)) != null) as $present
    | (if $action == "hide" or ($action == "toggle" and ($present | not))
       then ($hidden + [$id] | unique)
       else ($hidden - [$id])
       end)
    | { hidden: . }' >"$tray_config.new"
  mv "$tray_config.new" "$tray_config"
}

# Menu bar module placement. Only explicit choices are stored; the shell owns
# the default for a module nobody has moved yet.
bar_modules() {
  if [[ -r $bar_config ]]; then
    jq -c '(.modules // {}) | with_entries(select(.value | type == "boolean"))' "$bar_config" 2>/dev/null || printf '{}'
  else
    printf '{}'
  fi
}

bar_set_module() {
  local id=$1 action=$2 modules
  modules=$(bar_modules)
  mkdir -p "$seele_config_dir"
  jq -nc --argjson modules "$modules" --arg id "$id" --argjson shown "$([[ $action == show ]] && printf true || printf false)" '
    { modules: ($modules + { ($id): $shown }) }' >"$bar_config.new"
  mv "$bar_config.new" "$bar_config"
}

upower_batteries() {
  local paths path properties entries=()
  paths=$(busctl --json=short call org.freedesktop.UPower /org/freedesktop/UPower \
    org.freedesktop.UPower EnumerateDevices 2>/dev/null | jq -r '.data[0][]? // empty' 2>/dev/null || true)
  while IFS= read -r path; do
    [[ -n $path ]] || continue
    [[ $path == */DisplayDevice ]] && continue
    properties=$(busctl --json=short call org.freedesktop.UPower "$path" \
      org.freedesktop.DBus.Properties GetAll s org.freedesktop.UPower.Device 2>/dev/null) || continue
    entries+=("$(jq -c '
      (.data[0] // {}) as $device
      | {
          kind: (if ($device.PowerSupply.data // false) then "system" else "device" end),
          name: (($device.Model.data // "") | if . == "" then "Battery" else . end),
          percent: (($device.Percentage.data // 0) | round),
          status: (
            {"1": "Charging", "2": "Discharging", "3": "Empty", "4": "Full", "5": "Charging", "6": "Discharging"}[
              ($device.State.data // 0) | tostring
            ] // "Unknown"
          ),
          icon: "",
          present: (($device.IsPresent.data // false) and (($device.Type.data // 0) != 1))
        }
      | select(.present and .percent > 0)
      | del(.present)
    ' <<<"$properties" 2>/dev/null || true)")
  done <<<"$paths"
  if ((${#entries[@]} > 0)); then
    printf '%s\n' "${entries[@]}" | jq -sc 'map(select(type == "object"))'
  else
    printf '[]'
  fi
}

airpods_batteries() {
  local state
  state=$(timeout 2 librepods-ctl status 2>/dev/null) || state='{"batteries":[]}'
  jq -c '[
    .batteries[]?
    | select((.percent // 0) > 0)
    | {
        kind: "device",
        name: .name,
        percent: (.percent | round),
        status: (.status // ""),
        icon: "audio-headphones"
      }
  ]' <<<"$state" 2>/dev/null || printf '[]'
}

openlogi_batteries() {
  local now modified output batteries temp
  if ! command -v openlogi >/dev/null 2>&1; then
    printf '[]'
    return
  fi

  now=$(date +%s)
  modified=$(stat -c %Y "$openlogi_battery_cache" 2>/dev/null || printf 0)
  if ((now - modified < 30)) && jq -e 'type == "array"' "$openlogi_battery_cache" >/dev/null 2>&1; then
    cat "$openlogi_battery_cache"
    return
  fi

  output=$(timeout 5 openlogi list 2>/dev/null || true)
  batteries=$(jq -Rsc '
    [
      split("\n")[]
      | try capture("slot [0-9]+ +● +(?<name>.+?) \\((?<deviceKind>[^,]+),[^\n]*battery=(?<percent>[0-9]+)% [^ ]+ \\((?<status>[^)]+)\\)")
      | {
          kind: "logitech",
          name: .name,
          percent: (.percent | tonumber),
          status: (
            if (.status | startswith("charging")) then "Charging"
            elif .status == "discharging" then "Discharging"
            elif .status == "full" then "Full"
            else .status
            end
          ),
          icon: (
            if .deviceKind == "mouse" then "input-mouse"
            elif .deviceKind == "keyboard" then "input-keyboard"
            else "input-gaming"
            end
          )
        }
    ]
    | unique_by(.name)
  ' <<<"$output")

  mkdir -p "${openlogi_battery_cache%/*}"
  temp=$(mktemp "${openlogi_battery_cache}.XXXXXX")
  printf '%s\n' "$batteries" >"$temp"
  mv "$temp" "$openlogi_battery_cache"
  printf '%s' "$batteries"
}

system_batteries() {
  local supply type capacity status name entries=()
  shopt -s nullglob
  for supply in /sys/class/power_supply/*; do
    type=$(cat "$supply/type" 2>/dev/null || true)
    [[ $type == Battery ]] || continue
    capacity=$(cat "$supply/capacity" 2>/dev/null || true)
    [[ $capacity =~ ^[0-9]+$ ]] || continue
    status=$(cat "$supply/status" 2>/dev/null || printf 'Unknown')
    name=$(cat "$supply/model_name" 2>/dev/null || true)
    [[ -n $name ]] || name=${supply##*/}
    entries+=("$(jq -nc --arg name "$name" --argjson percent "$capacity" --arg status "$status" \
      '{kind:"system",name:$name,percent:$percent,status:$status,icon:""}')")
  done
  shopt -u nullglob
  if ((${#entries[@]} > 0)); then
    printf '%s\n' "${entries[@]}" | jq -sc '.'
  else
    printf '[]'
  fi
}

airpods_ear_detection() {
  local value
  value=$(awk -F= '/^\[/ { section = $0 } section == "[earDetection]" && $1 == "setting" { print $2 }' \
    "$librepods_config" 2>/dev/null | tail -1)
  [[ ${value:-0} == 2 ]] && printf 'false' || printf 'true'
}

airpods_set_ear_detection() {
  local target=$1 value=2 current temp
  current=$(airpods_ear_detection)
  case "$target" in
    on) value=0 ;;
    off) value=2 ;;
    toggle) [[ $current == true ]] && value=2 || value=0 ;;
    *) return 2 ;;
  esac
  systemctl --user stop librepods.service >/dev/null 2>&1 || true
  mkdir -p "${librepods_config%/*}"
  temp=$(mktemp "${librepods_config}.XXXXXX")
  if [[ -f $librepods_config ]]; then
    awk '/^\[/ { skip = ($0 == "[earDetection]") } !skip { print }' "$librepods_config" >"$temp"
  fi
  printf '[earDetection]\nsetting=%s\n' "$value" >>"$temp"
  mv "$temp" "$librepods_config"
  systemctl --user start librepods.service >/dev/null 2>&1 || true
}

tailscale_state() {
  local raw
  if ! command -v tailscale >/dev/null 2>&1; then
    jq -nc '{available:false,backend:"Unavailable",connected:false,needsLogin:false,name:"",ip:"",tailnet:"",peers:0,onlinePeers:0}'
    return
  fi
  raw=$(timeout 2 tailscale status --json 2>/dev/null || printf '{}')
  jq -nc --argjson status "$raw" '
    ($status.BackendState // "Unavailable") as $backend
    | ($status.Peer // {} | to_entries | map(.value)) as $peers
    | {
        available:true,
        backend:$backend,
        connected:($backend == "Running"),
        needsLogin:($backend == "NeedsLogin"),
        name:($status.Self.HostName // ""),
        ip:($status.Self.TailscaleIPs[0] // ""),
        tailnet:($status.CurrentTailnet.Name // $status.MagicDNSSuffix // ""),
        peers:($peers | length),
        onlinePeers:([$peers[] | select(.Online)] | length)
      }
  '
}

proton_vpn_state() {
  local connection_line connection
  if ! command -v protonvpn >/dev/null 2>&1; then
    jq -nc '{available:false,connected:false,connection:""}'
    return
  fi
  connection_line=$(nmcli -t -f TYPE,NAME connection show --active 2>/dev/null |
    awk -F: 'tolower($0) ~ /proton[[:space:]_-]*vpn|protonvpn|pvpn/ && $1 ~ /^(vpn|wireguard|tun)$/ {print; exit}' || true)
  connection=${connection_line#*:}
  jq -nc \
    --argjson connected "$([[ -n $connection_line ]] && printf true || printf false)" \
    --arg connection "$connection" \
    '{available:true,connected:$connected,connection:$connection}'
}

ssh_server_state() {
  local load_state running=false
  load_state=$(systemctl show --property=LoadState --value sshd.service 2>/dev/null || true)
  if [[ $load_state != loaded ]]; then
    jq -nc '{available:false,running:false}'
    return
  fi
  systemctl is-active --quiet sshd.service 2>/dev/null && running=true
  jq -nc --argjson running "$running" '{available:true,running:$running}'
}

openlogi_camera_settings() {
  local device=$1 properties vendor product serial key config_dir config_file selected temp
  properties=$(udevadm info --query=property --name "$device" 2>/dev/null) || return 1
  vendor=$(awk -F= '$1 == "ID_VENDOR_ID" { print tolower($2); exit }' <<<"$properties")
  product=$(awk -F= '$1 == "ID_MODEL_ID" { print tolower($2); exit }' <<<"$properties")
  serial=$(awk -F= '$1 == "ID_SERIAL_SHORT" { print tolower($2); exit }' <<<"$properties")
  [[ $vendor == 046d && -n $product ]] || return 1

  key="camera:${vendor}:${product}"
  [[ -z $serial || $serial == 0 ]] || key+=":serial:${serial}"
  selected="selected_device = $(jq -Rn --arg value "$key" '$value')"
  config_dir=${XDG_CONFIG_HOME:-$HOME/.config}/openlogi
  config_file=$config_dir/config.toml
  mkdir -p "$config_dir"
  temp=$(mktemp "$config_file.XXXXXX")
  if [[ -r $config_file ]]; then
    awk -v selected="$selected" '
      !written && /^selected_device[[:space:]]*=/ { print selected; written = 1; next }
      !written && /^\[/ { print selected; print ""; written = 1 }
      { print }
      END { if (!written) print selected }
    ' "$config_file" >"$temp"
  else
    printf 'schema_version = 2\n%s\n' "$selected" >"$temp"
  fi
  mv "$temp" "$config_file"

  if pgrep -x openlogi-gui >/dev/null 2>&1; then
    pkill -TERM -x openlogi-gui
    for _ in {1..20}; do
      pgrep -x openlogi-gui >/dev/null 2>&1 || break
      sleep 0.05
    done
  fi
  pgrep -x openlogi-gui >/dev/null 2>&1 && return 1
  setsid -f openlogi-gui >/dev/null 2>&1
}

status() {
  [[ ${SEELE_CONTROL_NO_STATUS:-0} != 1 ]] || return 0

  audio=$(wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null || printf 'Volume: 0.00')
  volume=$(percent_for "$audio")
  muted=false
  [[ $audio == *MUTED* ]] && muted=true

  microphone=$(wpctl get-volume @DEFAULT_AUDIO_SOURCE@ 2>/dev/null || printf 'Volume: 0.00')
  microphone_volume=$(percent_for "$microphone")
  microphone_muted=false
  [[ $microphone == *MUTED* ]] && microphone_muted=true

  connection_line=$(nmcli -t -f TYPE,NAME connection show --active 2>/dev/null | awk -F: '$1 ~ /(wireless|ethernet|wifi)/ {print; exit}' || true)
  connection_type=${connection_line%%:*}
  connection=${connection_line#*:}
  [[ -n $connection_line ]] || {
    connection_type=""
    connection="Disconnected"
  }
  wifi_available=false
  nmcli -t -f TYPE device 2>/dev/null | grep -qx wifi && wifi_available=true
  wifi_enabled=false
  [[ $(nmcli -t -f WIFI general 2>/dev/null || true) == enabled ]] && wifi_enabled=true
  connectivity=$(nmcli networking connectivity 2>/dev/null || printf 'unknown')
  tailscale=$(tailscale_state)
  proton_vpn=$(proton_vpn_state)
  ssh_server=$(ssh_server_state)

  route=$(ip -json route get 1.1.1.1 2>/dev/null || printf '[]')
  ip_address=$(jq -r '.[0].prefsrc // ""' <<<"$route")
  gateway=$(jq -r '.[0].gateway // ""' <<<"$route")

  bluetooth=$(bluetooth_state)
  batteries=$(jq -sc "add | unique_by(.name)" <(upower_batteries) <(system_batteries) <(airpods_batteries) <(openlogi_batteries))
  ear_detection=$(airpods_ear_detection)
  tray_hidden=$(tray_hidden)
  bar_modules=$(bar_modules)

  voxtype_status=$(voxtype status 2>/dev/null | head -1 || printf 'unavailable')
  [[ -n $voxtype_status ]] || voxtype_status=unavailable

  camera_devices=$((v4l2-ctl --list-devices 2>/dev/null || true) |
    awk '/^[^[:space:]]/ {
      name=$0
      sub(/:$/, "", name)
      sub(/[[:space:]]*\(usb-[^)]*\)$/, "", name)
    }
    /^[[:space:]]*\/dev\/video/ {print name "\t" $1}' |
    jq -Rsc 'split("\n") | map(select(length > 0) | split("\t") | {name:.[0],device:.[1]})')
  camera_device=$(jq -r '.[0].device // ""' <<<"$camera_devices")
  pw_dump=$(pw-dump 2>/dev/null || printf '[]')
  audio_devices=$(audio_devices "$pw_dump")
  microphone_active=false
  # The receiver's own loopback captures the phone, not a microphone, so it
  # must not raise the recording indicator.
  if jq -e 'any(.[];
      .info.props["media.class"] == "Stream/Input/Audio"
      and .info.state == "running"
      and (.info.props["seele.role"] // "") != "bluetooth-receiver")' <<<"$pw_dump" >/dev/null 2>&1; then
    microphone_active=true
  fi
  screen_recording=$(screen_recording "$pw_dump")
  camera_active=false
  if jq -e 'any(.[].info?; .state == "running" and .props["media.class"] == "Video/Source")' <<<"$pw_dump" >/dev/null 2>&1; then
    camera_active=true
  fi

  dnd=false
  makoctl mode 2>/dev/null | grep -qx 'do-not-disturb' && dnd=true
  agents=$(agent_states)
  notifications=$(notification_state)

  jq -nc \
    --argjson volume "$volume" \
    --argjson muted "$muted" \
    --argjson microphoneVolume "$microphone_volume" \
    --argjson microphoneMuted "$microphone_muted" \
    --argjson microphoneActive "$microphone_active" \
    --arg connection "$connection" \
    --arg connectionType "$connection_type" \
    --arg connectivity "$connectivity" \
    --argjson wifiEnabled "$wifi_enabled" \
    --argjson wifiAvailable "$wifi_available" \
    --arg ipAddress "$ip_address" \
    --arg gateway "$gateway" \
    --argjson tailscale "$tailscale" \
    --argjson protonVpn "$proton_vpn" \
    --argjson sshServer "$ssh_server" \
    --argjson bluetooth "$bluetooth" \
    --argjson batteries "$batteries" \
    --argjson earDetection "$ear_detection" \
    --argjson trayHidden "$tray_hidden" \
    --argjson barModules "$bar_modules" \
    --arg voxtypeStatus "$voxtype_status" \
    --argjson cameraDevices "$camera_devices" \
    --arg cameraDevice "$camera_device" \
    --argjson cameraActive "$camera_active" \
    --argjson screenRecording "$screen_recording" \
    --argjson audioDevices "$audio_devices" \
    --argjson agentStates "$agents" \
    --argjson notifications "$notifications" \
    --argjson dnd "$dnd" \
    '{
      volume:$volume,
      muted:$muted,
      microphoneVolume:$microphoneVolume,
      microphoneMuted:$microphoneMuted,
      microphoneActive:$microphoneActive,
      connection:$connection,
      connectionType:$connectionType,
      connectivity:$connectivity,
      wifiEnabled:$wifiEnabled,
      wifiAvailable:$wifiAvailable,
      ipAddress:$ipAddress,
      gateway:$gateway,
      tailscale:$tailscale,
      protonVpn:$protonVpn,
      sshServer:$sshServer,
      bluetoothAvailable:$bluetooth.available,
      bluetoothPowered:$bluetooth.powered,
      bluetoothScanning:$bluetooth.scanning,
      bluetoothReceiver:$bluetooth.receiver,
      bluetoothDiscoverable:$bluetooth.discoverable,
      bluetoothConnected:$bluetooth.connected,
      bluetoothDevices:$bluetooth.devices,
      airpodsConnected:$bluetooth.airpodsConnected,
      airpodsName:$bluetooth.airpodsName,
      airpodsEarDetection:$earDetection,
      trayHidden:$trayHidden,
      barModules:$barModules,
      batteries:(
        $batteries
        + [$bluetooth.devices[] | select(.connected and .battery != null) | {kind:"device",name:.name,percent:.battery,status:"",icon:.icon}]
        | unique_by(.name)
      ),
      voxtypeStatus:$voxtypeStatus,
      cameraDevices:$cameraDevices,
      cameraDevice:$cameraDevice,
      cameraActive:$cameraActive,
      screenRecording:$screenRecording,
      audioDevices:$audioDevices,
      agentStates:$agentStates,
      notifications:$notifications,
      dnd:$dnd
    }'
}

case "${1:-status}" in
  status) status ;;
  speedtest) speedtest_result ;;
  launcher-toggle) launcher_toggle ;;
  bluetooth-status) bluetooth_state ;;
  bluetooth-pairing-answer)
    bluetooth_pairing_answer "${2:?request token required}" "${3:?accept or reject required}" "${4:-}"
    ;;
  volume)
    case "${2:-}" in
      up) wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+ ;;
      down) wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- ;;
      mute) wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle ;;
      ''|*[!0-9]*) exit 2 ;;
      *) wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ "${2}%" ;;
    esac
    status
    ;;
  audio-device)
    device=${2:?device id required}
    [[ $device =~ ^[0-9]+$ ]] || exit 2
    profile=${3:-}
    if [[ -n $profile ]]; then
      [[ $profile =~ ^[0-9]+$ ]] || exit 2
      # A profile entry names a card that is switched off, so there is no sink
      # node to make default yet. Switch the profile, then wait for the node
      # PipeWire creates for that device; without the second step the click
      # would only half-apply and leave the previous output selected.
      wpctl set-profile "$device" "$profile"
      for _ in {1..20}; do
        node=$(pw-dump 2>/dev/null | jq -r --argjson device "$device" \
          'first(.[] | select(.type == "PipeWire:Interface:Node" and .info.props["media.class"] == "Audio/Sink" and .info.props["device.id"] == $device) | .id) // empty')
        if [[ -n $node ]]; then
          wpctl set-default "$node"
          break
        fi
        sleep 0.1
      done
    else
      wpctl set-default "$device"
    fi
    status
    ;;
  microphone)
    case "${2:-}" in
      mute) wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle ;;
      up) wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SOURCE@ 5%+ ;;
      down) wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 5%- ;;
      ''|*[!0-9]*) exit 2 ;;
      *) wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SOURCE@ "${2}%" ;;
    esac
    status
    ;;
  voxtype)
    voxtype record toggle >/dev/null
    status
    ;;
  wifi)
    target=${2:-toggle}
    if [[ $target == toggle ]]; then
      [[ $(nmcli -t -f WIFI general 2>/dev/null || true) == enabled ]] && target=off || target=on
    fi
    nmcli radio wifi "$target"
    status
    ;;
  bluetooth)
    case "${2:-toggle}" in
      toggle)
        if [[ $(jq -r '.powered' <<<"$(bluetooth_state)") == true ]]; then
          bluetooth_scan_stop
          bluetooth_receiver_stop
          bluetooth_pairing_close
          bluetoothctl power off >/dev/null
        else
          bluetoothctl power on >/dev/null
        fi
        ;;
      scan)
        case "${3:-toggle}" in
          on) bluetooth_discovery_start ;;
          off) bluetooth_discovery_stop ;;
          toggle)
            if [[ $(jq -r '.scanning' <<<"$(bluetooth_state)") == true ]]; then
              bluetooth_discovery_stop
            else
              bluetooth_discovery_start
            fi
            ;;
          *) exit 2 ;;
        esac
        ;;
      receiver)
        case "${3:-toggle}" in
          on) bluetooth_receiver_start ;;
          off)
            bluetooth_receiver_stop
            bluetooth_pairing_close
            ;;
          toggle)
            if bluetooth_receiver_active; then
              bluetooth_receiver_stop
              bluetooth_pairing_close
            else
              bluetooth_receiver_start
            fi
            ;;
          *) exit 2 ;;
        esac
        ;;
      pairing)
        case "${3:-open}" in
          open) bluetooth_pairing_open ;;
          close) bluetooth_pairing_close ;;
          *) exit 2 ;;
        esac
        ;;
      connect)
        address=${3:?device address required}
        bluetoothctl power on >/dev/null 2>&1 || true
        if [[ $(bluetooth_paired "$address") == true ]]; then
          bluetooth_scan_stop
          timeout 20 bluetoothctl connect "$address" >/dev/null 2>&1 || true
        else
          bluetooth_pair_device "$address"
        fi
        ;;
      pair)
        address=${3:?device address required}
        bluetoothctl power on >/dev/null 2>&1 || true
        bluetooth_pair_device "$address"
        ;;
      forget)
        address=${3:?device address required}
        timeout 20 bluetoothctl remove "$address" >/dev/null 2>&1 || true
        ;;
      trust)
        address=${3:?device address required}
        case "${4:-toggle}" in
          on) timeout 10 bluetoothctl trust "$address" >/dev/null 2>&1 || true ;;
          off) timeout 10 bluetoothctl untrust "$address" >/dev/null 2>&1 || true ;;
          toggle)
            if [[ $(jq -r --arg address "$address" 'any(.devices[]; .address == $address and .trusted)' <<<"$(bluetooth_state)") == true ]]; then
              timeout 10 bluetoothctl untrust "$address" >/dev/null 2>&1 || true
            else
              timeout 10 bluetoothctl trust "$address" >/dev/null 2>&1 || true
            fi
            ;;
          *) exit 2 ;;
        esac
        ;;
      disconnect)
        address=${3:?device address required}
        timeout 20 bluetoothctl disconnect "$address" >/dev/null 2>&1 || true
        ;;
      *) exit 2 ;;
    esac
    status
    ;;
  airpods)
    case "${2:-}" in
      off|anc|transparency|adaptive) librepods-ctl "noise:${2}" ;;
      ear-detection) airpods_set_ear_detection "${3:-toggle}" ;;
      open)
        # Reuse the running instance so the tray icon is not duplicated.
        if ! timeout 5 librepods-ctl reopen >/dev/null 2>&1; then
          setsid -f librepods >/dev/null 2>&1
        fi
        ;;
      *) exit 2 ;;
    esac
    status
    ;;
  bar)
    case "${2:-}" in
      show|hide) bar_set_module "${3:?module id required}" "${2}" ;;
      *) printf 'Usage: seele-control bar <show|hide> <module>\n' >&2; exit 2 ;;
    esac
    status
    ;;
  tray)
    case "${2:-}" in
      hide|show|toggle) tray_set_hidden "${3:?tray item id required}" "${2}" ;;
      *) exit 2 ;;
    esac
    status
    ;;
  camera-settings)
    device=${2:-$(v4l2-ctl --list-devices 2>/dev/null | awk '/^[[:space:]]*\/dev\/video/ {print $1; exit}')}
    [[ -n $device ]]
    openlogi_camera_settings "$device"
    status
    ;;
  camera-preview)
    device=${2:-$(v4l2-ctl --list-devices 2>/dev/null | awk '/^[[:space:]]*\/dev\/video/ {print $1; exit}')}
    [[ -n $device ]]
    setsid -f cameraview -d "$device" >/dev/null 2>&1
    status
    ;;
  notifications)
    case "${2:-}" in
      restore) makoctl restore >/dev/null 2>&1 || true ;;
      dismiss)
        notification_id=${3:?notification id required}
        [[ $notification_id =~ ^[0-9]+$ ]] || exit 2
        makoctl dismiss -n "$notification_id" >/dev/null 2>&1 || true
        ;;
      invoke)
        notification_id=${3:?notification id required}
        [[ $notification_id =~ ^[0-9]+$ ]] || exit 2
        makoctl invoke -n "$notification_id" >/dev/null 2>&1 || true
        ;;
      clear)
        makoctl dismiss --all --no-history >/dev/null 2>&1 || true
        for _ in $(seq 1 50); do
          [[ $(makoctl history -j 2>/dev/null | jq 'length') == 0 ]] && break
          makoctl restore >/dev/null 2>&1 || break
          makoctl dismiss --no-history >/dev/null 2>&1 || break
        done
        ;;
      *) exit 2 ;;
    esac
    status
    ;;
  tailscale)
    case "${2:-toggle}" in
      up) tailscale up ;;
      down) tailscale down ;;
      login) setsid -f ghostty -e tailscale up >/dev/null 2>&1 ;;
      *) exit 2 ;;
    esac
    status
    ;;
  proton-vpn)
    case "${2:-toggle}" in
      connect) timeout 90 protonvpn connect ;;
      disconnect) timeout 30 protonvpn disconnect ;;
      open) setsid -f protonvpn-app >/dev/null 2>&1 ;;
      *) exit 2 ;;
    esac
    status
    ;;
  ssh-server)
    action=${2:-toggle}
    if [[ $action == toggle ]]; then
      systemctl is-active --quiet sshd.service 2>/dev/null && action=stop || action=start
    fi
    case "$action" in
      start|stop) systemctl "$action" sshd.service ;;
      *) exit 2 ;;
    esac
    status
    ;;
  copy-ip)
    ip -json route get 1.1.1.1 2>/dev/null | jq -r '.[0].prefsrc // empty' | wl-copy
    status
    ;;
  network-settings)
    setsid -f nm-connection-editor >/dev/null 2>&1
    status
    ;;
  dnd)
    makoctl mode -t do-not-disturb >/dev/null
    status
    ;;
  tray-menu)
    wanted_id=${2:?tray item id required}
    read -r cursor_x cursor_y < <(hyprctl cursorpos -j | jq -r '[.x, .y] | @tsv')
    while IFS= read -r address; do
      service=${address%%/*}
      object=/${address#*/}
      item_id=$(busctl --user --json=short get-property "$service" "$object" org.kde.StatusNotifierItem Id 2>/dev/null | jq -r '.data // empty')
      if [[ $item_id == "$wanted_id" ]]; then
        busctl --user call "$service" "$object" org.kde.StatusNotifierItem ContextMenu ii "$cursor_x" "$cursor_y" >/dev/null
        exit 0
      fi
    done < <(busctl --user --json=short get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems | jq -r '.data[]')
    exit 1
    ;;
  lock) "${SEELE_LOCK:-seele-lock}" ;;
  lock-suspend)
    "${SEELE_LOCK:-seele-lock}"
    systemctl suspend
    ;;
  logout) hyprctl dispatch exit ;;
  reboot) systemctl reboot ;;
  shutdown) systemctl poweroff ;;
  reboot-windows) systemctl --no-block start reboot-windows.service ;;
  *) exit 2 ;;
esac
