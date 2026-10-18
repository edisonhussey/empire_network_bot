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
IMAGE_TYPES = ("image/",)
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".svg"}
SKIP_SUFFIXES = IMAGE_SUFFIXES | {".mp3", ".ogg", ".wav", ".mp4", ".woff", ".woff2", ".ttf", ".otf"}
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
        self._flag = (None, False, False)
        self.bytes_saved = 0
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

    def _read_flags(self) -> tuple:
        """`(enabled, images)` from the flag file, re-read only when the file changes."""
        try:
            stamp = self.flag_path.stat().st_mtime_ns
        except OSError:
            self._flag = (None, False, False)
            return self._flag
        if self._flag[0] != stamp:
            try:
                body = json.loads(self.flag_path.read_text(encoding="utf-8"))
                self._flag = (stamp, bool(body.get("enabled")), bool(body.get("images")))
            except (OSError, ValueError, AttributeError):
                self._flag = (stamp, False, False)
        return self._flag

    def enabled(self) -> bool:
        """The "Download login data" checkbox."""
        return self._read_flags()[1]

    def images_enabled(self) -> bool:
        """The "Include images" checkbox: pictures are kept too (audio, video and fonts never are)."""
        return self._read_flags()[2]

    def wants_host(self, host: str) -> bool:
        return (host or "").lower() in self.hosts

    def wants(self, method: str, url: str, status: int, content_type: str) -> bool:
        if method != "GET" or status != 200:
            return False
        parts = urlsplit(url)
        if (parts.hostname or "").lower() not in self.hosts:
            return False
        skipped_types, skipped_suffixes = SKIP_TYPES, SKIP_SUFFIXES
        if self.images_enabled():
            skipped_types = tuple(t for t in SKIP_TYPES if t not in IMAGE_TYPES)
            skipped_suffixes = SKIP_SUFFIXES - IMAGE_SUFFIXES
        if content_type.lower().startswith(skipped_types):
            return False
        return Path(parts.path).suffix.lower() not in skipped_suffixes

    def stats(self) -> dict:
        return {"enabled": self.enabled(), "images": self.images_enabled(), "saved": self.saved, "logins": self.ws_seq,
                "megabytes": round(self.bytes_saved / 1e6, 1), "last_saved_at": self.last_saved_at, "dir": str(self.root)}

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
            self.bytes_saved += len(body)
            self.last_saved_at = time.time()
        entry = {"t": at, "ws_seq": self.ws_seq, "url": url, "host": parts.hostname, "status": status,
                 "type": content_type, "size": len(body), "sha256": digest, "file": relative, "new": new}
        self._append(entry)
        return entry
