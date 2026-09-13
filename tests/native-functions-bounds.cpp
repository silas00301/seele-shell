#include "functions-boundary.h"
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QUrl>
#include <cassert>
#include <iostream>

struct Convertible { };
Q_DECLARE_METATYPE(Convertible)

static bool bounded(const QVariant &value, std::size_t bytes = 4096, std::size_t nodes = 128) {
  return SeeleBoundary::bounded(value, bytes, nodes, 0);
}
int main() {
  const QStringList strings{QStringLiteral("artist"), QStringLiteral("🦀")};
  const QVariantHash hash{{QStringLiteral("artists"), strings}};
  assert(bounded(strings));
  assert(bounded(hash));
  assert(QJsonValue::fromVariant(hash).toObject()[QStringLiteral("artists")].toArray().size() == 2);
  assert(!bounded(QStringList{QString(1000, QLatin1Char('x'))}));
  assert(!bounded(QVariantHash{{QString(1000, QLatin1Char('x')), true}}));
  assert(!bounded(QVariantHash{{QStringLiteral("key"), QString(1000, QLatin1Char('x'))}}));
  assert(!bounded(QVariantList{1, 2, 3}, 4096, 3));
  QVariant nested = QStringLiteral("leaf");
  for (int i = 0; i < 66; ++i) nested = QVariantList{nested};
  assert(!bounded(nested, 100000, 1000));
  QObject object;
  assert(!bounded(QVariant::fromValue(&object)));
  assert(!bounded(QUrl(QStringLiteral("https://example.invalid"))));
  assert(!bounded(QVariant::fromValue(QJsonArray{1, 2})));
  bool converted = false;
  QMetaType::registerConverter<Convertible, QString>([&converted](Convertible) { converted = true; return QString(1000, QLatin1Char('x')); });
  assert(!bounded(QVariant::fromValue(Convertible{})));
  assert(!converted);
  assert(bounded(QVariant{}));
  assert(bounded(QVariantList{true, QVariant{}, 3.5, qint64(42)}));
  std::cout << "PASS native Qt argument bounds: typed lists/maps, depth, bytes, nodes and unsupported converters\n";
}
