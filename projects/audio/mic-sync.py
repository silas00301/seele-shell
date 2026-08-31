#!/usr/bin/env python3
"""Keep a USB microphone's own mute and the desktop's mute in step.

The Shure MV7+ mutes itself when its touch panel is tapped: the tap gates the
capsule inside the device, turns the panel red, and is published as the USB
Audio Class feature-unit mute that ALSA exposes as an ordinary
`Microphone Capture Switch`. The device announces its own changes on the status
interrupt endpoint of its audio control interface, so that switch is an event
source rather than something to poll.

PipeWire owns none of this. `wpctl set-mute` on a source node applies a
software mute inside the graph and leaves the device capturing, so the two
mutes were independent in both directions: a tap silenced a call the desktop
still showed as live, and unmuting in the desktop could not bring a
panel-muted microphone back, because nothing in the graph held the state that
was actually gating the signal.

This daemon makes them one state. An ALSA event moves the desktop to match the
panel, a desktop change moves the panel -- and its LED, which follows the same
control -- to match, and the switch wins on startup because it is what actually
gates the signal. Each side's write echoes back as an event on the other, so
every observation is compared against the state the two were last agreed on
rather than acted on blindly, which is what keeps the two from chasing each
other.

The microphone is named by USB vendor and product id, the one identity that
survives a replug. Its ALSA card, that card's capture switch, and its PipeWire
node are all derived from it.
"""

import argparse
import glob
import json
import os
import re
import select
import subprocess
import sys
import time

# ALSA names a capture mute `<source> Capture Switch`, and it reads as the
# capture path being open: `off` is the muted microphone.
SWITCH_SUFFIX = "Capture Switch"
CONTROL = re.compile(r"^numid=(?P<numid>\d+),iface=MIXER,name='(?P<name>.*)'$")
VALUE = re.compile(r"^\s*: values=(?P<value>\S+)")
# Only long enough that an unplugged microphone is picked up promptly on its
# way back; nothing polls while it is connected.
RESCAN_S = 2.0
# Where the USB tree lives, so the tests can present one of their own.
SYSFS_USB = os.environ.get("SEELE_MIC_SYNC_SYSFS") or "/sys/bus/usb/devices"


def log(message):
    print(message, flush=True)


def read_text(*path):
    try:
        with open(os.path.join(*path)) as handle:
            return handle.read().strip()
    except OSError:
        return None


def run(command):
    """Stdout of a helper, or None when it is missing or fails."""
    try:
        result = subprocess.run(command, capture_output=True, text=True)
    except OSError:
        return None
    return result.stdout if result.returncode == 0 else None


def card_index(vendor, product):
    """ALSA card index of a USB device, or None while it is unplugged."""
    for device in sorted(glob.glob(os.path.join(SYSFS_USB, "*"))):
        if read_text(device, "idVendor") != vendor:
            continue
        if read_text(device, "idProduct") != product:
            continue
        for card in sorted(glob.glob(os.path.join(device, "*", "sound", "card*"))):
            number = read_text(card, "number")
            if number is not None:
                return int(number)
    return None


def switch_numid(card):
    """The card's capture mute, or None on a device that exposes no such control."""
    listing = run(["amixer", "-c", str(card), "controls"]) or ""
    for line in listing.splitlines():
        match = CONTROL.match(line.strip())
        if match and match.group("name").endswith(SWITCH_SUFFIX):
            return int(match.group("numid"))
    return None


def switch_muted(card, numid):
    """Whether the device is currently gating its own capture."""
    listing = run(["amixer", "-c", str(card), "cget", f"numid={numid}"]) or ""
    for line in listing.splitlines():
        match = VALUE.match(line)
        if match:
            return match.group("value") == "off"
    return None


def set_switch(card, numid, muted):
    run(["amixer", "-q", "-c", str(card), "cset", f"numid={numid}", "off" if muted else "on"])


def node_muted(node):
    """Current mute of a source node, read rather than taken from the event that
    announced it: `pw-dump -m` replays changes in order, so by the time one is
    read its value may already be history."""
    text = run(["wpctl", "get-volume", str(node)])
    return None if text is None else "MUTED" in text


def set_node_mute(node, muted):
    run(["wpctl", "set-mute", str(node), "1" if muted else "0"])




def json_values(buffer):
    """Split `pw-dump -m`'s stream of concatenated JSON arrays as they arrive,
    returning the complete ones and whatever is still half written."""
    decoder = json.JSONDecoder()
    values = []
    while True:
        buffer = buffer.lstrip()
        if not buffer:
            return values, ""
        try:
            value, end = decoder.raw_decode(buffer)
        except ValueError:
            return values, buffer
        values.append(value)
        buffer = buffer[end:]


