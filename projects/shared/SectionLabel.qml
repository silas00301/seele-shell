import QtQuick

// Material's list subheader: a short sentence-case label in the primary
// colour, set a weight above the rows under it. It names a group rather than
// shouting it, so it needs neither capitals nor tracking to read as a rule.
Text {
  id: sectionLabel
  required property var theme
  color: sectionLabel.theme.primary
  font.family: sectionLabel.theme.fontFamily
  font.pixelSize: sectionLabel.theme.textLabel
  font.weight: sectionLabel.theme.weightStrong
}
