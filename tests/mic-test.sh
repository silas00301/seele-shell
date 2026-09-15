#!/usr/bin/env bash
# The microphone test against a private PipeWire instance and synthetic audio.
# Nothing here reaches the user's real microphone, real outputs or real default
# sink: the server, its devices and its signal are all created by this script.
set -euo pipefail
worker=${1:?microphone test executable required}
if [[ ${SEELE_MIC_TEST_BUS:-0} != 1 ]]; then
  bus_config=$(mktemp)
  trap 'rm -f "$bus_config"' EXIT
  cat > "$bus_config" <<'CONF'
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>
CONF
  env SEELE_MIC_TEST_BUS=1 dbus-run-session --config-file="$bus_config" -- bash "$0" "$worker"
  exit
fi
work=$(mktemp -d)
cleanup() {
  exec 3>&- 2>/dev/null || true
  jobs -pr | xargs -r kill 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
export XDG_RUNTIME_DIR="$work/run" XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state" XDG_CACHE_HOME="$work/cache"
export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR" PIPEWIRE_REMOTE=pipewire-0
export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
export DBUS_SYSTEM_BUS_ADDRESS="unix:path=$work/no-system-bus"
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

load() {
  for _ in $(seq 1 100); do
    if pactl load-module "$@" >"$work/module.id" 2>"$work/module.log"; then cat "$work/module.id"; return 0; fi
    sleep 0.05
  done
  cat "$work/module.log" "$work/pipewire.log" "$work/pulse.log"
  return 1
}
for name in test_out test_alt; do
  load module-null-sink "sink_name=$name" >/dev/null
done
# A virtual source stands in for the microphone: it is a real capture device to
# everything downstream, and it is fed only by this script.
source_module=$(load module-null-sink sink_name=test_mic media.class=Audio/Source/Virtual)
pactl set-default-sink test_out
default_sink=$(pactl get-default-sink)

# Synthetic signal: a quiet tone that a meter must respond to, and a full-scale
# square wave that must be reported as clipping however the bar is drawn.
python3 - "$work" <<'PY'
import math, struct, sys
work = sys.argv[1]
rate = 48000
with open(f"{work}/quiet.raw", "wb") as handle:
    handle.write(b"".join(struct.pack("<h", int(3000 * math.sin(i * 2 * math.pi * 440 / rate)))
                          for i in range(rate * 30)))
with open(f"{work}/loud.raw", "wb") as handle:
    handle.write(b"".join(struct.pack("<h", 32767 if (i // 8) % 2 == 0 else -32768)
                          for i in range(rate * 30)))
PY
feeder=
feed() {
  if [[ -n $feeder ]]; then kill "$feeder" 2>/dev/null || true; fi
  pacat --playback --raw --format=s16le --rate=48000 --channels=1 --device=test_mic \
    --stream-name=seele-signal "$work/$1.raw" >>"$work/feed.log" 2>&1 &
  feeder=$!
  sleep 0.3
}

mkfifo "$work/in"
"$worker" <"$work/in" >"$work/out.log" 2>"$work/err.log" &
exec 3>"$work/in"
send() { printf '%s\n' "$1" >&3; }
fail() { printf '%s\n' "$1" >&2; cat "$work/out.log" "$work/err.log" >&2; exit 1; }
mode() { grep -o '"mode":"[a-z]*"' "$work/out.log" | tail -1; }
await() {
  for _ in $(seq 1 "${2:-400}"); do
    if [[ $(mode) == "\"mode\":\"$1\"" ]]; then return 0; fi
    sleep 0.05
  done
  fail "the worker never reached $1"
}
frames() { grep -c '"level":' "$work/out.log" || true; }
streams() { pactl --format=json list sink-inputs | jq -r '[.[] | select(.properties["application.name"] == "Seele microphone test")] | length'; }
captures() { pactl --format=json list source-outputs | jq -r '[.[] | select(.properties["application.name"] == "Seele microphone test")] | length'; }

await idle 100
send '{"command":"probe"}'
for _ in $(seq 1 100); do grep -q '"detection":' "$work/out.log" && break; sleep 0.05; done
grep -q '"detection":true' "$work/out.log" || fail "microphone use was not established"

# A quiet signal moves the meter without being reported as clipping.
feed quiet
send '{"command":"sample","input":"test_mic","output":"test_out"}'
await recording
started=$(date +%s%N)
await playing 400
elapsed=$(( ( $(date +%s%N) - started ) / 1000000 ))
if (( elapsed < 4200 || elapsed > 7000 )); then fail "the five-second sample took ${elapsed}ms"; fi
[[ $(frames) -gt 20 ]] || fail "the meter produced no frames"
grep -q '"clipped":true' "$work/out.log" && fail "a quiet signal was reported as clipping"
grep -qE '"level":(0\.[1-9]|[1-9])' "$work/out.log" || fail "the meter did not respond to the microphone"
# The sample plays through the chosen output as this test's own stream.
for _ in $(seq 1 100); do [[ $(streams) -ge 1 ]] && break; sleep 0.05; done
pactl --format=json list sink-inputs \
  | jq -e 'any(.[]; .properties["application.name"] == "Seele microphone test")' >/dev/null \
  || fail "the sample never reached an output"
await idle 600
test "$(pactl get-default-sink)" = "$default_sink"

# Replay reaches a different test output without moving the system default or
# anything else already playing.
: > "$work/out.log"
send '{"command":"replay","input":"test_mic","output":"test_alt"}'
await playing 200
alt=$(pactl --format=json list sinks | jq -r '.[] | select(.name == "test_alt") | .index')
for _ in $(seq 1 200); do
  if pactl --format=json list sink-inputs | jq -e --argjson sink "$alt" \
    'any(.[]; .properties["application.name"] == "Seele microphone test" and .sink == $sink)' >/dev/null; then break; fi
  sleep 0.05
done
pactl --format=json list sink-inputs | jq -e --argjson sink "$alt" \
  'any(.[]; .properties["application.name"] == "Seele microphone test" and .sink == $sink)' >/dev/null \
  || fail "the replay did not follow the chosen test output"
test "$(pactl get-default-sink)" = "$default_sink"
await idle 600

# Clipping is reported for a deliberately clipped signal.
: > "$work/out.log"
feed loud
send '{"command":"live","input":"test_mic","output":"test_out"}'
await live
for _ in $(seq 1 200); do grep -q '"clipped":true' "$work/out.log" && break; sleep 0.05; done
grep -q '"clipped":true' "$work/out.log" || fail "a full-scale signal was not reported as clipping"

# Live mode has no duration limit and accumulates nothing.
sleep 7
[[ $(mode) == '"mode":"live"' ]] || fail "live mode stopped by itself"
[[ $(captures) -ge 1 ]] || fail "live mode is not capturing"
[[ $(streams) -ge 1 ]] || fail "live mode is not playing"
send '{"command":"stop"}'
await idle 400
for _ in $(seq 1 200); do [[ $(captures) == 0 && $(streams) == 0 ]] && break; sleep 0.05; done
[[ $(captures) == 0 ]] || fail "a capture stream outlived the test"
[[ $(streams) == 0 ]] || fail "a playback stream outlived the test"

# A microphone that disappears under a running test ends it rather than
# continuing on something else.
: > "$work/out.log"
send '{"command":"live","input":"test_mic","output":"test_out"}'
await live
pactl unload-module "$source_module"
await idle 600
for _ in $(seq 1 200); do [[ $(captures) == 0 && $(streams) == 0 ]] && break; sleep 0.05; done
[[ $(captures) == 0 && $(streams) == 0 ]] || fail "device loss left audio running"
# An unavailable device is refused by name before anything starts.
: > "$work/out.log"
send '{"command":"sample","input":"test_mic","output":"test_out"}'
await idle 200
grep -q '"error":"The selected microphone is no longer available"' "$work/out.log" \
  || fail "a missing microphone was not reported"
: > "$work/out.log"
send '{"command":"live","input":"test_out.monitor","output":"test_out"}'
await idle 200
grep -q 'monitor' "$work/out.log" || fail "an output monitor was accepted as a microphone"

# Closing the panel is the worker's stdin closing: every stream ends with it,
# and the sample it held was never anywhere but memory.
load module-null-sink sink_name=test_mic media.class=Audio/Source/Virtual >/dev/null
feed loud
: > "$work/out.log"
send '{"command":"live","input":"test_mic","output":"test_out"}'
await live
exec 3>&-
for _ in $(seq 1 200); do
  if ! jobs -pr | grep -q .; then break; fi
  if [[ $(captures) == 0 && $(streams) == 0 ]]; then break; fi
  sleep 0.05
done
[[ $(captures) == 0 && $(streams) == 0 ]] || fail "closing the session left audio running"
test "$(pactl get-default-sink)" = "$default_sink"
# Nothing was written outside this script's own scratch files.
find "$XDG_RUNTIME_DIR" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" -name '*mic*test*' -print -quit 2>/dev/null \
  | grep -q . && fail "the microphone test left a file behind"

printf 'Private PipeWire microphone test capture, clipping, routing, device loss and cleanup passed\n'
