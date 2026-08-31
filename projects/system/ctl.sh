#!/usr/bin/env bash
set -euo pipefail

quiet=false
if [[ ${1:-} == -q ]]; then quiet=true; shift; fi

usage() {
  cat <<'EOF'
Usage: seele-shellctl [-q] <command> [arguments]

Commands:
  menu [apps|commands]      Toggle the launcher
  agents                    Toggle the AI dashboard
  center                    Toggle the Control Center
  controls                  Toggle session controls
  control <panel>           Toggle control-center, audio, network, vpn, Bluetooth, AirPods, battery, notifications, camera, or session
  bluetooth-pairing <json>  Show a Bluetooth pairing request
  bluetooth-pairing-dismiss Withdraw the Bluetooth pairing request
  agent <name> [prompt...]  Launch pi, opencode, codex, or claude
  refresh-agents            Refresh AI usage data
  volume <up|down|mute>     Change volume and show its OSD
  microphone <up|down|mute> Change the microphone and show its OSD
  microphone-state <muted|live>
                            Show the OSD for a mute the device made itself
  voxtype                   Toggle voice dictation
  lock                      Lock the session
  ping                      Check shell IPC
EOF
}

ipc() {
  local output status
  set +e
  output=$(timeout 3s quickshell ipc -n -p "$SEELE_SHELL_PATH" call -- seele-shell "$@" 2>/dev/null)
  status=$?
  set -e
  if ((status != 0)); then
    $quiet && return 0
    echo "Seele Shell is not responding" >&2
    return "$status"
  fi
  $quiet || [[ -z $output ]] || printf '%s\n' "$output"
}

command=${1:---help}
shift || true
case "$command" in
  menu) ipc toggleLauncher "${1:-apps}" ;;
  agents) ipc toggleAgents ;;
  center) ipc toggleControl control-center ;;
  controls) ipc toggleControls ;;
  control) ipc toggleControl "${1:-system}" ;;
  bluetooth-pairing) ipc bluetoothPairingRequest "${1:?request payload required}" ;;
  bluetooth-pairing-dismiss) ipc bluetoothPairingDismiss ;;
  agent)
    agent=${1:-pi}
    shift || true
    ipc launchAgent "$agent" "$*"
    ;;
  refresh-agents) ipc refreshAgents ;;
  volume)
    output=$(seele-control volume "${1:?up, down, or mute required}")
    ipc updateStatus "$output"
    ipc showVolume
    ;;
  microphone)
    output=$(seele-control microphone "${1:?up, down, or mute required}")
    ipc updateStatus "$output"
    ipc showMicrophone ""
    ;;
  microphone-state)
    # The microphone changed its own mute, so the shell needs that one value.
    # Re-reading the whole system to find it costs several hundred milliseconds,
    # and every one of them would sit between the tap and its acknowledgement.
    ipc showMicrophone "${1:?muted or live required}"
    ;;
  voxtype)
    output=$(seele-control voxtype)
    ipc updateStatus "$output"
    ;;
  lock) exec seele-control lock ;;
  ping) ipc ping ;;
  -h|--help|help) usage ;;
  *) usage >&2; exit 2 ;;
esac
