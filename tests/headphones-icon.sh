#!/usr/bin/env bash
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$1" "$work/HeadphonesIcon.qml"
cp "$2" "$work/tst_headphones.qml"
QT_QPA_PLATFORM=offscreen qmltestrunner -input "$work" -import "$3"
