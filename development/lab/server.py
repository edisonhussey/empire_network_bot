#!/usr/bin/env python3
"""Local workbench for OpenAuto — a control panel that talks to the running
daemon, plus an offline simulator over recorded traffic.

Why this exists
---------------
Iterating on the Rust means a compile, a bundle, an install and a login before
anything can be observed. This runs from the repository with no build step and
no dependencies, so a question like "does the server really answer four
farmsteads in one `gaa` window?" can be answered by hand, slowly, while the bot
keeps running.

The two rules this process enforces itself
------------------------------------------
1. **Allowlist.** Only the daemon paths in `ALLOWED` are forwarded. The panel
   therefore cannot be turned into a general-purpose proxy for anything else on
   the machine, and adding a reachable endpoint is a deliberate edit here.
2. **Rate ceiling.** Every forwarded request passes one gate that permits two
   requests per two seconds, and it *waits* rather than failing. It is applied
   to reads as well as writes on purpose: the point of this tool is a slow,
   deliberate loop, not a faster one.

It binds to loopback only. There is no flag to change that.
"""

from __future__ import annotations

import argparse
import collections
import http.server
import json
import os
import socketserver
import sys
import threading
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

LAB_DIR = Path(__file__).resolve().parent
REPO_ROOT = LAB_DIR.parents[1]
LOG_DIR = LAB_DIR.parent / "logs"

DEFAULT_DAEMON = "http://127.0.0.1:47821"
DEFAULT_BIND = ("127.0.0.1", 8799)

# Daemon paths the panel may reach. Anything absent is refused before a socket
# is opened, so a typo in the UI reads as a clear 403 here rather than as a
# confusing 404 from the daemon.
ALLOWED_GET = {
    "health",
    "licence",
    "messages",
    "accounts",
    "dashboard",
    "hunt",
    "plans",
    "direct",
}
ALLOWED_POST = {
    "injections",
    "direct",
}

STATIC_TYPES = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json; charset=utf-8",
}


class Gate:
    """A sliding window that never lets more than `limit` requests through in
    `window` seconds. Callers block until there is room, so a burst degrades
    into a slower burst instead of a failure."""

    def __init__(self, limit: int, window: float) -> None:
        self._limit = limit
        self._window = window
        self._times: collections.deque[float] = collections.deque()
        self._lock = threading.Lock()

    def wait(self) -> float:
        """Block until a slot is free. Returns how long it waited."""
        waited_from = time.monotonic()
        while True:
            with self._lock:
                now = time.monotonic()
                while self._times and now - self._times[0] >= self._window:
                    self._times.popleft()
                if len(self._times) < self._limit:
                    self._times.append(now)
                    return time.monotonic() - waited_from
                sleep_for = self._window - (now - self._times[0])
            time.sleep(max(sleep_for, 0.01))

    def snapshot(self) -> dict:
        with self._lock:
            now = time.monotonic()
            while self._times and now - self._times[0] >= self._window:
                self._times.popleft()
            return {
                "limit": self._limit,
                "window_seconds": self._window,
                "used": len(self._times),
                "free_in_seconds": round(
                    max(0.0, self._window - (now - self._times[0])), 2
                )
                if self._times
                else 0.0,
            }


def log_line(message: str) -> None:
    stamp = datetime.now(timezone.utc).astimezone().strftime("%H:%M:%S")
    line = f"{stamp}  {message}"
    print(line, flush=True)
    try:
        LOG_DIR.mkdir(parents=True, exist_ok=True)
        day = datetime.now(timezone.utc).astimezone().strftime("%Y%m%d")
        with (LOG_DIR / f"lab-{day}.log").open("a", encoding="utf-8") as handle:
            handle.write(line + "\n")
    except OSError:
        # A read-only or missing log directory must not stop the server.
        pass


