#!/usr/bin/env python3
"""One-shot GGE handshake probe for the pingpoko account (WORLD2).

Purpose: find out what the game server does now that the WebSocket is expected to negotiate
`permessage-deflate`. It makes ONE connection, offers permessage-deflate exactly as a browser does,
logs the upgrade result and every frame in both directions (secrets redacted), walks the known
login sequence reactively only as far as the server lets it, and closes itself after --seconds
(default 5.0). It never retries and never touches any existing bot code.

Run from the project root, with the project venv (it has wsproto):

    venv/bin/python probe_pingpoko_ws.py

Output goes to the terminal and to probe_logs/pingpoko_<timestamp>.log (passwords/tokens redacted).
"""
from __future__ import annotations

import argparse
import json
import re
import select
import socket
import ssl
import sys
import time
from datetime import datetime
from pathlib import Path
from urllib.parse import urlparse

from wsproto import ConnectionType, WSConnection
from wsproto.events import (
    AcceptConnection,
    BytesMessage,
    CloseConnection,
    Ping,
    Pong,
    RejectConnection,
    RejectData,
    Request,
    TextMessage,
)
from wsproto.extensions import PerMessageDeflate

HERE = Path(__file__).resolve().parent
ORIGIN = "https://empire.goodgamestudios.com"          # goes in lli "REF" (as the browser sends it)
WS_ORIGIN = "https://empire-html5.goodgamestudios.com"  # the browser's Origin header on the upgrade (captured 2026-10-21)
BROWSER_HEADERS = [  # as captured from Chrome through the proxy, minus Cookie (consent/analytics cookies only)
    (b"pragma", b"no-cache"), (b"cache-control", b"no-cache"),
    (b"user-agent", b"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36"),
    (b"origin", WS_ORIGIN.encode()), (b"accept-encoding", b"gzip, deflate, br, zstd"), (b"accept-language", b"en-GB,en-US;q=0.9,en;q=0.8"),
]
SERVER_HEADERS = {"WORLD2": "EmpireEx_49"}  # world2 -> EmpireEx_49 in the Sept capture
SECRET = re.compile(r'("(?:PW|LT|RCT)"\s*:\s*)"[^"]*"')


class BrowserDeflate(PerMessageDeflate):
    """Offers `permessage-deflate; client_max_window_bits`, which is what Chrome/Firefox send.

    wsproto's stock offer adds explicit client/server window sizes, which a strict server could
    treat differently from a browser.
    """

    def offer(self) -> str:
        return "client_max_window_bits"


