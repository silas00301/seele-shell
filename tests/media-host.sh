#!/usr/bin/env bash
set -euo pipefail
quickshell=${1:?quickshell executable required}
sources=${2:?shell source directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -m 700 "$work/runtime"
cp "$sources/media.js" "$work/media.js"
source "$(dirname "${BASH_SOURCE[0]}")/qml-fixture.sh"
copy_qml_shared "$sources" "$work"
cp "$(dirname "${BASH_SOURCE[0]}")/media-host.qml" "$work/shell.qml"
if ! XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  timeout 10 "$quickshell" --no-color -p "$work" > "$work/log" 2>&1; then
  tail -40 "$work/log" >&2
  exit 1
fi
if ! grep -q 'MEDIA_HOST_PASS' "$work/log" || grep -Eq 'MEDIA_HOST_FAIL|ReferenceError|TypeError' "$work/log"; then
  tail -40 "$work/log" >&2
  exit 1
fi
printf '%s\n' 'Native media actual enum singleton, repeat labels, QObject writes and capability guards passed'
