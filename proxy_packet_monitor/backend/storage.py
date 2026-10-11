"""One SQLite database per recording; source text and decoded payload are both retained."""
from __future__ import annotations
import json
import os
import sqlite3
import threading
import time
import uuid
from pathlib import Path
from queue import SimpleQueue
from .protocol import parse_xt_packet, summary

SCHEMA = """CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE events(seq INTEGER PRIMARY KEY, received_at REAL NOT NULL, type TEXT NOT NULL,
 summary TEXT NOT NULL, raw TEXT NOT NULL, decoded TEXT NOT NULL);
CREATE INDEX event_type ON events(type, seq);"""


def db_path(root: Path, session_id: str) -> Path:
    if not session_id or any(c not in "0123456789abcdef" for c in session_id):
        raise ValueError("invalid session id")
    path = root / f"{session_id}.sqlite3"
    if not path.exists():
        raise FileNotFoundError(session_id)
    return path


def metadata(conn: sqlite3.Connection) -> dict:
    return {key: json.loads(value) for key, value in conn.execute("SELECT key,value FROM metadata")}


def set_meta(conn: sqlite3.Connection, **values) -> None:
    conn.executemany("INSERT OR REPLACE INTO metadata VALUES (?,?)", [(key, json.dumps(value)) for key, value in values.items()])


class Recorder:
    def __init__(self, root: Path):
        self.root = root
        self.root.mkdir(parents=True, exist_ok=True)
        self.queue: SimpleQueue = SimpleQueue()
        self.lock = threading.Lock()
        self.active = False
        self.session_id = None
        self.seq = 0
        self.worker = threading.Thread(target=self._run, daemon=True)
        self.worker.start()

    def start(self) -> str:
        with self.lock:
            if self.active:
                return self.session_id
            self.session_id = uuid.uuid4().hex
            self.seq = 0
            self.active = True
            session_id = self.session_id
            self.queue.put(("start", session_id, time.time()))
            return session_id

    def stop(self) -> None:
        with self.lock:
            if self.active:
                self.active = False
                self.queue.put(("stop", self.session_id, time.time()))

    def capture(self, raw: str, received_at: float) -> None:
        with self.lock:
            if not self.active:
                return
            self.seq += 1
            self.queue.put(("event", self.session_id, self.seq, received_at, raw))

    def close(self) -> None:
        self.stop()
        self.queue.put(("quit",))
        self.worker.join(timeout=10)

    def _run(self) -> None:
        conn = None
        count = 0
        while True:
            item = self.queue.get()
            op = item[0]
            if op == "quit":
                break
            try:
                if op == "start":
                    path = self.root / f"{item[1]}.partial.sqlite3"
                    conn = sqlite3.connect(path)
                    conn.executescript(SCHEMA)
                    set_meta(conn, version=1, id=item[1], started_at=item[2], ended_at=None, event_count=0)
                    conn.commit()
                    count = 0
                elif op == "event" and conn:
                    _, _, seq, received_at, raw = item
                    parsed = parse_xt_packet(raw)
                    kind = (parsed or {}).get("command") or "unknown"
                    conn.execute("INSERT INTO events VALUES (?,?,?,?,?,?)", (seq, received_at, kind.upper(), summary(parsed), raw, json.dumps(parsed, ensure_ascii=False)))
                    count += 1
                    if count % 100 == 0:
                        conn.commit()
                elif op == "stop" and conn:
                    set_meta(conn, ended_at=item[2], event_count=count)
                    conn.commit()
                    conn.close()
                    conn = None
                    (self.root / f"{item[1]}.partial.sqlite3").replace(self.root / f"{item[1]}.sqlite3")
            except Exception as exc:
                # The queue worker must survive a malformed packet or disk error.
                print(f"recorder error: {exc!r}", flush=True)
        if conn:
            conn.commit()
            conn.close()


def recover_sessions(root: Path) -> int:
    """Finalize committed events from interrupted recordings without rewriting them."""
    recovered = 0
    for path in root.glob("*.partial.sqlite3"):
        try:
            with sqlite3.connect(path) as conn:
                if conn.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                    continue
                info = metadata(conn)
                count, last_time = conn.execute("SELECT count(*),max(received_at) FROM events").fetchone()
                set_meta(conn, ended_at=last_time or info["started_at"], event_count=count, recovered=True)
                conn.commit()
            path.replace(root / path.name.replace(".partial.sqlite3", ".sqlite3"))
            recovered += 1
        except (sqlite3.Error, OSError, KeyError, ValueError):
            continue
    return recovered


def list_sessions(root: Path) -> list[dict]:
    result = []
    for path in root.glob("*.sqlite3"):
        try:
            with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as conn:
                entry = metadata(conn)
                result.append(entry)
        except (sqlite3.Error, ValueError):
            continue
    return sorted(result, key=lambda item: item["started_at"], reverse=True)


def events(root: Path, session_id: str, types: list[str], query: str, limit: int, offset: int) -> dict:
    with sqlite3.connect(db_path(root, session_id)) as conn:
        conn.row_factory = sqlite3.Row
        where, params = [], []
        if types:
            where.append("type IN (" + ",".join("?" for _ in types) + ")")
            params.extend(types)
        if query:
            where.append("(summary LIKE ? OR type LIKE ? OR raw LIKE ?)")
            params.extend([f"%{query}%"] * 3)
        clause = " WHERE " + " AND ".join(where) if where else ""
        count = conn.execute("SELECT count(*) FROM events" + clause, params).fetchone()[0]
        rows = conn.execute("SELECT seq,received_at,type,summary FROM events" + clause + " ORDER BY seq LIMIT ? OFFSET ?", [*params, limit, offset]).fetchall()
        counts = conn.execute("SELECT type,count(*) FROM events GROUP BY type ORDER BY count(*) DESC").fetchall()
        return {"total": count, "events": [dict(row) for row in rows], "types": dict(counts)}


def event(root: Path, session_id: str, seq: int) -> dict:
    with sqlite3.connect(db_path(root, session_id)) as conn:
        conn.row_factory = sqlite3.Row
        row = conn.execute("SELECT * FROM events WHERE seq=?", (seq,)).fetchone()
        if row is None:
            raise FileNotFoundError(seq)
        result = dict(row)
        result["decoded"] = json.loads(result["decoded"])
        return result


def export_session(root: Path, session_id: str, destination: Path) -> None:
    with sqlite3.connect(db_path(root, session_id)) as conn:
        conn.row_factory = sqlite3.Row
        info = metadata(conn)
        temp = destination.with_suffix(destination.suffix + ".tmp")
        with temp.open("w", encoding="utf-8") as file:
            file.write('{"metadata":' + json.dumps(info, ensure_ascii=False) + ',"events":[')
            for index, row in enumerate(conn.execute("SELECT * FROM events ORDER BY seq")):
                record = dict(row)
                record["decoded"] = json.loads(record["decoded"])
                if index:
                    file.write(",")
                file.write(json.dumps(record, ensure_ascii=False))
            file.write("]}\n")
            file.flush()
            os.fsync(file.fileno())
        temp.replace(destination)
