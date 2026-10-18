# Theme folder registration end to end, as the flake builds it. Every folder
# in themes/default.nix, the template a new theme is copied from
# (themes/AUTHORING.md) and the example and pictured fixtures register; they
# then list beside a preset through the real helper, the pictured folder's own
# wallpaper is published while it is selected and the configured image again
# once a theme without one is; and each invalid fixture fails its own build,
# naming the folder and what it broke.
{ pkgs, configTools }:
let
  lib = pkgs.lib;
  themes = import ./themes.nix { inherit pkgs configTools; };
  fixtures = ../../tests/theme-folders;
  registered = themes.entries (import ../../themes);
  example = themes.entry (fixtures + "/example");
  pictured = themes.entry (fixtures + "/pictured");
  template = themes.entry ../../themes/template;
  folders = [
    example
    pictured
  ]
  ++ registered
  ++ [ template ];
  configured = "/etc/wallpaper/wallpaper.jpg";
  preset = {
    id = "catppuccin-mocha";
    name = "Catppuccin Mocha";
    mode = "dark";
    palette = example.palette;
    vicinaeTheme = launcher;
  };
  launcher = pkgs.writeText "seele-theme-folders-vicinae.toml" ''
    [meta]
    name = "Fixture"
    variant = "dark"
  '';
  entry = theme: {
    inherit (theme)
      id
      name
      mode
      palette
      document
      ;
    vicinaeTheme = launcher;
  };
  catalog = pkgs.writeText "seele-theme-folders-catalog.json" (
    builtins.toJSON {
      version = 2;
      default = "catppuccin-mocha";
      fontFamily = "Maple Mono NF CN";
      wallpaper = configured;
      commands = { };
      themes = [ preset ] ++ map entry folders;
    }
  );
  # What each invalid fixture's build has to say.
  refusals = {
    incomplete = "missing required slot accent";
    misnamed = ''id: "example" is not the folder's name'';
    unknown-slots = "sparkle: not a contract slot";
    escaping-file = ''soundNotification: "../misnamed/theme.json" is not a path inside the theme folder'';
    wallpaper-not-image = ''wallpaper: "wallpaper.png" is not a PNG or JPEG image'';
    sound-not-wave = ''soundNotification: "sounds/ping.oga" is not a WAV sound'';
    material-drift = "material: theme.json is not what its seed grows (radius: 9 here, 16 from its seed); `seele-theme material` writes it again";
  };
  failures = lib.mapAttrs (
    name: _: pkgs.testers.testBuildFailure (themes.register (fixtures + "/invalid/${name}"))
  ) refusals;
in
pkgs.runCommand "seele-theme-folders-check" { nativeBuildInputs = [ pkgs.jq ]; } ''
  export HOME="$TMPDIR" XDG_CONFIG_HOME="$TMPDIR/config" XDG_STATE_HOME="$TMPDIR/state"
  mkdir -p "$XDG_CONFIG_HOME/seele-theme"
  cp ${catalog} "$XDG_CONFIG_HOME/seele-theme/catalog.json"
  ${configTools}/bin/seele-theme list > listed.json
  test "$(jq -r '[.themes[].id] | join(" ")' listed.json)" = ${
    lib.escapeShellArg (
      lib.concatStringsSep " " ([ "catppuccin-mocha" ] ++ map (theme: theme.id) folders)
    )
  }
  test "$(jq -r '.themes[1].radius' listed.json)" = 14
  wallpaper() {
    ${configTools}/bin/seele-theme set "$1" > /dev/null
    test "$(jq -r .wallpaper "$XDG_STATE_HOME/seele-theme/selection.json")" = "$2"
  }
  wallpaper pictured ${pictured.registered}/wallpaper.png
  wallpaper example ${configured}
  wallpaper pictured ${pictured.registered}/wallpaper.png
  wallpaper catppuccin-mocha ${configured}
  ${lib.concatStrings (
    lib.mapAttrsToList (name: line: ''
      grep -qxF ${lib.escapeShellArg "Seele Themes: Invalid theme folder ${name}:"} ${failures.${name}}/testBuildFailure.log
      grep -qxF ${lib.escapeShellArg "  ${line}"} ${failures.${name}}/testBuildFailure.log
    '') refusals
  )}
  touch "$out"
''
