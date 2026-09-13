// A live Markdown editor that never rewrites the file it is editing.
//
// Qt's MarkdownText mode parses a document into rich text and re-serializes it
// on the way out, which loses frontmatter, wikilinks, embeds and every
// construct it does not model. Formatting the source in place instead keeps the
// bytes on disk exactly as the vault's other tools wrote them, keeps the syntax
// at the caret directly editable, and leaves the caret, the selection and the
// undo stack untouched, because a highlighter applies character formats without
// touching the text.
#pragma once

#include <QColor>
#include <QPointer>
#include <QQmlEngine>
#include <QQuickTextDocument>
#include <QString>
#include <QSyntaxHighlighter>

// Applying an editing command through TextArea's own insert and remove costs
// two undo steps and passes through a document the writer never saw: the text
// with the selection deleted and nothing put back yet. One edit block makes the
// replacement one step, so Ctrl+Z after Ctrl+B gives back the words with their
// emphasis removed rather than giving back nothing.
class MarkdownEdit : public QObject {
  Q_OBJECT
  QML_ELEMENT

public:
  using QObject::QObject;

  Q_INVOKABLE void replace(QQuickTextDocument *document, int start, int end,
                           const QString &text);
};

class MarkdownHighlighter : public QSyntaxHighlighter {
  Q_OBJECT
  QML_ELEMENT

  Q_PROPERTY(QQuickTextDocument *document READ document WRITE setDocument NOTIFY documentChanged)
  Q_PROPERTY(QColor textColor MEMBER m_text WRITE setTextColor)
  Q_PROPERTY(QColor mutedColor MEMBER m_muted WRITE setMutedColor)
  Q_PROPERTY(QColor accentColor MEMBER m_accent WRITE setAccentColor)
  Q_PROPERTY(QColor codeColor MEMBER m_code WRITE setCodeColor)
  Q_PROPERTY(QColor codeBackground MEMBER m_codeBackground WRITE setCodeBackground)
  Q_PROPERTY(QColor quoteColor MEMBER m_quote WRITE setQuoteColor)
  Q_PROPERTY(QColor doneColor MEMBER m_done WRITE setDoneColor)
  Q_PROPERTY(qreal baseSize MEMBER m_baseSize WRITE setBaseSize)
  Q_PROPERTY(QString monoFamily MEMBER m_mono WRITE setMonoFamily)

public:
  explicit MarkdownHighlighter(QObject *parent = nullptr);

  QQuickTextDocument *document() const { return m_document.data(); }
  void setDocument(QQuickTextDocument *document);

  void setTextColor(const QColor &value) { assign(m_text, value); }
  void setMutedColor(const QColor &value) { assign(m_muted, value); }
  void setAccentColor(const QColor &value) { assign(m_accent, value); }
  void setCodeColor(const QColor &value) { assign(m_code, value); }
  void setCodeBackground(const QColor &value) { assign(m_codeBackground, value); }
  void setQuoteColor(const QColor &value) { assign(m_quote, value); }
  void setDoneColor(const QColor &value) { assign(m_done, value); }
  void setBaseSize(qreal value);
  void setMonoFamily(const QString &value);

Q_SIGNALS:
  void documentChanged();

protected:
  void highlightBlock(const QString &text) override;

private:
  template <typename T> void assign(T &field, const T &value) {
    if (field == value)
      return;
    field = value;
    rehighlight();
  }

  QPointer<QQuickTextDocument> m_document;
  QMetaObject::Connection m_documentDestroyed;
  QColor m_text = QColor(QStringLiteral("#cdd6f4"));
  QColor m_muted = QColor(QStringLiteral("#6c7086"));
  QColor m_accent = QColor(QStringLiteral("#b4befe"));
  QColor m_code = QColor(QStringLiteral("#f9e2af"));
  QColor m_codeBackground = QColor(0, 0, 0, 60);
  QColor m_quote = QColor(QStringLiteral("#a6adc8"));
  QColor m_done = QColor(QStringLiteral("#a6e3a1"));
  qreal m_baseSize = 13;
  QString m_mono;
};
