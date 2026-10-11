"""Independent browser experiment; leaves the desktop service and proxy untouched."""
from __future__ import annotations
import argparse
import json
import os
import subprocess
import threading
import sys
import time
import uuid
import mimetypes
import socket
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from .storage import db_path, event, events, export_session, list_sessions
from .incoming import IncomingLedger
from .auto_support import load_config as load_support_config, save_config as save_support_config
from .protocol import matches_game_url

BASE = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = {"data_dir": str(BASE / "web_data"), "proxy_host": "127.0.0.1", "proxy_port": 8080,
                  "api_host": "127.0.0.1", "api_port": 8799, "shortcut": "",
                  "mitmdump": str(BASE.parents[0] / "venv" / "bin" / "mitmdump")}


class Service:
    def __init__(self, config_path: Path):
        self.config_path = config_path
        config_path.parent.mkdir(parents=True, exist_ok=True)
        if not config_path.exists():
            self.save_config(DEFAULT_CONFIG)
        self.config = {**DEFAULT_CONFIG, **json.loads(config_path.read_text(encoding="utf-8"))}
        self.data = Path(self.config["data_dir"]).expanduser().resolve()
        self.data.mkdir(parents=True, exist_ok=True)
        self.sessions = BASE / "data" / "sessions"
        self.sessions.mkdir(parents=True, exist_ok=True)
        self.incoming = IncomingLedger(self.data / "incoming_location.json")
        self.support_config_path = self.data / "auto_support.json"
        self.support_status_path = self.data / "auto_support_status.json"
        self.proxy = None
        self.keyboard = None
        self.shortcut_error = None
        self._start_shortcut()

    def save_config(self, config):
        temp = self.config_path.with_suffix(".tmp")
        temp.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
        temp.replace(self.config_path)

    def _start_shortcut(self):
        if not self.config.get("shortcut"):
            return
        try:
            from pynput import keyboard
            self.keyboard = keyboard.GlobalHotKeys({self.config["shortcut"]: lambda: self.control("toggle")})
            self.keyboard.start()
        except Exception as exc:
            self.shortcut_error = f"Global shortcut unavailable: {exc}"

    def control(self, action):
        command = {"id": uuid.uuid4().hex, "action": action, "at": time.time()}
        temporary = self.data / "control.tmp"
        temporary.write_text(json.dumps(command), encoding="utf-8")
        temporary.replace(self.data / "control.json")
        return command

    def start_proxy(self):
        if self.proxy and self.proxy.poll() is None:
            return
        executable = self.config["mitmdump"]
        if not Path(executable).exists():
            raise FileNotFoundError(f"mitmdump missing: {executable}")
        host, port = self.config["proxy_host"], int(self.config["proxy_port"])
        with socket.socket() as probe:
            try:
                probe.bind((host, port))
            except OSError as exc:
                raise RuntimeError(
                    f"Proxy port {host}:{port} is already in use. Stop the other proxy "
                    "or choose its port in Settings, then point the game client at that port."
                ) from exc
        env = {**os.environ, "PACKET_MONITOR_DATA": str(self.data), "PACKET_MONITOR_SESSIONS": str(self.sessions)}
        env["PACKET_MONITOR_API"] = f"http://{self.config['api_host']}:{self.config['api_port']}"
        log = (self.data / "proxy.log").open("a", encoding="utf-8")
        self.proxy = subprocess.Popen([executable, "-s", str(BASE / "backend" / "incoming_addon.py"),
            "--listen-host", host, "--listen-port", str(port)],
            cwd=BASE, env=env, stdout=log, stderr=subprocess.STDOUT)
        log.close()
        check_host = "127.0.0.1" if host in {"0.0.0.0", "::"} else host
        for _ in range(30):
            if self.proxy.poll() is not None:
                raise RuntimeError(f"Proxy exited during startup. Check {self.data / 'proxy.log'}")
            try:
                with socket.create_connection((check_host, port), timeout=0.1):
                    return
            except OSError:
                time.sleep(0.05)
        self.stop_proxy()
        raise RuntimeError(f"Proxy did not open {check_host}:{port}. Check {self.data / 'proxy.log'}")

    def stop_proxy(self):
        if self.proxy and self.proxy.poll() is None:
            self.proxy.terminate()
            try:
                self.proxy.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proxy.kill()
                self.proxy.wait()
        self.proxy = None

    def status(self):
        try:
            addon = json.loads((self.data / "status.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            addon = {}
        running = self.proxy is not None and self.proxy.poll() is None
        return {"proxy_running": running, "recording": bool(addon.get("recording")) if running else False,
                "connected": bool(addon.get("connected")) if running else False,
                "endpoint": addon.get("endpoint") if running else None,
                "last_websocket_url": addon.get("last_websocket_url") if running else None,
                "last_websocket_matches_game": matches_game_url(addon["last_websocket_url"]) if running and addon.get("last_websocket_url") else None,
                "shortcut": self.config["shortcut"], "shortcut_error": self.shortcut_error,
                "proxy_port": self.config["proxy_port"], "data_dir": str(self.data)}


class Handler(BaseHTTPRequestHandler):
    service: Service
    server_instance: ThreadingHTTPServer
    def _reply(self, status, body):
        data = json.dumps(body, ensure_ascii=False).encode("utf-8")
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")
            self.send_header("Access-Control-Allow-Headers", "Content-Type")
            self.end_headers()
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def _static(self, path):
        names = {"/": "index.html", "/app.js": "app.js", "/style.css": "style.css"}
        filename = names.get(path)
        if filename is None:
            return False
        content = (BASE / "web" / filename).read_bytes()
        try:
            self.send_response(200)
            self.send_header("Content-Type", mimetypes.guess_type(filename)[0] or "text/plain")
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)
        except (BrokenPipeError, ConnectionResetError):
            pass
        return True

    def do_OPTIONS(self): self._reply(200, {})

    def do_GET(self):
        path = urlparse(self.path)
        if self._static(path.path):
            return
        args = parse_qs(path.query)
        parts = path.path.strip("/").split("/")
        try:
            if path.path == "/api/status": result = self.service.status()
            elif path.path == "/api/incoming":
                self.service.incoming.watch_castles(load_support_config(self.service.support_config_path)["castles"])
                result = self.service.incoming.snapshot(time.time())
            elif path.path == "/api/auto-support":
                try:
                    status = json.loads(self.service.support_status_path.read_text(encoding="utf-8"))
                except (OSError, ValueError):
                    status = {"jobs": [], "last_event": "Proxy has not reported yet"}
                result = {"config": load_support_config(self.service.support_config_path), "status": status}
            elif path.path == "/api/config": result = self.service.config
            elif path.path == "/api/sessions": result = list_sessions(self.service.sessions)
            elif len(parts) == 4 and parts[:2] == ["api", "sessions"] and parts[3] == "events":
                result = events(self.service.sessions, parts[2], args.get("type", []), args.get("q", [""])[0],
                                min(300, max(1, int(args.get("limit", ["100"])[0]))), max(0, int(args.get("offset", ["0"])[0])))
            elif len(parts) == 5 and parts[:2] == ["api", "sessions"] and parts[3] == "events":
                result = event(self.service.sessions, parts[2], int(parts[4]))
            else: return self._reply(404, {"error": "not found"})
            self._reply(200, result)
        except (ValueError, FileNotFoundError) as exc:
            self._reply(404, {"error": str(exc)})
        except Exception as exc:
            self._reply(500, {"error": str(exc)})

    def do_POST(self):
        path = urlparse(self.path).path
        try:
            body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
            if path == "/api/proxy/start": self.service.start_proxy(); result = self.service.status()
            elif path == "/api/proxy/stop": self.service.stop_proxy(); result = self.service.status()
            elif path == "/api/recording/toggle": result = self.service.control("toggle")
            elif path == "/api/recording/start": result = self.service.control("start")
            elif path == "/api/recording/stop": result = self.service.control("stop")
            elif path == "/api/incoming/config": result = self.service.incoming.configure(body)
            elif path == "/api/auto-support/config":
                result = save_support_config(self.service.support_config_path, body)
            elif path == "/api/incoming/observe":
                self.service.incoming.watch_castles(load_support_config(self.service.support_config_path)["castles"])
                result = {"added": self.service.incoming.observe(str(body.get("raw", "")), float(body.get("received_at", time.time())))}
            elif path == "/api/shutdown":
                threading.Thread(target=self.server_instance.shutdown, daemon=True).start()
                result = {"shutting_down": True}
            elif path == "/api/folder":
                subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", str(self.service.sessions)])
                result = {"opened": str(self.service.sessions)}
            elif path == "/api/config":
                allowed = {"data_dir", "proxy_host", "proxy_port", "api_host", "api_port", "shortcut", "mitmdump"}
                self.service.config.update({key: value for key, value in body.items() if key in allowed})
                self.service.save_config(self.service.config)
                result = {"saved": True, "restart_required": True}
            elif path.startswith("/api/sessions/") and path.endswith("/export"):
                session_id = path.split("/")[3]
                destination = Path(body.get("path") or self.service.data / f"{session_id}.json").expanduser().resolve()
                export_session(self.service.sessions, session_id, destination)
                result = {"path": str(destination)}
            else: return self._reply(404, {"error": "not found"})
            self._reply(200, result)
        except Exception as exc: self._reply(400, {"error": str(exc)})

    def do_DELETE(self):
        try:
            session_id = self.path.split("/")[3]
            path = db_path(self.service.sessions, session_id)
            path.unlink()
            self._reply(200, {"deleted": session_id})
        except Exception as exc: self._reply(400, {"error": str(exc)})

    def log_message(self, fmt, *args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=Path, default=BASE / "web_config.json")
    args = parser.parse_args()
    service = Service(args.config)
    Handler.service = service
    server = ThreadingHTTPServer((service.config["api_host"], int(service.config["api_port"])), Handler)
    Handler.server_instance = server
    try: server.serve_forever()
    except KeyboardInterrupt: pass
    finally:
        service.stop_proxy()
        if service.keyboard: service.keyboard.stop()
        server.server_close()

if __name__ == "__main__": main()
