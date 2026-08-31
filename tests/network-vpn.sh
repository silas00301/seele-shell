#!/usr/bin/env bash
set -euo pipefail

control=${1:?control script required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
export XDG_CONFIG_HOME=$work/config
export XDG_RUNTIME_DIR=$work/runtime
export XDG_STATE_HOME=$work/state

cat >"$work/bin/tailscale" <<'SH'
#!/usr/bin/env bash
if [[ ${1:-} == status && ${2:-} == --json ]]; then
  printf '%s\n' "$MOCK_TAILSCALE_JSON"
else
  printf 'tailscale %s\n' "$*" >>"$MOCK_ACTIONS"
fi
SH
cat >"$work/bin/protonvpn" <<'SH'
#!/usr/bin/env bash
printf 'protonvpn %s\n' "$*" >>"$MOCK_ACTIONS"
SH
cat >"$work/bin/protonvpn-app" <<'SH'
#!/usr/bin/env bash
printf 'protonvpn-app %s\n' "$*" >>"$MOCK_ACTIONS"
SH
cat >"$work/bin/systemctl" <<'SH'
#!/usr/bin/env bash
case "${1:-}" in
  show) printf 'loaded\n' ;;
  is-active) [[ $(cat "$MOCK_SSH_STATE") == active ]] ;;
  start) printf 'active\n' >"$MOCK_SSH_STATE"; printf 'systemctl start %s\n' "${2:-}" >>"$MOCK_ACTIONS" ;;
  stop) printf 'inactive\n' >"$MOCK_SSH_STATE"; printf 'systemctl stop %s\n' "${2:-}" >>"$MOCK_ACTIONS" ;;
  *) exit 2 ;;
esac
SH
cat >"$work/bin/nmcli" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "${MOCK_NMCLI:-}"
exit "${MOCK_NMCLI_EXIT:-0}"
SH
cat >"$work/bin/openlogi" <<'SH'
#!/usr/bin/env bash
cat <<'OUT'
Logi Bolt Receiver (fixture, vid=046d pid=c548)
  └─ slot 1 ● MX Master 3S (mouse, wpid=4082, battery=73% good (discharging))
OUT
SH
cat >"$work/bin/openlogi-gui" <<'SH'
#!/usr/bin/env bash
printf 'openlogi-gui\n' >>"$MOCK_ACTIONS"
SH
cat >"$work/bin/pgrep" <<'SH'
#!/usr/bin/env bash
exit 1
SH
cat >"$work/bin/pkill" <<'SH'
#!/usr/bin/env bash
printf 'pkill %s\n' "$*" >>"$MOCK_ACTIONS"
SH
cat >"$work/bin/setsid" <<'SH'
#!/usr/bin/env bash
[[ ${1:-} == -f ]] && shift
"$@"
SH
cat >"$work/bin/udevadm" <<'SH'
#!/usr/bin/env bash
cat <<'OUT'
ID_VENDOR_ID=046d
ID_MODEL_ID=0944
ID_SERIAL_SHORT=CAMERA-FIXTURE
OUT
SH
cat >"$work/bin/speedtest" <<'SH'
#!/usr/bin/env bash
printf 'speedtest %s\n' "$*" >>"$MOCK_ACTIONS"
printf '%s\n' \
  '{"type":"ping","ping":{"latency":12.34,"jitter":1.23}}' \
  '{"type":"download","download":{"bandwidth":57097500}}' \
  '{"type":"upload","upload":{"bandwidth":2931250}}' \
  '{"type":"result","ping":{"latency":12.34,"jitter":1.23},"download":{"bandwidth":57097500},"upload":{"bandwidth":2931250},"server":{"name":"Fixture Server"}}'
SH
cat >"$work/bin/vicinae" <<'SH'
#!/usr/bin/env bash
case "${1:-}" in
  state) [[ $(cat "$MOCK_VICINAE_STATE") == open ]] ;;
  open) printf '%s\n' open >"$MOCK_VICINAE_STATE" ;;
  close) printf '%s\n' closed >"$MOCK_VICINAE_STATE" ;;
  *) exit 2 ;;
esac
SH
cat >"$work/bin/v4l2-ctl" <<'SH'
#!/usr/bin/env bash
cat <<'OUT'
Fixture Camera (usb-0000:00:14.0-7):
        /dev/video0
OUT
SH
for mock in "$work/bin/"*; do
  sed -i "1c#!$BASH" "$mock"
done
chmod +x "$work/bin/"*

export PATH="$work/bin:$PATH"
export MOCK_ACTIONS="$work/actions"
export MOCK_VICINAE_STATE="$work/vicinae-state"
export MOCK_SSH_STATE="$work/ssh-state"
export MOCK_TAILSCALE_JSON='{"BackendState":"Running","Self":{"HostName":"fixture-host","TailscaleIPs":["100.64.0.1"]},"CurrentTailnet":{"Name":"fixture.ts.net"},"Peer":{"one":{"Online":true},"two":{"Online":false}}}'
export MOCK_NMCLI='wireguard:Proton VPN DE#1'

