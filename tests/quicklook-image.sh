#!/usr/bin/env bash
set -euo pipefail
sources=$1 fixture=$2 qt_import=$3
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -m700 "$work/production" "$work/runtime"
cp "$sources/QuickLookImage.qml" "$work/production/"
cp -r "$sources/shared" "$work/production/"
sed -i 's|import "../shared" as Shared|import "shared" as Shared|' "$work/production/QuickLookImage.qml"
python3 - "$work/production/shared/Theme.qml" <<'PY'
import re,sys
from pathlib import Path
p=Path(sys.argv[1]);s=p.read_text().split('  FileView {')[0]
s=re.sub(r'import Quickshell.*\n','',s).replace('ShellRoot {','Item {').replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ','').replace('Qt.resolvedUrl("grain.png")','""');p.write_text(s+'}\n')
PY
python3 - "$work" <<'PYPNG'
import struct,zlib,sys
from pathlib import Path
def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
for name,w,h in [('large',2000,1000),('small',20,10)]:
    raw=(b'\x00'+b'\x00\x00\xff'*w)*h
    Path(sys.argv[1],name+'.png').write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b''))
PYPNG
cp "$fixture" "$work/tst_quicklookimage.qml"
XDG_RUNTIME_DIR="$work/runtime" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner -import "$qt_import" -input "$work/tst_quicklookimage.qml"