def source_node(obj, card):
    """The id of a PipeWire capture node belonging to this card, if this object
    is one. A removed object arrives as a bare id with no info."""
    props = ((obj.get("info") or {}).get("props")) or {}
    if props.get("media.class") != "Audio/Source":
        return None
    return obj.get("id") if props.get("alsa.card") == card else None


def carries_mute(obj):
    """Whether this object is an update to the node's properties, which is the
    only place its mute can have moved."""
    params = ((obj.get("info") or {}).get("params") or {})
    return any("mute" in entry for entry in params.get("Props") or [])


class Session:
    """One connected microphone, for as long as it and PipeWire are both there."""

    def __init__(self, card, numid):
        self.card = card
        self.numid = numid
        self.node = None
        # The state the switch and the node were last made to agree on. Until
        # the node turns up there is nothing to agree with, and every write this
        # daemon makes comes back as an event on the other side, which compares
        # equal to it and is ignored. Both sides are read rather than trusted to
        # the event that woke us, so a burst that collapses into one wake-up
        # still settles on what the two actually hold.
        self.applied = None
        # Outstanding OSD notifications. They are never waited on: the mute has
        # already happened, and a shell that is slow to answer -- or not running
        # -- must not hold up the sync that is only telling it what changed.
        self.notices = []

    def on_node_change(self):
        muted = node_muted(self.node)
        if muted is None:
            return
        if self.applied is None:
            # First sight of the node, which is either startup or a replug. The
            # switch is what gates the signal, so it wins: adopting the node's
            # mute here would leave a microphone the user muted by hand
            # reporting itself as live.
            device = switch_muted(self.card, self.numid)
            if device is None:
                return
            self.applied = device
            if muted != device:
                log(f"adopting device mute={device} on node {self.node}")
                set_node_mute(self.node, device)
            return
        if muted == self.applied:
            return
        self.applied = muted
        log(f"desktop mute={muted}, following on card {self.card}")
        set_switch(self.card, self.numid, muted)

    def on_switch_event(self):
        if self.applied is None:
            return
        muted = switch_muted(self.card, self.numid)
        if muted is None or muted == self.applied:
            return
        self.applied = muted
        log(f"device mute={muted}, following on node {self.node}")
        set_node_mute(self.node, muted)
        self.show_osd(muted)

    def show_osd(self, muted):
        """Acknowledge on the desktop what the user did on the device, the way
        the volume keys acknowledge themselves. The new state travels with the
        call, because the shell asking the system to rediscover it would put
        several hundred milliseconds between the tap and its OSD."""
        self.notices = [notice for notice in self.notices if notice.poll() is None]
        try:
            self.notices.append(
                subprocess.Popen(
                    ["seele-shellctl", "-q", "microphone-state", "muted" if muted else "live"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
            )
        except OSError:
            pass

    def on_graph_object(self, obj):
        node = source_node(obj, self.card)
        if node is None:
            if self.node is not None and obj.get("id") == self.node and obj.get("info") is None:
                self.node, self.applied = None, None
            return
        self.node = node
        if carries_mute(obj):
            self.on_node_change()


def watch(session):
    """Run until the microphone or one of the two event streams goes away."""
    alsa = subprocess.Popen(
        ["alsactl", "monitor", f"hw:{session.card}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    graph = subprocess.Popen(
        ["pw-dump", "-m"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )
    # Both streams are read as raw descriptors rather than through Python's
    # buffered files: poll() reports the descriptor, so a second line already
    # sitting in a buffer would never announce itself and the event would be
    # lost.
    alsa_fd, graph_fd = alsa.stdout.fileno(), graph.stdout.fileno()
    poller = select.poll()
    poller.register(alsa_fd, select.POLLIN)
    poller.register(graph_fd, select.POLLIN)
    pending = ""
    try:
        while True:
            for fd, _ in poller.poll(1000):
                chunk = os.read(fd, 1 << 16)
                if not chunk:
                    return
                if fd == alsa_fd:
                    # Any control on this card may have moved; the switch's own
                    # value decides whether anything actually changed, so the
                    # event line itself needs no parsing.
                    session.on_switch_event()
                    continue
                pending += chunk.decode("utf-8", "replace")
                batches, pending = json_values(pending)
                for batch in batches:
                    for obj in batch:
                        session.on_graph_object(obj)
            if alsa.poll() is not None or graph.poll() is not None:
                return
    finally:
        for process in (alsa, graph):
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "device",
        help="USB microphone that publishes its own mute, as vendor:product",
    )
    arguments = parser.parse_args()
    try:
        vendor, product = (part.lower() for part in arguments.device.split(":", 1))
    except ValueError:
        parser.error("device must be vendor:product, such as 14ed:1019")

    while True:
        card = card_index(vendor, product)
        numid = switch_numid(card) if card is not None else None
        if numid is None:
            time.sleep(RESCAN_S)
            continue
        log(f"watching card {card} control {numid} for {vendor}:{product}")
        watch(Session(card, numid))
        time.sleep(RESCAN_S)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(0)
