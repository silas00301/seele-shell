import QtQuick
import QtQuick.Controls
import "../shared" as Shared

Column {
  id:panel
  required property var theme
  required property var store
  property bool showSnoozed:false
  property bool showHistory:false
  spacing:theme.spaceMedium
  function urgencyLabel(value) {
    return ({now:"Action required now",soon:"Action required soon",eventually:"Action required eventually",informational:"Informational"})[value] || value
  }
  Text {
    width:parent.width; visible:panel.store.error!=="" || panel.store.activeModel.count===0
    text:panel.store.error || "No active maintenance findings"
    textFormat:Text.PlainText
    color:panel.store.error?panel.theme.red:panel.theme.subtext
    font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textBody; wrapMode:Text.Wrap
  }
  Repeater {
    model:Object.keys(panel.store.snapshot.checkErrors || {})
    Text {
      required property string modelData
      width:panel.width; text:modelData+" · "+panel.store.snapshot.checkErrors[modelData]
      textFormat:Text.PlainText; color:panel.theme.yellow; font.family:panel.theme.fontFamily
      font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
    }
  }
  component FindingCard: Rectangle {
      id:card
      required property var finding
      readonly property bool expanded:!!panel.store.expandedIds[finding.id]
      width:panel.width; implicitHeight:body.implicitHeight+panel.theme.cardPadding*2
      radius:panel.theme.radius; color:panel.theme.cardColor
      Shared.CardEdge { theme:panel.theme }
      Column {
        id:body
        anchors { left:parent.left; right:parent.right; top:parent.top; margins:panel.theme.cardPadding }
        spacing:panel.theme.spaceSmall
        Text {
          width:parent.width; text:card.finding.title; textFormat:Text.PlainText
          wrapMode:Text.Wrap; color:panel.theme.text; font.family:panel.theme.fontFamily
          font.pixelSize:panel.theme.textBody; font.weight:panel.theme.weightStrong
        }
        Text {
          width:parent.width; text:panel.urgencyLabel(card.finding.urgency)+" · "+card.finding.source; textFormat:Text.PlainText
          color:card.finding.urgency==="now"?panel.theme.red:card.finding.urgency==="soon"?panel.theme.yellow:panel.theme.subtext
          font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
        }
        Text {
          width:parent.width; text:card.finding.explanation; textFormat:Text.PlainText
          color:panel.theme.subtext; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
        }
        Shared.ActionButton {
          theme:panel.theme; text:card.expanded?"Less":"Details"
          onClicked:{var ids=Object.assign({},panel.store.expandedIds);ids[card.finding.id]=!card.expanded;panel.store.expandedIds=ids}
        }
        Text {
          width:parent.width; visible:card.expanded
          text:card.finding.details+"\nFirst seen · "+new Date(card.finding.firstSeen*1000).toLocaleString()
            +"\nUpdated · "+new Date(card.finding.updated*1000).toLocaleString()
            +(card.finding.recurrence?"\nRecurrences · "+card.finding.recurrence:"")
            +(card.finding.resolved?"\nResolved · "+new Date(card.finding.resolved*1000).toLocaleString():"")
            +(card.finding.snoozedUntil>Date.now()/1000?"\nSnoozed until · "+new Date(card.finding.snoozedUntil*1000).toLocaleString():"")
          textFormat:Text.PlainText; color:panel.theme.subtext; font.family:panel.theme.fontFamily
          font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
        }
        Flow {
          width:parent.width; spacing:panel.theme.spaceSmall
          visible:!card.finding.resolved
          enabled:!card.finding.busy && panel.store.pendingId==="" && panel.store.error===""
          Repeater {
            model:card.finding.actions
            Shared.ActionButton {
              required property var modelData
              theme:panel.theme; text:modelData.label
              onClicked:panel.store.repair(card.finding,modelData,false)
            }
          }
          Shared.ActionButton { theme:panel.theme; visible:card.finding.canAnalyze; text:"Analyze with AI"; onClicked:panel.store.request(card.finding,"analyze") }
          Shared.ActionButton { theme:panel.theme; visible:card.finding.lifecycle==="notice"; text:"Done"; onClicked:panel.store.request(card.finding,"done") }
          Shared.ActionButton { theme:panel.theme; visible:card.finding.snoozedUntil>Date.now()/1000; text:"Unsnooze"; onClicked:panel.store.request(card.finding,"unsnooze") }
        }
        Flow {
          width:parent.width; spacing:panel.theme.spaceSmall; visible:card.expanded && !card.finding.resolved
          enabled:!card.finding.busy && panel.store.pendingId==="" && panel.store.error===""
          Repeater {
            model:[{label:"1 hour",seconds:3600},{label:"1 day",seconds:86400},{label:"1 week",seconds:604800}]
            Shared.ActionButton {
              required property var modelData
              theme:panel.theme; text:"Snooze "+modelData.label
              onClicked:panel.store.request(card.finding,"snooze",{seconds:modelData.seconds})
            }
          }
          TextField {
            id:customMinutes
            width:panel.theme.controlHeight*4
            placeholderText:"Snooze minutes"
            validator:IntValidator {bottom:1;top:43200}
            color:panel.theme.text; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption
            background:Rectangle {radius:panel.theme.radius; color:panel.theme.wellColor; border.color:customMinutes.activeFocus?panel.theme.accent:panel.theme.cardBorder}
          }
          Shared.ActionButton { theme:panel.theme; text:"Snooze"; enabled:customMinutes.acceptableInput; onClicked:panel.store.request(card.finding,"snooze",{seconds:Number(customMinutes.text)*60}) }
        }
        Column {
          width:parent.width; spacing:panel.theme.spaceSmall
          visible:!!panel.store.confirmation && panel.store.confirmation.id===card.finding.id
          Text { width:parent.width; text:panel.store.confirmation && panel.store.confirmation.proposed ? "Review this AI-proposed action before allowing it to run." : "Confirm this repair before it runs. It may interrupt active work."; wrapMode:Text.Wrap; color:panel.theme.yellow; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption }
          Row {
            enabled:!card.finding.busy && panel.store.pendingId==="" && panel.store.error===""
            spacing:panel.theme.spaceSmall
            Shared.ActionButton {theme:panel.theme;text:"Confirm "+(panel.store.confirmation?panel.store.confirmation.label:"");danger:true;onClicked:panel.store.confirm()}
            Shared.ActionButton {theme:panel.theme;text:"Cancel";onClicked:panel.store.confirmation=null}
          }
        }
        Text {
          width:parent.width; visible:!!card.finding.busy || panel.store.pendingId===card.finding.id || panel.store.actionErrorId===card.finding.id
          text:card.finding.busy || panel.store.pendingId===card.finding.id ? "Working…" : panel.store.actionError
          color:panel.theme.yellow; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
        }
        Text {
          width:parent.width; visible:card.finding.outcomes.length>0
          text:card.finding.outcomes.length ? card.finding.outcomes[card.finding.outcomes.length-1].action+" · "+card.finding.outcomes[card.finding.outcomes.length-1].result : ""
          textFormat:Text.PlainText; color:panel.theme.subtext; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
        }
        Column {
          width:parent.width; spacing:panel.theme.spaceSmall; visible:!!card.finding.analysis
          Text {
            width:parent.width
            text:card.finding.analysis ? (card.finding.analysisStale?"Previous analysis · finding changed\n":"AI analysis\n")+card.finding.analysis.cause
              +"\n"+card.finding.analysis.evidence.join("\n")+"\n"+card.finding.analysis.nextSteps.join("\n") : ""
            textFormat:Text.PlainText; color:panel.theme.subtext; font.family:panel.theme.fontFamily; font.pixelSize:panel.theme.textCaption; wrapMode:Text.Wrap
          }
          Flow {
            width:parent.width; spacing:panel.theme.spaceSmall
            visible:!card.finding.analysisStale && !card.finding.resolved
            enabled:!card.finding.busy && panel.store.pendingId==="" && panel.store.error===""
            Repeater {
              model:card.finding.analysis?card.finding.analysis.actions:[]
              Shared.ActionButton {
                required property string modelData
                readonly property var proposed:card.finding.actions.find(function(a){return a.id===modelData})
                theme:panel.theme; text:proposed?"Review "+proposed.label:"Unavailable action"; enabled:!!proposed
                onClicked:panel.store.repair(card.finding,proposed,true)
              }
            }
          }
        }
      }
  }
  Repeater {
    model:panel.store.activeModel
    FindingCard { required property var modelData; finding:modelData }
  }
  Shared.ActionButton { theme:panel.theme; text:"Snoozed · "+panel.store.snoozedModel.count; selected:panel.showSnoozed; onClicked:panel.showSnoozed=!panel.showSnoozed }
  Column {
    width:parent.width; visible:panel.showSnoozed; spacing:panel.theme.spaceSmall
    Repeater {
      model:panel.store.snoozedModel
      FindingCard { required property var modelData; finding:modelData }
    }
  }
  Shared.ActionButton { theme:panel.theme; text:"History · "+panel.store.historyModel.count; selected:panel.showHistory; onClicked:panel.showHistory=!panel.showHistory }
  Column {
    width:parent.width; visible:panel.showHistory; spacing:panel.theme.spaceSmall
    Repeater {
      model:panel.store.historyModel
      FindingCard { required property var modelData; finding:modelData }
    }
  }
}
