#include "navigator.h"
#include <QCoreApplication>
#include <QKeyEvent>
#include <algorithm>
#include <cmath>
#include <limits>

namespace {
bool input(QQuickItem *item) {
  return item && item->flags().testFlag(QQuickItem::ItemAcceptsInputMethod)
      && !item->property("readOnly").toBool();
}
QQuickItem *scope(QQuickWindow *window) {
  // A Qt popup is a focus boundary: do not walk its obscured parent controls.
  for (auto *item = window->activeFocusItem(); item; item = item->parentItem()) {
    if (item->inherits("QQuickPopupItem") || item->property("keyboardScope").toBool())
      return item;
  }
  return window->contentItem();
}
void collect(QQuickItem *item, QList<QQuickItem *> &items) {
  if (!item || !item->isVisible() || !item->isEnabled() || item->opacity() <= 0)
    return;
  if (item->clip() && (item->width() <= 0 || item->height() <= 0)) return;
  if (item->width() > 0 && item->height() > 0 && item->activeFocusOnTab())
    items.append(item);
  for (auto *child : item->childItems()) collect(child, items);
}
QRectF rect(QQuickItem *item) {
  return item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
}
QQuickItem *listFor(QQuickItem *item) {
  for (; item; item = item->parentItem())
    if (item->inherits("QQuickListView") || item->inherits("QQuickGridView")) return item;
  return nullptr;
}
QQuickItem *currentDelegate(QQuickItem *list) {
  return qobject_cast<QQuickItem *>(list->property("currentItem").value<QObject *>());
}
void alignList(QQuickItem *list, QQuickItem *current) {
  auto *content = qobject_cast<QQuickItem *>(list->property("contentItem").value<QObject *>());
  if (!current || !content || !content->isAncestorOf(current)) return;
  const auto point = current->mapToItem(content, QPointF(current->width() / 2, current->height() / 2));
  int index = -1;
  QMetaObject::invokeMethod(list, "indexAt", Q_RETURN_ARG(int, index),
                            Q_ARG(qreal, point.x()), Q_ARG(qreal, point.y()));
  if (index >= 0) list->setProperty("currentIndex", index);
}
QQuickItem *scroller(QQuickItem *item) {
  if (!item || !item->isVisible() || !item->isEnabled()) return nullptr;
  if (item->inherits("QQuickFlickable")) return item;
  for (auto *child : item->childItems())
    if (auto *found = scroller(child)) return found;
  return nullptr;
}
}

