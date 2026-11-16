"""Start injection bot: link this proxy's live game socket to the OpenAuto daemon.

The daemon runs its normal bot engine over the browser's socket instead of logging in itself
(`rust_distributable/docs/architecture/injection-mode.md`). This side is only a pipe:

* every game frame the proxy sees is forwarded: `{"dir": "s2c" | "c2s", "text": ...}` (the bot's own
  injections are not echoed back as c2s, so the daemon can tell the person's clicks from its own frames);
* each `{"inject": frame}` the daemon sends is written to the game server through `inject.websocket`.

The daemon speaks WebSocket at `ws://127.0.0.1:47821/v1/attach?account=<id>`. mitmproxy ships `wsproto`, so the
client is a small asyncio loop over it, no extra package. Closing the link stops the injection bot; the browser
keeps playing.
"""
from __future__ import annotations

import asyncio
import json
from urllib.parse import quote, urlparse

from wsproto import ConnectionType, WSConnection
from wsproto.events import (AcceptConnection, CloseConnection, Ping, RejectConnection, RejectData, Request,
                            TextMessage)

DEFAULT_DAEMON = "http://127.0.0.1:47821"
QUEUE_LIMIT = 2000


def frame_message(direction: str, text: str) -> str:
    """One forwarded frame, as the daemon expects it."""
    if direction not in ("s2c", "c2s"):
        raise ValueError("direction must be s2c or c2s")
    return json.dumps({"dir": direction, "text": text})


def injected_frame(message: str):
    """The frame to send to the server from a daemon message, or None for anything else."""
    try:
        body = json.loads(message)
    except ValueError:
        return None
    frame = body.get("inject") if isinstance(body, dict) else None
    return frame if isinstance(frame, str) and frame.startswith("%xt%") else None


def attach_target(daemon: str, account: str) -> tuple[str, int, str]:
    url = urlparse(daemon or DEFAULT_DAEMON)
    if url.hostname not in ("127.0.0.1", "localhost", "::1"):
        raise ValueError("the OpenAuto daemon must be on this machine")
    return url.hostname, url.port or 80, f"/v1/attach?account={quote(account.strip().lower())}"


class OpenAutoLink:
    def __init__(self, inject):
        self.inject = inject          # callable(frame: str) -> None, raises when there is no game socket
        self.task = None
        self.queue = None
        self.account = None
        self.state = "off"            # off | connecting | linked | failed
        self.error = None
        self.injected = 0

    def status(self) -> dict:
        return {"state": self.state, "account": self.account, "error": self.error, "injected": self.injected}

    def start(self, account: str, daemon: str = DEFAULT_DAEMON):
        self.stop()
        self.account = account.strip().lower()
        self.error = None
        self.injected = 0
        self.state = "connecting"
        self.queue = asyncio.Queue(QUEUE_LIMIT)
        self.task = asyncio.create_task(self._run(daemon))

    def stop(self):
        if self.task and not self.task.done():
            self.task.cancel()
        self.task = None
        self.queue = None
        if self.state != "failed":
            self.state = "off"

    def forward(self, direction: str, text: str):
        """Called from the proxy's message hook (same event loop). Drops a frame rather than block the game."""
        if self.queue is None or self.state != "linked":
            return
        try:
            self.queue.put_nowait(frame_message(direction, text))
        except asyncio.QueueFull:
            pass

    async def _run(self, daemon: str):
        writer = None
        try:
            host, port, target = attach_target(daemon, self.account)
            reader, writer = await asyncio.open_connection(host, port)
            ws = WSConnection(ConnectionType.CLIENT)
            writer.write(ws.send(Request(host=f"{host}:{port}", target=target)))
            await writer.drain()
            sender = None
            partial = ""
            refused = b""
            while True:
                data = await reader.read(65536)
                ws.receive_data(data or None)
                for event in ws.events():
                    if isinstance(event, AcceptConnection):
                        self.state = "linked"
                        sender = asyncio.create_task(self._send(ws, writer))
                    elif isinstance(event, RejectConnection):
                        refused = b""
                    elif isinstance(event, RejectData):
                        refused += event.data
                        if event.body_finished:
                            raise RuntimeError(_refusal(refused))
                    elif isinstance(event, TextMessage):
                        partial += event.data
                        if event.message_finished:
                            frame = injected_frame(partial)
                            partial = ""
                            if frame:
                                self.inject(frame)
                                self.injected += 1
                    elif isinstance(event, Ping):
                        writer.write(ws.send(event.response()))
                    elif isinstance(event, CloseConnection):
                        raise RuntimeError("the daemon closed the link")
                if not data:
                    raise RuntimeError("the daemon went away")
        except asyncio.CancelledError:
            self.state = "off"
            raise
        except Exception as exc:  # the link must never break the game connection
            self.state = "failed"
            self.error = str(exc)
            print(f"OpenAuto link: {exc!r}", flush=True)
        finally:
            if writer is not None:
                writer.close()

    async def _send(self, ws, writer):
        while True:
            message = await self.queue.get()
            writer.write(ws.send(TextMessage(data=message)))
            await writer.drain()


def _refusal(body: bytes) -> str:
    try:
        return json.loads(body.decode("utf-8")).get("error") or "the daemon refused the link"
    except (ValueError, UnicodeDecodeError, AttributeError):
        return "the daemon refused the link"
