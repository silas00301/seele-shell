#!/usr/bin/env bash
set -euo pipefail
panel=${1:?GitHubInboxPanel.qml required}
shared=${2:?shared source directory required}
fixture=${3:?Qt fixture required}
qt_import=${4:?Qt QML import directory required}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$panel" "$work/GitHubInboxPanel.qml"
cp -r "$shared" "$work/shared"
sed -i 's|import "../shared" as Shared|import "shared" as Shared|' "$work/GitHubInboxPanel.qml"
# Render the real tokens without loading a user theme or a desktop ShellRoot.
python3 - "$work" <<'PY'
from pathlib import Path
import sys
root=Path(sys.argv[1]);source=(root/'shared/Theme.qml').read_text()
source=source[:source.index('  FileView {')]+'}\n'
source=source.replace('import Quickshell\n','').replace('import Quickshell.Io\n','').replace('ShellRoot {','QtObject {')
source=source.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ','')
(root/'shared/TestTheme.qml').write_text(source)
PY
cp "$fixture" "$work/tst_githubinbox.qml"
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  qmltestrunner -import "$qt_import" -input "$work/tst_githubinbox.qml"
