#!/usr/bin/env bash
# Drive the production Markdown editor with real key presses on a private
# offscreen Qt platform. This is the coverage a JavaScript callback cannot give:
# focus, the undo stack, the caret across a refresh, and whether the highlighter
# actually draws anything.
set -euo pipefail
sources=${1:?notes source directory required}
test_file=${2:?editor test required}
qt_import=${3:?Qt QML import path required}
markdown_import=${4:?Seele.Markdown import path required}
native_import=${5:?Seele.Core import path required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cp "$sources"/*.qml "$sources"/*.js "$work/"
mkdir -p "$work/shared"
if [[ -d "$sources/shared" ]]; then
  cp "$sources/shared"/*.qml "$sources/shared"/*.js "$work/shared/"
else
  cp "$sources/../shared"/*.qml "$sources/../shared"/*.js "$work/shared/"
fi
# A source checkout reaches its siblings through ../shared; an installed
# package carries shared/ inside its own root. The fixture uses the second
# layout, so a checkout is rewritten into it.
sed -i 's|import "../shared" as Shared|import "shared" as Shared|' "$work"/*.qml
sed -i 's|../shared/Native.js|shared/Native.js|' "$work"/*.js
cp "$test_file" "$work/tst_noteseditor.qml"

QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  QML_IMPORT_PATH="$markdown_import:$native_import" QML2_IMPORT_PATH="$markdown_import:$native_import" \
  qmltestrunner \
    -import "$qt_import" \
    -import "$markdown_import" \
    -import "$native_import" \
    -input "$work/tst_noteseditor.qml"
