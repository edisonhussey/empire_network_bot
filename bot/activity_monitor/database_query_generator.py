from __future__ import annotations

import os
import re
import time
from collections import OrderedDict
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import date, datetime, time as dt_time, timedelta, timezone
from queue import Empty, Queue
from typing import Any
from zoneinfo import ZoneInfo

import psycopg
from psycopg.rows import dict_row


DEFAULT_TIMEZONE = "Europe/London"
DEFAULT_DATABASE = "empire_bot"
COMMANDER_LIDS_BY_HUMAN_NUMBER = (
    0,
    2,
    3,
    6,
    7,
    8,
    9,
    10,
    11,
    16,
    17,
    18,
    20,
    21,
    22,
    23,
    24,
    25,
    26,
    27,
    28,
    29,
    30,
    31,
    32,
    33,
    34,
    35,
    36,
    37,
    38,
    39,
    40,
    41,
    42,
)


def utc_iso(epoch: int | float | None) -> str | None:
    if epoch is None:
        return None
    return datetime.fromtimestamp(int(epoch), timezone.utc).isoformat().replace("+00:00", "Z")


def slug(value: str) -> str:
    text = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return text or "commander"


def commander_label(row: dict[str, Any]) -> str:
    lord_id = row.get("lord_id")
    if lord_id is not None:
        try:
            return f"COM {COMMANDER_LIDS_BY_HUMAN_NUMBER.index(int(lord_id)) + 1}"
        except ValueError:
            pass
    commander_number = row.get("commander_number")
    if commander_number is not None:
        return f"COM {int(commander_number)}"
    return "COM unknown"


def format_duration(seconds: int) -> str:
    seconds = max(0, int(seconds))
    hours, remainder = divmod(seconds, 3600)
    minutes, secs = divmod(remainder, 60)
    if hours:
        return f"{hours}h {minutes}m"
    if minutes:
        return f"{minutes}m {secs}s"
    return f"{secs}s"


def build_conninfo() -> str:
    if os.getenv("DATABASE_URL"):
        return os.environ["DATABASE_URL"]
    parts = {
        "host": os.getenv("PGHOST"),
        "port": os.getenv("PGPORT"),
        "dbname": os.getenv("PGDATABASE", DEFAULT_DATABASE),
        "user": os.getenv("PGUSER"),
        "password": os.getenv("PGPASSWORD"),
    }
    return " ".join(f"{key}={value}" for key, value in parts.items() if value)


class ConnectionPool:
    def __init__(self, conninfo: str | None = None, *, min_size: int = 1, max_size: int = 4) -> None:
        self.conninfo = conninfo or build_conninfo()
        self.max_size = max(1, int(max_size))
        self._queue: Queue[psycopg.Connection] = Queue(maxsize=self.max_size)
        self._opened = 0
        for _ in range(max(0, min(int(min_size), self.max_size))):
            self._queue.put(self._connect())

    def _connect(self) -> psycopg.Connection:
        self._opened += 1
        return psycopg.connect(self.conninfo, row_factory=dict_row, autocommit=True)

    @contextmanager
    def connection(self):
        conn = None
        try:
            try:
                conn = self._queue.get_nowait()
            except Empty:
                conn = self._connect() if self._opened < self.max_size else self._queue.get()
            if conn.closed:
                conn = self._connect()
            yield conn
        finally:
            if conn is not None and not conn.closed:
                self._queue.put(conn)

    def close(self) -> None:
        while True:
            try:
                conn = self._queue.get_nowait()
            except Empty:
                return
            conn.close()


@dataclass(frozen=True)
class TimelineRequest:
    view_date: date
    aid: str | None = None
    timezone_name: str = DEFAULT_TIMEZONE

    @property
    def timezone(self) -> ZoneInfo:
        return ZoneInfo(self.timezone_name)

    @property
    def start_epoch(self) -> int:
        local_start = datetime.combine(self.view_date, dt_time.min, tzinfo=self.timezone)
        return int(local_start.timestamp())

    @property
    def end_epoch(self) -> int:
        local_end = datetime.combine(self.view_date + timedelta(days=1), dt_time.min, tzinfo=self.timezone)
        return int(local_end.timestamp())


ATTACK_TIMELINE_QUERY = """
WITH selected_attacks AS (
    SELECT
        id,
        aid,
        time_created,
        march_id AS outbound_mid,
        NULLIF(raw_result #>> '{A,M,MID}', '')::bigint AS inbound_mid,
        lord_id,
        commander_number,
        target_kind,
        target_id,
        task_name,
        target_level,
        x_coordinate,
        y_coordinate,
        troop_count,
        duration,
        COALESCE(return_duration, NULLIF(raw_result #>> '{A,M,TT}', '')::integer) AS resolved_return_duration,
        status,
        coin_loot,
        ruby_loot,
        landed_at,
        result_received_at
    FROM attack
    WHERE time_created >= %(start_epoch)s
      AND time_created < %(end_epoch)s
      AND (%(aid)s::text IS NULL OR aid = %(aid)s::text)
)
SELECT *
FROM selected_attacks
ORDER BY COALESCE(commander_number, CASE WHEN lord_id = 0 THEN 1 ELSE lord_id END, 9999), time_created, id;
"""