def read_env(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            values[key.strip().upper()] = value.strip()
    return values


class Probe:
    def __init__(self, args: argparse.Namespace, creds: dict[str, str]) -> None:
        self.args, self.creds = args, creds
        self.t0 = time.monotonic()
        self.lines: list[str] = []
        self.wire_in = self.wire_out = self.decoded_in = 0
        self.sent_login = self.sent_lli = self.done = False
        self.header = args.header

    def log(self, text: str) -> None:
        line = f"[{time.monotonic() - self.t0:6.3f}s] {text}"
        print(line, flush=True)
        self.lines.append(line)

    @staticmethod
    def clean(text: str, limit: int = 1500) -> str:
        text = SECRET.sub(r'\1"<redacted>"', text)
        return text if len(text) <= limit else f"{text[:limit]}...(+{len(text) - limit} chars)"

    # -- sending ---------------------------------------------------------------------------
    def send_text(self, ws: WSConnection, sock: socket.socket, text: str) -> None:
        data = ws.send(TextMessage(data=text))
        self.wire_out += len(data)
        sock.sendall(data)
        self.log(f"SEND text {len(text)}B (wire {len(data)}B): {self.clean(text)}")

    def lli(self) -> str:
        payload = {
            "CONM": 676, "RTM": 118, "ID": 0, "PL": 1, "NOM": self.creds["USERNAME"],
            "PW": self.creds["PASSWORD"], "LT": None, "LANG": "en", "DID": "0",
            "AID": self.creds.get("AID", ""), "KID": "", "REF": ORIGIN, "GCI": "",
            "SID": 9, "PLFID": 1,
        }
        return f"%xt%{self.header}%lli%1%{json.dumps(payload, separators=(',', ':'))}%"

    # -- reacting to the server (only the sequence that is already documented) ----------------
    def react(self, ws: WSConnection, sock: socket.socket, text: str) -> None:
        if "action='apiOK'" in text or 'action="apiOK"' in text:
            self.send_text(ws, sock, (
                f"<msg t='sys'><body action='login' r='0'><login z='{self.header}'><nick><![CDATA[]]></nick>"
                f"<pword><![CDATA[{self.args.client_version}%en%0]]></pword></login></body></msg>"))
        elif "action='joinOK'" in text or 'action="joinOK"' in text:
            self.send_text(ws, sock, "<msg t='sys'><body action='roundTrip' r='1'></body></msg>")
            self.send_text(ws, sock, f"%xt%{self.header}%vck%1%{self.args.client_version}%web-html5%<RoundHouseKick>%5.659797992169144e+307%")
        elif text.startswith("%xt%rlu%"):
            self.send_text(ws, sock, "<msg t='sys'><body action='autoJoin' r='-1'></body></msg>")
        elif text.startswith("%xt%vck%"):
            status = text.strip("%").split("%")[3] if text.count("%") >= 5 else "?"
            if status == "0" and not self.sent_lli:
                self.sent_lli = True
                self.send_text(ws, sock, self.lli())
            elif status != "0":
                self.log(f"RESULT vck refused (status {status}): client version not accepted; not sending credentials")
                self.done = True
        elif text.startswith("%xt%lli%"):
            fields = text.strip("%").split("%")
            self.log(f"RESULT lli status={fields[3] if len(fields) > 3 else '?'}")
            self.done = True

    # -- main --------------------------------------------------------------------------------
    def run(self) -> None:
        url = urlparse(self.args.url)
        host, secure = url.hostname or "", url.scheme == "wss"
        port = url.port or (443 if secure else 80)
        if not secure and host not in ("127.0.0.1", "localhost"):
            sys.exit("refusing a non-TLS connection to a remote host")
        self.log(f"CONNECT {self.args.url} header={self.header} version={self.args.client_version} "
                 f"user={self.creds['USERNAME']} limit={self.args.seconds}s (single attempt, no retry)")
        raw = socket.create_connection((host, port), timeout=self.args.seconds)
        if secure:
            try:
                import certifi
                ctx = ssl.create_default_context(cafile=certifi.where())
            except ImportError:
                ctx = ssl.create_default_context()
            raw = ctx.wrap_socket(raw, server_hostname=host)
        sock = raw
        self.log(f"TLS/TCP up: {getattr(sock, 'version', lambda: 'plain')()}")

        ws = WSConnection(ConnectionType.CLIENT)
        request = ws.send(Request(host=url.netloc, target=url.path or "/",
                                  extensions=[BrowserDeflate()],
                                  extra_headers=BROWSER_HEADERS))
        self.wire_out += len(request)
        sock.sendall(request)
        self.log("SEND upgrade request: Origin set, Sec-WebSocket-Extensions: permessage-deflate; client_max_window_bits (browser style)")

        deadline = self.t0 + self.args.seconds
        pending_text = ""
        pending_bytes = b""
        closed = False
        silent_noted = False
        while not self.done and not closed:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                self.log(f"DEADLINE {self.args.seconds}s reached, closing")
                break
            # TLS may already hold decrypted bytes that select() cannot see.
            buffered = getattr(sock, "pending", lambda: 0)()
            ready = buffered or select.select([sock], [], [], min(remaining, 0.25))[0]
            if not ready:
                if not silent_noted and time.monotonic() - self.t0 > 2.0 and not self.wire_in:
                    self.log("NOTE nothing received from the server after 2s")
                    silent_noted = True
                continue
            try:
                chunk = sock.recv(65536)
            except (ssl.SSLWantReadError, BlockingIOError):
                continue
            if not chunk:
                self.log("SERVER closed the TCP connection")
                closed = True
                break
            self.wire_in += len(chunk)
            ws.receive_data(chunk)
            for event in ws.events():
                if isinstance(event, AcceptConnection):
                    names = [f"{e.name}(enabled={e.enabled()})" for e in event.extensions] or ["none"]
                    self.log(f"UPGRADE accepted (101). negotiated extensions: {', '.join(names)}; "
                             f"subprotocol={event.subprotocol}; extra headers={event.extra_headers}")
                    # First documented step of the sequence. Anything new the server says before
                    # or instead of apiOK is logged by the RECV branch below.
                    self.send_text(ws, sock, "<msg t='sys'><body action='verChk' r='0'><ver v='166' /></body></msg>")
                elif isinstance(event, RejectConnection):
                    self.log(f"UPGRADE REJECTED status={event.status_code} headers={event.headers}")
                    if not event.has_body:
                        self.done = True
                elif isinstance(event, RejectData):
                    self.log(f"REJECT BODY: {self.clean(event.data.decode('utf-8', 'replace'))}")
                    if event.body_finished:
                        self.done = True
                elif isinstance(event, (TextMessage, BytesMessage)):
                    is_text = isinstance(event, TextMessage)
                    if is_text:
                        pending_text += event.data
                    else:
                        pending_bytes += event.data
                    if not event.message_finished:
                        continue
                    if is_text:
                        text, kind = pending_text, "text"
                        pending_text = ""
                    else:
                        try:
                            text, kind = pending_bytes.decode("utf-8"), "binary(utf-8)"
                        except UnicodeDecodeError:
                            text, kind = f"<non-utf8 {len(pending_bytes)}B head={pending_bytes[:48].hex()}>", "binary(raw)"
                        pending_bytes = b""
                    self.decoded_in += len(text)
                    self.log(f"RECV {kind} {len(text)}B: {self.clean(text)}")
                    self.react(ws, sock, text)
                elif isinstance(event, Ping):
                    sock.sendall(ws.send(event.response()))
                    self.log("RECV ping -> pong")
                elif isinstance(event, Pong):
                    self.log("RECV pong")
                elif isinstance(event, CloseConnection):
                    self.log(f"SERVER close frame code={event.code} reason={event.reason!r}")
                    closed = True
                    try:
                        sock.sendall(ws.send(event.response()))
                    except OSError:
                        pass
                    break
        if not closed:
            try:
                sock.sendall(ws.send(CloseConnection(code=1000, reason="probe done")))
                self.log("SEND close frame 1000")
                time.sleep(0.2)
            except Exception as error:  # noqa: BLE001 - the probe is ending anyway
                self.log(f"close send failed: {error!r}")
        try:
            sock.close()
        except OSError:
            pass
        self.log(f"SUMMARY elapsed={time.monotonic() - self.t0:.2f}s wire_in={self.wire_in}B "
                 f"decoded_in={self.decoded_in}B wire_out={self.wire_out}B lli_sent={self.sent_lli}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--env", default=str(HERE / "credentials" / "pingpoko.env"))
    parser.add_argument("--url", help="default: derived from SERVER in the env file")
    parser.add_argument("--header", help="xt server header (default: EmpireEx_49 for WORLD2)")
    parser.add_argument("--client-version", default="1171008")
    parser.add_argument("--seconds", type=float, default=5.0)
    args = parser.parse_args()

    creds = read_env(Path(args.env))
    world = creds.get("SERVER", "").upper()
    match = re.fullmatch(r"WORLD(\d+)", world)
    if not args.url:
        if not match:
            sys.exit(f"cannot derive an endpoint from SERVER={world!r}; pass --url")
        args.url = f"wss://ep-live-world{match.group(1)}-game.goodgamestudios.com/"
    args.header = args.header or SERVER_HEADERS.get(world)
    if not args.header:
        sys.exit(f"no known server header for {world!r}; pass --header")

    probe = Probe(args, creds)
    try:
        probe.run()
    finally:
        out = HERE / "probe_logs"
        out.mkdir(exist_ok=True)
        path = out / f"pingpoko_{datetime.now():%Y%m%d_%H%M%S}.log"
        path.write_text("\n".join(probe.lines) + "\n")
        print(f"log written: {path}")


if __name__ == "__main__":
    main()
