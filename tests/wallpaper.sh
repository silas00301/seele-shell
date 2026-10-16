#!/usr/bin/env bash
# The background's Wallpaper component, run from the shared directory given
# (the source tree's or the one a package installs), with the fixture images
# beside the test exactly where tst_wallpaper.qml looks for them.
set -euo pipefail
shared=${1:?shared directory required}
qt_import=${2:?Qt QML import directory required}
tests=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/projects/shared" "$work/tests/wallpaper" "$work/tests/theme-folders/pictured"
cp "$shared"/*.qml "$shared"/*.js "$work/projects/shared/"
cp "$tests/tst_wallpaper.qml" "$work/tests/"
cp "$tests/wallpaper/configured.png" "$work/tests/wallpaper/"
cp "$tests/theme-folders/pictured/wallpaper.png" "$work/tests/theme-folders/pictured/"
# Fresh copies for each asynchronous load, and a name a bare file:// URL would
# cut at its `#` and `?`.
for n in $(seq 1 16); do
  mkdir -p "$work/tests/wallpaper/fresh/$n"
  cp "$tests/wallpaper/configured.png" "$tests/theme-folders/pictured/wallpaper.png" "$work/tests/wallpaper/fresh/$n/"
done
cp "$tests/theme-folders/pictured/wallpaper.png" "$work/tests/wallpaper/dusk #1 ?%é.png"
HOME="$work" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -input "$work/tests/tst_wallpaper.qml"
