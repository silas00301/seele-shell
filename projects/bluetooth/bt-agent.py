#!/usr/bin/env python3
"""BlueZ pairing agent for Seele Shell's Bluetooth receiver.

BlueZ hands every pairing decision to whichever agent is registered and
rejects the request outright when none is, so this is what lets a phone pair
with this machine at all. Its DisplayYesNo capability is also what makes
Secure Simple Pairing choose numeric comparison over just-works, which is what
gives the pairing a six digit code to check on both ends.

The agent draws nothing itself. It writes the pending request where Seele
Shell can read it, asks the shell to show it, and waits for the verdict the
shell writes back, so the trust decision is made in the desktop's own surface
rather than a terminal.
"""

import json
import os
import secrets
import subprocess

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

AGENT_PATH = "/org/seele/bluetooth/agent"
# KeyboardDisplay covers every association model Secure Simple Pairing can
# pick. Against a peer that can display it still resolves to numeric
# comparison, and it additionally lets this end enter a passkey a peer shows,
# which DisplayYesNo has to refuse -- and a refused model is a pairing that
# simply fails.
CAPABILITY = "KeyboardDisplay"
ANSWER_POLL_MS = 100
# Long enough for someone to walk back to the machine, short enough that a
# request nobody answers cannot pin the prompt open for the whole session.
REQUEST_TIMEOUT_S = 90

# The window this agent keeps open, and the discoverability timeout to put back
# when it closes. control.sh owns both values and passes them in.
WINDOW_S = int(os.environ.get("SEELE_BLUETOOTH_PAIRING_WINDOW") or 120)
RESTORE_TIMEOUT_S = int(os.environ.get("SEELE_BLUETOOTH_DISCOVERABLE_TIMEOUT") or 180)

RUNTIME = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
STATE_DIR = os.path.join(RUNTIME, "seele-shell")
REQUEST_PATH = os.path.join(STATE_DIR, "bluetooth-pairing.json")
ANSWER_PATH = os.path.join(STATE_DIR, "bluetooth-pairing.answer")


class Rejected(dbus.DBusException):
    _dbus_error_name = "org.bluez.Error.Rejected"


class Canceled(dbus.DBusException):
    _dbus_error_name = "org.bluez.Error.Canceled"


def log(message):
    """Which model the remote picked is the first thing worth knowing when a
    pairing fails, and it is invisible from anywhere else."""
    print(message, flush=True)


def shell(*arguments):
    """Nudge the shell, but never let its absence break a pairing."""
    try:
        subprocess.run(["seele-shellctl", "-q", *arguments], timeout=5, check=False)
    except (OSError, subprocess.SubprocessError):
        pass


def close_window():
    """Put the adapter back. Only reached when the window times out on its own;
    an explicit close from the shell has already done this itself."""
    for arguments in (
        ["discoverable", "off"],
        ["pairable", "off"],
        ["discoverable-timeout", str(RESTORE_TIMEOUT_S)],
    ):
        try:
            subprocess.run(["bluetoothctl", *arguments], timeout=5, check=False)
        except (OSError, subprocess.SubprocessError):
            pass


def clear_files():
    for path in (REQUEST_PATH, ANSWER_PATH):
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass


