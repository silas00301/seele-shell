#!/usr/bin/env bash
set -euo pipefail

control=${1:?control script required}
receiver=${2:?bt-receiver script required}
agent=${3:?bt-agent script required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
export XDG_CONFIG_HOME=$work/config
export XDG_RUNTIME_DIR=$work/runtime
export XDG_STATE_HOME=$work/state
sed '/^case "${1:-status}" in/,$d' "$control" >"$work/functions.sh"

cat >"$work/bin/busctl" <<'SH'
#!/usr/bin/env bash
discoverable=$(cat "$MOCK_BT_DISCOVERABLE")
cat <<JSON
{"data":[{
  "/org/bluez/hci0": {
    "org.bluez.Adapter1": { "Powered": {"data":true}, "Discoverable": {"data":$discoverable} }
  },
  "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01": {
    "org.bluez.Device1": {
      "Address": {"data":"AA:BB:CC:DD:EE:01"},
      "Alias": {"data":"Fixture Phone"},
      "Icon": {"data":"phone"},
      "Paired": {"data":true},
      "Trusted": {"data":true},
      "Connected": {"data":true},
      "UUIDs": {"data":["0000110A-0000-1000-8000-00805F9B34FB","0000111F-0000-1000-8000-00805F9B34FB"]}
    }
  },
  "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01/fd3": {
    "org.bluez.MediaTransport1": {
      "Device": {"data":"/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01"},
      "State": {"data":"active"}
    }
  },
  "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_02": {
    "org.bluez.Device1": {
      "Address": {"data":"AA:BB:CC:DD:EE:02"},
      "Alias": {"data":"Fixture Headset"},
      "Icon": {"data":"audio-headset"},
      "Paired": {"data":true},
      "Trusted": {"data":false},
      "Connected": {"data":true},
      "UUIDs": {"data":["0000110B-0000-1000-8000-00805F9B34FB"]}
    }
  }
}]}
JSON
SH
cat >"$work/bin/bluetoothctl" <<'SH'
#!/usr/bin/env bash
printf 'bluetoothctl %s\n' "$*" >>"$MOCK_ACTIONS"
case "$*" in
  "discoverable on") printf 'true\n' >"$MOCK_BT_DISCOVERABLE" ;;
  "discoverable off") printf 'false\n' >"$MOCK_BT_DISCOVERABLE" ;;
esac
SH
cat >"$work/bin/seele-bt-agent" <<'SH'
#!/usr/bin/env bash
printf 'seele-bt-agent window=%s restore=%s\n' \
  "${SEELE_BLUETOOTH_PAIRING_WINDOW:-}" "${SEELE_BLUETOOTH_DISCOVERABLE_TIMEOUT:-}" >>"$MOCK_ACTIONS"
sleep 60
SH
cat >"$work/bin/seele-shellctl" <<'SH'
#!/usr/bin/env bash
printf 'seele-shellctl %s\n' "$*" >>"$MOCK_ACTIONS"
SH
cat >"$work/bin/seele-bt-receiver" <<'SH'
#!/usr/bin/env bash
printf 'seele-bt-receiver\n' >>"$MOCK_ACTIONS"
sleep 60
SH
cat >"$work/bin/pw-dump" <<'SH'
#!/usr/bin/env bash
cat <<'JSON'
[
  { "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source", "device.api": "bluez5",
                         "node.name": "bluez_input.AA_BB_CC_DD_EE_01" } } },
  { "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Source", "device.api": "alsa",
                         "node.name": "alsa_input.pci-0000_00_1f.3.analog-stereo" } } },
  { "type": "PipeWire:Interface:Node",
    "info": { "props": { "media.class": "Audio/Sink", "device.api": "bluez5",
                         "node.name": "bluez_output.AA_BB_CC_DD_EE_02" } } }
]
JSON
SH
cat >"$work/bin/pw-loopback" <<'SH'
#!/usr/bin/env bash
printf 'pw-loopback %s\n' "$*" >>"$MOCK_LOOPBACKS"
sleep 60
SH
for mock in "$work/bin/"*; do
  sed -i "1c#!$BASH" "$mock"
done
chmod +x "$work/bin/"*

export PATH="$work/bin:$PATH"
export MOCK_ACTIONS="$work/actions"
export MOCK_LOOPBACKS="$work/loopbacks"
export MOCK_BT_DISCOVERABLE="$work/discoverable"
: >"$MOCK_ACTIONS"
: >"$MOCK_LOOPBACKS"
printf 'false\n' >"$MOCK_BT_DISCOVERABLE"

# A phone advertises the A2DP Audio Source role and a headset does not, which is
# what the receiver's fold-out section filters on. The active media transport is
# what marks the phone as currently playing.
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '
  .available and .powered and (.receiver | not) and (.discoverable | not)
  and .connected == 2
  and any(.devices[]; .name == "Fixture Phone" and .source and .streaming)
  and any(.devices[]; .name == "Fixture Headset" and (.source | not) and (.streaming | not))
' <<<"$state" >/dev/null

# Ordering must not depend on connection state: that changes on its own and
# would pull a row out from under the pointer aimed at it.
jq -e '[.devices[] | .name] == ["Fixture Headset", "Fixture Phone"]' <<<"$state" >/dev/null

