// Writes material-oracle.json: Material Color Utilities' own colour roles for
// SchemeExpressive, 2025 spec, phone platform, which `seele-theme material`
// ports. The Rust tests hold the port to this file role for role.
//
//   node material-oracle.mjs <material-color-utilities.mjs> > material-oracle.json
//
// The module is the library's TypeScript sources at the revision the port
// follows (typescript/index.ts at material-color-utilities
// dd62982dfdefdfc9232ce8d76e122fb1c6d188ae), bundled to one ES module, e.g.
// with `esbuild index.ts --bundle --format=esm --platform=node`. The released
// npm package 0.4.0 predates that revision and differs in Expressive's dark
// primary container. Only a test fixture: nothing runs this at build time.
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';

const mcu = await import(pathToFileURL(resolve(process.argv[2])).href);
const {Hct, SchemeExpressive, MaterialDynamicColors, hexFromArgb} = mcu;
// The baseline seed, the primaries and secondaries, greys at both ends, one
// seed in each hue the 2025 spec treats specially (yellow, cyan, blue), and
// the accents the shell's Catppuccin preset and example folder use.
const seeds = ['#6750a4', '#000000', '#ffffff', '#808080', '#ff0000', '#00ff00', '#0000ff',
  '#e6c419', '#19c3e6', '#4285f4', '#b4befe', '#f2aa4c'];
const levels = [-1, 0, 0.5, 1];
const colors = new MaterialDynamicColors();
const roles = [...colors.allColors.filter(color => !color.name.endsWith('_palette_key_color')),
  colors.surfaceVariant(), colors.shadow(), colors.scrim(), colors.surfaceTint()];
// One line a case: its seed, mode and contrast, then every role's colour in
// the order `roles` names them.
const cases = [];
for (const seed of seeds) for (const dark of [false, true]) for (const contrast of levels) {
  const scheme = new SchemeExpressive(Hct.fromInt(parseInt(seed.slice(1), 16) | 0xff000000), dark, contrast, '2025', 'phone');
  cases.push(JSON.stringify({seed, dark, contrast, colors: roles.map(color => hexFromArgb(color.getArgb(scheme)))}));
}
process.stdout.write(`{"roles": ${JSON.stringify(roles.map(color => color.name))},\n"cases": [\n${cases.join(',\n')}\n]}\n`);