class Agent(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, AGENT_PATH)
        self.bus = bus
        self.pending = None

    # -- device helpers ---------------------------------------------------

    def device_property(self, path, name, default=""):
        try:
            properties = dbus.Interface(
                self.bus.get_object("org.bluez", path),
                "org.freedesktop.DBus.Properties",
            )
            return str(properties.Get("org.bluez.Device1", name))
        except dbus.DBusException:
            return default

    def device_name(self, path):
        for name in ("Alias", "Name", "Address"):
            value = self.device_property(path, name)
            if value:
                return value
        return "Unknown device"

    def trust(self, path):
        """A device nobody trusts needs an agent to authorise every service it
        opens, and none runs once the pairing window closes, so a phone
        accepted here could never reconnect on its own without this."""
        try:
            properties = dbus.Interface(
                self.bus.get_object("org.bluez", path),
                "org.freedesktop.DBus.Properties",
            )
            properties.Set("org.bluez.Device1", "Trusted", dbus.Boolean(True))
        except dbus.DBusException:
            pass

    # -- prompt plumbing --------------------------------------------------

    def publish(self, kind, path, passkey):
        token = secrets.token_hex(8)
        request = {
            "token": token,
            "kind": kind,
            "address": self.device_property(path, "Address"),
            "name": self.device_name(path),
            "icon": self.device_property(path, "Icon"),
            "passkey": f"{passkey:06d}" if passkey is not None else "",
        }
        os.makedirs(STATE_DIR, exist_ok=True)
        try:
            os.unlink(ANSWER_PATH)
        except FileNotFoundError:
            pass
        with open(REQUEST_PATH, "w", encoding="utf-8") as handle:
            json.dump(request, handle)
        shell("bluetooth-pairing", json.dumps(request))
        return token

    def ask(self, kind, path, passkey, reply, error):
        # One decision at a time: a second request while the first is still on
        # screen would replace a prompt the user is looking at.
        if self.pending is not None:
            error(Rejected())
            return
        log(f"request kind={kind} device={path}")
        token = self.publish(kind, path, passkey)
        self.pending = {
            "token": token,
            "kind": kind,
            "path": path,
            "reply": reply,
            "error": error,
            "waited": 0,
        }
        GLib.timeout_add(ANSWER_POLL_MS, self.poll)

    def settle(self, accepted, value=""):
        pending, self.pending = self.pending, None
        clear_files()
        shell("bluetooth-pairing-dismiss")
        if pending is None:
            return
        log(f"settle kind={pending['kind']} accepted={accepted}")
        if not accepted:
            pending["error"](Rejected())
            return
        self.trust(pending["path"])
        if pending["kind"] == "passkey":
            digits = "".join(character for character in value if character.isdigit())
            if not digits:
                pending["error"](Rejected())
                return
            pending["reply"](dbus.UInt32(int(digits)))
        elif pending["kind"] == "pincode":
            if not value:
                pending["error"](Rejected())
                return
            pending["reply"](dbus.String(value))
        else:
            pending["reply"]()

    def poll(self):
        pending = self.pending
        if pending is None:
            return False
        try:
            with open(ANSWER_PATH, encoding="utf-8") as handle:
                fields = handle.read().strip().split(" ", 2)
        except (FileNotFoundError, OSError):
            fields = []
        token = fields[0] if fields else ""
        verdict = fields[1] if len(fields) > 1 else ""
        value = fields[2] if len(fields) > 2 else ""
        if token == pending["token"] and verdict in ("accept", "reject"):
            self.settle(verdict == "accept", value)
            return False
        pending["waited"] += ANSWER_POLL_MS
        if pending["waited"] >= REQUEST_TIMEOUT_S * 1000:
            self.settle(False)
            return False
        return True

    # -- org.bluez.Agent1 -------------------------------------------------

    @dbus.service.method("org.bluez.Agent1", in_signature="", out_signature="")
    def Release(self):
        self.settle(False)

    @dbus.service.method("org.bluez.Agent1", in_signature="", out_signature="")
    def Cancel(self):
        self.settle(False)

    @dbus.service.method(
        "org.bluez.Agent1",
        in_signature="ou",
        out_signature="",
        async_callbacks=("reply", "error"),
    )
    def RequestConfirmation(self, device, passkey, reply, error):
        self.ask("confirm", device, int(passkey), reply, error)

    @dbus.service.method(
        "org.bluez.Agent1",
        in_signature="o",
        out_signature="",
        async_callbacks=("reply", "error"),
    )
    def RequestAuthorization(self, device, reply, error):
        # Just-works pairing, where neither end can show a code. It still gets
        # a prompt, so nothing is ever accepted silently.
        self.ask("authorize", device, None, reply, error)

    @dbus.service.method("org.bluez.Agent1", in_signature="os", out_signature="")
    def AuthorizeService(self, device, uuid):
        # The device is already paired by the time it opens a service, and the
        # pairing itself was gated above.
        return

    @dbus.service.method("org.bluez.Agent1", in_signature="ouq", out_signature="")
    def DisplayPasskey(self, device, passkey, entered):
        self.publish("display", device, int(passkey))

    @dbus.service.method("org.bluez.Agent1", in_signature="os", out_signature="")
    def DisplayPinCode(self, device, pincode):
        self.publish("display", device, None)

    @dbus.service.method(
        "org.bluez.Agent1",
        in_signature="o",
        out_signature="u",
        async_callbacks=("reply", "error"),
    )
    def RequestPasskey(self, device, reply, error):
        # The remote shows six digits and this end types them. Reached when
        # pairing something that can display but not confirm.
        self.ask("passkey", device, None, reply, error)

    @dbus.service.method(
        "org.bluez.Agent1",
        in_signature="o",
        out_signature="s",
        async_callbacks=("reply", "error"),
    )
    def RequestPinCode(self, device, reply, error):
        # Legacy pairing, where the code is free-form rather than six digits.
        self.ask("pincode", device, None, reply, error)


def main():
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SystemBus()
    agent = Agent(bus)
    manager = dbus.Interface(
        bus.get_object("org.bluez", "/org/bluez"), "org.bluez.AgentManager1"
    )
    manager.RegisterAgent(AGENT_PATH, CAPABILITY)
    manager.RequestDefaultAgent(AGENT_PATH)
    loop = GLib.MainLoop()

    def expire():
        # Reject whatever is still on screen rather than leaving a stale prompt
        # behind, then let the window close.
        if agent.pending is not None:
            agent.settle(False)
        loop.quit()
        return False

    GLib.timeout_add_seconds(WINDOW_S, expire)
    try:
        loop.run()
    except KeyboardInterrupt:
        pass
    finally:
        clear_files()
        shell("bluetooth-pairing-dismiss")
        try:
            manager.UnregisterAgent(AGENT_PATH)
        except dbus.DBusException:
            pass
        close_window()


if __name__ == "__main__":
    main()
