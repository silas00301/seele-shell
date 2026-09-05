#!/usr/bin/env bash
set -euo pipefail
control=${1:?control executable required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
export MOCK_ACTIONS="$work/actions"
export SEELE_CONTROL_NO_STATUS=1
export SEELE_LOCK="$work/bin/lock"
cat >"$SEELE_LOCK" <<'SH'
#!/usr/bin/env bash
printf 'lock\n' >>"$MOCK_ACTIONS"
exit "${MOCK_LOCK_EXIT:-0}"
SH
cat >"$work/bin/systemctl" <<'SH'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >>"$MOCK_ACTIONS"
exit "${MOCK_SYSTEMCTL_EXIT:-0}"
SH
for mock in "$work/bin/"*; do
  sed -i "1c#!$BASH" "$mock"
done
chmod +x "$work/bin/"*
export PATH="$work/bin:$PATH"
for command in lock lock-suspend; do
  : >"$MOCK_ACTIONS"
  if MOCK_LOCK_EXIT=1 "$control" "$command"; then
    echo "failed lock reported success" >&2
    exit 1
  fi
  test "$(cat "$MOCK_ACTIONS")" = lock
done
: >"$MOCK_ACTIONS"
"$control" lock-suspend
test "$(cat "$MOCK_ACTIONS")" = $'lock\nsystemctl suspend'
if MOCK_SYSTEMCTL_EXIT=1 "$control" lock-suspend; then
  echo "failed suspend reported success" >&2
  exit 1
fi