class Handler(http.server.BaseHTTPRequestHandler):
    server_version = "OpenAutoLab/1.0"
    daemon = DEFAULT_DAEMON
    gate: Gate

    # -- plumbing ---------------------------------------------------------

    def log_message(self, fmt: str, *args) -> None:  # noqa: A003 - stdlib hook
        # The default writes to stderr in a different shape; route everything
        # through the one logger so the file and the console agree.
        log_line(f"{self.address_string()} {fmt % args}")

    def _send(self, status: int, body: bytes, content_type: str) -> None:
        self.send_response(status)
        self.send_header("content-type", content_type)
        self.send_header("content-length", str(len(body)))
        # The panel is same-origin, but these keep a stray local page from
        # reading daemon state through a browser fetch.
        self.send_header("cache-control", "no-store")
        self.send_header("x-content-type-options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    def _json(self, status: int, payload: object) -> None:
        body = json.dumps(payload, indent=2).encode("utf-8")
        self._send(status, body, "application/json; charset=utf-8")

    # -- routes -----------------------------------------------------------

    def do_GET(self) -> None:  # noqa: N802 - stdlib hook
        path = self.path.split("?", 1)[0]
        if path.startswith("/api/"):
            self._proxy("GET")
            return
        if path in ("/", "/index.html"):
            self._serve_file(LAB_DIR / "index.html")
            return
        self._serve_file(LAB_DIR / path.lstrip("/"))

    def do_POST(self) -> None:  # noqa: N802 - stdlib hook
        if self.path.split("?", 1)[0].startswith("/api/"):
            self._proxy("POST")
            return
        self._json(404, {"error": "not found"})

    def _serve_file(self, candidate: Path) -> None:
        try:
            resolved = candidate.resolve()
            # Refuse anything that escapes the lab directory.
            resolved.relative_to(LAB_DIR)
        except (ValueError, OSError):
            self._json(403, {"error": "outside the lab directory"})
            return
        if not resolved.is_file():
            self._json(404, {"error": "no such file", "path": str(candidate.name)})
            return
        body = resolved.read_bytes()
        kind = STATIC_TYPES.get(resolved.suffix, "application/octet-stream")
        self._send(200, body, kind)

    def _proxy(self, method: str) -> None:
        query = ""
        rest = self.path[len("/api/") :]
        if "?" in rest:
            rest, query = rest.split("?", 1)
        rest = rest.strip("/")

        if rest == "gate":
            self._json(200, self.gate.snapshot())
            return

        allowed = ALLOWED_GET if method == "GET" else ALLOWED_POST
        if rest not in allowed:
            log_line(f"REFUSED {method} /v1/{rest} (not in the allowlist)")
            self._json(
                403,
                {
                    "error": "path is not in the allowlist",
                    "path": rest,
                    "allowed": sorted(allowed),
                },
            )
            return

        payload = None
        if method == "POST":
            length = int(self.headers.get("content-length") or 0)
            payload = self.rfile.read(length) if length else b""
            if payload:
                try:
                    json.loads(payload)
                except json.JSONDecodeError as error:
                    self._json(400, {"error": f"invalid JSON body: {error}"})
                    return

        url = f"{self.daemon}/v1/{rest}"
        if query:
            url += f"?{query}"

        waited = self.gate.wait()
        if waited > 0.05:
            log_line(f"gate: waited {waited:.2f}s before {method} /v1/{rest}")

        request = urllib.request.Request(url, data=payload, method=method)
        if payload:
            request.add_header("content-type", "application/json")
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                body = response.read()
                status = response.status
        except urllib.error.HTTPError as error:
            # The daemon's own message is the useful part; pass it through
            # untouched rather than flattening it into a generic failure.
            body = error.read()
            status = error.code
            log_line(f"{method} /v1/{rest} -> {status} {body[:160].decode('utf-8', 'replace')}")
        except urllib.error.URLError as error:
            log_line(f"{method} /v1/{rest} -> unreachable ({error.reason})")
            self._json(
                503,
                {
                    "error": "the daemon is not answering",
                    "detail": str(error.reason),
                    "expected_at": self.daemon,
                    "hint": "start OpenAuto, or pass --daemon with the right address",
                },
            )
            return

        self._send(status, body, "application/json; charset=utf-8")


class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--daemon", default=DEFAULT_DAEMON, help="daemon base URL")
    parser.add_argument("--host", default=DEFAULT_BIND[0], help="must stay on loopback")
    parser.add_argument("--port", type=int, default=DEFAULT_BIND[1])
    parser.add_argument("--rate", type=int, default=2, help="requests per window")
    parser.add_argument("--window", type=float, default=2.0, help="window in seconds")
    args = parser.parse_args()

    if args.host not in ("127.0.0.1", "localhost", "::1"):
        print(
            f"refusing to bind {args.host}: this tool forwards to a service that "
            "controls a game session and must stay on loopback",
            file=sys.stderr,
        )
        return 2
    if args.rate < 1:
        print("--rate must be at least 1", file=sys.stderr)
        return 2

    Handler.daemon = args.daemon.rstrip("/")
    Handler.gate = Gate(args.rate, args.window)

    server = Server((args.host, args.port), Handler)
    log_line(f"lab on http://{args.host}:{args.port}  ->  {Handler.daemon}")
    log_line(f"ceiling: {args.rate} requests per {args.window}s, enforced before every forward")
    log_line(f"log file: {LOG_DIR / ('lab-' + datetime.now().strftime('%Y%m%d') + '.log')}")
    print("  open the address above. ctrl-c to stop.", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        log_line("stopped")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
