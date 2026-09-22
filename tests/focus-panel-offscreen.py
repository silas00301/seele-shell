"""Optional PySide6 offscreen regression for the production Focus panel.

Usage: QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  python tests/focus-panel-offscreen.py /path/to/seele-qml-functions /tmp/focus-images

The panel, theme tokens and JS adapters are production files. Only Quickshell's
host/reload objects and the C++ ABI transport are substituted; every policy call
runs the actual Rust fixture binary. The packaged focus-timer.sh separately
exercises real Quickshell and its native plugin.
"""
import sys, json, shutil, subprocess, tempfile
from pathlib import Path
from PySide6.QtCore import QObject, Slot, QUrl, Qt
from PySide6.QtQml import QJSValue, qmlRegisterSingletonType
from PySide6.QtGui import QGuiApplication
from PySide6.QtQuick import QQuickView
from PySide6.QtTest import QTest
source = Path(__file__).resolve().parent.parent
binary = str(Path(sys.argv[1]).resolve())
images = Path(sys.argv[2]).resolve()
images.mkdir(parents=True, exist_ok=True)
staging = tempfile.TemporaryDirectory(prefix='seele-focus-qt-')
work = Path(staging.name)
shutil.copytree(source/'projects/shared', work/'shared', dirs_exist_ok=True)
for filename in ['FocusPanel.qml', 'FocusTimer.qml', 'focus.js']:
    text = (source/'projects/shell'/filename).read_text().replace('../shared','shared')
    if filename == 'FocusTimer.qml':
        text = text.replace('import Quickshell\n','').replace('PersistentProperties {','QtObject {').replace('reloadableId: "seele-focus-timer"','').replace('onLoaded:', 'Component.onCompleted:')
    (work/filename).write_text(text)
p = work/'shared/Theme.qml'
text = p.read_text().replace('import Quickshell\n','').replace('import Quickshell.Io\n','').replace('ShellRoot {','Item {')
text = text[:text.index('  FileView {')] + '}\n'
text = text.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ', '')
p.write_text(text)
class Functions(QObject):
    def __init__(self, engine):
        super().__init__(engine)
        self.engine = engine
    @Slot(str, 'QVariantList', result=QJSValue)
    def call(self, op, args):
        result = subprocess.run([binary], input=json.dumps({'operation':op,'arguments':args}), text=True,capture_output=True,check=True)
        payload = json.loads(result.stdout)
        assert payload['ok'], payload
        return self.engine.evaluate('(' + json.dumps(payload['value']) + ')')
refs=[]
def provider(engine):
    obj=Functions(engine);refs.append(obj);return obj
qmlRegisterSingletonType(Functions,'Seele.Core',1,0,'Functions',provider)
(work/'preview.qml').write_text('''import QtQuick
import "shared" as Shared
Rectangle {
  id: root
  width: 350
  height: panel.implicitHeight + theme.panelMargin * 2
  color: theme.mantle
  Shared.Theme { id: theme }
  FocusTimer { id: timer; objectName: "timer" }
  FocusPanel {
    id: panel
    objectName: "panel"
    anchors { left: parent.left; right: parent.right; top: parent.top; margins: theme.panelMargin }
    theme: theme
    timer: timer
  }
}''')
app=QGuiApplication(sys.argv)
view=QQuickView()
view.setSource(QUrl.fromLocalFile(str(work/'preview.qml')))
assert view.status()==QQuickView.Ready, view.errors()
view.show()
QTest.qWait(100)
root=view.rootObject()
field=root.findChild(QObject,'focusCustomMinutes')
panel=root.findChild(QObject,'panel')
start=root.findChild(QObject,'focusCustomStart')
extend=root.findChild(QObject,'focusExtend')
timer=root.findChild(QObject,'timer')
preset=root.findChild(QObject,'focusPreset50')
def state(): return timer.property('timerState').toVariant()
def snapshot(name):
    QTest.qWait(150)
    assert view.grabWindow().save(str(images/(name+'.png')))
preset.forceActiveFocus()
QTest.keyClick(view,Qt.Key_Return)
assert state()['duration']==3000, state()
timer.command('cancel')
field.forceActiveFocus()
[QTest.keyClick(view, Qt.Key(ord(c))) for c in '241']
assert not start.property('enabled')
snapshot('invalid')
QTest.keyClick(view,Qt.Key_A,Qt.ControlModifier)
[QTest.keyClick(view, Qt.Key(ord(c))) for c in '37']
assert state()['status']=='idle', state()
QTest.keyClick(view,Qt.Key_Return)
assert state()['duration']==2220, state()
snapshot('running')
QTest.keyClick(view,Qt.Key_Plus)
assert state()['duration']==2520, state()
QTest.keyClick(view,Qt.Key_Space)
assert state()['status']=='paused', state()
QTest.keyClick(view,Qt.Key_Plus)
assert state()['duration']==2820 and state()['status']=='paused', state()
snapshot('paused')
field.forceActiveFocus()
QTest.keyClick(view,Qt.Key_A,Qt.ControlModifier)
[QTest.keyClick(view, Qt.Key(ord(c))) for c in '240']
QTest.keyClick(view,Qt.Key_Return)
assert state()['duration']==14400 and not extend.property('enabled'),state()
snapshot('limit')
QTest.keyClick(view,Qt.Key_Delete)
assert state()['status']=='idle', state()
QTest.keyClick(view,Qt.Key_Plus)
assert state()['status']=='idle', state()
print('PASS: real production FocusPanel preset focus, Enter, text entry, +, Space, Delete, native policy and renders')
