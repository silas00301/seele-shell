#pragma once
#include <QElapsedTimer>
#include <QHash>
#include <QPointer>
#include <QQuickItem>
#include <QQuickWindow>
#include <QQmlEngine>

// Qt input/focus adaptation only. No global shortcuts, desktop input injection,
// service access, timers or persisted state. One instance per application.
class Navigator : public QObject {
  Q_OBJECT
  QML_ELEMENT
  Q_PROPERTY(QQuickItem *focusItem READ focusItem NOTIFY focusItemChanged)
public:
  explicit Navigator(QObject *parent = nullptr);
  ~Navigator() override;
  QQuickItem *focusItem() const { return focusItem_; }
signals:
  void focusItemChanged();
protected:
  bool eventFilter(QObject *object, QEvent *event) override;
private:
  bool forwarding_ = false;
  QPointer<QQuickItem> focusItem_;
  QPointer<QQuickWindow> window_;
  QHash<int, int> releases_;
  QElapsedTimer prefix_;
  void indicate(QQuickItem *item);
  void focus(QQuickItem *item);
  void focusRow(QQuickItem *list, int column = 0);
  QList<QQuickItem *> targets(QQuickWindow *window) const;
  bool move(QQuickWindow *window, int dx, int dy);
  bool forward(QQuickWindow *window, QKeyEvent *event, int key,
               Qt::KeyboardModifiers modifiers = Qt::NoModifier);
};