# The agent's capability is the whole security property: anything that cannot
# display a code downgrades Secure Simple Pairing to just-works, and a missing
# handler would let BlueZ fall through to a model nobody confirms.
# KeyboardDisplay is the only capability that can satisfy every association
# model Secure Simple Pairing may pick, and a model the agent refuses is a
# pairing that simply fails, so every handler has to be present.
grep -qx 'CAPABILITY = "KeyboardDisplay"' "$agent"
for handler in RequestConfirmation RequestAuthorization RequestPasskey RequestPinCode DisplayPasskey; do
  grep -q "def $handler" "$agent"
done
if grep -A 3 'def RequestPasskey' "$agent" | grep -q 'raise Rejected'; then exit 1; fi

# Receiver mode carries audio; it must not open the adapter to strangers on its
# own. Only the explicit pairing window does that.
SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth receiver on
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '.receiver and (.discoverable | not)' <<<"$state" >/dev/null
grep -qx 'seele-bt-receiver' "$MOCK_ACTIONS"
if grep -q 'bluetoothctl discoverable on' "$MOCK_ACTIONS"; then exit 1; fi
if grep -q 'bluetoothctl pairable on' "$MOCK_ACTIONS"; then exit 1; fi

# Discovery is one symmetric window: the same action that searches also makes
# the machine answerable and registers the agent, so a phone can reach it.
SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth scan on
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '.discoverable' <<<"$state" >/dev/null
# The scan itself is detached, so it records itself out of band. Both halves
# carry the same window, or the spinner would stop while the adapter is open.
for _ in $(seq 1 40); do
  grep -qx 'bluetoothctl --timeout 120 scan on' "$MOCK_ACTIONS" && break
  sleep 0.1
done
grep -qx 'bluetoothctl --timeout 120 scan on' "$MOCK_ACTIONS"
grep -qx 'seele-bt-agent window=120 restore=180' "$MOCK_ACTIONS"

SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth scan off
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '(.scanning | not) and (.discoverable | not)' <<<"$state" >/dev/null

# The pairing window registers a DisplayYesNo agent, so Secure Simple Pairing
# picks numeric comparison instead of accepting the phone silently, and BlueZ
# retires discoverability on its own when the window expires.
SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth pairing open
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '.receiver and .discoverable' <<<"$state" >/dev/null
grep -qx 'bluetoothctl discoverable-timeout 120' "$MOCK_ACTIONS"
grep -qx 'bluetoothctl pairable on' "$MOCK_ACTIONS"
grep -qx 'bluetoothctl discoverable on' "$MOCK_ACTIONS"
# The agent closes the window itself when it expires, so it has to know both
# the window it is holding and the timeout to put back.
grep -qx 'seele-bt-agent window=120 restore=180' "$MOCK_ACTIONS"
test -s "$XDG_RUNTIME_DIR/seele-shell/bluetooth-agent.pid"

# The shell answers the prompt by token, and the agent only acts on its own.
SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth-pairing-answer deadbeef accept
grep -qx 'deadbeef accept ' "$XDG_RUNTIME_DIR/seele-shell/bluetooth-pairing.answer"
# A typed code rides back with the verdict for the passkey and PIN models.
SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth-pairing-answer deadbeef accept 481625
grep -qx 'deadbeef accept 481625' "$XDG_RUNTIME_DIR/seele-shell/bluetooth-pairing.answer"
if SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth-pairing-answer deadbeef maybe 2>/dev/null; then exit 1; fi

SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth pairing close
test ! -e "$XDG_RUNTIME_DIR/seele-shell/bluetooth-agent.pid"
test ! -e "$XDG_RUNTIME_DIR/seele-shell/bluetooth-pairing.answer"
grep -qx 'seele-shellctl -q bluetooth-pairing-dismiss' "$MOCK_ACTIONS"
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '.receiver and (.discoverable | not)' <<<"$state" >/dev/null
grep -qx 'bluetoothctl discoverable off' "$MOCK_ACTIONS"
grep -qx 'bluetoothctl pairable off' "$MOCK_ACTIONS"
grep -qx 'bluetoothctl discoverable-timeout 180' "$MOCK_ACTIONS"

SEELE_CONTROL_NO_STATUS=1 bash "$control" bluetooth receiver off
state=$(bash -c 'source "$1"; bluetooth_state' _ "$work/functions.sh")
jq -e '(.receiver | not) and (.discoverable | not)' <<<"$state" >/dev/null
test ! -e "$XDG_RUNTIME_DIR/seele-shell/bluetooth-receiver.pid"

# The bridge carries Bluetooth sources only, and tags its capture stream so the
# recording indicator does not read a phone as a microphone.
setsid bash -c 'echo $$ >"$1"; shift; exec "$@"' seele-bt-receiver-test "$work/bridge.pid" \
  bash "$receiver" >/dev/null 2>&1 &
for _ in $(seq 1 40); do
  [[ -s $MOCK_LOOPBACKS ]] && break
  sleep 0.1
done
bridge=$(<"$work/bridge.pid")
kill -TERM -- "-$bridge" 2>/dev/null || true
wait 2>/dev/null || true
test "$(wc -l <"$MOCK_LOOPBACKS")" -eq 1
grep -q -- '--capture bluez_input.AA_BB_CC_DD_EE_01' "$MOCK_LOOPBACKS"
grep -q 'seele.role=bluetooth-receiver' "$MOCK_LOOPBACKS"
if grep -q 'alsa_input' "$MOCK_LOOPBACKS"; then exit 1; fi
if pgrep -f 'bin/pw-loopback' >/dev/null 2>&1; then exit 1; fi
