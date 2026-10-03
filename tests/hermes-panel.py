"""Render and interact with production Hermes cards using offscreen Qt."""
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
os.environ.setdefault('QT_QUICK_BACKEND', 'software')
from PySide6.QtCore import QObject, QPoint, QUrl, Qt
from PySide6.QtGui import QGuiApplication
from PySide6.QtQuick import QQuickView
from PySide6.QtTest import QTest
source = Path(sys.argv[1]).resolve()
app = QGuiApplication([])
warnings = []
with tempfile.TemporaryDirectory(prefix='seele-hermes-panel-') as temp:
    root = Path(temp)
    shutil.copytree(source / 'shared' if (source / 'shared').is_dir() else source / '../shared', root / 'shared')
    theme = (root / 'shared/Theme.qml').read_text().split('  FileView {')[0]
    theme = re.sub(r'import Quickshell.*\n', '', theme).replace('ShellRoot {', 'Item {')
    theme = theme.replace('Quickshell.env("SEELE_SHELL_WALLPAPER") || ', '')
    (root / 'shared/Theme.qml').write_text(theme + '}\n')
    (root / 'HermesPanel.qml').write_text((source / 'HermesPanel.qml').read_text().replace('../shared', 'shared'))
    (root / 'fixture.qml').write_text('''import QtQuick
import "shared" as Shared
Rectangle {
  width: 380; height: panel.implicitHeight + theme.panelMargin * 2
  color: theme.crust
  Shared.Theme { id: theme }
  QtObject {
    id: store
    property var snapshot: ({session: "opaque"})
    property var projection: ({label: "Thinking", detail: "Connected over Tailscale", pending: [{id: "one", revision: "0123456789abcdef", remaining: 120}]})
    property bool busy: false
    property string error: ""
    property string received: ""
    function openDesktop() { received = "desktop" }
    function send(value) { received = JSON.stringify(value) }
    objectName: "store"
  }
  HermesPanel { id: panel; objectName: "panel"; theme: theme; store: store; x: theme.panelMargin; y: theme.panelMargin; width: parent.width - theme.panelMargin * 2 }
}''')
    view = QQuickView()
    view.engine().warnings.connect(lambda messages: warnings.extend(m.toString() for m in messages))
    view.setSource(QUrl.fromLocalFile(str(root / 'fixture.qml')))
    assert view.status() == QQuickView.Ready, [e.toString() for e in view.errors()]
    view.show()
    QTest.qWait(150)
    panel = view.rootObject().findChild(QObject, 'panel')
    store = view.rootObject().findChild(QObject, 'store')
    def find_text(item, text):
        if item.property('text') == text:
            return item
        for child in (item.childItems() if hasattr(item, "childItems") else item.children()):
            found = find_text(child, text)
            if found:
                return found
    def click(text):
        label = find_text(panel, text)
        assert label is not None, text
        # Find the actual QQuick button, including its painted text child.
        while label.metaObject().indexOfSignal('clicked()') < 0:
            label = label.parent()
        point = label.mapToScene(QPoint(int(label.width()/2), int(label.height()/2)))
        QTest.mouseClick(view, Qt.LeftButton, Qt.NoModifier, QPoint(int(point.x()), int(point.y())))
        QTest.qWait(20)
    click('Open Hermes Desktop')
    assert store.property('received') == 'desktop'
    click('Deny')
    assert json.loads(store.property('received')) == {'op':'deny','id':'one'}
    click('Approve rebuild')
    assert json.loads(store.property('received')) == {'op':'approve','id':'one'}
    store.setProperty('busy', True)
    store.setProperty('received', '')
    click('Approve rebuild')
    assert store.property('received') == ''
    store.setProperty('busy', False)
    store.setProperty('error', 'The flake changed. Request and review a new rebuild.')
    QTest.qWait(30)
    assert panel.implicitHeight() > 0
    if len(sys.argv) > 2:
        view.grabWindow().save(sys.argv[2])
    click('Dismiss')
    assert store.property('error') == ''
    assert not warnings, warnings
    view.close()
print('Production Hermes panel renders; Desktop, deny, approve, busy and dismiss actions passed')
