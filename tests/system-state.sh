#!/usr/bin/env bash
set -euo pipefail
state=${1:?SystemState.qml required}
qml_import=${2:?Qt QML import directory required}
test_file=${3:?test QML required}
native_import=${4:?Seele.Core import directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/shell/shared"
cp "$state" "$work/shell/SystemState.qml"
state_dir=$(dirname "$state")
if [[ -f "$state_dir/shared/Native.js" ]]; then
  cp "$state_dir/shared/Native.js" "$work/shell/shared/Native.js"
else
  cp "$state_dir/../shared/Native.js" "$work/shell/shared/Native.js"
fi
sed -i 's|../shared/Native.js|shared/Native.js|' "$work/shell/SystemState.qml"
cp "$test_file" "$work/shell/tst_systemstate.qml"
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  QML_IMPORT_PATH="$native_import" QML2_IMPORT_PATH="$native_import" \
  qmltestrunner -import "$qml_import" -import "$native_import" -input "$work/shell/tst_systemstate.qml"
