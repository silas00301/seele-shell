// Exercise production password key handlers with the real Navigator. No PAM,
// greeter, compositor, password submission or power action is instantiated.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { spawnSync } = require('node:child_process');
const assert = require('node:assert/strict');
const [shared, imports, ...clients] = process.argv.slice(2);
assert(shared && imports && clients.length === 2, 'shared, navigation imports, lock and greeter sources required');

function passwordHandler(file) {
  const source = fs.readFileSync(file, 'utf8');
  const pattern = /Keys\.onPressed:\s*(function\(event\)\s*\{)/g;
  for (const match of source.matchAll(pattern)) {
    const start = match.index + match[0].indexOf('function');
    let end = source.indexOf('{', start) + 1, depth = 1;
    while (depth && end < source.length) {
      if (source[end] === '{') depth++;
      if (source[end] === '}') depth--;
      end++;
    }
    const handler = source.slice(start, end);
    if (handler.includes('root.passwordText')) return handler;
  }
  throw new Error(`Missing production password handler: ${file}`);
}

const work = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-auth-keys-'));
try {
  fs.mkdirSync(path.join(work, 'shared'));
  for (const component of ['ActionArea', 'FocusRing', 'KeyboardNavigation'])
    fs.copyFileSync(path.join(shared, component + '.qml'), path.join(work, 'shared', component + '.qml'));
  for (const [index, client] of clients.entries()) {
    fs.writeFileSync(path.join(work, `tst_auth${index}.qml`), `
import QtQuick
import QtTest
import "shared" as Shared
Item {
  id: root
  width: 300; height: 100
  property bool powerMenuOpen: false
  property string pendingPowerAction: ""
  property string passwordText: ""
  QtObject {
    id: theme
    property color accent: "#aabbff"
    property int radius: 8
    function alpha(color, opacity) { return Qt.rgba(color.r, color.g, color.b, opacity) }
  }
  Shared.KeyboardNavigation { theme: theme }
  TextInput {
    id: field
    width: 280; height: 40
    text: root.passwordText
    echoMode: TextInput.Password
    Keys.onPressed: ${passwordHandler(client)}
  }
  TestCase {
    name: "AuthKeys${index}"
    when: windowShown
    function init() {
      root.powerMenuOpen = true
      root.pendingPowerAction = "poweroff"
      root.passwordText = "fixture draft"
      field.forceActiveFocus()
    }
    function test_escape_closes_menu_and_clears_confirmation() {
      keyClick(Qt.Key_Escape)
      compare(root.powerMenuOpen, false)
      compare(root.pendingPowerAction, "")
      compare(root.passwordText, "")
      verify(field.activeFocus)
    }
    function test_control_u_only_clears_password() {
      keyClick(Qt.Key_U, Qt.ControlModifier)
      compare(root.powerMenuOpen, true)
      compare(root.pendingPowerAction, "poweroff")
      compare(root.passwordText, "")
    }
  }
}
`);
  }
  const result = spawnSync('qmltestrunner', ['-import', imports, '-input', work], {
    stdio: 'inherit', env: { ...process.env, QT_QPA_PLATFORM: 'offscreen', QT_QUICK_BACKEND: 'software' },
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, 'production auth keyboard fixtures');
} finally {
  fs.rmSync(work, { recursive: true, force: true });
}
