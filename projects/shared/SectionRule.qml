import QtQuick

// The heading a group of rows sits under: a subheader on the left, a quiet
// summary on the right, an optional control after it, and a fold arrow when
// the group can be closed.
Item {
  id: sectionRule

  required property var theme
  property string label: ""
  property string detail: ""
  property color detailColor: sectionRule.theme.subtext
  // Icon actions have more empty space around their ink than these small labels.
  property real textBottomInset: 0
  property real detailTrailingSpacing: sectionRule.theme.spaceMedium
  property real trailingSpacing: sectionRule.theme.spaceMedium
  property bool collapsible: false
  property bool expanded: false
  default property alias trailing: sectionRuleTrailing.data
  signal toggled()

  height: Math.max(26, sectionRuleTrailing.implicitHeight + sectionRule.theme.spaceTight)

  SectionLabel {
    theme: sectionRule.theme
    anchors.left: parent.left
    anchors.bottom: parent.bottom
    anchors.bottomMargin: sectionRule.textBottomInset
    text: sectionRule.label
    textFormat: Text.PlainText
    color: sectionRule.collapsible && sectionRuleMouse.containsMouse
      ? sectionRule.theme.textOnPrimaryContainer
      : sectionRule.theme.primary
  }

  // Everything in the rule sits on its bottom edge, so a group's label, its
  // summary, its fold arrow and whatever control governs it share one line
  // and the rule's extra height stays above them all.
  Row {
    id: sectionRuleTrailing

    anchors.right: sectionRuleChevron.left
    anchors.rightMargin: sectionRule.collapsible ? sectionRule.theme.spaceSmall : 0
    anchors.bottom: parent.bottom
    spacing: sectionRule.trailingSpacing
  }

  Text {
    anchors.right: sectionRuleTrailing.left
    anchors.rightMargin: sectionRuleTrailing.width > 0 ? sectionRule.detailTrailingSpacing : 0
    anchors.bottom: parent.bottom
    anchors.bottomMargin: sectionRule.textBottomInset
    text: sectionRule.detail
    textFormat: Text.PlainText
    color: sectionRule.detailColor
    font.family: sectionRule.theme.fontFamily
    font.pixelSize: sectionRule.theme.textCaption
  }

  Text {
    id: sectionRuleChevron

    visible: sectionRule.collapsible
    width: visible ? 14 : 0
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.bottomMargin: -2
    text: sectionRule.expanded ? "󰅃" : "󰅀"
    color: sectionRuleMouse.containsMouse ? sectionRule.theme.text : sectionRule.theme.subtext
    font.family: sectionRule.theme.fontFamily
    font.pixelSize: sectionRule.theme.textBody
    horizontalAlignment: Text.AlignRight
  }

  MouseArea {
    id: sectionRuleMouse

    anchors.fill: parent
    enabled: sectionRule.collapsible
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    onClicked: sectionRule.toggled()
  }
}
