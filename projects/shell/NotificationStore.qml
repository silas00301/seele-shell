import QtQuick
import Quickshell
import Quickshell.Services.Notifications
import "notifications.js" as Notifications

Item {
  id: store

  property bool restored: false
  property var controller: Notifications.createStore(
    function(view, dnd) {
      if (store.restored) retained.saved = store.controller.save()
      store.published(view, dnd)
    },
    function(entry, fresh) { store.arrived(entry, fresh) }, Date.now() / 1000)
  signal published(var view, bool dnd)
  signal arrived(var entry, bool fresh)

  PersistentProperties {
    id: retained
    reloadableId: "seele-notifications"
    property var saved: ({})
    onLoaded: {
      store.controller.restore(saved)
      store.restored = true
    }
  }

  NotificationServer {
    id: server
    keepOnReload: true
    actionsSupported: true
    actionIconsSupported: true
    persistenceSupported: true
    bodySupported: true
    bodyMarkupSupported: true
    bodyHyperlinksSupported: true
    imageSupported: true
    // Replies use the app's action, as chosen by the user.
    inlineReplySupported: false
    bodyImagesSupported: false
    extraHints: ["x-dunst-stack-tag", "x-canonical-private-synchronous"]
    onNotification: notification => store.receive(notification)
  }

  function receive(notification) {
    notification.tracked = true
    var id = notification.id
    var update = function() {
      if (store.controller.find(id)) store.controller.receive(notification, Date.now() / 1000)
    }
    var queue = function() { Qt.callLater(update) }
    // A replacement updates the existing object without emitting notification.
    notification.summaryChanged.connect(queue)
    notification.bodyChanged.connect(queue)
    notification.actionsChanged.connect(queue)
    notification.hintsChanged.connect(queue)
    notification.imageChanged.connect(queue)
    notification.urgencyChanged.connect(queue)
    notification.residentChanged.connect(queue)
    notification.transientChanged.connect(queue)
    notification.expireTimeoutChanged.connect(queue)
    notification.appNameChanged.connect(queue)
    notification.appIconChanged.connect(queue)
    notification.desktopEntryChanged.connect(queue)
    notification.closed.connect(function(reason) { store.controller.closed(id, Number(reason)) })
    store.controller.advance(Date.now() / 1000)
    store.controller.receive(notification, Date.now() / 1000)
  }

  Timer {
    interval: 250
    running: true
    repeat: true
    onTriggered: store.controller.advance(Date.now() / 1000)
  }
}
