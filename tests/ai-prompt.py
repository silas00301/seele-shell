#!/usr/bin/env python3
import importlib.util
import json
import os
import select
import subprocess
import sys
import tempfile
import time
from pathlib import Path


worker_path = Path(sys.argv[1]).resolve()
spec = importlib.util.spec_from_file_location("seele_ai_prompt", worker_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)

assert module.context_mentions("Use @clip, @window, then @clip and me@example.org") == ["clip", "window"]
assert module.clean_prompt("Explain @screen and foo@bar.example") == "Explain and foo@bar.example"
thread, answer = module.event_data(
    '{"type":"thread.started","thread_id":"00000000-0000-0000-0000-000000000023"}\n'
    '{"type":"item.completed","item":{"type":"agent_message","text":"fallback"}}\n'
)
assert thread.endswith("0023") and answer == "fallback"
invalid_thread, _ = module.event_data('{"type":"thread.started","thread_id":"--force"}')
assert invalid_thread == ""
grounded = module.prompt_with_context("Explain @clip", {"clip": "literal $(touch /tmp/never)"})
assert "<CLIPBOARD TEXT>" in grounded and "literal $(touch /tmp/never)" in grounded
assert "@clip" not in grounded


def write_tool(path: Path) -> None:
    path.write_text(
        f"""#!{sys.executable}
import json, os, pathlib, sys, time
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
log = pathlib.Path(os.environ['TOOL_LOG'])
def record(**values):
    with log.open('a', encoding='utf-8') as handle:
        handle.write(json.dumps({{'tool': name, 'args': args, **values}}, ensure_ascii=False) + '\\n')
if name == 'wl-paste':
    record()
    sys.stdout.write('selected text' if '--primary' in args else "clipboard text\\n$(touch /tmp/never)")
elif name == 'grim':
    record()
    if args[1] == 'SLOW':
        time.sleep(0.2)
    pathlib.Path(args[-1]).write_bytes(b'\\x89PNG\\r\\n\\x1a\\nfixture')
elif name == 'codex':
    if args and args[0] == 'delete':
        record()
    else:
        prompt = sys.stdin.read()
        output = pathlib.Path(args[args.index('--output-last-message') + 1])
        resumed = 'resume' in args
        session = '00000000-0000-0000-0000-000000000026' if 'TERM-BLOCK' in prompt else '00000000-0000-0000-0000-000000000024' if 'BLOCK' in prompt else '00000000-0000-0000-0000-000000000025' if 'SIGTERM' in prompt else '00000000-0000-0000-0000-000000000023'
        print(json.dumps({{'type': 'thread.started', 'thread_id': session}}), flush=True)
        record(stdin=prompt)
        if 'BLOCK' in prompt:
            time.sleep(60)
        output.write_text('Follow-up answer' if resumed else 'Initial answer', encoding='utf-8')
elif name == 'hyprctl':
    record()
    if args[:2] == ['activewindow', '-j']:
        print(json.dumps({{'address': os.environ['SOURCE_ADDRESS'], 'pid': int(os.environ['SOURCE_PID'])}}))
elif name == 'wl-copy':
    record(stdin=sys.stdin.read())
elif name == 'wtype':
    record()
else:
    record()
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


with tempfile.TemporaryDirectory(prefix="seele-ai-prompt-test-") as temporary:
    root = Path(temporary)
    tools = root / "bin"
    tools.mkdir()
    implementation = tools / "fake-tool"
    write_tool(implementation)
    paths = {}
    for name in ("codex", "wl-paste", "wl-copy", "grim", "hyprctl", "wtype"):
        target = tools / name
        target.symlink_to(implementation)
        paths[name] = str(target)
    log = root / "tools.jsonl"
    address = "0xabc123"
    environment = {
        **os.environ,
        "XDG_RUNTIME_DIR": temporary,
        "TOOL_LOG": str(log),
        "SOURCE_ADDRESS": address,
        "SOURCE_PID": str(os.getpid()),
        "SEELE_SHELL_CODEX": paths["codex"],
        "SEELE_SHELL_WL_PASTE": paths["wl-paste"],
        "SEELE_SHELL_WL_COPY": paths["wl-copy"],
        "SEELE_SHELL_GRIM": paths["grim"],
        "SEELE_SHELL_HYPRCTL": paths["hyprctl"],
        "SEELE_SHELL_WTYPE": paths["wtype"],
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    worker = subprocess.Popen(
        [sys.executable, str(worker_path)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=environment,
    )
    assert worker.stdin is not None and worker.stdout is not None

    def send(**message):
        worker.stdin.write(json.dumps(message) + "\n")
        worker.stdin.flush()

    def receive(event: str, timeout: float = 5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            ready, _, _ = select.select([worker.stdout], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                break
            line = worker.stdout.readline()
            if not line:
                break
            message = json.loads(line)
            if message["event"] == event:
                return message
        stderr = worker.stderr.read() if worker.poll() is not None and worker.stderr else ""
        raise AssertionError(f"missing event {event}; stderr={stderr}")

    source = {
        "address": address,
        "title": "A private title",
        "app": "Ghostty",
        "classes": ["com.mitchellh.ghostty"],
        "pid": os.getpid(),
    }
    send(command="open", id=1, screen="DP-1", window=source)
    opened = receive("opened")
    assert opened["screen"] == "DP-1"
    assert not log.exists(), "opening the panel must not start Codex or read any context source"

    send(command="preview", id=1, kind="dir", token=1)
    directory = receive("preview")
    assert directory["kind"] == "dir" and directory["preview"] == os.getcwd()
    assert not log.exists(), "directory lookup should use /proc rather than an external process"

    send(command="submit", id=1, request=1, prompt="Explain @clip", permissions=[])
    denied = receive("permission")
    assert denied["kind"] == "clip" and not log.exists()

    send(command="preview", id=1, kind="clip", token=2)
    preview = receive("preview")
    assert preview["kind"] == "clip" and preview["text"] == "clipboard text\n$(touch /tmp/never)"
    send(command="preview", id=1, kind="screen", token=3)
    screen = receive("preview")
    assert screen["kind"] == "screen"
    screenshot = Path(screen["path"])
    assert screenshot.is_file() and screenshot.stat().st_mode & 0o777 == 0o600
    assert not any('codex' in line for line in log.read_text(encoding="utf-8").splitlines())

    prompt = "Explain @clip with @window from @dir and inspect @screen"
    send(command="submit", id=1, request=2, prompt=prompt, permissions=["clip"])
    receive("started")
    first = receive("answer")
    assert first["text"] == "Initial answer" and first["resumable"] is True

    records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
    grim = next(entry for entry in records if entry["tool"] == "grim")
    initial = next(entry for entry in records if entry["tool"] == "codex" and entry["args"][:1] == ["exec"])
    assert grim["args"][:2] == ["-o", "DP-1"]
    assert not Path(grim["args"][-1]).exists(), "the private screenshot must be removed after the turn"
    assert "--sandbox" in initial["args"] and "read-only" in initial["args"]
    assert "--ignore-rules" in initial["args"] and "--image" in initial["args"]
    assert json.dumps("clipboard text\n$(touch /tmp/never)", ensure_ascii=False) in initial["stdin"]
    assert json.dumps("Application: Ghostty\nTitle: A private title") in initial["stdin"]
    assert f"<TERMINAL DIRECTORY>\n{json.dumps(os.getcwd())}" in initial["stdin"]
    assert "@clip" not in initial["stdin"] and not Path("/tmp/never").exists()

    send(command="submit", id=1, request=3, prompt="Follow up", permissions=[])
    receive("started")
    follow_up = receive("answer")
    assert follow_up["text"] == "Follow-up answer"
    records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
    resume = next(entry for entry in records if entry["tool"] == "codex" and "resume" in entry["args"])
    assert "00000000-0000-0000-0000-000000000023" in resume["args"]

    send(command="copy", id=1)
    receive("copied")
    send(command="insert", id=1)
    receive("inserted")
    records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
    copied = next(entry for entry in records if entry["tool"] == "wl-copy")
    typed = next(entry for entry in records if entry["tool"] == "wtype")
    dispatch = next(entry for entry in records if entry["tool"] == "hyprctl" and entry["args"][:1] == ["dispatch"])
    assert copied["stdin"] == "Follow-up answer"
    assert typed["args"] == ["--", "Follow-up answer"]
    assert address in dispatch["args"][1]

    send(command="close", id=1)
    deadline = time.monotonic() + 3
    deleted = False
    while time.monotonic() < deadline:
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        deleted = any(entry["tool"] == "codex" and entry["args"][:2] == ["delete", "--force"] for entry in records)
        if deleted:
            break
        time.sleep(0.02)
    assert deleted, "closing the panel must delete its Codex session"

    # A close that races an in-flight first turn must terminate the whole Codex
    # process group and delete the thread id recovered from its partial JSONL.
    send(command="open", id=2, screen="DP-1", window=source)
    receive("opened")
    send(command="submit", id=2, request=1, prompt="BLOCK until closed", permissions=[])
    receive("started")
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        if any(entry["tool"] == "codex" and "BLOCK until closed" in entry.get("stdin", "") for entry in records):
            break
        time.sleep(0.02)
    else:
        raise AssertionError("blocking Codex fixture did not start")
    send(command="close", id=2)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        if any(entry["tool"] == "codex" and entry["args"][-1:] == ["00000000-0000-0000-0000-000000000024"] for entry in records):
            break
        time.sleep(0.02)
    else:
        raise AssertionError("cancelled first turn did not delete its partial session")

    send(command="open", id=3, screen="DP-1", window=source)
    receive("opened")
    send(command="preview", id=3, kind="screen", token=4)
    forgotten = Path(receive("preview")["path"])
    assert forgotten.is_file()
    send(command="forget", id=3, kind="screen")
    deadline = time.monotonic() + 2
    while forgotten.exists() and time.monotonic() < deadline:
        time.sleep(0.02)
    assert not forgotten.exists(), "removing @screen must remove its unsent capture"
    send(command="close", id=3)

    send(command="open", id=4, screen="SLOW", window=source)
    receive("opened")
    send(command="preview", id=4, kind="screen", token=5)
    send(command="forget", id=4, kind="screen")
    deadline = time.monotonic() + 3
    stale_target = None
    while time.monotonic() < deadline:
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        stale = [entry for entry in records if entry["tool"] == "grim" and entry["args"][:2] == ["-o", "SLOW"]]
        if stale:
            stale_target = Path(stale[-1]["args"][-1])
            if not stale_target.exists():
                break
        time.sleep(0.02)
    assert stale_target is not None and not stale_target.exists(), "a forgotten in-flight capture must be deleted"
    ready, _, _ = select.select([worker.stdout], [], [], 0.1)
    assert not ready, "a forgotten context request must not publish a stale preview"
    send(command="close", id=4)

    send(command="open", id=5, screen="DP-1", window=source)
    receive("opened")
    send(command="submit", id=5, request=1, prompt="SIGTERM cleanup", permissions=[])
    receive("started")
    receive("answer")
    worker.terminate()
    assert worker.wait(timeout=5) == 0
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        if any(entry["tool"] == "codex" and entry["args"][-1:] == ["00000000-0000-0000-0000-000000000025"] for entry in records):
            break
        time.sleep(0.02)
    else:
        raise AssertionError("SIGTERM did not delete the active panel session")

    shutdown_worker = subprocess.Popen(
        [sys.executable, str(worker_path)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=environment,
    )
    assert shutdown_worker.stdin is not None
    shutdown_worker.stdin.write(json.dumps({"command": "open", "id": 5, "screen": "DP-1", "window": source}) + "\n")
    shutdown_worker.stdin.write(json.dumps({"command": "submit", "id": 5, "request": 1, "prompt": "TERM-BLOCK cleanup", "permissions": []}) + "\n")
    shutdown_worker.stdin.flush()
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
        if any(entry["tool"] == "codex" and "TERM-BLOCK cleanup" in entry.get("stdin", "") for entry in records):
            break
        time.sleep(0.02)
    else:
        raise AssertionError("shutdown Codex fixture did not start")
    shutdown_worker.terminate()
    assert shutdown_worker.wait(timeout=10) == 0
    records = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
    assert any(
        entry["tool"] == "codex"
        and entry["args"][-1:] == ["00000000-0000-0000-0000-000000000026"]
        for entry in records
    ), "SIGTERM during the first turn must recover and delete its partial session"

print("AI prompt privacy gates, context assembly, session reuse, cancellation, actions, and active-turn shutdown cleanup passed")
