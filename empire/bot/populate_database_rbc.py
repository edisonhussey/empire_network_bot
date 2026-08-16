from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path
from typing import Any

import psycopg


if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[2]
    sys.path.insert(0, str(REPO_ROOT))
else:
    REPO_ROOT = Path(__file__).resolve().parents[2]

from empire.bot.test_psql_connection import connect, read_connection_config
from empire.sand_rbc_farm.main import AREA_BARRON, adi_level, extract_raw_packet, parse_xt_packet, sands_level_from_gaa_value


HERE = Path(__file__).resolve().parent
LOGS_DIR = HERE / "logs"


def file_epoch(path: Path) -> int:
    return int(path.stat().st_mtime)


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def iter_server_payloads(path: Path, command: str) -> list[dict[str, Any]]:
    text = path.read_text(encoding="utf-8", errors="replace")
    payloads: list[dict[str, Any]] = []

    for block in text.split("---"):
        if "SERVER -> CLIENT" not in block:
            continue
        packet = extract_raw_packet(block)
        if packet is None:
            continue
        parsed = parse_xt_packet(packet)
        if not parsed or parsed.get("command") != command or parsed.get("status") not in {None, "0", 0}:
            continue
        payload = parsed.get("payload")
        if isinstance(payload, dict):
            payloads.append(payload)

    return payloads


def iter_server_gaa_payloads(path: Path) -> list[dict[str, Any]]:
    return iter_server_payloads(path, "gaa")


def gaa_level_from_value(kingdom_id: int, gaa_value: int) -> int | None:
    if kingdom_id == 1:
        return sands_level_from_gaa_value(gaa_value)
    return None


def rbc_rows_from_payload(payload: dict[str, Any]) -> list[tuple[int, int, int, int]]:
    rows: list[tuple[int, int, int, int]] = []
    try:
        kingdom_id = int(payload["KID"])
    except (KeyError, TypeError, ValueError):
        return rows

    for row in payload.get("AI") or []:
        if not isinstance(row, list) or len(row) < 5:
            continue
        try:
            area_type = int(row[0])
            x_coordinate = int(row[1])
            y_coordinate = int(row[2])
            gaa_value = int(row[4])
        except (TypeError, ValueError):
            continue

        # Critical NPC/RBC filter: player castles, villages, fortresses, etc.
        # use different AI area types. Only type 2 is the RBC/baron tower type.
        if area_type != AREA_BARRON:
            continue

        current_level = gaa_level_from_value(kingdom_id, gaa_value)
        if current_level is None:
            continue

        rows.append((kingdom_id, x_coordinate, y_coordinate, int(current_level)))

    return rows


def adi_target_row(payload: dict[str, Any]) -> tuple[int, int, int, int] | None:
    try:
        ai = (payload.get("gaa") or {}).get("AI")
        if not isinstance(ai, list) or len(ai) < 5:
            return None
        kingdom_id = int(payload.get("KID", payload.get("gaa", {}).get("KID", ai[-1])))
        if int(ai[0]) != AREA_BARRON:
            return None
        gaa_value = int(ai[4])
        level = gaa_level_from_value(kingdom_id, gaa_value)
        if level is None:
            level = adi_level(payload)
        if level is None:
            return None
        return kingdom_id, int(ai[1]), int(ai[2]), int(level)
    except (TypeError, ValueError):
        return None


def ensure_meta_table(conn: psycopg.Connection) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS processed_log_file (
                path TEXT PRIMARY KEY,
                sha256 TEXT NOT NULL,
                processed_at BIGINT NOT NULL,
                rbc_rows INTEGER NOT NULL
            )
            """
        )


def already_processed(conn: psycopg.Connection, path: Path, digest: str) -> bool:
    with conn.cursor() as cur:
        cur.execute(
            "SELECT 1 FROM processed_log_file WHERE path = %s AND sha256 = %s",
            (str(path), digest),
        )
        return cur.fetchone() is not None


def mark_processed(conn: psycopg.Connection, path: Path, digest: str, processed_at: int, rbc_rows: int) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO processed_log_file (path, sha256, processed_at, rbc_rows)
            VALUES (%s, %s, %s, %s)
            ON CONFLICT (path) DO UPDATE SET
                sha256 = EXCLUDED.sha256,
                processed_at = EXCLUDED.processed_at,
                rbc_rows = EXCLUDED.rbc_rows
            """,
            (str(path), digest, processed_at, rbc_rows),
        )


def upsert_rbc_rows(conn: psycopg.Connection, rows: list[tuple[int, int, int, int]]) -> int:
    if not rows:
        return 0

    with conn.cursor() as cur:
        cur.executemany(
            """
            INSERT INTO rbc (
                kingdom_id,
                x_coordinate,
                y_coordinate,
                current_level
            )
            VALUES (%s, %s, %s, %s)
            ON CONFLICT (kingdom_id, x_coordinate, y_coordinate) DO UPDATE SET
                current_level = EXCLUDED.current_level
            """,
            rows,
        )
        return cur.rowcount


def process_log_file(conn: psycopg.Connection, path: Path, *, force: bool) -> tuple[int, int]:
    digest = file_sha256(path)
    if not force and already_processed(conn, path, digest):
        return 0, 0

    payloads = iter_server_gaa_payloads(path)
    rows_by_location = {
        (kingdom_id, x_coordinate, y_coordinate): (kingdom_id, x_coordinate, y_coordinate, current_level)
        for payload in payloads
        for kingdom_id, x_coordinate, y_coordinate, current_level in rbc_rows_from_payload(payload)
    }
    for payload in iter_server_payloads(path, "adi"):
        row = adi_target_row(payload)
        if row is not None:
            rows_by_location[(row[0], row[1], row[2])] = row
    rows = list(rows_by_location.values())
    changed = upsert_rbc_rows(conn, rows)
    mark_processed(conn, path, digest, file_epoch(path), len(rows))
    return len(rows), changed


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Populate empire_bot.rbc from captured GAA server logs.")
    parser.add_argument("--logs-dir", type=Path, default=LOGS_DIR)
    parser.add_argument("--force", action="store_true", help="reprocess logs even if their checksum was seen")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    config = read_connection_config()
    log_files = sorted(args.logs_dir.glob("*.log"))

    if not log_files:
        print(f"no .log files found in {args.logs_dir}")
        return 0

    total_seen = 0
    total_changed = 0
    with connect(config) as conn:
        ensure_meta_table(conn)
        for path in log_files:
            seen, changed = process_log_file(conn, path, force=args.force)
            total_seen += seen
            total_changed += changed
            print(f"{path.name}: npc_rbc_rows={seen} db_writes={changed}")
        conn.commit()

    print(f"done files={len(log_files)} npc_rbc_rows={total_seen} db_writes={total_changed}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
