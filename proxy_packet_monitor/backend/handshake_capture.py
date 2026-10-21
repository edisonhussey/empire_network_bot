"""Records the game's full connection handshake while the page's "Record full handshake" box is ticked.

Everything the frame recorder cannot see lives here: the WebSocket upgrade request and response (every header, in
the order and case they were sent, so `Sec-WebSocket-Extensions`/`permessage-deflate` is visible), the TLS facts the
proxy learned on both legs, and the game-host HTTP requests made in the minute before the socket opened (the
"pre-login" part of a login).

A record is attached to the session being recorded (see `Recorder.handshake`) and is shown on that connection's
LOGIN event. The newest record is also written to `<data_dir>/handshake_latest.json` so it can be read without the page.

What is hidden: the values of `Cookie`, `Set-Cookie` and `Authorization` headers (cookie names are kept) and any JSON
field that looks like a password or token in a captured body. Everything else is recorded as seen.

Seen through the proxy: the request is what the browser sent (the proxy forwards it unchanged), and the response is
what the server answered. The *server-side TLS* facts are the proxy's own connection, not the browser's.
"""
from __future__ import annotations
import json
import re
import time
from collections import deque
from pathlib import Path
from urllib.parse import urlsplit

GAME_SUFFIXES = ("goodgamestudios.com", "ggs-ep.com")
SENSITIVE_HEADERS = {"cookie", "set-cookie", "authorization", "proxy-authorization"}
STATIC_TYPES = ("image/", "audio/", "video/", "font/")
STATIC_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".svg", ".mp3", ".ogg", ".wav", ".mp4",
                   ".woff", ".woff2", ".ttf", ".otf", ".css"}
WINDOW_SECONDS = 90          # how far back "requests just before the socket" reaches
MAX_BEFORE = 40              # most entries kept in that list
BUFFER = 120                 # recent exchanges held in memory while the box is ticked
REQUEST_BODY_PREVIEW = 1500
RESPONSE_BODY_PREVIEW = 600
SECRET_FIELD = re.compile(r'("(?:[^"\\]*(?:token|password|passwd|secret|auth)[^"\\]*|PW|LT|RCT)"\s*:\s*)"[^"]*"', re.I)


def is_game_host(host: str | None) -> bool:
    host = (host or "").lower()
    return any(host == suffix or host.endswith("." + suffix) for suffix in GAME_SUFFIXES)


def _text(value) -> str:
    return value.decode("utf-8", "replace") if isinstance(value, (bytes, bytearray)) else str(value)


def hide_value(name: str, value: str) -> str:
    """The header value to store: secrets are replaced by a note that keeps only what helps (cookie names)."""
    lowered = name.lower()
    if lowered not in SENSITIVE_HEADERS:
        return value
    if lowered == "cookie":
        names = [part.split("=", 1)[0].strip() for part in value.split(";") if part.strip()]
        return f"<hidden {len(value)} chars; cookie names: {', '.join(names) or 'none'}>"
    if lowered == "set-cookie":
        return f"<hidden {len(value)} chars; sets: {value.split('=', 1)[0].strip()}>"
    scheme = value.split(" ", 1)[0] if " " in value else ""
    return f"<hidden {len(value)} chars{'; scheme ' + scheme if scheme else ''}>"


def header_list(headers) -> list[list[str]]:
    """`[[name, value], ...]` in wire order and original case, secrets hidden."""
    rows = []
    for name, value in headers.fields:
        name, value = _text(name), _text(value)
        rows.append([name, hide_value(name, value)])
    return rows


def header_value(rows: list[list[str]], name: str) -> str | None:
    values = [value for key, value in rows if key.lower() == name.lower()]
    return ", ".join(values) if values else None


def hide_body_secrets(text: str) -> str:
    return SECRET_FIELD.sub(r'\1"<hidden>"', text)


def body_preview(message, limit: int, always: bool = False) -> dict | None:
    """A short, secret-scrubbed text preview of a body, or None when there is nothing useful to show."""
    raw = message.raw_content or b""
    if not raw:
        return None
    content_type = (message.headers.get("content-type") or "").lower()
    texty = always or any(kind in content_type for kind in ("json", "xml", "text", "javascript", "x-www-form"))
    if not texty:
        return {"size": len(raw), "text": None}
    try:
        text = message.get_text(strict=False) or ""
    except (ValueError, LookupError):
        text = raw[:limit * 4].decode("utf-8", "replace")
    text = hide_body_secrets(text)
    return {"size": len(raw), "text": text[:limit], "truncated": len(text) > limit}


def tls_facts(connection) -> dict:
    """What the proxy knows about one TLS leg. Missing values stay None rather than failing the capture."""
    def get(name, default=None):
        value = getattr(connection, name, default)
        return value.decode("ascii", "replace") if isinstance(value, bytes) else value
    offers = get("alpn_offers") or []
    return {"version": get("tls_version"), "cipher": get("cipher"), "alpn": get("alpn"), "sni": get("sni"),
            "alpn_offered": [_text(item) for item in offers],
            "ciphers_offered": list(get("cipher_list") or [])}


