#!/usr/bin/env bash
set -euo pipefail
shell=$1
shared=$2
qt_import=$3
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
node "$(dirname "$0")/uri-overlay.js" "$shell" "$shared" "$work"
cp "$(dirname "$0")/tst_uri_overlay.qml" "$work/"
mkdir "$work/home" "$work/config"
HOME="$work/home" XDG_CONFIG_HOME="$work/config" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -input "$work/tst_uri_overlay.qml"
