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
for name in test_a test_b test_c; do
  created=0
  for _ in $(seq 1 100); do
    if pactl load-module module-null-sink "sink_name=$name" > /dev/null 2>"$work/create.log"; then created=1; break; fi
    sleep 0.05
  done
  if [[ $created != 1 ]]; then cat "$work/create.log" "$work/pipewire.log" "$work/pulse.log"; exit 1; fi
done
pactl set-default-sink test_a
pacat --playback --raw --device=test_a --stream-name=seele-default-test /dev/zero >"$work/play.log" 2>&1 &
pacat --playback --raw --device=test_c --stream-name=seele-explicit-test /dev/zero >"$work/explicit.log" 2>&1 &
for _ in $(seq 1 100); do
  if [[ $(pactl --format=json list sink-inputs | jq 'length') -ge 2 ]]; then break; fi
  sleep 0.05
done
"$control" audio-outputs '["test_a","test_b"]'
test "$(pactl get-default-sink)" = seele_outputs_a
# The module really creates two target streams and carries the selected nodes
# into the PipeWire graph consumed by the shell.
pactl --format=json list sinks | jq -e 'any(.[]; .name == "seele_outputs_a" and .properties["seele.outputs"] == "test_a,test_b")' >/dev/null
for _ in $(seq 1 100); do
  if [[ $(pactl --format=json list sink-inputs | jq 'length') -ge 4 ]]; then break; fi
  sleep 0.05
done
sinks=$(pactl --format=json list sinks)
streams=$(pactl --format=json list sink-inputs)
combined=$(jq -r '.[] | select(.name == "seele_outputs_a") | .index' <<<"$sinks")
separate=$(jq -r '.[] | select(.name == "test_c") | .index' <<<"$sinks")
jq -e --argjson sink "$combined" 'any(.[]; .properties["media.name"] == "seele-default-test" and .sink == $sink)' <<<"$streams" >/dev/null
jq -e --argjson sink "$separate" 'any(.[]; .properties["media.name"] == "seele-explicit-test" and .sink == $sink)' <<<"$streams" >/dev/null
if "$control" audio-outputs '["test_a","disconnected"]'; then exit 1; fi
test "$(pactl get-default-sink)" = seele_outputs_a
# Exercise rollback against the real server while injecting one failed call.
export SEELE_TEST_REAL_PACTL="$(command -v pactl)"
mkdir "$work/fail-bin"
cat > "$work/fail-bin/pactl" <<'SH'
#!/usr/bin/env bash
if [[ ${SEELE_TEST_FAIL:-} == load && $1 == load-module ]]; then exit 1; fi
if [[ ${SEELE_TEST_FAIL:-} == move && $1 == move-sink-input && ${3:-} == seele_outputs_b ]]; then exit 1; fi
exec "$SEELE_TEST_REAL_PACTL" "$@"
SH
sed -i "1s|.*|#!$BASH|" "$work/fail-bin/pactl"
chmod +x "$work/fail-bin/pactl"
for failure in load move; do
  if PATH="$work/fail-bin:$PATH" SEELE_TEST_FAIL="$failure" "$control" audio-outputs '["test_b","test_c"]'; then exit 1; fi
  test "$(pactl get-default-sink)" = seele_outputs_a
  for _ in $(seq 1 100); do
    if pactl --format=json list sinks | jq -e 'all(.[]; .name != "seele_outputs_b")' >/dev/null; then break; fi
    sleep 0.05
  done
  pactl --format=json list modules | jq -e '[.[] | select(.name == "module-combine-sink")] | length == 1' >/dev/null
done
"$control" audio-outputs '["test_b","test_c"]'
test "$(pactl get-default-sink)" = seele_outputs_b
pactl --format=json list sinks | jq -e 'all(.[]; .name != "seele_outputs_a")' >/dev/null
"$control" audio-outputs '["test_b"]'
test "$(pactl get-default-sink)" = test_b
pactl --format=json list modules | jq -e 'all(.[]; .name != "module-combine-sink")' >/dev/null
if "$control" audio-outputs '[]'; then exit 1; fi
test "$(pactl get-default-sink)" = test_b
printf 'Private PipeWire simultaneous-output routing and cleanup passed\n'
