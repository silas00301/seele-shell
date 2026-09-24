#!/usr/bin/env bash
set -euo pipefail
sources=${1:?shell source directory required}
fixture=${2:?Qt interaction fixture required}
qt_import=${3:?Qt import directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/production/shared" "$work/home"
cp "$sources/MeetingPlanner.qml" "$work/production/"
shared="$sources/shared"
[[ -d "$shared" ]] || shared="$sources/../shared"
cp "$shared"/*.qml "$shared"/*.js "$work/production/shared/"
sed -i 's|import "../shared" as Shared|import "shared" as Shared|' "$work/production/MeetingPlanner.qml"
# Keep production palette, tokens, materials and panel. Only Quickshell's
# executable-owned ShellRoot and configuration IO are omitted in the Qt host.
python3 - "$work/production/shared/Theme.qml" <<'PY'
import re, sys
from pathlib import Path
path = Path(sys.argv[1])
source = path.read_text().split('  FileView {')[0]
source = re.sub(r'import Quickshell.*\n', '', source).replace('ShellRoot {', 'Item {')
source = source.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ', '')
source = source.replace('Qt.resolvedUrl("grain.png")', '""')
path.write_text(source + '}\n')
PY
cp "$fixture" "$work/tst_meetingplanner.qml"
HOME="$work/home" XDG_CONFIG_HOME="$work/home" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -input "$work/tst_meetingplanner.qml"
