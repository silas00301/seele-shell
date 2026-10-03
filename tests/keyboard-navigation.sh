#!/usr/bin/env bash
set -euo pipefail
shared=${1:?shared QML directory}
fixture=${2:?keyboard test}
imports=${3:?navigation module import path}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/shared"
cp "$shared"/{ActionArea,FocusRing,KeyboardNavigation}.qml "$work/shared/"
cp "$fixture" "$work/tst_keyboardnavigation.qml"
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner -import "$imports" -input "$work/tst_keyboardnavigation.qml"
