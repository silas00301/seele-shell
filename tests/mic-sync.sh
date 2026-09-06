#!/usr/bin/env bash
# Drives mic-sync against a microphone, an ALSA card, and a PipeWire graph made
# of files, so both directions of the sync and the tie-break between them are
# checked without hardware. Each side records every write rather than its
# current value, because the states this has to get right are the ones where a
# second change lands before the first has been read back.
set -euo pipefail

sync=${1:?mic-sync script required}
shellctl=${2:?shellctl binary required}
work=$(mktemp -d)
daemon=""
cleanup() {
  [[ -n $daemon ]] && kill "$daemon" 2>/dev/null
  rm -rf "$work"
}
trap cleanup EXIT

mkdir -p "$work/bin" "$work/sysfs/usb1/usb1:1.1/sound/card9"
export PATH=$work/bin:$PATH
# The build sandbox has no /usr/bin/env, so every stub names the running shell
# rather than looking an interpreter up.
stub() {
  {
    printf '#!%s\n' "$BASH"
    cat
  } >"$work/bin/$1"
  chmod +x "$work/bin/$1"
}
printf '14ed' >"$work/sysfs/usb1/idVendor"
printf '1019' >"$work/sysfs/usb1/idProduct"
printf '9' >"$work/sysfs/usb1/usb1:1.1/sound/card9/number"

export MOCK_SWITCH=$work/switch
export MOCK_NODE=$work/node-mute
export MOCK_OSD=$work/osd.log
export MOCK_PARTIAL=$work/partial
export SEELE_MIC_SYNC_SYSFS=$work/sysfs
# Every acknowledgement this run sends goes to a shell that takes three seconds
# to answer, so a sync that waited on one could not keep up with the panel.
export MOCK_OSD_DELAY=3

stub mock-write <<'SH'
# Replace a state file whole and log the write, so a reader polling the pair
# cannot miss a value that was immediately replaced.
printf '%s' "$2" >"$1.next"
mv "$1.next" "$1"
printf '.' >>"$1.writes"
SH

# The microphone starts muted by its own panel while the desktop believes it is
# live, which is the disagreement this exists to settle.
mock-write "$MOCK_SWITCH" off
mock-write "$MOCK_NODE" false
: >"$MOCK_OSD"

stub amixer <<'SH'
mode=
for argument in "$@"; do
  case "$argument" in
    controls | cget | cset) mode=$argument ;;
  esac
done
case "$mode" in
  controls)
    echo "numid=3,iface=MIXER,name='Microphone Capture Switch'"
    echo "numid=4,iface=MIXER,name='Microphone Capture Volume'"
    ;;
  cget)
    echo "numid=3,iface=MIXER,name='Microphone Capture Switch'"
    echo "  ; type=BOOLEAN,access=rw------,values=1"
    echo "  : values=$(cat "$MOCK_SWITCH")"
    ;;
  cset) mock-write "$MOCK_SWITCH" "${*: -1}" ;;
esac
SH

# The card announces its own changes over the device's status endpoint, so the
# stub reports that something moved rather than waiting to be asked.
stub alsactl <<'SH'
seen=
while true; do
  writes=$(cat "$MOCK_SWITCH.writes" 2>/dev/null || true)
  if [[ $writes != "$seen" ]]; then
    seen=$writes
    echo "node hw:9, #3 (2,0,0,Microphone Capture Switch,0) VALUE"
  fi
  sleep 0.05
done
SH

stub pw-dump <<'SH'
emit() {
  if [[ -f $MOCK_PARTIAL ]]; then
    printf '[{"id":63,"info":{"params":{"Props":[{"mute":%s}]}}}]\n' "$(cat "$MOCK_NODE")"
    return
  fi
  cat <<JSON
[ { "id": 63, "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source", "alsa.card": 9 },
              "params": { "Props": [ { "volume": 1.0, "mute": $(cat "$MOCK_NODE") } ] } } } ]
JSON
}
seen=
emit
while true; do
  writes=$(cat "$MOCK_NODE.writes" 2>/dev/null || true)
  if [[ $writes != "$seen" ]]; then
    seen=$writes
    emit
  fi
  sleep 0.05
done
SH

stub wpctl <<'SH'
case "${1:-}" in
  set-mute)
    [[ ${3:-} == 1 ]] && mock-write "$MOCK_NODE" true || mock-write "$MOCK_NODE" false
    ;;
  get-volume)
    [[ $(cat "$MOCK_NODE") == true ]] && echo "Volume: 1.00 [MUTED]" || echo "Volume: 1.00"
    ;;
esac
SH

stub seele-shellctl <<'SH'
printf '%s\n' "$*" >>"$MOCK_OSD"
# Stand in for a shell that is slow to answer. The sync must not wait on it.
sleep "${MOCK_OSD_DELAY:-0}"
SH

