// Build an offscreen fixture from the production delegate, substituting only
// compositor/window IO. Run the real layout policy and shared materials.
const fs = require('node:fs')
const path = require('node:path')
const [shell, shared, work] = process.argv.slice(2)
fs.mkdirSync(work, {recursive:true})
fs.cpSync(shared, path.join(work,'shared'), {recursive:true})
const theme = path.join(work,'shared/Theme.qml')
fs.writeFileSync(theme, fs.readFileSync(theme,'utf8').split('  FileView {')[0]
  .replace(/import Quickshell.*\n/g,'').replace('ShellRoot {','Item {')
  .replace(/Quickshell.env\("SEELE_SHELL_WALLPAPER"\) \|\| /,'') + '}\n')
fs.writeFileSync(path.join(work,'uri-picker.js'),fs.readFileSync(path.join(path.dirname(shell),'uri-picker.js'),'utf8').replace('../shared/','shared/'))
const source = fs.readFileSync(shell,'utf8')
const begin = source.indexOf('      Repeater {\n        model: uriPicker.links')
const end = source.indexOf('\n      Connections {\n        target: uriWindow.modelData',begin)
if (begin < 0 || end < 0) throw Error('Production URI overlay not found')
fs.writeFileSync(path.join(work,'Overlay.qml'),`import QtQuick
import "shared" as Shared
import "uri-picker.js" as Uris
Shared.Theme {
  id: root
  width: 1000; height: 600
  property alias links: linkModel
  property alias digits: uriPicker.digits
  property alias hoveredUri: uriPicker.hoveredUri
  signal activated(string uri, bool copy)
  component SurfaceWash: Shared.SurfaceWash { theme: root }
  component SurfaceEdge: Shared.SurfaceEdge { theme: root }
  component SurfaceGrain: Shared.SurfaceGrain { theme: root }
  component HoverWash: Shared.HoverWash { theme: root }
  Rectangle { anchors.fill: parent; color: root.crust }
  ListModel { id: linkModel }
  QtObject {
    id: uriPicker
    property var links: linkModel
    property string digits: ""
    property string hoveredUri: ""
    function launch(link, copy) { root.activated(link.uri, copy) }
  }
  Item {
    id: uriWindow
    anchors.fill: parent
    property var modelData: ({name:"DP-1"})
    property real badgeWidth: root.chipHeight
    property real badgeHeight: root.chipHeight
    property var positions: {
      var links = []
      for (var i = 0; i < linkModel.count; i++) {
        var item = linkModel.get(i), regions = []
        for (var j = 0; j < item.regions.count; j++) regions.push(item.regions.get(j))
        links.push({number:item.number,output:item.output,x0:item.x0,y0:item.y0,w:item.w,h:item.h,regions:regions})
      }
      return Uris.layout(links,"DP-1",width,height,badgeWidth,badgeHeight,root.spaceTight)
    }
${source.slice(begin,end)}
  }
}
`)
