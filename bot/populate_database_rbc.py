from __future__ import annotations

import argparse
import hashlib
import sys
import time
from pathlib import Path
from typing import Any

import psycopg


if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(REPO_ROOT))
else:
    REPO_ROOT = Path(__file__).resolve().parents[1]

from bot.test_psql_connection import connect, read_connection_config
from bot import accounts
from bot.db_account import DEFAULT_BACKFILL_AID, ensure_account_columns
from bot.packets import (
    AREA_BARRON,
    adi_level,
    extract_raw_packet,
    parse_xt_packet,
    sands_level_from_gaa_value,
)


HERE = Path(__file__).resolve().parent
CURRENT_AID = DEFAULT_BACKFILL_AID


def set_account_aid(aid: str) -> None:
    global CURRENT_AID
    CURRENT_AID = str(aid)


def current_aid() -> str:
    return str(CURRENT_AID)


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
    conn.commit()
    ensure_account_columns(conn, current_aid())


def already_processed(conn: psycopg.Connection, path: Path, digest: str) -> bool:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            "SELECT 1 FROM processed_log_file WHERE aid = %s AND path = %s AND sha256 = %s",
            (aid, str(path), digest),
        )
        return cur.fetchone() is not None


def mark_processed(conn: psycopg.Connection, path: Path, digest: str, processed_at: int, rbc_rows: int) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO processed_log_file (aid, path, sha256, processed_at, rbc_rows)
            VALUES (%s, %s, %s, %s, %s)
            ON CONFLICT (aid, path) DO UPDATE SET
                sha256 = EXCLUDED.sha256,
                processed_at = EXCLUDED.processed_at,
                rbc_rows = EXCLUDED.rbc_rows
            """,
            (aid, str(path), digest, processed_at, rbc_rows),
        )


def upsert_rbc_rows(conn: psycopg.Connection, rows: list[tuple[int, int, int, int]]) -> int:
    if not rows:
        return 0

    aid = current_aid()
    with conn.cursor() as cur:
        cur.executemany(
            """
            INSERT INTO rbc (
                aid,
                kingdom_id,
                x_coordinate,
                y_coordinate,
                current_level
            )
            VALUES (%s, %s, %s, %s, %s)
            ON CONFLICT (aid, kingdom_id, x_coordinate, y_coordinate) DO UPDATE SET
                current_level = EXCLUDED.current_level
            """,
            [(aid, *row) for row in rows],
        )
        return cur.rowcount


def process_log_file(conn: psycopg.Connection, path: Path, *, force: bool) -> tuple[int, int, str]:
    """Ingest one capture. Returns ``(npc_rows, db_writes, status)``.

    ``status`` is ``"already"`` (checksum seen before), ``"empty"`` (read but the
    file held no GAA/ADI map data) or ``"data"`` (it carried targets). Telling
    these apart matters: a silent zero looks like a failure when it is not.
    """

    digest = file_sha256(path)
    if not force and already_processed(conn, path, digest):
        return 0, 0, "already"

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
    return len(rows), changed, "data" if rows else "empty"


#: Default poll interval for --watch, in seconds.
WATCH_POLL_SECONDS = 10.0

#: Skip captures modified this recently: the listener appends to the newest file
#: for ROLL_SECONDS, so it is still being written and would be reprocessed as it
#: grows. Waiting one roll for it to settle avoids that churn.
WATCH_ACTIVE_GRACE_SECONDS = 15.0


def settled_capture_files(logs_dir: Path, *, active_grace: float) -> list[Path]:
    """Capture files that have stopped being written, oldest first."""

    cutoff = time.time() - max(0.0, active_grace)
    settled: list[Path] = []
    for path in logs_dir.glob("gge_*.log"):
        try:
            if path.stat().st_mtime <= cutoff:
                settled.append(path)
        except OSError:
            continue
    return sorted(settled, key=lambda path: path.name)


def run_watch(
    conn: psycopg.Connection,
    logs_dir: Path,
    *,
    force: bool,
    interval: float,
    active_grace: float,
) -> int:
    """Keep ingesting new captures until interrupted.

    Only files whose write has settled are read, and the checksum check means a
    file is ingested once. Pruning of already-ingested captures is handled by the
    listener, so the folder stays bounded while this runs.
    """

    print(
        f"populate_watch logs_dir={logs_dir} poll={interval:.0f}s "
        f"settle={active_grace:.0f}s :: ctrl-c to stop"
    )
    passes = 0
    files_seen = 0
    rows_seen = 0
    writes = 0
    # Remember size+mtime per file so an unchanged capture is not re-hashed on
    # every pass; the DB checksum check would catch it, but that costs a full
    # read of the file each time.
    fingerprints: dict[Path, tuple[int, int]] = {}
    try:
        while True:
            passes += 1
            inspected = 0
            pass_rows = 0
            pass_writes = 0
            counts = {"data": 0, "empty": 0, "already": 0}
            for path in settled_capture_files(logs_dir, active_grace=active_grace):
                try:
                    stat = path.stat()
                except OSError:
                    continue
                fingerprint = (stat.st_size, int(stat.st_mtime))
                if fingerprints.get(path) == fingerprint:
                    continue
                seen, changed, status = process_log_file(conn, path, force=force)
                fingerprints[path] = fingerprint
                inspected += 1
                counts[status] += 1
                pass_rows += seen
                pass_writes += changed
                if seen or changed:
                    print(f"{path.name}: npc_rbc_rows={seen} db_writes={changed}")
            if inspected:
                conn.commit()
            files_seen += inspected
            rows_seen += pass_rows
            writes += pass_writes

            with conn.cursor() as cur:
                cur.execute("SELECT count(*) FROM rbc WHERE aid = %s", (current_aid(),))
                rbc_total = int(cur.fetchone()[0])

            # Always report, so a zero is visibly a zero rather than a hang.
            print(
                f"watch pass={passes} new={inspected} "
                f"[with_data={counts['data']} no_map_data={counts['empty']} "
                f"already_done={counts['already']}] "
                f"npc_rows={pass_rows} db_writes={pass_writes} rbc_total={rbc_total}"
            )
            if passes == 1 and not inspected:
                print(f"  (nothing new settled in {logs_dir} yet; waiting for the listener)")

            time.sleep(interval)
    except KeyboardInterrupt:
        print(
            f"\npopulate_watch_stopped passes={passes} files_read={files_seen} "
            f"npc_rows={rows_seen} db_writes={writes}"
        )
        return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Populate empire_bot.rbc from captured GAA server logs.")
    accounts.add_account_arguments(parser)
    parser.add_argument(
        "--logs-dir",
        type=Path,
        default=None,
        help="Directory of mitmproxy .log captures. Defaults to this account's own "
        "bot/account_data/<aid>/logs (never the shared bot/logs).",
    )
    parser.add_argument("--force", action="store_true", help="reprocess logs even if their checksum was seen")
    parser.add_argument(
        "--watch",
        action="store_true",
        help="keep running and keep ingesting new captures until Ctrl+C",
    )
    parser.add_argument(
        "--poll",
        type=float,
        default=WATCH_POLL_SECONDS,
        metavar="SECONDS",
        help=f"with --watch: seconds between scans (default {WATCH_POLL_SECONDS:.0f})",
    )
    return parser.parse_args(accounts.apply_account_flags(list(sys.argv[1:] if argv is None else argv)))


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    account = accounts.account_from_args(args)
    set_account_aid(account.aid)

    # Deliberately no fallback to the legacy shared bot/logs: those captures
    # belong to whichever account produced them, and loading them under a
    # different aid would inject another account's targets into this one.
    logs_dir = args.logs_dir if args.logs_dir is not None else account.logs_dir
    print(f"populate_account username={account.name} aid={account.aid}")
    print(f"populate_source  logs_dir={logs_dir}")

    config = read_connection_config()

    with connect(config) as conn:
        ensure_meta_table(conn)

        if args.watch:
            return run_watch(
                conn,
                logs_dir,
                force=args.force,
                interval=max(1.0, float(args.poll)),
                active_grace=WATCH_ACTIVE_GRACE_SECONDS,
            )

        log_files = sorted(logs_dir.glob("*.log"))
        if not log_files:
            print(f"no .log files found in {logs_dir}")
            print("log in through the listener with this account first so its captures exist")
            return 0

        total_seen = 0
        total_changed = 0
        total_files = 0
        counts = {"data": 0, "empty": 0, "already": 0}
        for path in log_files:
            seen, changed, status = process_log_file(conn, path, force=args.force)
            total_files += 1
            counts[status] += 1
            total_seen += seen
            total_changed += changed
            if seen or changed:
                print(f"{path.name}: npc_rbc_rows={seen} db_writes={changed}")
        conn.commit()
        with conn.cursor() as cur:
            cur.execute("SELECT count(*) FROM rbc WHERE aid = %s", (current_aid(),))
            rbc_total = int(cur.fetchone()[0])

    print(
        f"done files={total_files} with_data={counts['data']} "
        f"no_map_data={counts['empty']} already_done={counts['already']} "
        f"npc_rbc_rows={total_seen} db_writes={total_changed} rbc_total={rbc_total}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