expect() {
  local file=$1 want=$2 label=$3 seen
  for _ in $(seq 1 120); do
    seen=$(cat "$file" 2>/dev/null || true)
    [[ $seen == "$want" ]] && return 0
    sleep 0.05
  done
  echo "mic-sync: $label -- expected '$want', found '$seen'" >&2
  [[ -f $work/log ]] && cat "$work/log" >&2
  exit 1
}

"$sync" 14ED:1019 >"$work/log" 2>&1 &
daemon=$!

# The switch is what gates the signal, so it wins the disagreement it started in.
expect "$MOCK_NODE" true "startup adopts the device's mute"

# A tap on the panel unmutes the microphone in the device; the desktop follows.
mock-write "$MOCK_SWITCH" on
expect "$MOCK_NODE" false "the desktop follows the panel unmuting"
expect "$MOCK_OSD" "-q microphone-state live" "the panel raises an OSD carrying the new state"

started=$(date +%s%N)
mock-write "$MOCK_SWITCH" off
expect "$MOCK_NODE" true "the desktop follows the panel muting"
elapsed=$((($(date +%s%N) - started) / 1000000))
if ((elapsed > 1500)); then
  echo "mic-sync: the sync waited on the previous OSD (${elapsed}ms)" >&2
  exit 1
fi

# Unmuting in the desktop reaches the device, which is the direction that was
# impossible before: nothing in the graph owned the mute gating the signal.
mock-write "$MOCK_NODE" false
expect "$MOCK_SWITCH" on "the device follows the desktop unmuting"

mock-write "$MOCK_NODE" true
expect "$MOCK_SWITCH" off "the device follows the desktop muting"

# One OSD per tap, each carrying the state that tap produced, and none for the
# two changes the desktop made itself. An echo answered as though it came from
# the other side would show up here as an extra.
sleep 0.6
if [[ $(cat "$MOCK_OSD") != "-q microphone-state live
-q microphone-state muted" ]]; then
  echo "mic-sync: expected one OSD per panel tap, each naming the new state" >&2
  cat "$MOCK_OSD" "$work/log" >&2
  exit 1
fi

# pw-dump monitor updates need not repeat unchanged identity properties.
# Rapid desktop toggles must still reach the physical gate in that format.
touch "$MOCK_PARTIAL"
for _ in $(seq 1 8); do
  mock-write "$MOCK_NODE" false
  expect "$MOCK_SWITCH" on "partial update unmutes hardware"
  mock-write "$MOCK_NODE" true
  expect "$MOCK_SWITCH" off "partial update mutes hardware"
done

# Repeated panel taps without an extra settling delay must converge in both
# directions, even while earlier OSD acknowledgements are still outstanding.
for _ in $(seq 1 8); do
  mock-write "$MOCK_SWITCH" on
  expect "$MOCK_NODE" false "rapid panel unmute"
  mock-write "$MOCK_SWITCH" off
  expect "$MOCK_NODE" true "rapid panel mute"
done

kill "$daemon" 2>/dev/null
wait "$daemon" || true
daemon=""
# A failed second spawn must not leave the first monitor behind. Record the
# parent PID from a successful command before replacing pw-dump's interpreter.
export MOCK_MONITOR_PID="$work/monitor-pid"
stub alsactl <<'SH'
echo "$$" >"$MOCK_MONITOR_PID"
exec sleep 60
SH
printf '#!/nonexistent-seele-audit-interpreter\n' >"$work/bin/pw-dump"
if "$sync" 14ED:1019 >"$work/startup-failure.log" 2>&1; then
  echo "mic-sync: failed monitor startup reported success" >&2
  exit 1
fi
if [[ -s $MOCK_MONITOR_PID ]] && kill -0 "$(cat "$MOCK_MONITOR_PID")" 2>/dev/null; then
  echo "mic-sync: ALSA monitor leaked after PipeWire failed to start" >&2
  exit 1
fi
echo "mic-sync ok"

# Keyboard mute must not collect full status before reporting the new mute.
export MOCK_IPC=$work/ipc.log
export SEELE_SHELL_PATH=$work/shell
stub seele-control <<'SH'
[[ $* == 'microphone mute' && ${SEELE_CONTROL_NO_STATUS:-} == 1 ]] || exit 1
SH
stub quickshell <<'SH'
printf '%s\n' "$*" >>"$MOCK_IPC"
SH
for state in true false; do
  mock-write "$MOCK_NODE" "$state"
  : >"$MOCK_IPC"
  "$shellctl" microphone mute
  [[ $state == true ]] && expected=muted || expected=live
  [[ $(cat "$MOCK_IPC") == "ipc -n -p $SEELE_SHELL_PATH call -- seele-shell showMicrophone $expected
ipc -n -p $SEELE_SHELL_PATH call -- seele-shell refreshStatus" ]]
done
stub seele-control <<'SH'
exit 1
SH
: >"$MOCK_IPC"
if "$shellctl" microphone mute 2>/dev/null; then
  echo 'mic-sync: failed keyboard mute reported success' >&2
  exit 1
fi
[[ ! -s $MOCK_IPC ]]
echo "microphone keyboard feedback ok"
