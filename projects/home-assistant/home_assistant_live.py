"""Resident Home Assistant connection. Only sanitized display data crosses stdout."""
import asyncio
import contextlib
import json
import math
import sys
import urllib.parse

import aiohttp


class Live:
    def __init__(self, bridge, emit=None):
        self.ha = bridge
        self.emit = emit or (lambda value: print(json.dumps(value, ensure_ascii=True), flush=True))
        self.config = None
        self.states = {}
        self.rooms = {}
        self.connected = False
        self.pending = {}
        self.error = ""
        self.connection = None
        self.ws = None
        self.serial = 0
        self.replies = {}
        self.tasks = set()
        self.settings_lock = asyncio.Lock()
        self.catalog_open = False
        self.snapshot_id = None

    def spawn(self, coro):
        task = asyncio.create_task(coro)
        self.tasks.add(task)
        task.add_done_callback(self.tasks.discard)
        return task

    def clean(self, value, limit=120):
        return self.ha.display(value, self.config["token"] if self.config else "\0", limit)

    @staticmethod
    def number(value, fallback=0):
        return float(value) if isinstance(value, (int, float)) and math.isfinite(value) else fallback

    def entry(self, entity_id, selected=None):
        selected = selected or {}
        item = self.states.get(entity_id) or {}
        attrs = item.get("attributes") or {}
        if not isinstance(attrs, dict):
            attrs = {}
        modes = attrs.get("supported_color_modes") or []
        if not isinstance(modes, list):
            modes = []
        state = item.get("state", "unavailable")
        display_state = self.clean(state)
        if entity_id.startswith("sensor."):
            try:
                value = float(display_state)
                if math.isfinite(value):
                    display_state = self.clean(f"{value:.1f}".removesuffix(".0"))
            except ValueError:
                pass
        light = entity_id.startswith("light.")
        fan = entity_id.startswith("fan.")
        features = int(self.number(attrs.get("supported_features")))
        minimum = self.number(attrs.get("min_color_temp_kelvin"), 2000)
        maximum = self.number(attrs.get("max_color_temp_kelvin"), 6500)
        return {
            "entity_id": entity_id, "name": self.clean(selected.get("name") or attrs.get("friendly_name") or entity_id),
            "state": display_state, "unit": self.clean(attrs.get("unit_of_measurement", ""), 24),
            "room": self.clean(selected.get("room") or self.rooms.get(entity_id, "Unassigned")),
            "favorite": bool(selected.get("favorite")),
            "device_class": attrs.get("device_class") if attrs.get("device_class") in ("temperature", "humidity") else "",
            "available": state not in ("unknown", "unavailable"),
            "controllable": entity_id.split(".")[0] in self.ha.ALLOWED_DOMAINS and state in ("on", "off"),
            "speed_control": fan and bool(features & 1),
            "percentage": self.number(attrs.get("percentage")),
            "percentage_step": max(1, min(100, self.number(attrs.get("percentage_step"), 1))),
            "dimmable": light and any(mode in modes for mode in ("brightness", "color_temp", "hs", "xy", "rgb", "rgbw", "rgbww", "white")),
            "temperature": light and "color_temp" in modes and 0 < minimum < maximum,
            "brightness": round(self.number(attrs.get("brightness")) * 100 / 255),
            "kelvin": self.number(attrs.get("color_temp_kelvin"), minimum),
            "min_kelvin": minimum, "max_kelvin": maximum,
        }

    def publish(self):
        selected = [{**item, "name": self.clean(item.get("name", "")), "room": self.clean(item.get("room", ""))} for item in self.config["entities"]] if self.config else []
        entries = [self.entry(item["entity_id"], item) for item in selected]
        summary_id = self.config.get("summary", "") if self.config else ""
        summary = next((item for item in entries if item["entity_id"] == summary_id), None)
        self.emit({"ready": True, "configured": self.config is not None, "connected": self.connected,
                   "entities": entries, "preferences": selected, "pending": {key: value["desired"] for key, value in self.pending.items()},
                   "error": self.error, "summary": summary_id,
                   "summary_text": (summary["state"] + " " + summary["unit"]).strip() if summary and summary["available"] else "",
                   "url": self.config["url"] if self.config else ""})
        if self.catalog_open:
            self.emit({"catalog": [self.entry(key) for key in sorted(self.states) if self.ha.ENTITY.fullmatch(key)]})

    async def rpc(self, kind, **fields):
        if self.ws is None or self.ws.closed:
            raise self.ha.Problem("Home Assistant is disconnected.")
        self.serial += 1
        serial = self.serial
        future = asyncio.get_running_loop().create_future()
        self.replies[serial] = future
        if kind == "get_states":
            self.snapshot_id = serial
        try:
            await self.ws.send_json({"id": serial, "type": kind, **fields})
            return await asyncio.wait_for(future, 12)
        finally:
            self.replies.pop(serial, None)

    async def receive(self):
        async for message in self.ws:
            if message.type != aiohttp.WSMsgType.TEXT:
                break
            data = json.loads(message.data)
            if data.get("type") == "result":
                future = self.replies.get(data.get("id"))
                if future and not future.done():
                    if data.get("success"):
                        if data.get("id") == self.snapshot_id:
                            states = data.get("result")
                            if not isinstance(states, list):
                                raise self.ha.Problem("Home Assistant returned an invalid state list.")
                            self.states = {item["entity_id"]: item for item in states if isinstance(item, dict) and isinstance(item.get("entity_id"), str)}
                        future.set_result(data.get("result"))
                    else:
                        future.set_exception(self.ha.Problem("Home Assistant could not complete the request."))
            elif data.get("type") == "event":
                event = data.get("event", {}).get("data", {})
                entity_id = event.get("entity_id")
                if isinstance(entity_id, str) and self.ha.ENTITY.fullmatch(entity_id):
                    if event.get("new_state") is None:
                        self.states.pop(entity_id, None)
                    else:
                        self.states[entity_id] = event["new_state"]
                    self.settle(entity_id)
                    if self.connected and entity_id in [item["entity_id"] for item in self.config["entities"]]:
                        self.publish()
        raise self.ha.Problem("Home Assistant is disconnected. Reconnecting…")

    def settle(self, entity_id):
        pending = self.pending.get(entity_id)
        if not pending:
            return
        entry = self.entry(entity_id)
        desired = pending["desired"]
        matches = all((abs(entry.get(key, -99999) - value) <= (50 if key == "kelvin" else 1))
                      if key in ("brightness", "kelvin", "percentage") else entry.get(key) == value
                      for key, value in desired.items())
        if matches:
            pending["confirmed"].set()

    async def connect(self):
        delay = 1
        while self.config:
            reader = None
            try:
                trace = aiohttp.TraceConfig()
                async def reject_redirect(*args):
                    raise self.ha.Problem("The server redirected the request. Check the connection URL.")
                trace.on_request_redirect.append(reject_redirect)
                async with aiohttp.ClientSession(trust_env=False, trace_configs=[trace], timeout=aiohttp.ClientTimeout(total=15, connect=8)) as session:
                    url = self.config["url"] + "/api/websocket"
                    async with session.ws_connect(url, heartbeat=20, max_msg_size=self.ha.MAX_RESPONSE,
                                                  timeout=aiohttp.ClientWSTimeout(ws_close=2, ws_receive=45)) as ws:
                        self.ws = ws
                        async with asyncio.timeout(12):
                            greeting = await ws.receive_json()
                            if greeting.get("type") != "auth_required":
                                raise self.ha.Problem("Home Assistant did not accept the connection.")
                            await ws.send_json({"type": "auth", "access_token": self.config["token"]})
                            if (await ws.receive_json()).get("type") != "auth_ok":
                                raise self.ha.Problem("Access was denied. Update the token in setup.")
                        self.serial = 0
                        reader = asyncio.create_task(self.receive())
                        await self.rpc("subscribe_events", event_type="state_changed")
                        await self.rpc("get_states")
                        self.rooms = {}
                        try:
                            areas = {item["area_id"]: item["name"] for item in await self.rpc("config/area_registry/list")}
                            devices = {item["id"]: item.get("area_id") for item in await self.rpc("config/device_registry/list")}
                            registry = await self.rpc("config/entity_registry/list")
                            self.rooms = {item["entity_id"]: areas.get(item.get("area_id") or devices.get(item.get("device_id")), "Unassigned") for item in registry}
                        except self.ha.Problem:
                            pass  # Non-admin accounts can still control and organize their devices.
                        self.connected = True
                        self.error = ""
                        delay = 1
                        self.publish()
                        await reader
            except asyncio.CancelledError:
                raise
            except Exception as error:
                self.error = str(error) if isinstance(error, self.ha.Problem) else "Home Assistant is unavailable. Reconnecting…"
            finally:
                self.connected = False
                self.ws = None
                if reader:
                    reader.cancel()
                    with contextlib.suppress(asyncio.CancelledError, Exception):
                        await reader
                for future in list(self.replies.values()):
                    if not future.done():
                        future.set_exception(self.ha.Problem("Connection lost. Check the device before retrying."))
                self.publish()
            await asyncio.sleep(delay)
            delay = min(delay * 2, 30)

    async def restart(self):
        if self.connection:
            self.connection.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self.connection
        self.connection = self.spawn(self.connect()) if self.config else None

    async def setup(self, message):
        url = message.get("url", "").strip().rstrip("/")
        token = message.pop("token", "")
        try:
            parts = urllib.parse.urlsplit(url)
            _ = parts.port
            valid = parts.scheme in ("http", "https") and parts.hostname and not parts.username and not parts.password and not parts.query and not parts.fragment and not any(c.isspace() for c in url)
        except ValueError:
            valid = False
        if not valid or not isinstance(token, str) or not token or len(token) > 4096 or not token.isascii() or any(ord(c) < 33 for c in token):
            raise self.ha.Problem("Enter a valid server URL and access token.")
        config = {"url": url, "token": token, "entities": [], "summary": ""}
        if self.config and self.config["url"] == url:
            config.update(entities=self.config["entities"], summary=self.config.get("summary", ""))
        await asyncio.to_thread(self.ha.request, config, "/api/")
        await asyncio.to_thread(self.ha.secret, "store", url, token)
        await asyncio.to_thread(self.ha.save_config, config)
        self.config = config
        self.states = {}
        self.error = ""
        await self.restart()
        self.publish()

    async def preferences(self, message):
        if not self.config:
            raise self.ha.Problem("Connect Home Assistant first.")
        entries = message.get("entities")
        if not isinstance(entries, list) or len(entries) > 32:
            raise self.ha.Problem("Select up to 32 entities.")
        selected = []
        for item in entries:
            entity_id = item.get("entity_id", "")
            if not isinstance(entity_id, str) or not self.ha.ENTITY.fullmatch(entity_id) or entity_id in [entry["entity_id"] for entry in selected]:
                raise self.ha.Problem("Invalid entity selection.")
            selected.append({"entity_id": entity_id, "name": self.clean(item.get("name", "")),
                             "room": self.clean(item.get("room", "")), "favorite": bool(item.get("favorite"))})
        summary = message.get("summary", "")
        if summary and summary not in [entry["entity_id"] for entry in selected]:
            raise self.ha.Problem("Choose a selected entity for the menu bar.")
        config = {**self.config, "entities": selected, "summary": summary}
        await asyncio.to_thread(self.ha.save_config, config)
        self.config = config
        self.publish()

    async def control(self, message):
        entity_id = message.get("entity_id")
        if not self.connected or not self.config or entity_id not in [item["entity_id"] for item in self.config["entities"]]:
            raise self.ha.Problem("This device is not selected or is disconnected.")
        entry = self.entry(entity_id)
        if not entry["available"] or not entry["controllable"] or entity_id in self.pending:
            raise self.ha.Problem("This device is unavailable, read-only or still updating.")
        desired = message.get("desired", {})
        if not isinstance(desired, dict) or not desired or set(desired) - {"state", "brightness", "kelvin", "percentage"}:
            raise self.ha.Problem("Unsupported device control.")
        body = {}
        service = "turn_on"
        if "state" in desired:
            if desired["state"] not in ("on", "off") or len(desired) != 1:
                raise self.ha.Problem("Choose an explicit on or off state.")
            service = "turn_" + desired["state"]
        if "percentage" in desired:
            if len(desired) != 1:
                raise self.ha.Problem("Choose one fan control at a time.")
            service = "set_percentage"
        for key, capability, minimum, maximum, field in (
            ("percentage", "speed_control", 0, 100, "percentage"),
            ("brightness", "dimmable", 1, 100, "brightness_pct"),
            ("kelvin", "temperature", entry["min_kelvin"], entry["max_kelvin"], "color_temp_kelvin"),
        ):
            if key in desired:
                value = desired[key]
                if not entry[capability] or type(value) not in (int, float) or not math.isfinite(value) or not minimum <= value <= maximum:
                    raise self.ha.Problem("This device does not support that value.")
                body[field] = round(value)
        pending = {"desired": dict(desired), "confirmed": asyncio.Event()}
        self.pending[entity_id] = pending
        self.error = ""
        self.publish()
        try:
            await self.rpc("call_service", domain=entity_id.split(".")[0], service=service,
                           service_data=body, target={"entity_id": entity_id})
            self.settle(entity_id)
            await asyncio.wait_for(pending["confirmed"].wait(), 12)
        except TimeoutError:
            raise self.ha.Problem("The device has not confirmed the change. Check its state before retrying.") from None
        finally:
            self.pending.pop(entity_id, None)
            self.publish()

    async def command(self, message):
        request = message.get("request")
        try:
            action = message.get("action")
            if action in ("setup", "preferences"):
                async with self.settings_lock:
                    if self.pending:
                        raise self.ha.Problem("Wait for device changes to finish before editing settings.")
                    await (self.setup(message) if action == "setup" else self.preferences(message))
            elif action == "set":
                if self.settings_lock.locked():
                    raise self.ha.Problem("Wait for settings to finish saving.")
                await self.control(message)
            elif action == "catalog":
                self.catalog_open = bool(message.get("open", True))
                self.publish()
            elif action == "refresh":
                async with self.settings_lock:
                    if not self.config:
                        await self.bootstrap()
                    elif not self.connected:
                        await self.restart()
                self.publish()
            else:
                raise self.ha.Problem("Unknown Home Assistant action.")
            self.emit({"request": request, "ok": True})
        except Exception as error:
            self.error = str(error) if isinstance(error, self.ha.Problem) else "Home Assistant could not complete the request."
            self.emit({"request": request, "ok": False, "error": self.error})
            self.publish()

    async def bootstrap(self):
        try:
            self.config = await asyncio.to_thread(self.ha.load_config)
            if self.config and self.config.pop("legacy", False):
                await asyncio.to_thread(self.ha.secret, "store", self.config["url"], self.config["token"])
                await asyncio.to_thread(self.ha.save_config, self.config)
            await self.restart()
        except Exception as error:
            self.config = None
            self.error = str(error) if isinstance(error, self.ha.Problem) else "Could not load the Home Assistant connection."
        self.publish()

    async def run(self):
        # Pipe EOF owns lifetime; no thread blocked on stdin can hold shutdown open.
        reader = asyncio.StreamReader(limit=self.ha.MAX_CONFIG + 1)
        transport, _ = await asyncio.get_running_loop().connect_read_pipe(lambda: asyncio.StreamReaderProtocol(reader), sys.stdin)
        self.publish()
        bootstrap = self.spawn(self.bootstrap())
        try:
            while line := await reader.readline():
                try:
                    message = json.loads(line)
                    if not isinstance(message, dict):
                        continue
                    await bootstrap
                    if len(self.tasks) < 40:
                        self.spawn(self.command(message))
                except (ValueError, TypeError):
                    self.emit({"ok": False, "error": "Invalid Home Assistant request."})
        finally:
            transport.close()
            for task in list(self.tasks):
                task.cancel()
            await asyncio.gather(*self.tasks, return_exceptions=True)