COMMANDER_BUCKET_QUERY = """
SELECT DISTINCT
    aid,
    lord_id,
    CASE WHEN lord_id = 0 THEN 1 ELSE lord_id END AS commander_number
FROM commander_state
WHERE (%(aid)s::text IS NULL OR aid = %(aid)s::text)
UNION
SELECT DISTINCT
    aid,
    lord_id,
    COALESCE(commander_number, CASE WHEN lord_id = 0 THEN 1 ELSE lord_id END) AS commander_number
FROM attack
WHERE time_created >= %(start_epoch)s
  AND time_created < %(end_epoch)s
  AND lord_id IS NOT NULL
  AND (%(aid)s::text IS NULL OR aid = %(aid)s::text)
ORDER BY commander_number NULLS LAST, lord_id;
"""


RECOMMENDED_INDEX_SQL = """
CREATE INDEX IF NOT EXISTS idx_attack_aid_time_commander
ON attack (aid, time_created, commander_number, lord_id);
"""


def ensure_indexes(pool: ConnectionPool) -> None:
    with pool.connection() as conn:
        with conn.cursor() as cur:
            cur.execute(RECOMMENDED_INDEX_SQL)


def row_duration_seconds(row: dict[str, Any]) -> int:
    outbound = int(row.get("duration") or 0)
    inbound = row.get("resolved_return_duration")
    return outbound + max(0, int(inbound or 0))


def row_label(row: dict[str, Any]) -> str:
    total = row_duration_seconds(row)
    outbound_mid = row.get("outbound_mid")
    inbound_mid = row.get("inbound_mid")
    target = f"{row.get('x_coordinate')}:{row.get('y_coordinate')}"
    loot_bits = []
    if row.get("coin_loot") is not None:
        loot_bits.append(f"C1 {int(row['coin_loot'])}")
    if row.get("ruby_loot") is not None:
        loot_bits.append(f"C2 {int(row['ruby_loot'])}")
    mid_text = f"MID {outbound_mid}"
    if inbound_mid and inbound_mid != outbound_mid:
        mid_text = f"{mid_text}->{inbound_mid}"
    suffix = f" | {' '.join(loot_bits)}" if loot_bits else ""
    return f"{format_duration(total)} | {target} | {mid_text}{suffix}"


def commander_sort_key(label: str) -> tuple[int, str]:
    match = re.search(r"\d+", label)
    return (int(match.group(0)) if match else 9999, label)


def build_timeline_payload(pool: ConnectionPool, request: TimelineRequest) -> tuple[dict[str, Any], int, float]:
    started = time.perf_counter()
    params = {
        "start_epoch": request.start_epoch,
        "end_epoch": request.end_epoch,
        "aid": request.aid,
    }
    with pool.connection() as conn:
        with conn.cursor() as cur:
            cur.execute(COMMANDER_BUCKET_QUERY, params)
            bucket_rows = cur.fetchall()
            cur.execute(ATTACK_TIMELINE_QUERY, params)
            attack_rows = cur.fetchall()

    commanders: OrderedDict[str, dict[str, Any]] = OrderedDict()
    for row in bucket_rows:
        label = commander_label(row)
        commanders.setdefault(label, {"id": slug(label), "label": label, "events": []})

    for row in attack_rows:
        label = commander_label(row)
        bucket = commanders.setdefault(label, {"id": slug(label), "label": label, "events": []})
        duration = row_duration_seconds(row)
        bucket["events"].append(
            {
                "id": f"attack-{row['id']}",
                "start_time": utc_iso(row["time_created"]),
                "duration_seconds": duration,
                "label": row_label(row),
                "icon": None,
            }
        )

    ordered_commanders = sorted(commanders.values(), key=lambda item: commander_sort_key(item["label"]))
    payload = {
        "date": request.view_date.isoformat(),
        "commanders": ordered_commanders,
        "server_time": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }
    elapsed_ms = (time.perf_counter() - started) * 1000
    return payload, len(attack_rows), elapsed_ms


def parse_date(value: str | None, *, timezone_name: str = DEFAULT_TIMEZONE) -> date:
    if value:
        return date.fromisoformat(value)
    return datetime.now(ZoneInfo(timezone_name)).date()
