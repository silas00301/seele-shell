#include "markdownhighlighter.h"

#include <QQuickTextDocument>
#include <QRegularExpression>
#include <QTextCursor>
#include <QTextDocument>

namespace {
// Headings step the way the shell's type ramp does: a level is a multiple of
// the editor's own size rather than a pixel count decided here.
constexpr qreal headingScale[6] = {1.55, 1.35, 1.20, 1.10, 1.05, 1.0};

int headingLevel(const QString &text) {
  int hashes = 0;
  while (hashes < text.size() && text.at(hashes) == QLatin1Char('#'))
    ++hashes;
  if (hashes == 0 || hashes > 6)
    return 0;
  if (hashes < text.size() && !text.at(hashes).isSpace())
    return 0;
  return hashes;
}

bool isFence(const QString &text) {
  const QString trimmed = text.trimmed();
  return trimmed.startsWith(QLatin1String("```")) || trimmed.startsWith(QLatin1String("~~~"));
}

bool isRule(const QString &text) {
  const QString trimmed = text.trimmed();
  if (trimmed.size() < 3)
    return false;
  const QChar first = trimmed.at(0);
  if (first != QLatin1Char('-') && first != QLatin1Char('*') && first != QLatin1Char('_'))
    return false;
  for (const QChar character : trimmed)
    if (character != first && !character.isSpace())
      return false;
  return true;
}
} // namespace

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
  m_document = document;
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

// A syntax character is dimmed rather than hidden. It stays selectable,
// countable and directly editable at the caret, which is what makes the source
// and the rendering the same object instead of two views that can disagree.
void MarkdownHighlighter::marker(int start, int length) {
  if (length <= 0)
    return;
  QTextCharFormat format;
  format.setForeground(m_muted);
  setFormat(start, length, format);
}

bool MarkdownHighlighter::claimed(int start, int length) const {
  for (const auto &claim : m_claims)
    if (start < claim.first + claim.second && claim.first < start + length)
      return true;
  return false;
}

void MarkdownHighlighter::claim(int start, int length) { m_claims.append({start, length}); }

// Code is resolved first and claims its range, so a backtick span is never
// re-read as emphasis and `**` inside code keeps its asterisks.
void MarkdownHighlighter::codeSpans(const QString &text, int from) {
  static const QRegularExpression pattern(QStringLiteral("(`+)([^`]|[^`].*?[^`])\\1(?!`)"));
  auto matches = pattern.globalMatch(text, from);
  while (matches.hasNext()) {
    const auto match = matches.next();
    const int start = match.capturedStart();
    const int length = match.capturedLength();
    const int fence = match.captured(1).size();
    QTextCharFormat format;
    format.setForeground(m_code);
    format.setBackground(m_codeBackground);
    if (!m_mono.isEmpty())
      format.setFontFamilies({m_mono});
    setFormat(start, length, format);
    marker(start, fence);
    marker(start + length - fence, fence);
    claim(start, length);
  }
}

void MarkdownHighlighter::inlineSpans(const QString &text, int from) {
  codeSpans(text, from);

  // An embed, a wikilink and a Markdown link all read as one target with its
  // punctuation quietened, so a note full of links still reads as prose.
  static const QRegularExpression wikilink(QStringLiteral("(!?)\\[\\[([^\\]]*)\\]\\]"));
  auto wikilinks = wikilink.globalMatch(text, from);
  while (wikilinks.hasNext()) {
    const auto match = wikilinks.next();
    if (claimed(match.capturedStart(), match.capturedLength()))
      continue;
    QTextCharFormat format;
    format.setForeground(m_accent);
    setFormat(match.capturedStart(), match.capturedLength(), format);
    marker(match.capturedStart(), match.captured(1).size() + 2);
    marker(match.capturedEnd() - 2, 2);
    claim(match.capturedStart(), match.capturedLength());
  }

  static const QRegularExpression link(QStringLiteral("(!?)\\[([^\\]]*)\\]\\(([^)]*)\\)"));
  auto links = link.globalMatch(text, from);
  while (links.hasNext()) {
    const auto match = links.next();
    if (claimed(match.capturedStart(), match.capturedLength()))
      continue;
    QTextCharFormat label;
    label.setForeground(m_accent);
    setFormat(match.capturedStart(2), match.capturedLength(2), label);
    marker(match.capturedStart(), match.captured(1).size() + 1);
    marker(match.capturedEnd(2), match.capturedEnd() - match.capturedEnd(2));
    claim(match.capturedStart(), match.capturedLength());
  }

  static const QRegularExpression autolink(
      QStringLiteral("<[a-zA-Z][a-zA-Z0-9+.-]*:[^>\\s]+>|\\bhttps?://[^\\s<>\\)\\]]+"));
  auto autolinks = autolink.globalMatch(text, from);
  while (autolinks.hasNext()) {
    const auto match = autolinks.next();
    if (claimed(match.capturedStart(), match.capturedLength()))
      continue;
    QTextCharFormat format;
    format.setForeground(m_accent);
    format.setFontUnderline(true);
    setFormat(match.capturedStart(), match.capturedLength(), format);
    claim(match.capturedStart(), match.capturedLength());
  }

  struct Emphasis {
    const char *pattern;
    bool bold;
    bool italic;
    bool strike;
  };
  // Longest run first, so `***word***` is not eaten by the single-marker rule.
  static const Emphasis emphases[] = {
      {"(\\*\\*\\*|___)(?=\\S)(.+?)(?<=\\S)\\1", true, true, false},
      {"(\\*\\*|__)(?=\\S)(.+?)(?<=\\S)\\1", true, false, false},
      {"(~~)(?=\\S)(.+?)(?<=\\S)\\1", false, false, true},
      {"(?<![\\w*])(\\*|_)(?=\\S)(.+?)(?<=\\S)\\1(?![\\w*])", false, true, false},
  };
  for (const auto &emphasis : emphases) {
    const QRegularExpression pattern(QString::fromLatin1(emphasis.pattern));
    auto matches = pattern.globalMatch(text, from);
    while (matches.hasNext()) {
      const auto match = matches.next();
      if (claimed(match.capturedStart(), match.capturedLength()))
        continue;
      QTextCharFormat format = this->format(match.capturedStart(2));
      if (emphasis.bold)
        format.setFontWeight(QFont::DemiBold);
      if (emphasis.italic)
        format.setFontItalic(true);
      if (emphasis.strike) {
        format.setFontStrikeOut(true);
        format.setForeground(m_muted);
      }
      setFormat(match.capturedStart(2), match.capturedLength(2), format);
      marker(match.capturedStart(), match.capturedLength(1));
      marker(match.capturedEnd(2), match.capturedLength(1));
      claim(match.capturedStart(), match.capturedLength());
    }
  }
}

