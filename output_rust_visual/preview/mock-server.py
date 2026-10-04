#!/usr/bin/env python3
"""Serve the UI with sample data, so it renders populated instead of empty.

The frontend hardcodes its API base as http://127.0.0.1:47821/v1, so this listens
on that same port and answers both the page and the API. Serving both from one
origin is deliberate: the real daemon only allows a couple of origins, and same
origin means there is nothing to configure and nothing to patch in the JS.

Only the Python standard library is used.

    python3 preview/mock-server.py            # http://127.0.0.1:47821
    python3 preview/mock-server.py 4180       # spare port, if 47821 is taken

Any port works: when it is not the default one, the copy of `src/main.js` that is
served has its API base rewritten to match, because the page and the API must
share an origin. The file on disk is never modified. If the port is busy the real
OpenAuto app is probably running - quit it first.

Responses are static: writes are accepted and answered, but nothing is stored, so
the UI returns to the sample state on the next poll.
"""

from __future__ import annotations

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "ui"
SAMPLES = ROOT / "api-samples"
DEFAULT_PORT = 47821

# Static files the page asks for, mapped explicitly. No directory walking, so a
# crafted path cannot read anything outside `ui/`.
STATIC = {
    "/": ("index.html", "text/html; charset=utf-8"),
    "/index.html": ("index.html", "text/html; charset=utf-8"),
    "/src/main.js": ("src/main.js", "text/javascript; charset=utf-8"),
    "/src/style.css": ("src/style.css", "text/css; charset=utf-8"),
    "/app-icon.svg": ("app-icon.svg", "image/svg+xml"),
}

# GET /v1/<name> -> api-samples/<file>.json
GET_ENDPOINTS = {
    "health": "health.json",
    "licence": "licence.json",
    "accounts": "accounts.json",
    "direct": "direct.json",
    "hunt": "hunt.json",
    "plans": "plans.json",
    "plans/example": "plans_example.json",
    "dashboard": "dashboard.json",
    "messages": "messages.json",
}

# Creating things returns an id; deleting returns nothing.
POST_PREFIXES = (
    "plans/attacks",
    "plans/tasks",
    "plans/modes",
    "plans/recruitments",
    "plans/recruit-bots",
)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args) -> None:  # quieter, one line each
        sys.stderr.write("  %s\n" % (fmt % args))

    # -- helpers ----------------------------------------------------------
    def send_json(self, payload, status: int = 200) -> None:
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_empty(self, status: int = 204) -> None:
        self.send_response(status)
        self.send_header("content-length", "0")
        self.end_headers()

    def read_json_body(self) -> dict:
        length = int(self.headers.get("content-length") or 0)
        if not length:
            return {}
        try:
            return json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            return {}

    # -- routes -----------------------------------------------------------
    def do_GET(self) -> None:
        path = self.path.split("?", 1)[0]

        if path in STATIC:
            name, content_type = STATIC[path]
            file = UI / name
            if not file.exists():
                self.send_json({"error": f"missing {name}"}, 500)
                return
            body = file.read_bytes()
            # The page and the API have to share an origin, so when this runs on a
            # port other than the one baked into the source, point the page at the
            # port actually in use. The file on disk is never modified.
            if name == "src/main.js" and self.server.server_port != DEFAULT_PORT:
                body = body.replace(
                    f"http://127.0.0.1:{DEFAULT_PORT}/v1".encode(),
                    f"http://127.0.0.1:{self.server.server_port}/v1".encode(),
                )
            self.send_response(200)
            self.send_header("content-type", content_type)
            self.send_header("content-length", str(len(body)))
            self.send_header("cache-control", "no-store")
            self.end_headers()
            self.wfile.write(body)
            return

        if path.startswith("/v1/"):
            sample = GET_ENDPOINTS.get(path[4:])
            if sample is None:
                self.send_json({"error": f"no sample for {path}"}, 404)
                return
            file = SAMPLES / sample
            if not file.exists():
                self.send_json({"error": f"missing {sample}"}, 500)
                return
            body = file.read_bytes()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        self.send_json({"error": f"not found: {path}"}, 404)

    def do_POST(self) -> None:
        route = self.path.split("?", 1)[0].removeprefix("/v1/")
        body = self.read_json_body()

        if route == "licence":
            self.send_json(json.loads((SAMPLES / "licence.json").read_text()))
            return
        if route == "accounts":
            self.send_json(json.loads((SAMPLES / "accounts.json").read_text()))
            return
        if route.startswith(POST_PREFIXES):
            # Shaped like the real thing so the handlers that read `id` work.
            self.send_json({"id": "mock-id" if route.endswith("ments") else "1"}, 201)
            return
        if route == "plans/start":
            self.send_json({"ok": True})
            return
        if route == "pulse":
            self.send_json({"ok": True})
            return
        self.send_json({"error": f"no mock for POST {route}"}, 404)

    def do_DELETE(self) -> None:
        route = self.path.split("?", 1)[0].removeprefix("/v1/")
        if route == "direct" or route.startswith("plans/"):
            self.send_empty(204)
            return
        self.send_json({"error": f"no mock for DELETE {route}"}, 404)


def main() -> int:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_PORT
    try:
        server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    except OSError:
        print(
            f"Port {port} is already in use.\n"
            "The real OpenAuto app is probably running - quit it, or pass a spare\n"
            "port (any port works; the page is pointed at it automatically).",
            file=sys.stderr,
        )
        return 1

    missing = [name for name in GET_ENDPOINTS.values() if not (SAMPLES / name).exists()]
    if missing:
        print(f"warning: missing samples: {', '.join(missing)}", file=sys.stderr)

    print(f"OpenAuto UI preview  ->  http://127.0.0.1:{port}")
    print(f"  serving page from  {UI}")
    print(f"  serving data from  {SAMPLES}")
    print("  writes are accepted but not stored; Ctrl-C to stop")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
