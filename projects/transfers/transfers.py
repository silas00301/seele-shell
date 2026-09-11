#!/usr/bin/env python3
"""Private transfer metadata service; only TaildropProvider speaks the LocalAPI."""
import argparse
import contextlib
import copy
import http.client
import json
import os
from pathlib import Path
import socket
import socketserver
import signal
import stat
import subprocess
import sys
import threading
import time
import urllib.parse
import uuid

CHUNK = 256 * 1024
WEEK = 7 * 86400
ACTIVE = {"sending", "receiving", "retrying"}


class Failure(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def regular(path):
    try:
        path = Path(path).expanduser().resolve(strict=True)
        if not path.is_file():
            raise Failure("not-a-file")
        return path
    except (OSError, ValueError):
        raise Failure("source-missing") from None


def filename(value):
    if not isinstance(value, str) or not value or value in (".", "..") or any(c in value for c in "/\\\x00"):
        raise Failure("invalid-filename")
    return value


def reserve(directory, name):
    """Atomic O_EXCL numbered reservation; symlinks never replace existing files."""
    directory = Path(directory).resolve(strict=True)
    name = filename(name)
    stem, suffix = os.path.splitext(name)
    for n in range(100000):
        path = directory / (name if n == 0 else f"{stem} ({n}){suffix}")
        try:
            return path, os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        except FileExistsError:
            continue
    raise Failure("destination-full")


class LocalConnection(http.client.HTTPConnection):
    def __init__(self, path, timeout=30):
        super().__init__("local-tailscaled.sock", timeout=timeout)
        self.path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


class TaildropProvider:
    capabilities = {"send": True, "receive": True, "resume": "provider-managed", "cancelSend": True,
                    "cancelRemoteReceive": False, "incomingSourceIdentity": False}

    def __init__(self, path="/var/run/tailscale/tailscaled.sock"):
        self.path = path

    def connection(self, timeout=30):
        return LocalConnection(self.path, timeout)

    def request(self, method, route):
        with contextlib.closing(self.connection()) as conn:
            conn.request(method, "/localapi/v0/" + route)
            response = conn.getresponse()
            if response.status not in (200, 204):
                raise Failure("provider-unavailable")
            body = response.read(2 * 1024 * 1024)
            return json.loads(body) if body else None

    def targets(self):
        status = self.request("GET", "status")
        owner = (status.get("Self") or {}).get("UserID")
        if owner is None:
            return []
        result = []
        for entry in self.request("GET", "file-targets") or []:
            node = entry.get("Node") or {}
            if node.get("User") == owner and node.get("Online") is True and node.get("StableID"):
                result.append({"id": node["StableID"], "name": node.get("ComputedName") or node.get("Name", "Personal device").rstrip(".")})
        return result

    def waiting(self):
        return [{"name": f["Name"], "size": f["Size"]} for f in self.request("GET", "files/") or []]

    def forget(self, name):
        self.request("DELETE", "files/" + urllib.parse.quote(name, safe=""))

    def send(self, target, path, control):
        source_fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with contextlib.closing(self.connection(60)) as conn, os.fdopen(source_fd, "rb") as source:
            info = os.fstat(source.fileno())
            if not stat.S_ISREG(info.st_mode):
                raise Failure("not-a-file")
            control.connection = conn
            conn.putrequest("PUT", "/localapi/v0/file-put/" + urllib.parse.quote(target, safe="") + "/" + urllib.parse.quote(path.name, safe=""))
            conn.putheader("Content-Length", str(info.st_size))
            conn.endheaders()
            remaining = info.st_size
            while remaining:
                control.check()
                chunk = source.read(min(CHUNK, remaining))
                if not chunk:
                    raise Failure("source-changed")
                conn.send(chunk)
                remaining -= len(chunk)
            control.check()
            response = conn.getresponse()
            if response.status != 200:
                raise Failure("interrupted")
            response.read(65536)
            control.connection = None

    def receive(self, name, directory, control, progress):
        path = None
        with contextlib.closing(self.connection(60)) as conn:
            control.connection = conn
            conn.request("GET", "/localapi/v0/files/" + urllib.parse.quote(filename(name), safe=""))
            response = conn.getresponse()
            if response.status != 200 or response.length is None:
                raise Failure("receive-unavailable")
            expected = response.length
            path, fd = reserve(directory, name)
            try:
                total = 0
                with os.fdopen(fd, "wb") as output:
                    while True:
                        control.check()
                        chunk = response.read(CHUNK)
                        if not chunk:
                            break
                        output.write(chunk)
                        total += len(chunk)
                        progress(total, expected)
                    if total != expected:
                        raise Failure("interrupted")
                    output.flush()
                    os.fsync(output.fileno())
                return str(path), total
            except BaseException:
                path.unlink(missing_ok=True)  # Only our incomplete, O_EXCL-created download.
                raise
            finally:
                control.connection = None

    def events(self, callback, stop):
        while not stop.is_set():
            try:
                with contextlib.closing(self.connection(30)) as conn:
                    # Initial outgoing files; no preferences, netmap, or credentials requested.
                    conn.request("GET", "/localapi/v0/watch-ipn-bus?mask=64")
                    response = conn.getresponse()
                    if response.status != 200:
                        raise Failure("provider-unavailable")
                    while not stop.is_set():
                        line = response.readline(2 * 1024 * 1024)
                        if not line:
                            break
                        raw = json.loads(line)
                        callback({
                            "outgoing": [{"target": f.get("PeerID"), "name": f.get("Name"), "bytes": f.get("Sent", 0), "finished": f.get("Finished", False)} for f in raw.get("OutgoingFiles") or []],
                            "incoming": [{"name": f["Name"], "size": f.get("DeclaredSize", -1), "bytes": f.get("Received", 0)} for f in raw.get("IncomingFiles") or []],
                        })
            except (OSError, ValueError, Failure, http.client.HTTPException):
                stop.wait(2)


class Control:
    def __init__(self):
        self.cancelled = threading.Event()
        self.connection = None

    def check(self):
        if self.cancelled.is_set():
            raise Failure("cancelled")

    def cancel(self):
        self.cancelled.set()
        conn = self.connection
        if conn and conn.sock:
            with contextlib.suppress(OSError):
                conn.sock.shutdown(socket.SHUT_RDWR)
        if conn:
            conn.close()


class Transfers:
    """Provider-neutral groups, lifecycle, privacy and desktop file operations."""
    def __init__(self, provider, directory, state, notify=None):
        self.provider, self.directory, self.state = provider, Path(directory), Path(state)
        self.notify = notify or (lambda group: None)
        self.lock = threading.RLock()
        self.groups, self.controls, self.selection, self.targets = [], {}, [], []
        self.focus = ""
        self.focus_revision = 0
        self.error = ""
        self.stop = threading.Event()
        self.state.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        try:
            values = json.loads(self.state.read_text())
            for group in values:
                if group["state"] in ACTIVE:
                    group.update(state="failed", error="interrupted")
                self.groups.append(group)
        except (OSError, ValueError, KeyError, TypeError):
            pass
        self.prune()

    def prune(self):
        self.groups = [g for g in self.groups if g["state"] in ACTIVE | {"failed"} or g["updated"] > time.time() - WEEK]

    def save(self):
        self.prune()
        temp = self.state.with_suffix(".tmp")
        fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "w") as output:
            json.dump(self.groups, output, ensure_ascii=True)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temp, self.state)

    def snapshot(self):
        with self.lock:
            self.prune()
            groups = copy.deepcopy(self.groups)
            for g in groups:
                # Private source paths stay service-side; incoming paths are user-facing.
                for f in g["files"]:
                    f.pop("remote", None)
                    f.pop("pendingAck", None)
                    if g["direction"] == "outgoing":
                        f.pop("path", None)
                g["size"] = sum(max(0, f["size"]) for f in g["files"])
                g["bytes"] = sum(max(0, f["bytes"]) for f in g["files"])
            return {"version": 1, "groups": groups, "targets": copy.deepcopy(self.targets),
                    "selection": [Path(p).name for p in self.selection], "focus": self.focus, "focusRevision": self.focus_revision,
                    "capabilities": self.provider.capabilities, "error": self.error}

    def group(self, identity):
        for g in self.groups:
            if g["id"] == identity:
                return g
        raise Failure("transfer-missing")

    def new(self, direction, device, files):
        now = time.time()
        group = {"id": str(uuid.uuid4()), "direction": direction, "device": device,
                 "files": files, "state": "sending" if direction == "outgoing" else "receiving",
                 "created": now, "updated": now, "error": "", "seen": direction == "outgoing",
                 "source": device if direction == "incoming" else "This device",
                 "destination": "This device" if direction == "incoming" else device}
        self.groups.insert(0, group)
        return group

    def dispatch(self, request):
        if not isinstance(request, dict):
            raise Failure("invalid-request")
        op = request.get("op")
        if op == "snapshot":
            return self.snapshot()
        with self.lock:
            if op == "select":
                paths = request.get("paths")
                if not isinstance(paths, list) or len(paths) > 256:
                    raise Failure("invalid-selection")
                self.selection = list(dict.fromkeys(str(regular(p)) for p in paths))
                return {"ok": True}
            if op == "send":
                # Consume selection atomically before starting, making repeat clicks harmless.
                if not self.selection:
                    raise Failure("choose-files")
                targets = self.provider.targets()
                target = next((t for t in targets if t["id"] == request.get("target")), None)
                if not target:
                    raise Failure("target-unavailable")
                files = []
                for p in self.selection:
                    path = regular(p)
                    files.append({"name": path.name, "path": str(path), "size": path.stat().st_size,
                                  "bytes": 0, "state": "pending", "error": "", "attempts": 0})
                group = self.new("outgoing", target["name"], files)
                group["target"] = target["id"]
                self.selection = []
                self.start(group)
                return {"ok": True, "id": group["id"]}
            group = self.group(request.get("id"))
            if op == "seen":
                group["seen"] = True
            elif op == "cancel":
                if group["id"] not in self.controls:
                    raise Failure("cancel-at-sender")
                self.controls[group["id"]].cancel()
            elif op == "retry":
                if group["state"] not in ("failed", "cancelled"):
                    raise Failure("already-active")
                self.start(group)
            elif op == "dismiss":
                if group["state"] in ACTIVE:
                    raise Failure("already-active")
                self.groups.remove(group)  # Metadata only: never remove user files.
            elif op == "focus":
                self.focus = group["id"]
                self.focus_revision += 1
            elif op in ("open", "reveal", "trash", "move"):
                self.file_action(group, request)
            else:
                raise Failure("unknown-operation")
            self.save()
            return {"ok": True}

    def file_action(self, group, request):
        if group["direction"] != "incoming":
            raise Failure("not-incoming")
        index = request.get("file")
        if not isinstance(index, int) or not 0 <= index < len(group["files"]):
            raise Failure("file-missing")
        entry = group["files"][index]
        if entry["state"] != "completed":
            raise Failure("file-unavailable")
        path = regular(entry["path"])
        op = request["op"]
        if op == "move":
            target, fd = reserve(Path(request["directory"]), path.name)
            try:
                with os.fdopen(fd, "wb") as dest, open(path, "rb") as source:
                    while chunk := source.read(CHUNK):
                        dest.write(chunk)
                    dest.flush()
                    os.fsync(dest.fileno())
                path.unlink()
                entry["path"] = str(target)
            except BaseException:
                target.unlink(missing_ok=True)
                raise
        else:
            command = ["gio", "trash", "--", str(path)] if op == "trash" else ["xdg-open", str(path if op == "open" else path.parent)]
            result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL, timeout=15, check=False)
            if result.returncode:
                raise Failure("desktop-action-failed")
            if op == "trash":
                entry.update(state="trashed", path="")

    def start(self, group):
        if group["id"] in self.controls:
            raise Failure("already-active")
        control = Control()
        self.controls[group["id"]] = control
        group.update(state="sending" if group["direction"] == "outgoing" else "receiving", error="", updated=time.time())
        self.save()
        threading.Thread(target=self.run, args=(group, control), daemon=True).start()

    def progress(self, group, entry, count, size=None):
        with self.lock:
            entry["bytes"] = min(count, max(0, entry["size"])) if size is None else count
            if size is not None:
                entry["size"] = size
            group["updated"] = time.time()

    def run(self, group, control):
        try:
            for entry in group["files"]:
                if entry["state"] in ("completed", "trashed"):
                    continue
                for attempt in range(3):
                    control.check()
                    with self.lock:
                        entry.update(state="active", error="", attempts=entry["attempts"] + 1, bytes=0)
                        group["state"] = "sending" if group["direction"] == "outgoing" else "receiving"
                    try:
                        if group["direction"] == "outgoing":
                            if not any(t["id"] == group["target"] for t in self.provider.targets()):
                                raise Failure("target-unavailable")
                            path = regular(entry["path"])
                            if path.stat().st_size != entry["size"]:
                                raise Failure("source-changed")
                            self.provider.send(group["target"], path, control)
                        else:
                            path, size = self.provider.receive(entry["remote"], self.directory, control,
                                                              lambda n, size: self.progress(group, entry, n, size))
                            # Persist delivered path before deleting daemon's copy. Restart never re-copies it.
                            with self.lock:
                                entry.update(path=path, size=size, bytes=size, state="completed", pendingAck=True)
                                self.save()
                                # Serialize acknowledgement with poll/recovery so only one
                                # caller can delete the daemon copy or publish completion.
                                self.provider.forget(entry["remote"])
                                entry.pop("pendingAck", None)
                                entry.pop("remote", None)
                        with self.lock:
                            entry.update(state="completed", bytes=entry["size"], error="")
                            self.save()
                        break
                    except (OSError, ValueError, Failure, http.client.HTTPException) as error:
                        control.check()
                        code = error.code if isinstance(error, Failure) else "interrupted"
                        with self.lock:
                            if not entry.get("pendingAck"):
                                entry.update(error=code, state="failed")
                        if code in ("source-missing", "source-changed", "target-unavailable", "not-a-file") or attempt == 2 or entry.get("pendingAck"):
                            raise Failure(code) from None
                        with self.lock:
                            group["state"] = "retrying"
                        control.cancelled.wait(2 ** attempt)
            with self.lock:
                group.update(state="completed", updated=time.time())
                if group["direction"] == "incoming":
                    self.notify(copy.deepcopy(group))
        except (OSError, ValueError, Failure, http.client.HTTPException) as error:
            with self.lock:
                cancelled = control.cancelled.is_set()
                group.update(state="cancelled" if cancelled else "failed",
                             error="cancelled" if cancelled else error.code if isinstance(error, Failure) else "interrupted",
                             updated=time.time())
                if not cancelled:
                    self.notify(copy.deepcopy(group))
        finally:
            with self.lock:
                try:
                    self.save()
                finally:
                    self.controls.pop(group["id"], None)

    def receive_waiting(self):
        for remote in self.provider.waiting():
            name = filename(remote["name"])
            with self.lock:
                existing = next((g for g in self.groups if g["direction"] == "incoming" and any(f.get("remote") == name for f in g["files"])), None)
                if existing:
                    entry = existing["files"][0]
                    if entry.get("pendingAck"):
                        self.provider.forget(name)
                        entry.pop("pendingAck", None)
                        entry.pop("remote", None)
                        existing.update(state="completed", error="", updated=time.time())
                        self.save()
                        self.notify(copy.deepcopy(existing))
                    elif existing["state"] == "receiving" and existing["id"] not in self.controls:
                        self.start(existing)
                    continue
                entry = {"name": name, "remote": name, "size": remote["size"], "bytes": 0,
                         "state": "pending", "error": "", "attempts": 0, "path": ""}
                group = self.new("incoming", "Personal device", [entry])
                self.start(group)
        # Once daemon acknowledges, remote filename must not deduplicate a later new transfer.
        with self.lock:
            present = {f["name"] for f in self.provider.waiting()}
            for group in self.groups:
                for entry in group["files"]:
                    if entry.get("remote") not in present and group["state"] == "completed":
                        entry.pop("remote", None)

    def event(self, message):
        with self.lock:
            for outgoing in message.get("outgoing") or []:
                if outgoing.get("finished"):
                    continue
                for group in self.groups:
                    if group["state"] == "sending" and group.get("target") == outgoing.get("target"):
                        for entry in group["files"]:
                            if entry["name"] == outgoing.get("name") and entry["state"] == "active":
                                self.progress(group, entry, outgoing.get("bytes", 0))
            for incoming in message.get("incoming") or []:
                name = filename(incoming["name"])
                group = next((g for g in self.groups if g["direction"] == "incoming" and g["state"] == "receiving" and g["files"][0].get("remote") == name), None)
                if group is None and any(g["direction"] == "incoming" and g["state"] in ("failed", "cancelled") and g["files"][0].get("remote") == name for g in self.groups):
                    continue
                if group is None:
                    group = self.new("incoming", "Personal device", [{"name": name, "remote": name,
                        "size": incoming.get("size", -1), "bytes": 0, "state": "active", "attempts": 0, "error": "", "path": ""}])
                if group["id"] not in self.controls:
                    self.progress(group, group["files"][0], incoming.get("bytes", 0), incoming.get("size", -1))

    def poll(self):
        threading.Thread(target=self.provider.events, args=(self.event, self.stop), daemon=True).start()
        while not self.stop.is_set():
            try:
                targets = self.provider.targets()
                with self.lock:
                    self.targets, self.error = targets, ""
                self.directory.mkdir(parents=True, exist_ok=True)
                self.receive_waiting()
                with self.lock:
                    for group in self.groups:
                        if group["state"] == "receiving" and group["id"] not in self.controls and group["updated"] < time.time() - 90:
                            group.update(state="failed", error="interrupted", updated=time.time())
                            self.notify(copy.deepcopy(group))
                    self.save()
            except (OSError, ValueError, KeyError, Failure, http.client.HTTPException):
                with self.lock:
                    self.targets, self.error = [], "provider-unavailable"
            self.stop.wait(2)


