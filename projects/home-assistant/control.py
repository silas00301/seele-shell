#!/usr/bin/env python3
"""Private Home Assistant REST bridge. Credentials never leave this process."""
import json
import os
from pathlib import Path
import re
import signal
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request

MAX_CONFIG = 32768
MAX_RESPONSE = 2 * 1024 * 1024
REQUEST_TIMEOUT = 4
ALLOWED_DOMAINS = {"light", "switch", "input_boolean"}
ENTITY = re.compile(r"[a-z_]+\.[a-z0-9_]+\Z")


class Problem(Exception):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise Problem("The server redirected the request. Check the configured URL.")


def config_path():
    return Path(os.environ.get("SEELE_HOME_ASSISTANT_CONFIG", str(
        Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config")))
        / "seele-shell" / "home-assistant.json")))


def load_config():
    try:
        fd = os.open(config_path(), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return None
    except OSError:
        raise Problem("Could not open the private connection file.") from None
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise Problem("The connection file must be owned by you with mode 0600.")
        with os.fdopen(fd, "rb") as stream:
            fd = -1
            raw = stream.read(MAX_CONFIG + 1)
        if len(raw) > MAX_CONFIG:
            raise Problem("The connection file is too large.")
        config = json.loads(raw)
        if not isinstance(config, dict):
            raise ValueError()
        url = config["url"]
        token = config["token"]
        entities = config["entities"]
        if not isinstance(url, str) or not isinstance(token, str):
            raise ValueError()
        parts = urllib.parse.urlsplit(url)
        if (parts.scheme not in ("https", "http") or not parts.hostname
                or parts.username is not None or parts.password is not None
                or parts.query or parts.fragment or any(c.isspace() for c in url)):
            raise ValueError()
        _ = parts.port
        if not token or len(token) > 4096 or not token.isascii() or any(ord(c) < 33 for c in token):
            raise ValueError()
        if not isinstance(entities, list) or len(entities) > 32:
            raise ValueError()
        selected = []
        for item in entities:
            if isinstance(item, str):
                item = {"entity_id": item}
            if (not isinstance(item, dict) or not isinstance(item.get("entity_id"), str)
                    or not ENTITY.fullmatch(item["entity_id"])
                    or not isinstance(item.get("name", ""), str)):
                raise ValueError()
            if item["entity_id"] in [entry["entity_id"] for entry in selected]:
                raise ValueError()
            selected.append({"entity_id": item["entity_id"], "name": item.get("name", "")[:120]})
        return {"url": url.rstrip("/"), "token": token, "entities": selected}
    except (ValueError, KeyError, UnicodeError, TypeError):
        raise Problem("The connection file needs a valid URL, token and entity list.") from None
    finally:
        if fd >= 0:
            os.close(fd)


def request(config, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(config["url"] + path, data=data, headers={
        "Authorization": "Bearer " + config["token"],
        "Content-Type": "application/json", "Accept": "application/json",
    })
    # Never inherit ambient proxy credentials or forward our token on redirects.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(req, timeout=REQUEST_TIMEOUT) as response:
            raw = response.read(MAX_RESPONSE + 1)
        if len(raw) > MAX_RESPONSE:
            raise Problem("The server response is too large.")
        return json.loads(raw)
    except urllib.error.HTTPError as error:
        if error.code in (401, 403):
            raise Problem("Access was denied. Check the Home Assistant token.") from None
        raise Problem("Home Assistant could not complete the request.") from None
    except (urllib.error.URLError, TimeoutError, OSError):
        raise Problem("Home Assistant is unreachable. Check the connection.") from None
    except (ValueError, UnicodeError):
        raise Problem("Home Assistant returned an invalid response.") from None


def display(value, token, limit=120):
    text = str(value).replace(token, "[redacted]")
    return "".join(c for c in text if c.isprintable())[:limit]


def snapshot(config):
    if config is None:
        return {"configured": False, "connected": False, "entities": [], "error": ""}
    states = request(config, "/api/states")
    if not isinstance(states, list):
        raise Problem("Home Assistant returned an invalid state list.")
    by_id = {item.get("entity_id"): item for item in states
             if isinstance(item, dict) and isinstance(item.get("entity_id"), str)}
    entries = []
    for selected in config["entities"]:
        entity_id = selected["entity_id"]
        item = by_id.get(entity_id, {})
        attrs = item.get("attributes", {})
        if not isinstance(attrs, dict):
            attrs = {}
        state = item.get("state", "unavailable")
        if not isinstance(state, str):
            state = "unavailable"
        entries.append({
            "entity_id": entity_id,
            "name": display(selected["name"] or attrs.get("friendly_name") or entity_id, config["token"]),
            "state": display(state, config["token"]),
            "unit": display(attrs.get("unit_of_measurement", ""), config["token"], 24),
            "available": state not in ("unavailable", "unknown"),
            "controllable": entity_id.split(".")[0] in ALLOWED_DOMAINS and state in ("on", "off"),
        })
    return {"configured": True, "connected": True, "entities": entries, "error": ""}


def run(args):
    config = load_config()
    if args == ["status"]:
        return snapshot(config)
    if len(args) != 3 or args[0] != "set" or args[2] not in ("on", "off"):
        raise Problem("Use status or set ENTITY on|off.")
    if config is None:
        raise Problem("Add a Home Assistant connection first.")
    entity_id, desired = args[1:]
    if (entity_id not in [item["entity_id"] for item in config["entities"]]
            or entity_id.split(".")[0] not in ALLOWED_DOMAINS):
        raise Problem("This entity is read-only or is not selected.")
    # Explicit idempotent intent, never a toggle or a broad Home Assistant service.
    request(config, "/api/services/" + entity_id.split(".")[0] + "/turn_" + desired,
            {"entity_id": entity_id})
    return snapshot(config)


def deadline(signum, frame):
    raise Problem("Home Assistant took too long to respond.")


def main():
    signal.signal(signal.SIGALRM, deadline)
    signal.alarm(12)
    try:
        result = run(sys.argv[1:] or ["status"])
    except Problem as error:
        result = {"configured": True, "connected": False, "error": str(error)}
    except Exception:
        # No exception text, URL, token, response body or traceback crosses IPC.
        result = {"configured": True, "connected": False, "error": "Home Assistant is unavailable."}
    finally:
        signal.alarm(0)
    print(json.dumps(result, ensure_ascii=True))
    return 1 if result["error"] else 0


if __name__ == "__main__":
    sys.exit(main())
