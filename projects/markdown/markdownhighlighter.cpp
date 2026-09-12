#include "markdownhighlighter.h"

#include <QQuickTextDocument>
#include "markdown-core.h"
#include <QTextCursor>
#include <QTextBlock>
#include <QTextDocument>

void MarkdownEdit::replace(QQuickTextDocument *document, int start, int end,
                           const QString &text) {
  if (!document || !document->textDocument())
    return;
  QTextDocument *target = document->textDocument();
  const int length = target->characterCount() - 1;
  start = qBound(0, start, length);
  end = qBound(start, end, length);
  if (start == end && text.isEmpty())
    return;
  QTextCursor cursor(target);
  cursor.setPosition(start);
  cursor.setPosition(end, QTextCursor::KeepAnchor);
  cursor.beginEditBlock();
  cursor.removeSelectedText();
  if (!text.isEmpty())
    cursor.insertText(text);
  cursor.endEditBlock();
}

MarkdownHighlighter::MarkdownHighlighter(QObject *parent) : QSyntaxHighlighter(parent) {}

void MarkdownHighlighter::setDocument(QQuickTextDocument *document) {
  if (m_document == document)
    return;
  QObject::disconnect(m_documentDestroyed);
  m_document = document;
  if (document)
    m_documentDestroyed = connect(document, &QObject::destroyed, this, [this] { Q_EMIT documentChanged(); });
  QSyntaxHighlighter::setDocument(document ? document->textDocument() : nullptr);
  Q_EMIT documentChanged();
}

void MarkdownHighlighter::setBaseSize(qreal value) {
  if (qFuzzyCompare(m_baseSize, value))
    return;
  m_baseSize = value;
  rehighlight();
}

void MarkdownHighlighter::setMonoFamily(const QString &value) {
  if (m_mono == value)
    return;
  m_mono = value;
  rehighlight();
}

// Qt alone owns QObject, document editing, theme values and layout. The Rust
// library returns ordered UTF-16 format operations and never rewrites source.
void MarkdownHighlighter::highlightBlock(const QString &text) {
  QTextCharFormat body;
  body.setForeground(m_text);
  setFormat(0, text.size(), body);
  const auto result = seele_markdown_highlight(text.utf16(), text.size(),
                                              previousBlockState(),
                                              currentBlock().blockNumber() == 0);
  constexpr qreal headingScale[] = {1.55, 1.35, 1.20, 1.10, 1.05, 1.0};
  const QColor colors[] = {m_text, m_muted, m_accent, m_code, m_quote, m_done};
  for (std::size_t index = 0; index < result.length; ++index) {
    const auto &span = result.spans[index];
    // The ABI only receives our own in-process parser's allocation. These
    // checks also contain a future version mismatch to an unformatted range.
    if (span.start > static_cast<std::uint32_t>(text.size()) ||
        span.length > static_cast<std::uint32_t>(text.size()) - span.start)
      continue;
    QTextCharFormat style = span.flags & SeeleInherit ? format(span.start) : QTextCharFormat();
    if (span.color < 6)
      style.setForeground(colors[span.color]);
    if (span.flags & SeeleBold)
      style.setFontWeight(QFont::DemiBold);
    if (span.flags & SeeleItalic)
      style.setFontItalic(true);
    if (span.flags & SeeleStrike)
      style.setFontStrikeOut(true);
    if (span.flags & SeeleUnderline)
      style.setFontUnderline(true);
    if ((span.flags & SeeleMono) && !m_mono.isEmpty())
      style.setFontFamilies({m_mono});
    if (span.flags & SeeleBackground)
      style.setBackground(m_codeBackground);
    if (span.heading > 0 && span.heading <= 6)
      style.setProperty(QTextFormat::FontPixelSize, m_baseSize * headingScale[span.heading - 1]);
    setFormat(span.start, span.length, style);
  }
  setCurrentBlockState(result.state);
  seele_markdown_free(result);
}
