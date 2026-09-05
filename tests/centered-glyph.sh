#!/usr/bin/env bash
set -euo pipefail

component=${1:?usage: centered-glyph.sh COMPONENT QT_IMPORT TEST}
qt_import=${2:?usage: centered-glyph.sh COMPONENT QT_IMPORT TEST}
test_file=${3:?usage: centered-glyph.sh COMPONENT QT_IMPORT TEST}
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT

cp "$component" "$test_dir/CenteredGlyph.qml"
cp "$test_file" "$test_dir/tst_centeredglyph.qml"

QT_QPA_PLATFORM=offscreen qmltestrunner \
  -import "$qt_import" \
  -input "$test_dir/tst_centeredglyph.qml"
