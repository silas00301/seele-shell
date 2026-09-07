#!/usr/bin/env bash
set -euo pipefail
control=${1:?control executable required}
if [[ ${SEELE_AUDIO_TEST_BUS:-0} != 1 ]]; then
  bus_config=$(mktemp)
  trap 'rm -f "$bus_config"' EXIT
  cat > "$bus_config" <<'CONF'
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>
CONF
  env SEELE_AUDIO_TEST_BUS=1 dbus-run-session --config-file="$bus_config" -- bash "$0" "$control"
  exit
fi
work=$(mktemp -d)
cleanup() {
  "$control" bluetooth receiver off >/dev/null 2>&1 || true
  jobs -pr | xargs -r kill 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
export XDG_RUNTIME_DIR="$work/run" XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state" XDG_CACHE_HOME="$work/cache"
export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR" PIPEWIRE_REMOTE=pipewire-0
export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
export DBUS_SYSTEM_BUS_ADDRESS="unix:path=$work/no-system-bus"
export SEELE_CONTROL_NO_STATUS=1
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d"
chmod 700 "$XDG_RUNTIME_DIR"
cat > "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d/90-test.conf" <<'CONF'
wireplumber.profiles = {
  main = {
    hardware.audio = disabled
    hardware.bluetooth = disabled
    hardware.video-capture = disabled
  }
}
CONF
pipewire >"$work/pipewire.log" 2>&1 &
pipewire-pulse >"$work/pulse.log" 2>&1 &
wireplumber >"$work/wireplumber.log" 2>&1 &
ready=0
for _ in $(seq 1 100); do
  if pactl info >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.05
done
if [[ $ready != 1 ]]; then cat "$work/"*.log; exit 1; fi

mkdir -p "$work/bin"
ln -s "$control" "$work/bin/seele-bt-receiver"
printf '#!%s\nexit 0\n' "$BASH" > "$work/bin/bluetoothctl"
chmod +x "$work/bin/bluetoothctl"
export PATH="$work/bin:$PATH"
# Pulse can answer before WirePlumber has registered the node factories.
created=0
for _ in $(seq 1 100); do
  if pactl load-module module-null-sink sink_name=receiver_output > /dev/null 2>"$work/create.log"; then created=1; break; fi
  sleep 0.05
done
if [[ $created != 1 ]]; then cat "$work/create.log"; exit 1; fi
pactl set-default-sink receiver_output
pw-cli create-node adapter '{ factory.name = support.null-audio-sink node.name = bluez_input.fixture media.class = Audio/Source object.linger = true audio.position = [ FL FR ] }'
receiver_links() {
  pw-dump | jq '[.[] | select(.type == "PipeWire:Interface:Node") | select(.info.props["seele.role"] == "bluetooth-receiver")] | length'
}
test "$(receiver_links)" = 0
"$control" bluetooth receiver on
for _ in $(seq 1 100); do
  test "$(receiver_links)" = 2 && break
  sleep 0.05
done
test "$(receiver_links)" = 2
for _ in $(seq 1 100); do
  if pw-dump | jq -e 'any(.[]; .type == "PipeWire:Interface:Link" and .info.state == "active")' >/dev/null; then break; fi
  sleep 0.05
done
pw-dump | jq -e 'any(.[]; .type == "PipeWire:Interface:Link" and .info.state == "active")' >/dev/null
"$control" bluetooth receiver off
for _ in $(seq 1 100); do
  test "$(receiver_links)" = 0 && break
  sleep 0.05
done
test "$(receiver_links)" = 0
pw-dump | jq -e 'all(.[]; .type != "PipeWire:Interface:Link")' >/dev/null
echo 'PASS: receiver on links the phone source to playback; off removes every link'