def socket_path():
    return Path(os.environ["XDG_RUNTIME_DIR"]) / "seele-transfers.sock"


def request(value):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.settimeout(30)
        conn.connect(str(socket_path()))
        conn.sendall(json.dumps(value).encode() + b"\n")
        with conn.makefile("rb") as stream:
            return json.loads(stream.readline(2 * 1024 * 1024))


def open_panel():
    subprocess.run(["seele-shellctl", "transfers"], stdin=subprocess.DEVNULL,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10, check=False)


def notify(group):
    def worker():
        try:
            result = subprocess.run(["notify-send", "--app-name=Seele Transfers", "--icon=folder-download",
                "--wait", "--action=open=Open Transfers", "Transfer failed" if group["state"] == "failed" else "File received",
                str(len(group["files"])) + " file(s) · " + group["device"]], capture_output=True, text=True, timeout=86400, check=False)
            if result.stdout.strip() == "open":
                request({"op": "focus", "id": group["id"]})
                open_panel()
        except (OSError, ValueError, subprocess.TimeoutExpired):
            pass
    threading.Thread(target=worker, daemon=True).start()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=("serve", "request", "select", "watch"))
    parser.add_argument("paths", nargs="*")
    args = parser.parse_args()
    if args.operation == "serve":
        os.umask(0o077)
        state = Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state"))) / "seele-transfers/history.json"
        transfers = Transfers(TaildropProvider(), os.environ["SEELE_TRANSFERS_DOWNLOADS"], state, notify)

        class Handler(socketserver.StreamRequestHandler):
            def handle(self):
                # Directory and socket mode restrict peers to this user. No network listener.
                try:
                    line = self.rfile.readline(256 * 1024 + 1)
                    if len(line) > 256 * 1024:
                        raise Failure("request-too-large")
                    value = transfers.dispatch(json.loads(line))
                except (Failure, OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
                    value = {"error": error.code if isinstance(error, Failure) else "operation-failed"}
                with contextlib.suppress(BrokenPipeError):
                    self.wfile.write(json.dumps(value).encode() + b"\n")

        class Server(socketserver.ThreadingUnixStreamServer):
            daemon_threads = True

        socket_path().unlink(missing_ok=True)
        with Server(str(socket_path()), Handler) as server:
            os.chmod(socket_path(), 0o600)
            threading.Thread(target=transfers.poll, daemon=True).start()
            signal.signal(signal.SIGTERM, lambda *_: threading.Thread(target=server.shutdown, daemon=True).start())
            signal.signal(signal.SIGINT, lambda *_: threading.Thread(target=server.shutdown, daemon=True).start())
            try:
                server.serve_forever()
            finally:
                transfers.stop.set()
                for control in list(transfers.controls.values()):
                    control.cancel()
                deadline = time.monotonic() + 10
                while transfers.controls and time.monotonic() < deadline:
                    time.sleep(0.05)
                socket_path().unlink(missing_ok=True)
    elif args.operation == "select":
        result = request({"op": "select", "paths": args.paths}) if args.paths else {"ok": True}
        if result.get("error"):
            print(json.dumps(result))
            return 1
        open_panel()
    elif args.operation == "request":
        print(json.dumps(request(json.loads(sys.stdin.readline(256 * 1024)))))
    else:
        while True:
            try:
                value = request({"op": "snapshot"})
            except (OSError, ValueError):
                value = {"version": 1, "groups": [], "selection": [], "targets": [], "error": "service-unavailable"}
            print(json.dumps(value), flush=True)
            time.sleep(0.5)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, Failure) as error:
        print(json.dumps({"error": error.code if isinstance(error, Failure) else "service-unavailable"}))
        sys.exit(1)
