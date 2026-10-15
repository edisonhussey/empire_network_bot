"""Saves what the game client downloads at login (items, config, language bundles) for offline searching.

Layout under `<data_dir>/login_data/`:
  files/<host>/<url path>   the response body; a changed body at the same path is kept as `<name>.<sha8>`
  index.jsonl               one JSON line per response seen, plus one `{"event":"websocket"}` line per game
                            WebSocket connect. `ws_seq` counts those connects, so files fetched before the
                            socket opens carry the previous number: order by `t` when that matters.
Identical bodies are stored once (matched by sha256), so repeated logins cost nothing.
"""
from __future__ import annotations
import hashlib
import json
import re
import time
from pathlib import Path
from queue import SimpleQueue
from threading import Thread
from urllib.parse import unquote, urlsplit

DEFAULT_HOSTS = "empire-html5.goodgamestudios.com,files-ak.goodgamestudios.com,langserv.public.ggs-ep.com"
SKIP_TYPES = ("image/", "audio/", "video/", "font/")
SKIP_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".svg", ".mp3", ".ogg", ".wav", ".mp4",
                 ".woff", ".woff2", ".ttf", ".otf"}
MAX_BYTES = 64 * 1024 * 1024


def _segment(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9._ -]", "_", text)[:120] or "_"


class LoginCapture:
    def __init__(self, root: Path, flag_path: Path, hosts: str = DEFAULT_HOSTS):
        self.root = root
        self.files = root / "files"
        self.index = root / "index.jsonl"
        self.flag_path = flag_path
        self.hosts = {host.strip().lower() for host in hosts.split(",") if host.strip()}
        self.ws_seq = 0
        self.saved = 0
        self.last_saved_at = None
        self._flag = (None, False)
        self._known: dict[str, str] = {}
        self._queue: SimpleQueue = SimpleQueue()
        self._worker = None
        self._load_known()

    def _load_known(self):
        try:
            for line in self.index.read_text(encoding="utf-8").splitlines():
                entry = json.loads(line)
                if entry.get("sha256") and entry.get("file"):
                    self._known[entry["sha256"]] = entry["file"]
                self.ws_seq = max(self.ws_seq, entry.get("ws_seq", 0))
        except (OSError, ValueError):
            pass

    def enabled(self) -> bool:
        """The checkbox. Re-read only when the flag file changes."""
        try:
            stamp = self.flag_path.stat().st_mtime_ns
        except OSError:
            self._flag = (None, False)
            return False
        if self._flag[0] != stamp:
            try:
                value = bool(json.loads(self.flag_path.read_text(encoding="utf-8")).get("enabled"))
            except (OSError, ValueError, AttributeError):
                value = False
            self._flag = (stamp, value)
        return self._flag[1]

    def wants_host(self, host: str) -> bool:
        return (host or "").lower() in self.hosts

    def wants(self, method: str, url: str, status: int, content_type: str) -> bool:
        if method != "GET" or status != 200:
            return False
        parts = urlsplit(url)
        if (parts.hostname or "").lower() not in self.hosts:
            return False
        if content_type.lower().startswith(SKIP_TYPES):
            return False
        return Path(parts.path).suffix.lower() not in SKIP_SUFFIXES

    def stats(self) -> dict:
        return {"enabled": self.enabled(), "saved": self.saved, "logins": self.ws_seq,
                "last_saved_at": self.last_saved_at, "dir": str(self.root)}

    def on_websocket(self, url: str) -> None:
        self.ws_seq += 1
        if self.enabled():
            self._append({"t": time.time(), "ws_seq": self.ws_seq, "event": "websocket", "url": url})

    def submit(self, url: str, status: int, content_type: str, body: bytes, at: float) -> None:
        if not body or len(body) > MAX_BYTES:
            return
        if self._worker is None:
            self._worker = Thread(target=self._run, daemon=True)
            self._worker.start()
        self._queue.put((url, status, content_type, body, at))

    def close(self) -> None:
        if self._worker is not None:
            self._queue.put(None)
            self._worker.join(timeout=3)

    def _run(self):
        while True:
            item = self._queue.get()
            if item is None:
                return
            try:
                self.save(*item)
            except OSError as exc:
                print(f"login capture failed: {exc!r}", flush=True)

    def _append(self, entry: dict) -> None:
        self.root.mkdir(parents=True, exist_ok=True)
        with self.index.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry, ensure_ascii=False) + "\n")

    def _destination(self, host: str, path: str) -> Path:
        parts = [_segment(part) for part in unquote(path).split("/") if part not in ("", ".", "..")]
        if not parts or path.endswith("/"):
            parts.append("index")
        return self.files.joinpath(_segment(host), *parts)

    def save(self, url: str, status: int, content_type: str, body: bytes, at: float) -> dict:
        digest = hashlib.sha256(body).hexdigest()
        parts = urlsplit(url)
        known = self._known.get(digest)
        if known is not None:
            relative, new = known, False
        else:
            destination = self._destination(parts.hostname or "unknown", parts.path)
            if destination.exists():
                destination = destination.with_name(f"{destination.name}.{digest[:8]}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            temporary = destination.with_name(destination.name + ".tmp")
            temporary.write_bytes(body)
            temporary.replace(destination)
            relative = destination.relative_to(self.root).as_posix()
            self._known[digest] = relative
            new = True
            self.saved += 1
            self.last_saved_at = time.time()
        entry = {"t": at, "ws_seq": self.ws_seq, "url": url, "host": parts.hostname, "status": status,
                 "type": content_type, "size": len(body), "sha256": digest, "file": relative, "new": new}
        self._append(entry)
        return entry
