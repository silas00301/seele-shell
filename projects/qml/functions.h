#pragma once
#include <QObject>
#include <QQmlEngine>
#include <QPointer>
#include <QVariant>
#include <QJSValue>

// Each policy owns one opaque Rust state. JavaScript ownership lets Qt collect it
// with its wrapper/engine; notification text never enters a global registry.
class NotificationPolicy : public QObject {
  Q_OBJECT
  QML_ANONYMOUS
public:
  NotificationPolicy(QQmlEngine *engine, double now);
  ~NotificationPolicy() override;
  Q_INVOKABLE QJSValue call(const QString &operation, const QVariantList &arguments);
private:
  QPointer<QQmlEngine> engine_;
  void *state_;
  QJSValue jsonParser_;
};

// Qt owns variant conversion, object lifetime and the QML singleton. Rust owns
// the pure algorithms; no desktop capabilities are exposed by this interface.
class Functions : public QObject {
  Q_OBJECT
  QML_ELEMENT
  QML_SINGLETON
public:
  using QObject::QObject;
  Q_INVOKABLE NotificationPolicy *notificationState(double now);
  Q_INVOKABLE QJSValue call(const QString &operation, const QVariantList &arguments);
private:
  QJSValue jsonParser_;
};
