from __future__ import annotations

import argparse
import json
import sys
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


if __package__ in {None, ""}:
    HERE = Path(__file__).resolve().parent
    if str(HERE) not in sys.path:
        sys.path.insert(0, str(HERE))
    from database_query_generator import (
        ConnectionPool,
        TimelineRequest,
        build_timeline_payload,
        ensure_indexes,
        parse_date,
    )
else:
    from .database_query_generator import (
        ConnectionPool,
        TimelineRequest,
        build_timeline_payload,
        ensure_indexes,
        parse_date,
    )


class TimelineServer(ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self, server_address, handler_class, *, pool: ConnectionPool, timezone_name: str) -> None:
        super().__init__(server_address, handler_class)
        self.pool = pool
        self.timezone_name = timezone_name


class Handler(BaseHTTPRequestHandler):
    server: TimelineServer

    def log_message(self, format: str, *args) -> None:
        return

    def end_headers(self) -> None:
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_OPTIONS(self) -> None:
        self.send_response(HTTPStatus.NO_CONTENT)
        self.end_headers()

    def do_HEAD(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/timeline.html":
            path = Path(__file__).resolve().parent / "timeline.html"
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(path.stat().st_size))
            self.end_headers()
            return
        if parsed.path == "/api/timeline":
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            return
        self.send_response(HTTPStatus.NOT_FOUND)
        self.end_headers()

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/":
            self.redirect_to_timeline()
            return
        if parsed.path == "/timeline.html":
            self.send_timeline_html()
            return
        if parsed.path != "/api/timeline":
            self.send_json({"error": "not found"}, HTTPStatus.NOT_FOUND)
            return

        query = parse_qs(parsed.query)
        try:
            request = TimelineRequest(
                view_date=parse_date(first(query.get("date")), timezone_name=self.server.timezone_name),
                aid=first(query.get("aid")),
                timezone_name=self.server.timezone_name,
            )
            payload, row_count, elapsed_ms = build_timeline_payload(self.server.pool, request)
        except ValueError as exc:
            self.send_json({"error": str(exc)}, HTTPStatus.BAD_REQUEST)
            return
        except Exception as exc:
            print(f"timeline_error error={exc!r}", flush=True)
            self.send_json({"error": "timeline query failed"}, HTTPStatus.INTERNAL_SERVER_ERROR)
            return

        print(
            f"timeline_request date={request.view_date.isoformat()} aid={request.aid or '*'} "
            f"rows={row_count} query_ms={elapsed_ms:.1f}",
            flush=True,
        )
        self.send_json(payload)

    def redirect_to_timeline(self) -> None:
        self.send_response(HTTPStatus.FOUND)
        self.send_header("Location", "/timeline.html")
        self.end_headers()

    def send_timeline_html(self) -> None:
        path = Path(__file__).resolve().parent / "timeline.html"
        text = path.read_text(encoding="utf-8")
        host, port = self.server.server_address
        api_host = "localhost" if host in {"", "0.0.0.0", "127.0.0.1"} else host
        text = text.replace("http://localhost:8000/api/timeline", f"http://{api_host}:{port}/api/timeline")
        body = text.encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_json(self, payload: dict, status: HTTPStatus = HTTPStatus.OK) -> None:
        body = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def first(values: list[str] | None) -> str | None:
    if not values:
        return None
    return values[0] or None


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Local JSON API for activity_monitor/timeline.html.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8015)
    parser.add_argument("--timezone", default="Europe/London")
    parser.add_argument("--skip-index", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    pool = ConnectionPool()
    if not args.skip_index:
        ensure_indexes(pool)
    server = TimelineServer((args.host, args.port), Handler, pool=pool, timezone_name=args.timezone)
    print(f"timeline_api listening=http://{args.host}:{args.port}/api/timeline", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("timeline_api stopped", flush=True)
    finally:
        server.server_close()
        pool.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
