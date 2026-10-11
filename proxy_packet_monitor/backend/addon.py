"""Receive-only mitmproxy addon, isolated from the automation listener."""
from __future__ import annotations
import asyncio
import json
import os
import sys
import time
from pathlib import Path

from mitmproxy import http

# mitmdump executes addons as scripts, so make the local package importable.
BASE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BASE))
from backend.protocol import Decoder, matches_game_url
from backend.storage import Recorder

DATA = Path(os.environ.get("PACKET_MONITOR_DATA", BASE / "data")).expanduser().resolve()
DATA.mkdir(parents=True, exist_ok=True)
COMMAND = DATA / "control.json"
STATUS = DATA / "status.json"


class PacketMonitor:
    def __init__(self):
        self.decoder = Decoder()
        self.recorder = Recorder(DATA / "sessions")
        self.last_command = None
        self.task = None
        self.connected = False
        self.endpoint = None
        self._status()

    def _status(self):
        body = {"recording": self.recorder.active, "session_id": self.recorder.session_id if self.recorder.active else None,
                "connected": self.connected, "endpoint": self.endpoint, "updated_at": time.time()}
        temporary = STATUS.with_suffix(".tmp")
        temporary.write_text(json.dumps(body), encoding="utf-8")
        temporary.replace(STATUS)

    def load(self, loader):
        self.task = asyncio.create_task(self._poll())

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
            await asyncio.sleep(0.12)

    def websocket_start(self, flow: http.HTTPFlow):
        if matches_game_url(flow.request.url):
            self.connected = True
            self.endpoint = flow.request.url
            self._status()

    def websocket_message(self, flow: http.HTTPFlow):
        if not matches_game_url(flow.request.url) or not flow.websocket:
            return
        message = flow.websocket.messages[-1]
        if message.from_client:
            return
        # Timestamp and enqueue only; parsing, summaries and disk writes are off path.
        received_at = getattr(message, "timestamp", None) or time.time()
        decoded = self.decoder.decode(message.content, str(id(flow)), message.is_text)
        if decoded.startswith(("TEXT FRAME; base64:", "BINARY FRAME; base64:")):
            return
        self.recorder.capture(decoded, received_at)
        # Long lived sockets otherwise retain every intercepted frame in RAM.
        if len(flow.websocket.messages) > 8:
            del flow.websocket.messages[:-8]

    def websocket_end(self, flow: http.HTTPFlow):
        self.decoder.forget(str(id(flow)))
        if matches_game_url(flow.request.url):
            self.connected = False
            self._status()

    def done(self):
        if self.task:
            self.task.cancel()
        self.recorder.close()
        self._status()


addons = [PacketMonitor()]
