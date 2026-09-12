#pragma once
#include <QVariant>
#include <QStringList>
#include <cstddef>

namespace SeeleBoundary {
// Inspect only the JSON-compatible types accepted by this bridge. Qt's generic
// converter can expand QStringList, hashes, URLs and custom metatypes; accepting
// an uninspected type would bypass the allocation budget before Rust sees it.
inline bool bounded(const QVariant &value, std::size_t &bytes, std::size_t &nodes, int depth) {
  if (depth > 64 || nodes == 0 || bytes < 32) return false;
  --nodes;
  bytes -= 32;
  const auto chargeString = [&bytes](const QString &text) {
    const auto units = static_cast<std::size_t>(text.size());
    // Six bytes per UTF-16 unit bounds JSON escaping without overflowing.
    if (units > bytes / 6) return false;
    bytes -= units * 6;
    return true;
  };
  switch (value.metaType().id()) {
  case QMetaType::UnknownType:
  case QMetaType::Nullptr:
  case QMetaType::Bool:
  case QMetaType::Int:
  case QMetaType::UInt:
  case QMetaType::LongLong:
  case QMetaType::ULongLong:
  case QMetaType::Float:
  case QMetaType::Double:
    return true;
  case QMetaType::QString:
    return chargeString(value.toString());
  case QMetaType::QStringList: {
    const auto values = value.toStringList();
    for (const auto &child : values)
      if (!bounded(child, bytes, nodes, depth + 1)) return false;
    return true;
  }
  case QMetaType::QVariantList: {
    const auto values = value.toList();
    for (const auto &child : values)
      if (!bounded(child, bytes, nodes, depth + 1)) return false;
    return true;
  }
  case QMetaType::QVariantMap: {
    const auto values = value.toMap();
    for (auto it = values.cbegin(); it != values.cend(); ++it)
      if (!chargeString(it.key()) || !bounded(it.value(), bytes, nodes, depth + 1)) return false;
    return true;
  }
  case QMetaType::QVariantHash: {
    const auto values = value.toHash();
    for (auto it = values.cbegin(); it != values.cend(); ++it)
      if (!chargeString(it.key()) || !bounded(it.value(), bytes, nodes, depth + 1)) return false;
    return true;
  }
  default:
    return false;
  }
}
}
