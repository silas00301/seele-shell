#!/usr/bin/env bash
set -euo pipefail
shared=${1:?shared component directory required}
fixture=${2:?plain-label fixture required}
imports=${3:?Qt QML import directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/projects/shared" "$work/tests"
cp "$shared"/*.qml "$shared"/*.js "$work/projects/shared/"
cp "$fixture" "$work/tests/tst_plainlabels.qml"
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$imports" -input "$work/tests/tst_plainlabels.qml"
