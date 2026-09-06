#!/usr/bin/env bash
set -euo pipefail
state=${1:?SystemState.qml required}
qml_import=${2:?Qt QML import directory required}
test_file=${3:?test QML required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$state" "$work/SystemState.qml"
cp "$test_file" "$work/tst_systemstate.qml"
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner \
  -import "$qml_import" -input "$work/tst_systemstate.qml"
