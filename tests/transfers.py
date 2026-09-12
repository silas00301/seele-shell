#!/usr/bin/env python3
import contextlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import socketserver
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("transfers", sys.argv.pop(1))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class FakeProvider:
    capabilities = m.TaildropProvider.capabilities
    def __init__(self):
        self.available = True
        self.sent = []
        self.failures = 0
        self.pending = []
        self.hold = None
    def targets(self):
        return [{"id": "personal-phone", "name": "Phone"}] if self.available else []
    def send(self, target, path, control):
        if self.hold:
            self.hold.set()
            while not control.cancelled.wait(.01):
                pass
            control.check()
        if self.failures:
            self.failures -= 1
            raise m.Failure("interrupted")
        self.sent.append((target, path.name, path.read_bytes()))
    def waiting(self):
        return self.pending


class TransfersTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.provider = FakeProvider()
        self.notifications = []
        self.manager = m.Transfers(self.provider, self.root, self.root / "state/history.json", self.notifications.append)
    def tearDown(self):
        self.temp.cleanup()
    def file(self, name="a.txt", data=b"original\x00bytes"):
        p = self.root / name
        p.write_bytes(data)
        return str(p)
    def send(self, paths):
        self.manager.dispatch({"op": "select", "paths": paths})
        return self.manager.dispatch({"op": "send", "target": "personal-phone"})["id"]
    def wait(self, identity):
        deadline = time.monotonic() + 8
        while identity in self.manager.controls and time.monotonic() < deadline:
            time.sleep(.01)
        self.assertNotIn(identity, self.manager.controls)
        return self.manager.group(identity)
    def test_group_original_files_dedup_and_no_outgoing_success_notification(self):
        a, b = self.file(), self.file("space and\nnewline.txt", b"second")
        identity = self.send([a, b, a])
        with self.assertRaises(m.Failure):
            self.manager.dispatch({"op": "send", "target": "personal-phone"})
        group = self.wait(identity)
        self.assertEqual(group["state"], "completed")
        self.assertEqual(self.provider.sent, [("personal-phone", "a.txt", b"original\x00bytes"), ("personal-phone", "space and\nnewline.txt", b"second")])
        self.assertEqual(self.notifications, [])
        self.assertNotIn("path", self.manager.snapshot()["groups"][0]["files"][0])
        self.assertNotIn("original", self.manager.state.read_text())
        self.assertEqual(self.manager.state.stat().st_mode & 0o777, 0o600)
    def test_offline_and_directories_rejected_without_consuming_selection(self):
        self.manager.dispatch({"op": "select", "paths": [self.file()]})
        self.provider.available = False
        with self.assertRaisesRegex(m.Failure, "target-unavailable"):
            self.manager.dispatch({"op": "send", "target": "personal-phone"})
        self.assertEqual(len(self.manager.selection), 1)
        with self.assertRaisesRegex(m.Failure, "not-a-file"):
            self.manager.dispatch({"op": "select", "paths": [str(self.root)]})
    def test_cancel_never_removes_source_and_retry_keeps_identity(self):
        source = self.file()
        self.provider.hold = threading.Event()
        identity = self.send([source])
        self.assertTrue(self.provider.hold.wait(2))
        self.manager.dispatch({"op": "cancel", "id": identity})
        self.assertEqual(self.wait(identity)["state"], "cancelled")
        self.assertTrue(Path(source).exists())
        self.provider.hold = None
        self.manager.dispatch({"op": "retry", "id": identity})
        self.assertEqual(self.wait(identity)["state"], "completed")
    def test_retry_bound_failure_and_missing_source(self):
        self.provider.failures = 10
        identity = self.send([self.file()])
        group = self.wait(identity)
        self.assertEqual(group["state"], "failed")
        self.assertEqual(group["files"][0]["attempts"], 3)
        self.assertEqual(len(self.notifications), 1)
        Path(group["files"][0]["path"]).unlink()
        self.manager.dispatch({"op": "retry", "id": identity})
        self.assertEqual(self.wait(identity)["error"], "source-missing")
    def test_progress_uses_daemon_bytes_not_upload_to_local_socket(self):
        self.provider.hold = threading.Event()
        identity = self.send([self.file()])
        self.provider.hold.wait(2)
        self.manager.event({"outgoing": [{"target": "personal-phone", "name": "a.txt", "bytes": 5}]})
        self.assertEqual(self.manager.group(identity)["files"][0]["bytes"], 5)
        self.manager.event({"outgoing": [{"target": "personal-phone", "name": "a.txt", "bytes": 100, "finished": True}]})
        self.assertEqual(self.manager.group(identity)["files"][0]["bytes"], 5)
        self.manager.dispatch({"op": "cancel", "id": identity})
        self.wait(identity)
    def test_history_expiry_keeps_failures_and_clear_keeps_received_file(self):
        path = self.file()
        group = self.manager.new("incoming", "Personal device", [{"name": "a.txt", "path": path, "size": 1, "bytes": 1, "state": "completed"}])
        group.update(state="completed")
        self.manager.dispatch({"op": "dismiss", "id": group["id"]})
        self.assertTrue(Path(path).exists())
        for state in ("completed", "failed", "sending"):
            g = self.manager.new("outgoing", "Phone", [])
            g.update(state=state, updated=time.time() - 8 * 86400)
        self.manager.prune()
        self.assertEqual({g["state"] for g in self.manager.groups}, {"failed", "sending"})
    def test_receive_ack_recovery_and_repeated_filename(self):
        notifications = self.notifications
        provider = self.provider
        provider.pending = [{"name": "a.txt", "size": 8}]
        acknowledgements = []
        fail_ack = [True]
        def receive(name, directory, control, progress):
            path, fd = m.reserve(directory, name)
            with os.fdopen(fd, "wb") as out:
                out.write(b"received")
            progress(8, 8)
            return str(path), 8
        def forget(name):
            if fail_ack[0]:
                fail_ack[0] = False
                raise m.Failure("provider-unavailable")
            acknowledgements.append(name)
            provider.pending = []
        provider.receive, provider.forget = receive, forget
        self.manager.receive_waiting()
        identity = self.manager.groups[0]["id"]
        group = self.wait(identity)
        self.assertEqual(group["state"], "failed")
        self.assertTrue(group["files"][0]["pendingAck"])
        self.assertEqual(group["files"][0]["state"], "completed")
        self.assertEqual((self.root / "a.txt").read_bytes(), b"received")
        self.manager.receive_waiting()
        self.assertEqual(acknowledgements, ["a.txt"])
        self.assertEqual(group["state"], "completed")
        self.assertFalse((self.root / "a (1).txt").exists(), "ack retry must not copy again")
        provider.pending = [{"name": "a.txt", "size": 8}]
        self.manager.receive_waiting()
        next_id = self.manager.groups[0]["id"]
        self.assertNotEqual(identity, next_id)
        self.wait(next_id)
        self.assertEqual((self.root / "a (1).txt").read_bytes(), b"received")
        self.assertEqual(len(notifications), 3)

    def test_move_collision_and_trash_are_explicit_actions(self):
        source = self.file()
        target = self.root / "destination"
        target.mkdir()
        (target / "a.txt").write_text("existing")
        group = self.manager.new("incoming", "Personal device", [{"name":"a.txt", "path":source,"state":"completed","size":14,"bytes":14}])
        group["state"] = "completed"
        self.manager.dispatch({"op":"move","id":group["id"],"file":0,"directory":str(target)})
        self.assertEqual(Path(group["files"][0]["path"]).name, "a (1).txt")
        self.assertEqual((target / "a.txt").read_text(), "existing")
        self.assertFalse(Path(source).exists())
        with patch.object(m.subprocess, "run") as run:
            run.return_value.returncode = 0
            self.manager.dispatch({"op":"trash","id":group["id"],"file":0})
            self.assertEqual(run.call_args.args[0][:3], ["gio", "trash", "--"])
        self.assertEqual(group["files"][0]["state"], "trashed")


