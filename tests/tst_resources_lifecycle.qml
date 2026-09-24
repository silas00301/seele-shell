import QtQuick
import QtTest
Item {
  width: 100; height: 100
  ResourcesStore { id: store }
  TestCase {
    name: "ResourcesLifecycle"; when: windowShown
    function cleanup() { store.panelOpen=false; wait(20) }
    function test_open_close_reopen_creates_new_generation() {
      store.panelOpen=true;
      tryVerify(function(){return store.session.item && store.session.item.running});
      var generation=store.session.item.sessionGeneration;
      verify(store.session.item.currentSession);
      store.panelOpen=false;
      compare(store.session.item,null);
      compare(store.snapshot.cpuHistory.length,0);compare(store.model.count,0);
      store.panelOpen=true;
      tryVerify(function(){return store.session.item && store.session.item.running});
      verify(store.session.item.sessionGeneration>generation);
      verify(store.session.item.currentSession);
    }
    function test_old_generation_callbacks_are_ignored() {
      store.panelOpen=true;
      tryVerify(function(){return store.session.item && store.session.item.running});
      var worker=store.session.item;
      var generation=worker.sessionGeneration;
      worker.sessionGeneration=generation-1;
      store.snapshot={version:1,type:"snapshot",rows:[],total:17,cpuHistory:[]};
      worker.stdout.read(JSON.stringify({version:1,type:"snapshot",rows:[],total:999,cpuHistory:[99]}));
      compare(store.snapshot.total,17);
      worker.exited(1,0);
      compare(store.snapshot.total,17);compare(store.error,"");verify(!store.retry.running);
      worker.sessionGeneration=generation;
      worker.stdout.read(JSON.stringify({version:1,type:"snapshot",rows:[],total:23,cpuHistory:[]}));
      compare(store.snapshot.total,23);
    }
    function test_current_worker_retry_preserves_selection() {
      store.panelOpen=true;
      tryVerify(function(){return store.session.item && store.session.item.running});
      store.selected="42:123";
      store.session.item.exited(1,0);
      compare(store.selected,"42:123");
      verify(store.retry.running);
    }
  }
}
