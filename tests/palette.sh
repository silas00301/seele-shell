#!/usr/bin/env bash
set -euo pipefail
shared=${1:?shared source required}
fixture=${2:?palette Qt fixture required}
qt_import=${3:?Qt QML import directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/projects/shared" "$work/tests" "$work/home" "$work/config"
cp "$shared"/*.qml "$shared"/*.js "$work/projects/shared/"
cp "$fixture" "$work/tests/tst_palette.qml"
# Quickshell's executable-only plugins cannot load in qmltestrunner. Retain
# production palette/material bindings, replacing only config IO and ShellRoot.
node - "$work/projects/shared/Theme.qml" <<'JS'
const fs=require('node:fs'), file=process.argv[2];
const source=fs.readFileSync(file,'utf8').split('  FileView {')[0]
  .replace(/import Quickshell.*\n/g,'').replace('ShellRoot {','Item {')
  .replace(/Quickshell.env\("SEELE_SHELL_WALLPAPER"\) \|\| /,'');
fs.writeFileSync(file,source+'}\n');
JS
HOME="$work/home" XDG_CONFIG_HOME="$work/config" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -input "$work/tests/tst_palette.qml"
