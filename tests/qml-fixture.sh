#!/usr/bin/env bash
# Stage shared QML/JS dependencies in the installed layout. Both repository
# sources and installed packages then exercise the same imports in fixtures.
copy_qml_shared() {
  local sources=${1:?QML source directory required}
  local destination=${2:?fixture destination required}
  local shared="$sources/shared"
  [[ -d "$shared" ]] || shared="$sources/../shared"
  mkdir -p "$destination/shared"
  cp "$shared"/*.qml "$shared"/*.js "$destination/shared/"
  local file
  for file in "$destination"/*.qml "$destination"/*.js; do
    [[ -f "$file" ]] || continue
    sed -i -e 's|import "../shared"|import "shared"|g' \
      -e 's|"../shared/|"shared/|g' "$file"
  done
}
