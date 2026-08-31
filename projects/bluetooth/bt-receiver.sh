#!/usr/bin/env bash
# Bluetooth receiver bridge.
#
# PipeWire publishes a phone that streams to this machine as a plain Bluetooth
# source node and links it to nothing, so the audio arrives and stops there.
# While receiver mode is on this supervisor keeps one loopback alive per
# connected Bluetooth source, playing it into whatever the current default
# output is. Killing the supervisor tears every bridge down with it.
set -euo pipefail

interval=${SEELE_BLUETOOTH_RECEIVER_INTERVAL:-2}

declare -A bridges=()

teardown() {
  local node
  for node in "${!bridges[@]}"; do
    kill "${bridges[$node]}" 2>/dev/null || true
  done
}
trap teardown EXIT INT TERM

bluetooth_sources() {
  pw-dump 2>/dev/null | jq -r '
    .[]
    | select(.type == "PipeWire:Interface:Node")
    | .info.props
    | select(.["media.class"] == "Audio/Source")
    | select((.["device.api"] // "") == "bluez5" or ((.["node.name"] // "") | startswith("bluez_")))
    | .["node.name"]
  ' 2>/dev/null || true
}

bridge_start() {
  local node=$1
  # `seele.role` marks the capture stream so the shell's recording indicator
  # does not read the phone as a microphone in use.
  pw-loopback --capture "$node" \
    --capture-props="node.name=seele-bluetooth-receiver.$node seele.role=bluetooth-receiver" \
    --playback-props="node.name=seele-bluetooth-receiver-out.$node node.description=Bluetooth Receiver seele.role=bluetooth-receiver" \
    >/dev/null 2>&1 &
  bridges[$node]=$!
}

while :; do
  mapfile -t current < <(bluetooth_sources)
  for node in "${!bridges[@]}"; do
    if kill -0 "${bridges[$node]}" 2>/dev/null && printf '%s\n' "${current[@]}" | grep -qxF -- "$node"; then
      continue
    fi
    kill "${bridges[$node]}" 2>/dev/null || true
    unset 'bridges[$node]'
  done
  for node in "${current[@]}"; do
    [[ -n $node ]] || continue
    [[ -n ${bridges[$node]:-} ]] || bridge_start "$node"
  done
  sleep "$interval"
done
