#include "functions.h"
#include "functions-boundary.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <cmath>

#include <seele-core.h>

namespace {
using SeeleBoundary::bounded;

template<typename Call>
QJSValue invoke(QQmlEngine *engine, QJSValue &jsonParser, const QString &operation,
                const QVariantList &arguments, Call native) {
  if (!engine) return {};
  // Capture this engine's JSON parser once. JSON decoding creates ordinary JS
  // arrays and own data properties (including __proto__), unlike QVariant
  // sequence wrappers or QJsonObject's property assignment conversion.
  if (jsonParser.isUndefined())
    jsonParser = engine->globalObject().property(QStringLiteral("JSON")).property(QStringLiteral("parse"));
  if (!jsonParser.isCallable()) {
    engine->throwError(QStringLiteral("Native JSON conversion is unavailable"));
    return {};
  }
  if (operation.size() > 128 || arguments.size() > 32) {
    if (engine) engine->throwError(QStringLiteral("Native function request exceeds its limit"));
    return {};
  }
  std::size_t bytes = seele_core_max_message() - 1024;
  std::size_t nodes = 256 * 1024;
  if (!bounded(arguments, bytes, nodes, 0)) {
    if (engine) engine->throwError(QStringLiteral("Native function arguments exceed their limit"));
    return {};
  }
  const auto input = QJsonDocument(QJsonObject{
    {QStringLiteral("operation"), operation},
    {QStringLiteral("arguments"), QJsonArray::fromVariantList(arguments)}
  }).toJson(QJsonDocument::Compact);
  const auto result = native(reinterpret_cast<const std::uint8_t *>(input.constData()), input.size());
  const auto release = [](SeeleBytes *bytes) { seele_qml_free(*bytes); };
  SeeleBytes owned = result;
  const auto guard = std::unique_ptr<SeeleBytes, decltype(release)>(&owned, release);
  if (!result.data || result.length == 0 || result.length > seele_core_max_message()) {
    engine->throwError(QStringLiteral("Native function returned an invalid result"));
    return {};
  }
  const auto output = jsonParser.call({QJSValue(QString::fromUtf8(
    reinterpret_cast<const char *>(result.data), static_cast<qsizetype>(result.length)))});
  if (output.isError() || !output.isObject()
      || !output.hasOwnProperty(QStringLiteral("ok"))
      || !output.property(QStringLiteral("ok")).isBool()) {
    engine->throwError(QStringLiteral("Native function returned an invalid result"));
    return {};
  }
  if (!output.property(QStringLiteral("ok")).toBool()) {
    const auto error = output.property(QStringLiteral("error"));
    engine->throwError(error.isString() ? error.toString()
                                      : QStringLiteral("Native function failed"));
    return {};
  }
  if (!output.hasOwnProperty(QStringLiteral("value"))) {
    engine->throwError(QStringLiteral("Native function returned an invalid result"));
    return {};
  }
  return output.property(QStringLiteral("value"));
}

}

QJSValue Functions::call(const QString &operation, const QVariantList &arguments) {
  return invoke(qmlEngine(this), jsonParser_, operation, arguments, seele_qml_call);
}
NotificationPolicy *Functions::notificationState(double now) {
  auto *engine = qmlEngine(this);
  if (!engine) return nullptr;
  if (!std::isfinite(now)) {
    if (engine) engine->throwError(QStringLiteral("Invalid notification timestamp"));
    return nullptr;
  }
  auto *policy = new NotificationPolicy(engine, now);
  QQmlEngine::setObjectOwnership(policy, QQmlEngine::JavaScriptOwnership);
  return policy;
}
NotificationPolicy::NotificationPolicy(QQmlEngine *engine, double now)
  : engine_(engine), state_(seele_notifications_new(now)) {}
NotificationPolicy::~NotificationPolicy() { seele_notifications_free(state_); }
QJSValue NotificationPolicy::call(const QString &operation, const QVariantList &arguments) {
  return invoke(engine_, jsonParser_, operation, arguments, [this](const std::uint8_t *data, std::size_t length) {
    return seele_notifications_call(state_, data, length);
  });
}
