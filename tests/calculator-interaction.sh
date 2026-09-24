#!/usr/bin/env bash
set -euo pipefail
sources=${1:?shell source directory required}
fixture=${2:?interaction fixture required}
qt_import=${3:?Qt import path required}
native_import=${4:?Seele.Core import path required}
quickshell_import=${5:?Quickshell import path required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -m 700 "$work/home" "$work/runtime"
cp -r "$sources" "$work/production"
if [[ ! -d "$work/production/shared" ]]; then
  cp -r "$sources/../shared" "$work/production/shared"
fi
sed -i -e 's|import "../shared" as Shared|import "shared" as Shared|' \
  -e 's|../shared/Native.js|shared/Native.js|' \
  -e 's|../shared/ListModels.js|shared/ListModels.js|' "$work/production/CalculatorPanel.qml"
# Quickshell plugins belong to its executable. Preserve the real theme tokens
# and panel; omit only ShellRoot/config IO in this offscreen Qt host.
python3 - "$work/production/shared/Theme.qml" <<'PYTHON'
import re, sys
from pathlib import Path
path = Path(sys.argv[1])
source = path.read_text().split("  FileView {")[0]
source = re.sub(r"import Quickshell.*\n", "", source).replace("ShellRoot {", "Item {")
source = source.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ', '')
source = source.replace('Qt.resolvedUrl("grain.png")', '""')
path.write_text(source + "}\n")
PYTHON
cp "$fixture" "$work/tst_calculator.qml"
HOME="$work/home" XDG_CONFIG_HOME="$work/home" XDG_RUNTIME_DIR="$work/runtime" \
  DBUS_SESSION_BUS_ADDRESS="unix:path=$work/no-bus" \
  QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -import "$native_import" -import "$quickshell_import" \
  -input "$work/tst_calculator.qml"
