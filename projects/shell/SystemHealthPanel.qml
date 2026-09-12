import QtQuick
import QtQuick.Controls
import "../shared" as Shared

Column {
  id: panel
  required property var theme
  required property var store
  property string tab: "integrations"
  property Component maintenanceContent: null
  property int maintenanceCount: 0
  property string confirmation: ""
  property string diagnostics: ""
  spacing: theme.spaceMedium
  Shared.PanelHeader { theme:panel.theme; width:parent.width; glyph:"󰅚"; title:"System Health"; detail:panel.store.attentionCount+" integrations need attention" }
  Row {
    spacing:panel.theme.spaceSmall
    Shared.ActionButton { theme:panel.theme; text:"Integrations"; selected:panel.tab==="integrations"; onClicked:panel.tab="integrations" }
    Shared.ActionButton { visible:panel.maintenanceContent!==null; theme:panel.theme; text:"Maintenance"+(panel.maintenanceCount ? " · "+panel.maintenanceCount : ""); selected:panel.tab==="maintenance"; onClicked:panel.tab="maintenance" }
  }
  Loader { width:parent.width; active:panel.tab==="maintenance"; visible:active; sourceComponent:panel.maintenanceContent }
  Column {
    width:parent.width; visible:panel.tab==="integrations"; spacing:panel.theme.spaceSmall
    Text { visible:panel.store.rows.length===0; text:"No integrations configured"; color:panel.theme.subtext; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textBody }
    Repeater {
      model:panel.store.model
      delegate:Rectangle {
        id:card
        required property var modelData
        required property int index
        readonly property real inset: modelData.state === "healthy" ? panel.theme.spaceSmall : panel.theme.cardPadding
        width:parent.width; implicitHeight:body.implicitHeight+card.inset*2
        radius:panel.theme.radius; color:panel.theme.cardColor
        Shared.CardEdge { theme:panel.theme }
        Column {
          id:body
          anchors { left:parent.left; right:parent.right; top:parent.top; margins:card.inset }
          spacing:panel.theme.spaceSmall
          Shared.SectionLabel { theme:panel.theme; visible:card.modelData.state === "healthy" && (card.index === 0 || panel.store.rows[card.index-1].state !== "healthy"); text:"HEALTHY" }
          Text { width:parent.width; text:card.modelData.name+" · "+card.modelData.state; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:card.modelData.state==="healthy"?panel.theme.text:panel.theme.yellow; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textBody; font.weight:panel.theme.weightStrong }
          Text { width:parent.width; text:card.modelData.summary; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:panel.theme.subtext; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption }
          Text { width:parent.width; text:card.modelData.lastSuccess ? "Last successful update · "+new Date(card.modelData.lastSuccess).toLocaleString() : "No successful update yet"; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:panel.theme.overlay; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textMicro }
          Flow {
            width:parent.width; spacing:panel.theme.spaceSmall
            Repeater {
              model:card.modelData.actions
              delegate:Shared.ActionButton {
                required property string modelData
                theme:panel.theme; text:modelData==="settings"?"Open settings":modelData.charAt(0).toUpperCase()+modelData.slice(1)
                enabled:!panel.store.pending[card.modelData.id]
                onClicked: {
                  var key=card.modelData.id+":"+modelData
                  if(modelData==="diagnostics") panel.diagnostics=panel.diagnostics===card.modelData.id?"":card.modelData.id
                  else if(panel.store.registrations[card.modelData.id].disruptive.indexOf(modelData)>=0 && panel.confirmation!==key) panel.confirmation=key
                  else { panel.store.act(card.modelData.id,modelData,panel.confirmation===key); panel.confirmation="" }
                }
              }
            }
          }
          Text { visible:panel.confirmation.indexOf(card.modelData.id+":")===0; width:parent.width; text:"This may interrupt active work. Select the action again to confirm."; wrapMode:Text.Wrap; color:panel.theme.yellow; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption }
          Text { visible:!!panel.store.pending[card.modelData.id] || !!panel.store.errors[card.modelData.id]; width:parent.width; text:panel.store.pending[card.modelData.id]?"Working…":panel.store.errors[card.modelData.id]||""; color:panel.theme.yellow; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption }
          Text { visible:panel.diagnostics===card.modelData.id; width:parent.width; text:card.modelData.detail||"No additional diagnostics"; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:panel.theme.subtext; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption }
        }
      }
    }
  }
}
