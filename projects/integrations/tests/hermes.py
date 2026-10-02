"""Private real-binary lifecycle/control fixture; never runs a rebuild."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading

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
    # A fake tailscaled returning loopback must never open an HTTP listener.
    tools = root / "bin"
    tools.mkdir()
    tailscale = tools / "tailscale"
    tailscale.write_text("#!" + sys.executable + "\nprint('127.0.0.1')\n")
    tailscale.chmod(0o700)
    env["PATH"] = str(tools)
    result = subprocess.run([binary, "serve"], text=True, capture_output=True, env=env, timeout=8)
    assert result.returncode != 0, result.stdout
    assert not (service / "control.sock").exists()
print("Hermes private CLI framing, bounds, socket identity and tailnet-only failure passed")