def extensions_summary(request_rows: list[list[str]], response_rows: list[list[str]]) -> dict:
    offered = header_value(request_rows, "sec-websocket-extensions")
    accepted = header_value(response_rows, "sec-websocket-extensions")
    return {"offered": offered, "accepted": accepted,
            "permessage_deflate_offered": bool(offered and "permessage-deflate" in offered.lower()),
            "permessage_deflate_accepted": bool(accepted and "permessage-deflate" in accepted.lower())}


class HandshakeCapture:
    def __init__(self, flag_path: Path, latest_path: Path | None = None):
        self.flag_path = flag_path
        self.latest_path = latest_path
        self._flag = (None, False)
        self._recent: deque = deque(maxlen=BUFFER)
        self.captured = 0
        self.last_at: float | None = None
        self.last_summary: dict | None = None

    # -- the checkbox ---------------------------------------------------------------------
    def enabled(self) -> bool:
        """The "Record full handshake" box. The flag file is re-read only when it changes."""
        try:
            stamp = self.flag_path.stat().st_mtime_ns
        except OSError:
            self._flag = (None, False)
            return False
        if self._flag[0] != stamp:
            try:
                self._flag = (stamp, bool(json.loads(self.flag_path.read_text(encoding="utf-8")).get("enabled")))
            except (OSError, ValueError, AttributeError):
                self._flag = (stamp, False)
        return self._flag[1]

    def stats(self) -> dict:
        return {"enabled": self.enabled(), "captured": self.captured, "last_at": self.last_at,
                "last": self.last_summary}

    # -- the game-host requests that come before the socket -----------------------------------
    def note_http(self, flow, now: float | None = None) -> None:
        """Remember a finished game-host request (called for every proxied response while the box is ticked)."""
        request, reply = flow.request, flow.response
        if reply is None or not is_game_host(request.host):
            return
        if reply.status_code == 101:                      # the upgrade itself is recorded by `snapshot`
            return
        content_type = (reply.headers.get("content-type") or "").lower()
        suffix = Path(urlsplit(request.path).path).suffix.lower()
        if content_type.startswith(STATIC_TYPES) or suffix in STATIC_SUFFIXES:
            return
        entry = {"t": request.timestamp_start or now or time.time(), "method": request.method,
                 "url": request.pretty_url, "status": reply.status_code, "http_version": request.http_version,
                 "content_type": content_type or None, "response_size": len(reply.raw_content or b""),
                 "request_headers": header_list(request.headers), "response_headers": header_list(reply.headers)}
        if request.method in ("POST", "PUT", "PATCH"):
            entry["request_body"] = body_preview(request, REQUEST_BODY_PREVIEW, always=True)
        preview = body_preview(reply, RESPONSE_BODY_PREVIEW)
        if preview and preview.get("text") is not None:
            entry["response_body"] = preview
        self._recent.append(entry)

    # -- the upgrade ---------------------------------------------------------------------------
    def snapshot(self, flow, now: float | None = None) -> dict:
        """Everything known about the handshake of this WebSocket flow, as a JSON-ready dict."""
        now = now or time.time()
        request, reply = flow.request, flow.response
        request_rows = header_list(request.headers)
        response_rows = header_list(reply.headers) if reply is not None else []
        opened = request.timestamp_start or now
        before = [entry for entry in self._recent if opened - WINDOW_SECONDS <= entry["t"] <= opened + 1.0]
        record = {
            "version": 1, "captured_at": now, "url": request.pretty_url,
            "request": {"line": f"{request.method} {request.path} {request.http_version}", "headers": request_rows},
            "response": {"line": f"{reply.http_version} {reply.status_code} {reply.reason}" if reply is not None else None,
                        "headers": response_rows},
            "extensions": extensions_summary(request_rows, response_rows),
            "tls": {"browser_to_proxy": tls_facts(flow.client_conn), "proxy_to_server": tls_facts(flow.server_conn)},
            "before": before[-MAX_BEFORE:],
            "notes": ["Request headers are what the browser sent and the proxy forwarded; the response is the server's.",
                      "proxy_to_server TLS is the proxy's own connection, not the browser's."],
        }
        self.captured += 1
        self.last_at = now
        self.last_summary = {"url": record["url"], "status": reply.status_code if reply is not None else None,
                             "permessage_deflate_offered": record["extensions"]["permessage_deflate_offered"],
                             "permessage_deflate_accepted": record["extensions"]["permessage_deflate_accepted"],
                             "requests_before": len(record["before"])}
        self._write_latest(record)
        return record

    def _write_latest(self, record: dict) -> None:
        if self.latest_path is None:
            return
        try:
            temporary = self.latest_path.with_suffix(".tmp")
            temporary.write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
            temporary.replace(self.latest_path)
        except OSError as exc:
            print(f"handshake latest write failed: {exc!r}", flush=True)