Navigator::Navigator(QObject *parent) : QObject(parent) {
  QCoreApplication::instance()->installEventFilter(this);
}
Navigator::~Navigator() {
  QCoreApplication::instance()->removeEventFilter(this);
}
void Navigator::indicate(QQuickItem *item) {
  if (focusItem_ == item) return;
  focusItem_ = item;
  emit focusItemChanged();
}
void Navigator::focus(QQuickItem *item) {
  if (!item) return;
  prefix_.invalidate();
  item->forceActiveFocus(Qt::TabFocusReason);
  // Keep focus in view without moving the pointer. QML's scrollables expose
  // contentY/contentHeight; nested panes are brought into view inside-out.
  for (auto *parent = item->parentItem(); parent; parent = parent->parentItem()) {
    if (!parent->inherits("QQuickFlickable")) continue;
    const auto bounds = item->mapRectToItem(parent, QRectF(0, 0, item->width(), item->height()));
    const double top = parent->property("contentY").toDouble();
    const double origin = parent->property("originY").toDouble();
    const double end = origin + std::max(0.0, parent->property("contentHeight").toDouble() - parent->height());
    const double delta = bounds.top() < 0 ? bounds.top()
        : bounds.bottom() > parent->height() ? bounds.bottom() - parent->height() : 0;
    if (delta != 0) parent->setProperty("contentY", std::clamp(top + delta, origin, end));
  }
  indicate(item);
}
void Navigator::focusRow(QQuickItem *list, int column) {
  QMetaObject::invokeMethod(list, "forceLayout");
  QList<QQuickItem *> controls;
  collect(currentDelegate(list), controls);
  if (!controls.isEmpty()) focus(controls[std::min(column, int(controls.size()) - 1)]);
  else focus(list);
}
QList<QQuickItem *> Navigator::targets(QQuickWindow *window) const {
  QList<QQuickItem *> items;
  collect(scope(window), items);
  return items;
}
bool Navigator::move(QQuickWindow *window, int dx, int dy) {
  const auto items = targets(window);
  if (items.isEmpty()) return false;
  auto *current = window->activeFocusItem();
  if (!current || !items.contains(current)) { focus(items.first()); return true; }
  const auto origin = rect(current);
  QQuickItem *best = nullptr;
  double score = std::numeric_limits<double>::infinity();
  for (auto *item : items) {
    if (item == current || item->isAncestorOf(current) || current->isAncestorOf(item)) continue;
    const auto candidate = rect(item);
    const auto difference = candidate.center() - origin.center();
    const double along = dx ? difference.x() * dx : difference.y() * dy;
    if (along < 1) continue;
    const bool aligned = dx ? candidate.top() < origin.bottom() && candidate.bottom() > origin.top()
                            : candidate.left() < origin.right() && candidate.right() > origin.left();
    const double across = std::abs(dx ? difference.y() : difference.x());
    const double value = (aligned ? 0 : window->width() * 4) + along + across * 2;
    if (value < score) { best = item; score = value; }
  }
  if (!best) return false;
  focus(best);
  return true;
}
bool Navigator::forward(QQuickWindow *window, QKeyEvent *event, int key,
                        Qt::KeyboardModifiers modifiers) {
  QKeyEvent mapped(event->type(), key, modifiers, QString(), event->isAutoRepeat(), event->count());
  mapped.setAccepted(false);
  forwarding_ = true;
  QCoreApplication::sendEvent(window, &mapped);
  forwarding_ = false;
  indicate(window->activeFocusItem());
  return mapped.isAccepted();
}
bool Navigator::eventFilter(QObject *object, QEvent *event) {
  if (forwarding_) return false;
  auto *window = qobject_cast<QQuickWindow *>(object);
  if (!window) return false;
  if (event->type() == QEvent::MouseButtonPress) {
    indicate(nullptr); prefix_.invalidate(); return false;
  }
  if (event->type() == QEvent::FocusOut) {
    releases_.clear(); prefix_.invalidate(); indicate(nullptr); return false;
  }
  if (event->type() != QEvent::KeyPress && event->type() != QEvent::KeyRelease
      && event->type() != QEvent::ShortcutOverride) return false;
  auto *key = static_cast<QKeyEvent *>(event);
  if (window_ != window) {
    window_ = window; releases_.clear(); prefix_.invalidate();
  }
  auto *current = window->activeFocusItem();
  const auto modifiers = key->modifiers();
  int dx = key->key() == Qt::Key_H ? -1 : key->key() == Qt::Key_L ? 1 : 0;
  int dy = key->key() == Qt::Key_K ? -1 : key->key() == Qt::Key_J ? 1 : 0;
  const bool editing = input(current);
  if (event->type() == QEvent::ShortcutOverride) {
    if ((modifiers == Qt::AltModifier && (dx || dy))
        || (editing && key->key() == Qt::Key_Escape && modifiers == Qt::NoModifier)) {
      key->accept(); return true;
    }
    return false;
  }
  if (event->type() == QEvent::KeyRelease) {
    if (!releases_.contains(key->key())) return false;
    const int mapped = releases_.value(key->key());
    if (!key->isAutoRepeat()) releases_.remove(key->key());
    if (mapped) forward(window, key, mapped);
    return true;
  }
  // Alt leaves Ctrl editing/application shortcuts intact (Notes uses Ctrl+K
  // for links and Ctrl+L for the library). This chord also exits range controls.
  if (modifiers == Qt::AltModifier && (dx || dy)) {
    move(window, dx, dy); releases_[key->key()] = 0; return true;
  }
  if (modifiers & (Qt::AltModifier | Qt::MetaModifier)) return false;
  if (!editing && modifiers == Qt::ControlModifier && (key->key() == Qt::Key_D || key->key() == Qt::Key_U)) {
    const int direction = key->key() == Qt::Key_D ? 1 : -1;
    if (auto *list = listFor(current)) {
      alignList(list, current);
      const int count = list->property("count").toInt();
      const double row = list->property("contentHeight").toDouble() / std::max(1, count);
      const int step = std::max(1, int(list->height() / std::max(1.0, row) / 2));
      list->setProperty("currentIndex", std::clamp(list->property("currentIndex").toInt() + direction * step, 0, std::max(0, count - 1)));
      focusRow(list);
    } else {
      QQuickItem *view = current;
      while (view && !view->inherits("QQuickFlickable")) view = view->parentItem();
      if (!view) view = scroller(scope(window));
      if (view) {
        const double origin = view->property("originY").toDouble();
        const double end = origin + std::max(0.0, view->property("contentHeight").toDouble() - view->height());
        view->setProperty("contentY", std::clamp(view->property("contentY").toDouble() + direction * view->height() / 2, origin, end));
        auto items = targets(window);
        if (direction < 0) std::reverse(items.begin(), items.end());
        for (auto *item : items) {
          if (view->isAncestorOf(item) && rect(item).intersects(rect(view))) { focus(item); break; }
        }
      }
    }
    releases_[key->key()] = 0; prefix_.invalidate(); return true;
  }
  if (modifiers & Qt::ControlModifier) return false;
  if (key->key() == Qt::Key_Escape && editing) {
    // Give the field's own Keys handler first refusal: a schedule field
    // reverts its draft, a password clears, and completion dismisses itself.
    // Deliver to the item, not the window's application-wide shortcuts.
    QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, modifiers);
    escape.setAccepted(false);
    forwarding_ = true;
    QCoreApplication::sendEvent(current, &escape);
    forwarding_ = false;
    if (!escape.isAccepted()) {
      auto items = targets(window);
      auto found = std::find_if(items.begin(), items.end(), [](auto *item) { return !input(item); });
      focus(found != items.end() ? *found : scope(window));
    }
    releases_[key->key()] = 0; return true;
  }
  if (editing && (key->key() == Qt::Key_Tab || key->key() == Qt::Key_Backtab)) {
    // A TextArea may use Tab for indentation; a field may leave for a custom
    // action. Preserve that choice and show where focus actually ended up.
    forward(window, key, key->key(), modifiers);
    releases_[key->key()] = 0; return true;
  }
  if (editing) return false;
  if (current && current->inherits("QQuickAbstractButton")
      && (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter)) {
    // Qt Quick buttons activate on Space, not Enter. Use their native press /
    // release path so checkable buttons, disabled state and click callbacks
    // retain the exact semantics of keyboard Space.
    if (!key->isAutoRepeat()) forward(window, key, Qt::Key_Space, modifiers);
    releases_[key->key()] = Qt::Key_Space; return true;
  }
  if (key->key() == Qt::Key_Q && modifiers == Qt::NoModifier) {
    if (!key->isAutoRepeat()) forward(window, key, Qt::Key_Escape);
    releases_[key->key()] = 0; return true;
  }
  if (key->key() == Qt::Key_I && modifiers == Qt::NoModifier) {
    for (auto *item : targets(window)) if (input(item)) { focus(item); break; }
    releases_[key->key()] = 0; return true;
  }
  // Slash needs Shift on the configured German layout. Match the resulting
  // symbol while preserving Ctrl/Alt/Meta application shortcuts above.
  if (key->key() == Qt::Key_Slash) {
    for (auto *item : targets(window)) {
      if (item->property("keyboardSearch").toBool()) { focus(item); break; }
    }
    releases_[key->key()] = 0; return true;
  }
  if (key->key() == Qt::Key_G) {
    if (key->isAutoRepeat()) return true;
    if (modifiers == Qt::ShiftModifier || (prefix_.isValid() && prefix_.elapsed() < 800)) {
      if (auto *list = listFor(current)) {
        const bool last = modifiers == Qt::ShiftModifier;
        list->setProperty("currentIndex", last ? list->property("count").toInt() - 1 : 0);
        focusRow(list);
      } else {
        const auto items = targets(window);
        if (!items.isEmpty()) focus(modifiers == Qt::ShiftModifier ? items.last() : items.first());
      }
      prefix_.invalidate();
    } else if (!key->isAutoRepeat()) prefix_.start();
    releases_[key->key()] = 0; return true;
  }
  prefix_.invalidate();
  if (key->key() == Qt::Key_Left) dx = -1;
  if (key->key() == Qt::Key_Right) dx = 1;
  if (key->key() == Qt::Key_Up) dy = -1;
  if (key->key() == Qt::Key_Down) dy = 1;
  if ((dx || dy) && modifiers == Qt::NoModifier) {
    const int mapped = dx < 0 ? Qt::Key_Left : dx > 0 ? Qt::Key_Right : dy < 0 ? Qt::Key_Up : Qt::Key_Down;
    // Existing sliders, editors, lists and panel-specific arrow handlers own
    // their semantics. Only an unhandled arrow becomes spatial navigation.
    QPointer<QQuickItem> list = listFor(current);
    int column = 0;
    if (list) {
      alignList(list, current);
      QList<QQuickItem *> controls;
      collect(currentDelegate(list), controls);
      column = std::max(0, int(controls.indexOf(current)));
    }
    const bool handled = forward(window, key, mapped);
    if (handled && list) {
      // ListView gives focus to its delegate, not a custom action nested in
      // it. Keep activation on the newly selected row, including virtual rows
      // that were not instantiated until the view scrolled.
      focusRow(list, column);
    }
    if (!handled) move(window, dx, dy);
    releases_[key->key()] = handled ? mapped : 0;
    return true;
  }
  // Arrow/Tab paths receive the same focus indication, including native
  // controls whose previous material only reported the pointer.
  if (key->key() == Qt::Key_Tab || key->key() == Qt::Key_Backtab) {
    const auto items = targets(window);
    if (items.isEmpty()) return false;
    const bool back = key->key() == Qt::Key_Backtab || modifiers == Qt::ShiftModifier;
    int index = items.indexOf(current);
    index = index < 0 ? (back ? items.size() - 1 : 0)
                     : (index + (back ? -1 : 1) + items.size()) % items.size();
    focus(items[index]); releases_[key->key()] = 0; return true;
  }
  return false;
}
