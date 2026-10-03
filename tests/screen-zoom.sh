#!/usr/bin/env bash
# Drives `seele-shellctl zoom` against a Hyprland that keeps cursor:zoom_factor
# in a file and answers getoption and eval the way the real control socket
# does, so the steps, the clamp, the exact return to 1x, the OSD messages and
# the serialization of overlapping scroll notches are checked without a
# compositor.
set -euo pipefail

shellctl=${1:?shellctl binary required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/bin" "$work/runtime"
chmod 700 "$work/runtime"
export PATH=$work/bin:$PATH
export XDG_RUNTIME_DIR=$work/runtime
export SEELE_SHELL_PATH=$work/shell
export MOCK_FACTOR=$work/factor
export MOCK_EVALS=$work/evals
export MOCK_IPC=$work/ipc
# The build sandbox has no /usr/bin/env, so every stub names the running shell
# rather than looking an interpreter up.
stub() {
  {
    printf '#!%s\n' "$BASH"
    cat
  } >"$work/bin/$1"
  chmod +x "$work/bin/$1"
}

stub hyprctl <<'SH'
set -euo pipefail
[[ ! -e ${MOCK_FAIL:-/nonexistent} ]] || exit 1
case "$1" in
  getoption)
    [[ $* == 'getoption cursor:zoom_factor -j' ]] || exit 2
    # Hold the read open long enough that two unserialized steps would both
    # see the same level.
    sleep 0.05
    printf '{"option": "cursor:zoom_factor", "float": %.6f, "set": true }' "$(cat "$MOCK_FACTOR")"
    ;;
  eval)
    [[ $# == 2 ]] || exit 2
    printf '%s\n' "$2" >>"$MOCK_EVALS"
    pattern='^hl\.config\(\{ cursor = \{ zoom_factor = ([0-9]+(\.[0-9]+)?) \} \}\)$'
    if [[ $2 =~ $pattern ]]; then
      printf '%s' "${BASH_REMATCH[1]}" >"$MOCK_FACTOR"
      printf 'ok'
    else
      printf 'error: unexpected Lua'
      exit 7
    fi
    ;;
  *) exit 2 ;;
esac
SH

stub quickshell <<'SH'
printf '%s\n' "$*" >>"$MOCK_IPC"
SH

fail() {
  echo "screen-zoom: $*" >&2
  exit 1
}
factor() { cat "$MOCK_FACTOR"; }
last_osd() { tail -n 1 "$MOCK_IPC"; }
reset_logs() { : >"$MOCK_EVALS"; : >"$MOCK_IPC"; }

printf '1.000000' >"$MOCK_FACTOR"
reset_logs

for want in 1.414214 2 2.828427 4 5.656854 8 8; do
  "$shellctl" zoom in
  [[ $(factor) == "$want" ]] || fail "key step reached $(factor), expected $want"
done
# The limit holds without writing, and still answers with the level it holds.
[[ $(wc -l <"$MOCK_EVALS") == 6 ]] || fail "zooming in at the limit wrote the option"
[[ $(last_osd) == "ipc -n -p $SEELE_SHELL_PATH call -- seele-shell showZoom "'{"factor":8.0,"label":"8×","ratio":1.0,"zoomed":true}' ]] ||
  fail "unexpected OSD message: $(last_osd)"

"$shellctl" zoom out
[[ $(factor) == 5.656854 ]] || fail "key step out reached $(factor)"
"$shellctl" zoom out --fine
[[ $(factor) == 4.756828 ]] || fail "scroll step out reached $(factor)"

"$shellctl" zoom reset
[[ $(factor) == 1 ]] || fail "reset reached $(factor)"
[[ $(tail -n 1 "$MOCK_EVALS") == 'hl.config({ cursor = { zoom_factor = 1 } })' ]] ||
  fail "reset did not write exactly 1"
[[ $(last_osd) == *'"zoomed":false'* ]] || fail "reset did not withdraw the OSD"

# Scroll notches that land together each move one step: twelve overlapping
# fine steps reach 8x rather than collapsing onto the same read.
reset_logs
pids=()
for _ in $(seq 1 12); do
  "$shellctl" zoom in --fine &
  pids+=($!)
done
for pid in "${pids[@]}"; do wait "$pid"; done
[[ $(factor) == 8 ]] || fail "overlapping scroll steps reached $(factor), expected 8"
[[ $(wc -l <"$MOCK_EVALS") == 12 ]] || fail "overlapping scroll steps wrote $(wc -l <"$MOCK_EVALS") times"

# Stepping back down by scroll lands on exactly 1 and stops there.
for _ in $(seq 1 13); do "$shellctl" zoom out --fine; done
[[ $(factor) == 1 ]] || fail "scrolling out reached $(factor)"
[[ $(tail -n 1 "$MOCK_EVALS") == 'hl.config({ cursor = { zoom_factor = 1 } })' ]] ||
  fail "scrolling out did not finish on exactly 1"

# An unreachable compositor fails the command and reports nothing.
reset_logs
touch "$work/unreachable"
if MOCK_FAIL=$work/unreachable "$shellctl" zoom in 2>/dev/null; then
  fail "a failed read reported success"
fi
[[ ! -s $MOCK_IPC && ! -s $MOCK_EVALS ]] || fail "a failed read still acted"

for bad in "" "sideways" "in --coarse" "reset --fine"; do
  # shellcheck disable=SC2086
  if "$shellctl" zoom $bad 2>/dev/null; then fail "accepted 'zoom $bad'"; fi
done
[[ ! -s $MOCK_IPC && ! -s $MOCK_EVALS ]] || fail "a rejected request still acted"

echo "screen zoom ok"
