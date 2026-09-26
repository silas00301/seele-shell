// Render the production Themes switcher and Control Center panel over a
// fixture store; catalog policy is
// covered by qml-core's themes tests and store round trips by themes.js.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {spawnSync} = require('node:child_process');
const source = path.resolve(process.argv[2]);
const work = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-themes-panel-'));
try {
  fs.mkdirSync(path.join(work, 'production'));
  const shared = fs.existsSync(path.join(source, 'shared')) ? path.join(source, 'shared') : path.resolve(source, '../shared');
  fs.cpSync(shared, path.join(work, 'production/shared'), {recursive: true});
  for (const file of ['ThemePanel.qml', 'ThemeSettingsPanel.qml'])
    fs.writeFileSync(path.join(work, 'production', file), fs.readFileSync(path.join(source, file), 'utf8').replace('import "../shared"', 'import "shared"'));
  // Only Quickshell's executable-owned config IO is omitted. The real tokens,
  // components and entire panel remain intact in this Qt host.
  const themePath = path.join(work, 'production/shared/Theme.qml');
  const theme = fs.readFileSync(themePath, 'utf8').split('  FileView {')[0]
    .replace(/import Quickshell.*\n/g, '').replace('ShellRoot {', 'Item {')
    .replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ', '')
    .replace('Qt.resolvedUrl("grain.png")', '""');
  fs.writeFileSync(themePath, theme + '}\n');
  fs.writeFileSync(path.join(work, 'tst_themes.qml'), fs.readFileSync(path.resolve(__dirname, 'tst_themes.qml'), 'utf8')
    .replace('property string screenshotPath: ""', 'property string screenshotPath: ' + JSON.stringify(process.env.SEELE_THEMES_SCREENSHOT || ''))
    .replace('property string settingsScreenshotPath: ""', 'property string settingsScreenshotPath: ' + JSON.stringify(process.env.SEELE_THEMES_SETTINGS_SCREENSHOT || '')));
  const result = spawnSync('qmltestrunner', ['-import', process.argv[3], '-input', path.join(work, 'tst_themes.qml')], {
    stdio: 'inherit', env: {...process.env, QT_QPA_PLATFORM: 'offscreen', QT_QUICK_BACKEND: 'software'},
  });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
} finally {
  fs.rmSync(work, {recursive: true, force: true});
}