void MarkdownHighlighter::highlightBlock(const QString &text) {
  m_claims.clear();
  const int previous = previousBlockState() < 0 ? Plain : previousBlockState();

  QTextCharFormat body;
  body.setForeground(m_text);
  setFormat(0, text.size(), body);

  // A YAML block only counts as frontmatter when it opens the file, so a rule
  // in the middle of a note is still a rule.
  if (previous == Plain && currentBlock().blockNumber() == 0 &&
      text.trimmed() == QLatin1String("---")) {
    marker(0, text.size());
    setCurrentBlockState(FrontMatter);
    return;
  }
  if (previous == FrontMatter) {
    QTextCharFormat format;
    format.setForeground(m_muted);
    if (!m_mono.isEmpty())
      format.setFontFamilies({m_mono});
    setFormat(0, text.size(), format);
    const QString trimmed = text.trimmed();
    setCurrentBlockState(trimmed == QLatin1String("---") || trimmed == QLatin1String("...")
                             ? Plain
                             : FrontMatter);
    return;
  }

  if (isFence(text)) {
    QTextCharFormat format;
    format.setForeground(m_muted);
    format.setBackground(m_codeBackground);
    if (!m_mono.isEmpty())
      format.setFontFamilies({m_mono});
    setFormat(0, text.size(), format);
    setCurrentBlockState(previous == Fenced ? Plain : Fenced);
    return;
  }
  if (previous == Fenced) {
    QTextCharFormat format;
    format.setForeground(m_code);
    format.setBackground(m_codeBackground);
    if (!m_mono.isEmpty())
      format.setFontFamilies({m_mono});
    setFormat(0, text.size(), format);
    setCurrentBlockState(Fenced);
    return;
  }
  setCurrentBlockState(Plain);
  if (text.isEmpty())
    return;
  if (isRule(text)) {
    marker(0, text.size());
    return;
  }

  int cursor = 0;
  while (cursor < text.size() && text.at(cursor).isSpace())
    ++cursor;
  if (cursor == text.size())
    return;

  if (const int level = headingLevel(QStringView(text).mid(cursor).toString()); level > 0) {
    QTextCharFormat format;
    format.setForeground(m_text);
    format.setFontWeight(QFont::DemiBold);
    // Pixels, because the editor's own font is set in pixels; a point size
    // here would silently override it and resize every heading by the
    // display's DPI instead of by the level.
    format.setProperty(QTextFormat::FontPixelSize, m_baseSize * headingScale[level - 1]);
    setFormat(cursor, text.size() - cursor, format);
    QTextCharFormat hashes = format;
    hashes.setForeground(m_muted);
    setFormat(cursor, level, hashes);
    inlineSpans(text, cursor + level);
    return;
  }

  if (text.at(cursor) == QLatin1Char('>')) {
    QTextCharFormat format;
    format.setForeground(m_quote);
    format.setFontItalic(true);
    setFormat(cursor, text.size() - cursor, format);
    marker(cursor, 1);
    inlineSpans(text, cursor + 1);
    return;
  }

  // A list marker and a task box are the two things a capture is most often
  // made of, so both are lit rather than left as punctuation.
  static const QRegularExpression bullet(QStringLiteral("^(\\s*)([-*+]|\\d+[.)])\\s"));
  const auto marked = bullet.match(text);
  int after = cursor;
  if (marked.hasMatch()) {
    QTextCharFormat format;
    format.setForeground(m_accent);
    setFormat(marked.capturedStart(2), marked.capturedLength(2), format);
    after = marked.capturedEnd();
    // Anchored at the offset after the bullet rather than at the start of the
    // line: `^` would still mean position zero here and the box would never
    // be found.
    static const QRegularExpression box(QStringLiteral("\\[([ xX])\\]\\s"));
    const auto task = box.match(text, after,
                                QRegularExpression::NormalMatch,
                                QRegularExpression::AnchorAtOffsetMatchOption);
    if (task.hasMatch()) {
      const bool done = task.captured(1) != QLatin1String(" ");
      QTextCharFormat check;
      check.setForeground(done ? m_done : m_muted);
      check.setFontWeight(QFont::DemiBold);
      setFormat(task.capturedStart(), task.capturedLength(), check);
      after = task.capturedEnd();
      if (done) {
        QTextCharFormat finished;
        finished.setForeground(m_muted);
        finished.setFontStrikeOut(true);
        setFormat(after, text.size() - after, finished);
      }
    }
  }
  inlineSpans(text, after);
}