printf '%s\n' open >"$MOCK_VICINAE_STATE"
printf '%s\n' active >"$MOCK_SSH_STATE"
SEELE_CONTROL_NO_STATUS=1 "$control" launcher-toggle
grep -qx closed "$MOCK_VICINAE_STATE"
SEELE_CONTROL_NO_STATUS=1 "$control" launcher-toggle
grep -qx open "$MOCK_VICINAE_STATE"

state=$("$control" status | jq '.tailscale')
jq -e '
  .available and .connected and (.needsLogin | not)
  and .name == "fixture-host" and .ip == "100.64.0.1"
  and .tailnet == "fixture.ts.net" and .peers == 2 and .onlinePeers == 1
' <<<"$state" >/dev/null

state=$("$control" status | jq '.protonVpn')
jq -e '.available and .connected and .connection == "Proton VPN DE#1"' <<<"$state" >/dev/null

state=$("$control" status | jq '.sshServer')
jq -e '.available and .running' <<<"$state" >/dev/null

state=$("$control" status | jq '[.batteries[] | select(.kind == "logitech")]')
jq -e '. == [{kind:"logitech",name:"MX Master 3S",percent:73,status:"Discharging",icon:"input-mouse"}]' <<<"$state" >/dev/null

export MOCK_TAILSCALE_JSON='{"BackendState":"NeedsLogin"}'
state=$("$control" status | jq '.tailscale')
jq -e '.available and (.connected | not) and .needsLogin' <<<"$state" >/dev/null

export MOCK_NMCLI=''
state=$("$control" status | jq '.protonVpn')
jq -e '.available and (.connected | not) and .connection == ""' <<<"$state" >/dev/null

# An offline NetworkManager state is normal. It must not make `set -e` abort
# the shared status payload, because that also takes the audio panel down. An
# interrupted write may leave an empty notification cache; it is optional too.
export MOCK_NMCLI_EXIT=10
mkdir -p "$XDG_STATE_HOME/seele-shell"
: >"$XDG_STATE_HOME/seele-shell/notification-times.json"
state=$("$control" status)
jq -e '
  .connection == "Disconnected"
  and .audioDevices != null
  and .volume != null
  and .sshServer == {available:true,running:true}
  and any(.batteries[]; .kind == "logitech" and .name == "MX Master 3S" and .percent == 73)
  and .cameraDevices == [{name:"Fixture Camera",device:"/dev/video0"}]
' <<<"$state" >/dev/null

SEELE_CONTROL_NO_STATUS=1 "$control" tailscale down
SEELE_CONTROL_NO_STATUS=1 "$control" proton-vpn connect
SEELE_CONTROL_NO_STATUS=1 "$control" proton-vpn disconnect
SEELE_CONTROL_NO_STATUS=1 "$control" ssh-server stop
SEELE_CONTROL_NO_STATUS=1 "$control" ssh-server start
mkdir -p "$XDG_CONFIG_HOME/openlogi"
printf 'schema_version = 2\n\n[app_settings]\ncheck_for_updates = false\n' >"$XDG_CONFIG_HOME/openlogi/config.toml"
SEELE_CONTROL_NO_STATUS=1 "$control" camera-settings /dev/video0
grep -qx 'selected_device = "camera:046d:0944:serial:camera-fixture"' "$XDG_CONFIG_HOME/openlogi/config.toml"
grep -qx 'check_for_updates = false' "$XDG_CONFIG_HOME/openlogi/config.toml"
speedtest=$(SEELE_CONTROL_NO_STATUS=1 "$control" speedtest 2>/dev/null)
jq -s -e '
  any(.[]; .phase == "ping")
  and any(.[]; .phase == "download" and .download == 456.78)
  and any(.[]; .phase == "upload" and .upload == 23.45)
' <<<"$speedtest" >/dev/null
jq -e '
  .ping == 12.34 and .download == 456.78 and .upload == 23.45
  and .jitter == 1.23 and .server == "Fixture Server"
' <<<"$(tail -n 1 <<<"$speedtest")" >/dev/null
grep -qx 'tailscale down' "$MOCK_ACTIONS"
grep -qx 'protonvpn connect' "$MOCK_ACTIONS"
grep -qx 'protonvpn disconnect' "$MOCK_ACTIONS"
grep -qx 'systemctl stop sshd.service' "$MOCK_ACTIONS"
grep -qx 'systemctl start sshd.service' "$MOCK_ACTIONS"
grep -qx 'openlogi-gui' "$MOCK_ACTIONS"
if grep -q '^pkill ' "$MOCK_ACTIONS"; then exit 1; fi
grep -qx 'speedtest --accept-license --accept-gdpr --format=jsonl --progress=yes --progress-update-interval=250' "$MOCK_ACTIONS"
