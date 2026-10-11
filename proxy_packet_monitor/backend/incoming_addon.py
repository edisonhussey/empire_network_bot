"""Experimental proxy copy: recording plus asynchronous GAM observation."""
from __future__ import annotations
import asyncio
import json
import os
import sys
import time
import urllib.request
from pathlib import Path
from queue import SimpleQueue
from threading import Thread

from mitmproxy import http

# mitmdump executes addons as scripts, so make the local package importable.
BASE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BASE))
from backend.protocol import Decoder, matches_game_url
from backend.storage import Recorder
from backend.auto_support import AutoSupport, SupportNotSent, packet as support_packet
from mitmproxy import ctx

DATA = Path(os.environ.get("PACKET_MONITOR_DATA", BASE / "data")).expanduser().resolve()
DATA.mkdir(parents=True, exist_ok=True)
COMMAND = DATA / "control.json"
STATUS = DATA / "status.json"
API = os.environ.get("PACKET_MONITOR_API", "http://127.0.0.1:8798")


class PacketMonitor:
    def __init__(self):
        self.decoder = Decoder()
        self.recorder = Recorder(Path(os.environ.get("PACKET_MONITOR_SESSIONS", DATA / "sessions")))
        self.last_command = None
        self.task = None
        self.connected = False
        self.endpoint = None
        self.last_websocket_url = None
        self.active_flow = None
        self.server_header = None
        self.auto_support = AutoSupport(DATA / "auto_support.json", DATA / "auto_support_status.json",
                                        Path(os.environ.get("PACKET_MONITOR_SESSIONS", DATA / "sessions")))
        self.incoming_queue = SimpleQueue()
        self.incoming_worker = Thread(target=self._send_incoming, daemon=True)
        self.incoming_worker.start()
        self._status()

    def _send_incoming(self):
        while True:
            item = self.incoming_queue.get()
            if item is None:
                return
            raw, received_at = item
            body = json.dumps({"raw": raw, "received_at": received_at}).encode("utf-8")
            try:
                request = urllib.request.Request(
                    API + "/api/incoming/observe", data=body,
                    headers={"Content-Type": "application/json"}, method="POST")
                with urllib.request.urlopen(request, timeout=2):
                    pass
            except Exception as exc:
                print(f"incoming GAM delivery failed: {exc!r}", flush=True)

    def _status(self):
        body = {"recording": self.recorder.active, "session_id": self.recorder.session_id if self.recorder.active else None,
                "connected": self.connected, "endpoint": self.endpoint,
                "last_websocket_url": self.last_websocket_url, "updated_at": time.time()}
        temporary = STATUS.with_suffix(".tmp")
        temporary.write_text(json.dumps(body), encoding="utf-8")
        temporary.replace(STATUS)

    def load(self, loader):
        self.task = asyncio.create_task(self._poll())

    def _send_support(self, command, payload):
        flow = self.active_flow
        if not flow or not flow.websocket or flow.websocket.timestamp_end is not None:
            raise SupportNotSent("no active game WebSocket")
        if not self.server_header:
            raise SupportNotSent("server XT header has not been observed")
        frame = support_packet(self.server_header, command, payload)
        ctx.master.commands.call("inject.websocket", flow, False, frame.encode("utf-8"))

    async def _poll(self):
        while True:
            try:
                if COMMAND.exists():
                    body = json.loads(COMMAND.read_text(encoding="utf-8"))
                    if body.get("id") != self.last_command:
                        self.last_command = body.get("id")
                        if body.get("action") == "toggle":
                            self.recorder.stop() if self.recorder.active else self.recorder.start()
                        elif body.get("action") == "start":
                            self.recorder.start()
                        elif body.get("action") == "stop":
                            self.recorder.stop()
                        self._status()
            except (OSError, ValueError) as exc:
                print(f"packet monitor control error: {exc!r}", flush=True)
            self.auto_support.tick(self._send_support, self.active_flow is not None)
            await asyncio.sleep(0.05)

    def websocket_start(self, flow: http.HTTPFlow):
        self.last_websocket_url = flow.request.url
        if matches_game_url(flow.request.url):
            if self.active_flow is not None and self.active_flow is not flow:
                self.auto_support.connection_lost()
            self.connected = True
            self.endpoint = flow.request.url
            self.active_flow = flow
        self._status()

    def websocket_message(self, flow: http.HTTPFlow):
        if not matches_game_url(flow.request.url) or not flow.websocket:
            return
        message = flow.websocket.messages[-1]
        # Timestamp and enqueue only; parsing, summaries and disk writes are off path.
        received_at = getattr(message, "timestamp", None) or time.time()
        decoded = self.decoder.decode(message.content, f"{id(flow)}:{message.from_client}", message.is_text)
        if decoded.startswith(("TEXT FRAME; base64:", "BINARY FRAME; base64:")):
            return
        self.recorder.capture(decoded, received_at, "sent" if message.from_client else "received")
        if message.from_client and flow is self.active_flow:
            parsed = self._client_packet(decoded)
            if parsed and parsed.get("server_header"):
                self.server_header = parsed["server_header"]
        elif not message.from_client and flow is self.active_flow:
            self.auto_support.observe(decoded, received_at)
        if not message.from_client and decoded.startswith(("%xt%gam%", "%xt%mrm%")):
            self.incoming_queue.put((decoded, received_at))
        # Long lived sockets otherwise retain every intercepted frame in RAM.
        if len(flow.websocket.messages) > 8:
            del flow.websocket.messages[:-8]

    def websocket_end(self, flow: http.HTTPFlow):
        self.decoder.forget(f"{id(flow)}:True")
        self.decoder.forget(f"{id(flow)}:False")
        if matches_game_url(flow.request.url) and self.active_flow is flow:
            self.auto_support.connection_lost()
            self.connected = False
            self.active_flow = None
            self.server_header = None
            self._status()

    @staticmethod
    def _client_packet(decoded):
        from backend.protocol import parse_xt_packet
        return parse_xt_packet(decoded)

    def done(self):
        if self.task:
            self.task.cancel()
        self.recorder.close()
        self.incoming_queue.put(None)
        self.incoming_worker.join(timeout=3)
        self._status()


addons = [PacketMonitor()]
