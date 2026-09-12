#!/usr/bin/env bash
# Validate generated ELF wrappers without installing or invoking Nix. Pass one
# or more audited nixpkgs make-binary-wrapper.sh files (for example at both locks).
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat >"$work/probe.c" <<'C'
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char **argv) {
 for(int i=1;i<argc;++i) puts(argv[i]);
 const char *keys[]={"SEELE_CONFIG","SEELE_SHELL_CODEX","QML2_IMPORT_PATH","QT_PLUGIN_PATH","PATH"};
 for(unsigned i=0;i<sizeof(keys)/sizeof(keys[0]);++i) puts(getenv(keys[i])?getenv(keys[i]):"MISSING");
 return 0;
}
C
cc -Wall -Werror "$work/probe.c" -o "$work/probe"
for source in "$@"; do
  (
    source "$source"
    # Nix's stdenv supplies the helper's optional locals; outside stdenv retain
    # the same empty-value behavior while exercising the unmodified generator.
    set +u
    makeCWrapper "$work/probe" \
      --add-flags '-n -p /nix/store/fixture/share/seele-shell' \
      --set SEELE_CONFIG /nix/store/fixture/share/seele-shell \
      --set-default SEELE_SHELL_CODEX /fixture/codex \
      --prefix QML2_IMPORT_PATH : /fixture/qt/qml \
      --prefix QT_PLUGIN_PATH : /fixture/qt/plugins \
      --prefix PATH : /fixture/bin >"$work/wrapper.c"
  )
  cc -Wall -Werror -Wpedantic -Wno-overlength-strings -Os "$work/wrapper.c" -o "$work/wrapper"
  python3 - "$work/wrapper" <<'PY'
import subprocess,sys
binary=sys.argv[1]
for override in [False,True]:
 env={'PATH':'/retained/bin','QML2_IMPORT_PATH':'/retained/qml','QT_PLUGIN_PATH':'/retained/plugins'}
 if override:env['SEELE_SHELL_CODEX']='/configured/codex'
 result=subprocess.run([binary,'literal argument','--last'],env=env,capture_output=True,timeout=5,check=True)
 assert result.stdout.decode().splitlines()==['-n','-p','/nix/store/fixture/share/seele-shell','literal argument','--last','/nix/store/fixture/share/seele-shell','/configured/codex' if override else '/fixture/codex','/fixture/qt/qml:/retained/qml','/fixture/qt/plugins:/retained/plugins','/fixture/bin:/retained/bin']
 assert open(binary,'rb').read(4)==b'\x7fELF'
PY
done
printf 'Native wrapper flags, environment, Qt import/plugin paths and caller argv passed\n'
