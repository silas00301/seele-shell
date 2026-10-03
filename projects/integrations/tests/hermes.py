"""Private real-binary lifecycle/control fixture; never runs a rebuild."""
import json
import http.client
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="seele-hermes-test-") as temp:
    root = Path(temp)
    root.chmod(0o700)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    service = runtime / "seele-hermes"
    service.mkdir(mode=0o700)
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), SEELE_HERMES_FLAKE=str(root))
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(service / "control.sock"))
    server.listen()
    received = []
    def reply():
        conn, _ = server.accept()
        with conn, conn.makefile("rwb") as wire:
            received.append(json.loads(wire.readline()))
            wire.write(b'{"ok":true}\n')
            wire.flush()
    thread = threading.Thread(target=reply)
    thread.start()
    result = subprocess.run([binary, "publish"], input='{"state":"thinking","session":"opaque"}\n', text=True, capture_output=True, env=env, timeout=5)
    thread.join(timeout=5)
    assert result.returncode == 0, result.stdout
    assert received == [{"op": "publish", "report": {"state": "thinking", "session": "opaque"}}]
    assert json.loads(result.stdout)["ok"] is True
    result = subprocess.run([binary, "publish"], input="x" * 65537 + "\n", text=True, capture_output=True, env=env, timeout=5)
    assert result.returncode != 0
    server.close()
    (service / "control.sock").unlink()
    # A symlinked control directory must not acquire the private publisher.
    service.rmdir()
    service.symlink_to(root, target_is_directory=True)
    result = subprocess.run([binary, "watch"], text=True, capture_output=True, env=env, timeout=5)
    assert result.returncode != 0
    service.unlink()
    # Invalid/missing Tailscale must leave the independent local MCP usable.
    tools = root / "bin"
    tools.mkdir()
    tailscale = tools / "tailscale"
    tailscale.write_text("#!" + sys.executable + "\nprint('127.0.0.1')\n")
    tailscale.chmod(0o700)
    env["PATH"] = str(tools)
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    env["SEELE_HERMES_PORT"] = str(port)
    env["SEELE_HERMES_ALLOW_REBUILD"] = "0"
    daemon = subprocess.Popen([binary, "serve"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    try:
        deadline = time.monotonic() + 5
        while True:
            assert daemon.poll() is None, "Local MCP stopped when Tailscale was unavailable"
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                    break
            except OSError:
                assert time.monotonic() < deadline, "Local MCP failed to start"
                time.sleep(0.05)

        def rpc(method, params=None, headers=None):
            conn = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
            try:
                conn.request("POST", "/mcp", json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}), headers=headers or {})
                response = conn.getresponse()
                return response.status, json.loads(response.read())
            finally:
                conn.close()

        status, reply = rpc("initialize")
        assert status == 200 and reply["result"]["protocolVersion"] == "2025-03-26"
        status, reply = rpc("tools/list")
        assert status == 200 and len(reply["result"]["tools"]) == 9
        status, reply = rpc("tools/call", {"name": "capabilities"})
        assert status == 200 and reply["result"]["isError"] is False
        status, reply = rpc("tools/call", {"name": "request_rebuild"})
        assert status == 200 and reply["result"]["isError"] is True
        status, reply = rpc("approve")
        assert status == 200 and reply["error"]["code"] == -32601
        for headers in [{"Host": "evil.test"}, {"Origin": "https://evil.test"}, {"Origin": "null"}]:
            assert rpc("tools/list", headers=headers)[0] == 400
        # Exercise another retry of the invalid tailnet address. It must not
        # collide with or replace the local listener.
        time.sleep(5.2)
        assert daemon.poll() is None and rpc("ping")[0] == 200
    finally:
        daemon.terminate()
        daemon.communicate(timeout=5)
    assert not (service / "control.sock").exists()
print("Hermes private CLI, local MCP, browser rejection, approval boundary and Tailscale outage passed")
