// The production highlighter is driven through QTextDocument. The JSON oracle
// records complete format properties and block states without font rasterizer
// or QDataStream-version dependencies. It never edits the document's source.
#include "markdownhighlighter.h"
#include <QFile>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTextBlock>
#include <QTextLayout>

static QJsonObject properties(const QTextCharFormat &format) {
  QJsonObject result;
  const auto values = format.properties();
  for (auto it = values.cbegin(); it != values.cend(); ++it) {
    const auto value = it.value();
    if (value.metaType() == QMetaType::fromType<QBrush>()) {
      const auto brush = value.value<QBrush>();
      result.insert(QString::number(it.key()), QJsonArray{int(brush.style()), brush.color().name(QColor::HexArgb)});
    } else {
      result.insert(QString::number(it.key()), QJsonValue::fromVariant(value));
    }
  }
  return result;
}

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QFile input;
  QFile output;
  if (!input.open(stdin, QIODevice::ReadOnly) ||
      !output.open(stdout, QIODevice::WriteOnly)) return 1;
  while (true) {
    const auto line = input.readLine();
    if (line.isEmpty()) break;
    const auto request = QJsonDocument::fromJson(line).array();
    QJsonArray responses;
    for (const auto &value : request) {
      QTextDocument document;
      MarkdownHighlighter highlighter;
      highlighter.setBaseSize(13.25);
      highlighter.setMonoFamily("monospace");
      highlighter.QSyntaxHighlighter::setDocument(&document);
      document.setPlainText(value.toString());
      highlighter.rehighlight();
      QJsonArray blocks;
      for (auto block = document.begin(); block.isValid(); block = block.next()) {
        QJsonArray ranges;
        for (const auto &range : block.layout()->formats())
          ranges.append(QJsonArray{range.start, range.length, properties(range.format)});
        blocks.append(QJsonObject{{"state", block.userState()}, {"formats", ranges}});
      }
      responses.append(blocks);
    }
    output.write(QJsonDocument(responses).toJson(QJsonDocument::Compact) + '\n');
    output.flush();
  }
}