class UnixServer(socketserver.ThreadingUnixStreamServer):
    daemon_threads = True


class LocalAPITest(unittest.TestCase):
    def test_real_local_http_target_filter_stream_receive_and_collision(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = b"unchanged\x00original" * 60000
            sent, deleted = [], []
            class Handler(http.server.BaseHTTPRequestHandler):
                def log_message(self, *args): pass
                def reply(self, value, status=200):
                    body = value if isinstance(value, bytes) else json.dumps(value).encode()
                    self.send_response(status)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                def do_GET(self):
                    if self.path.endswith("/status"):
                        self.reply({"Self":{"UserID":7}})
                    elif self.path.endswith("/file-targets"):
                        self.reply([{"Node": {"StableID":"same","ComputedName":"Phone","User":7,"Online":True}},
                                    {"Node": {"StableID":"other","User":8,"Online":True}},
                                    {"Node": {"StableID":"offline","User":7,"Online":False}},
                                    {"Node": {"StableID":"unknown","User":7}}])
                    elif self.path.endswith("/files/a.txt"):
                        self.reply(payload)
                    else: self.reply([])
                def do_PUT(self):
                    sent.append((self.path, self.rfile.read(int(self.headers["Content-Length"]))))
                    self.reply(b"")
                def do_DELETE(self):
                    deleted.append(self.path)
                    self.reply(b"", 204)
            with UnixServer(str(root / "daemon.sock"), Handler) as server:
                worker = threading.Thread(target=server.serve_forever, daemon=True)
                worker.start()
                provider = m.TaildropProvider(str(root / "daemon.sock"))
                self.assertEqual(provider.targets(), [{"id":"same","name":"Phone"}])
                source = root / "original #.txt"
                source.write_bytes(payload)
                provider.send("same", source, m.Control())
                self.assertEqual(sent, [("/localapi/v0/file-put/same/original%20%23.txt", payload)])
                (root / "a.txt").write_bytes(b"existing")
                (root / "a (1).txt").symlink_to(source)
                progress = []
                path, size = provider.receive("a.txt", root, m.Control(), lambda a,b: progress.append((a,b)))
                self.assertEqual(Path(path).name, "a (2).txt")
                self.assertEqual(Path(path).read_bytes(), payload)
                self.assertEqual(Path(path).stat().st_mode & 0o777, 0o600)
                self.assertEqual(size, len(payload))
                self.assertEqual(progress[-1], (len(payload), len(payload)))
                self.assertEqual((root / "a.txt").read_bytes(), b"existing")
                self.assertEqual(deleted, [])
                provider.forget("a.txt")
                self.assertEqual(deleted, ["/localapi/v0/files/a.txt"])
                for bad in ("../escape", "/absolute", "a/b", "..", "a\\b"):
                    with self.assertRaises(m.Failure):
                        m.reserve(root, bad)
                server.shutdown()

if __name__ == "__main__":
    unittest.main()
