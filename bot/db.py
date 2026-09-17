"""Database connection and schema management.

One shared database (`empire_bot` by default, see `bot/psql_connect.ini`) holds
every account's rows. Isolation is by the ``aid`` column, so tables have
``(aid, ...)`` primary keys / unique indexes.

Typical use::

    python -m bot.cli db init        # create the database if missing + apply schema
    python -m bot.cli db schema      # apply schema to an existing database
    python -m bot.cli db status      # row counts per account

Programmatic use::

    from bot import db
    with db.connection() as conn:
        db.ensure_schema(conn)
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

import psycopg

from .db_account import DEFAULT_BACKFILL_AID, ensure_account_columns
from .test_psql_connection import connect, read_connection_config

#: Tables owned by this project, in dependency order.
MANAGED_TABLES = (
    "rbc",
    "attack",
    "commander_state",
    "bot_runtime_state",
    "storm_target",
    "processed_log_file",
)

#: Database used for the maintenance connection when creating a new database.
MAINTENANCE_DB = "postgres"


def connection_config() -> dict[str, str]:
    return read_connection_config()


def connection() -> psycopg.Connection:
    """Open a connection to the shared bot database."""

    return connect(read_connection_config())


def database_name() -> str:
    return read_connection_config().get("dbname", "empire_bot")


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------

_RBC_DDL = """
CREATE TABLE IF NOT EXISTS rbc (
    id BIGSERIAL PRIMARY KEY,
    aid TEXT,
    kingdom_id INTEGER NOT NULL,
    x_coordinate INTEGER NOT NULL,
    y_coordinate INTEGER NOT NULL,
    current_level INTEGER,
    last_attacked BIGINT
)
"""

_RBC_EXTRA = (
    "CREATE UNIQUE INDEX IF NOT EXISTS rbc_aid_location_key "
    "ON rbc (aid, kingdom_id, x_coordinate, y_coordinate)",
    "CREATE INDEX IF NOT EXISTS rbc_reserve_idx "
    "ON rbc (aid, kingdom_id, current_level, last_attacked)",
)

_ATTACK_DDL = """
CREATE TABLE IF NOT EXISTS attack (
    id BIGSERIAL PRIMARY KEY,
    aid TEXT,
    kingdom_id INTEGER NOT NULL,
    x_coordinate INTEGER NOT NULL,
    y_coordinate INTEGER NOT NULL,
    march_id BIGINT NOT NULL,
    time_created BIGINT,
    troop_count INTEGER,
    duration INTEGER,
    return_duration INTEGER,
    coin_loot INTEGER,
    ruby_loot INTEGER,
    error_message TEXT
)
"""

_ATTACK_EXTRA = (
    "ALTER TABLE attack ALTER COLUMN march_id TYPE BIGINT",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS target_kind TEXT",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS target_id BIGINT",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS task_name TEXT",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS target_level INTEGER",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS lord_id INTEGER",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS commander_number INTEGER",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'sent'",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS result_flag INTEGER",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS landed_at BIGINT",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS result_received_at BIGINT",
    "ALTER TABLE attack ADD COLUMN IF NOT EXISTS raw_result JSONB",
)

_COMMANDER_STATE_DDL = """
CREATE TABLE IF NOT EXISTS commander_state (
    lord_id INTEGER NOT NULL,
    available_after BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'available',
    march_id BIGINT,
    target_rbc_id BIGINT,
    updated_at BIGINT NOT NULL DEFAULT 0
)
"""

_RUNTIME_STATE_DDL = """
CREATE TABLE IF NOT EXISTS bot_runtime_state (
    key TEXT NOT NULL,
    value_text TEXT NOT NULL
)
"""

_PROCESSED_LOG_DDL = """
CREATE TABLE IF NOT EXISTS processed_log_file (
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    processed_at BIGINT NOT NULL,
    rbc_rows INTEGER NOT NULL,
    aid TEXT
)
"""


def ensure_schema(conn: psycopg.Connection, aid: str | None = None) -> None:
    """Create every managed table (idempotent) and add per-account keys.

    Safe to run against an existing database: all statements are
    ``IF NOT EXISTS`` / ``ADD COLUMN IF NOT EXISTS``.
    """

    from .storm_database import ensure_storm_tables

    account = str(aid or DEFAULT_BACKFILL_AID)

    with conn.cursor() as cur:
        cur.execute(_RBC_DDL)
        cur.execute(_ATTACK_DDL)
        cur.execute(_COMMANDER_STATE_DDL)
        cur.execute(_RUNTIME_STATE_DDL)
        cur.execute(_PROCESSED_LOG_DDL)
    conn.commit()

    # storm_target lives with the storm code; keeps its extra indexes together.
    ensure_storm_tables(conn)

    with conn.cursor() as cur:
        for statement in (*_RBC_EXTRA, *_ATTACK_EXTRA):
            cur.execute(statement)
    conn.commit()

    # Backfill the aid column and (re)build the composite primary keys.
    ensure_account_columns(conn, account)


def table_counts(conn: psycopg.Connection, aid: str) -> dict[str, int]:
    """Row counts per managed table for one account."""

    counts: dict[str, int] = {}
    with conn.cursor() as cur:
        for table in MANAGED_TABLES:
            cur.execute("SELECT to_regclass(%s)", (table,))
            if cur.fetchone()[0] is None:
                counts[table] = -1
                continue
            cur.execute(f"SELECT count(*) FROM {table} WHERE aid = %s", (aid,))
            counts[table] = int(cur.fetchone()[0])
    return counts


# ---------------------------------------------------------------------------
# Account lifecycle
# ---------------------------------------------------------------------------


def _has_account_rows(conn: psycopg.Connection, aid: str) -> bool:
    with conn.cursor() as cur:
        cur.execute("SELECT 1 FROM commander_state WHERE aid = %s LIMIT 1", (str(aid),))
        return cur.fetchone() is not None


def register_account(aid: str, conn: psycopg.Connection | None = None) -> bool:
    """Ensure the schema exists and the account has its baseline rows.

    Called when a login handshake is seen, so a brand new account needs no
    manual setup: its aid simply appears in the tables on first login.

    Returns ``True`` when this aid had no rows before, i.e. it is new.
    """

    from .tasks import TASKS

    account = str(aid)
    lids = sorted({int(lid) for task in TASKS for lid in (task.commander_lids or ())})
    own_connection = conn is None
    if conn is None:
        conn = connection()
    try:
        ensure_schema(conn, account)
        is_new = not _has_account_rows(conn, account)
        now = int(time.time())
        with conn.cursor() as cur:
            for lid in lids:
                cur.execute(
                    """
                    INSERT INTO commander_state (aid, lord_id, available_after, status, updated_at)
                    VALUES (%s, %s, 0, 'available', %s)
                    ON CONFLICT (aid, lord_id) DO NOTHING
                    """,
                    (account, lid, now),
                )
        conn.commit()
        return is_new
    finally:
        if own_connection:
            conn.close()


def migrate_account_keys(mapping: dict[str, str], *, dry_run: bool = True) -> dict[str, int]:
    """Rename legacy scope keys onto their login-name keys in every table.

    Refuses to run when an account would be merged into a key that already has
    rows, so two accounts can never silently collapse into one.

    Returns ``{f"{old}->{new}:{table}": rows}``.
    """

    counts: dict[str, int] = {}
    with connection() as conn:
        for old, new in mapping.items():
            if old == new:
                continue
            present: list[str] = []
            with conn.cursor() as cur:
                for table in MANAGED_TABLES:
                    cur.execute("SELECT to_regclass(%s)", (table,))
                    if cur.fetchone()[0] is None:
                        continue
                    present.append(table)
                    cur.execute(f"SELECT count(*) FROM {table} WHERE aid = %s", (old,))
                    old_rows = int(cur.fetchone()[0])
                    cur.execute(f"SELECT count(*) FROM {table} WHERE aid = %s", (new,))
                    new_rows = int(cur.fetchone()[0])
                    if old_rows and new_rows:
                        raise RuntimeError(
                            f"refusing to merge {table}: '{old}' has {old_rows} rows and "
                            f"'{new}' already has {new_rows}"
                        )
                    counts[f"{old}->{new}:{table}"] = old_rows

            if dry_run:
                continue

            with conn.cursor() as cur:
                for table in present:
                    cur.execute(f"UPDATE {table} SET aid = %s WHERE aid = %s", (new, old))

            moved_paths = rewrite_processed_log_paths(conn, old, new)
            if moved_paths:
                counts[f"{old}->{new}:processed_log_file.path"] = moved_paths
        if not dry_run:
            conn.commit()
    return counts


# ---------------------------------------------------------------------------
# Capture log store
# ---------------------------------------------------------------------------

#: Default per-account capture folder cap. Override with GGE_CAPTURE_MAX_MB.
DEFAULT_CAPTURE_MAX_MB = 50


def capture_max_bytes() -> int:
    """Size cap for one account's capture folder, from GGE_CAPTURE_MAX_MB."""

    raw = os.environ.get("GGE_CAPTURE_MAX_MB")
    try:
        megabytes = int(raw) if raw else DEFAULT_CAPTURE_MAX_MB
    except (TypeError, ValueError):
        megabytes = DEFAULT_CAPTURE_MAX_MB
    return max(1, megabytes) * 1024 * 1024


def list_capture_logs(folder: Path) -> list[Path]:
    """Capture files in a folder, oldest first (names are timestamps)."""

    if not folder.is_dir():
        return []
    return sorted(folder.glob("gge_*.log"), key=lambda path: path.name)


def capture_folder_stats(folder: Path) -> dict[str, int]:
    files = list_capture_logs(folder)
    total = 0
    for path in files:
        try:
            total += path.stat().st_size
        except OSError:
            continue
    return {"files": len(files), "bytes": total}


def processed_log_paths(conn: psycopg.Connection, aid: str) -> set[str]:
    """Capture paths already ingested by ``db populate`` for one account."""

    with conn.cursor() as cur:
        cur.execute("SELECT to_regclass('processed_log_file')")
        if cur.fetchone()[0] is None:
            return set()
        cur.execute("SELECT path FROM processed_log_file WHERE aid = %s", (str(aid),))
        return {str(row[0]) for row in cur.fetchall()}


def all_processed_log_paths(conn: psycopg.Connection) -> set[str]:
    """Every capture path ingested by any account.

    Capture paths are unique, so the union is unambiguous and lets the legacy
    shared folder be pruned too.
    """

    with conn.cursor() as cur:
        cur.execute("SELECT to_regclass('processed_log_file')")
        if cur.fetchone()[0] is None:
            return set()
        cur.execute("SELECT DISTINCT path FROM processed_log_file")
        return {str(row[0]) for row in cur.fetchall()}


def rewrite_processed_log_paths(conn: psycopg.Connection, old: str, new: str) -> int:
    """Repoint recorded capture paths after a data directory rename.

    Without this, ``processed_log_file`` keeps pointing at ``account_data/<old>/``
    so every capture looks unprocessed: ``db populate`` would re-read everything
    and pruning would refuse to delete anything.
    """

    like = f"%/account_data/{old}/%"
    with conn.cursor() as cur:
        cur.execute("SELECT to_regclass('processed_log_file')")
        if cur.fetchone()[0] is None:
            return 0
        cur.execute("SELECT count(*) FROM processed_log_file WHERE path LIKE %s", (like,))
        matched = int(cur.fetchone()[0])
        if not matched:
            return 0
        cur.execute(
            "UPDATE processed_log_file SET path = replace(path, %s, %s) WHERE path LIKE %s",
            (f"/account_data/{old}/", f"/account_data/{new}/", like),
        )
        return matched


def prune_capture_logs(
    folder: Path,
    *,
    max_bytes: int,
    processed: set[str],
    dry_run: bool = False,
    require_processed: bool = True,
) -> dict[str, int]:
    """Delete the oldest captures until the folder fits ``max_bytes``.

    With ``require_processed`` (the default) only files recorded in
    ``processed_log_file`` are removed, so a capture that ``db populate`` has not
    read yet is never lost. Pass ``require_processed=False`` for folders with no
    tracking (e.g. the legacy shared ``bot/logs``) to delete oldest-first
    regardless. If the cap cannot be met, the result reports ``over_cap``.
    """

    files = list_capture_logs(folder)
    sizes: dict[Path, int] = {}
    total = 0
    for path in files:
        try:
            sizes[path] = path.stat().st_size
        except OSError:
            continue
        total += sizes[path]

    deleted = 0
    freed = 0
    protected = 0
    for path in files:  # oldest first
        if total <= max_bytes:
            break
        if require_processed and str(path) not in processed:
            protected += 1
            continue
        size = sizes.get(path, 0)
        if not dry_run:
            try:
                path.unlink()
            except OSError:
                continue
        deleted += 1
        freed += size
        total -= size

    return {
        "files": len(files),
        "deleted": deleted,
        "freed_bytes": freed,
        "protected": protected,
        "bytes": max(0, total),
        "over_cap": int(total > max_bytes),
    }


#: bot_runtime_state key holding the per-kingdom source castle coordinates.
SOURCE_KEY = "sources"


def read_account_sources(conn: psycopg.Connection, aid: str) -> dict[str, dict[str, object]]:
    """``{kingdom_id: {x, y, castle_id, name}}`` of known source castles."""

    with conn.cursor() as cur:
        cur.execute("SELECT to_regclass('bot_runtime_state')")
        if cur.fetchone()[0] is None:
            return {}
        cur.execute(
            "SELECT value_text FROM bot_runtime_state WHERE aid = %s AND key = %s",
            (str(aid), SOURCE_KEY),
        )
        row = cur.fetchone()
    if row is None:
        return {}
    try:
        data = json.loads(row[0])
    except (TypeError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def write_account_sources(
    conn: psycopg.Connection,
    aid: str,
    sources: dict[str, dict[str, object]],
) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO bot_runtime_state (aid, key, value_text)
            VALUES (%s, %s, %s)
            ON CONFLICT (aid, key) DO UPDATE SET value_text = EXCLUDED.value_text
            """,
            (str(aid), SOURCE_KEY, json.dumps(sources, sort_keys=True)),
        )
    conn.commit()


def update_account_sources(
    conn: psycopg.Connection,
    aid: str,
    discovered: dict[int, dict[str, object]],
) -> dict[str, dict[str, object]]:
    """Merge newly discovered castles in. Returns only what actually changed."""

    existing = read_account_sources(conn, aid)
    changed: dict[str, dict[str, object]] = {}
    for kingdom_id, castle in discovered.items():
        key = str(int(kingdom_id))
        previous = existing.get(key)
        if not isinstance(previous, dict):
            previous = {}
        entry = {
            "x": int(castle["x"]),
            "y": int(castle["y"]),
            "castle_id": int(castle.get("castle_id") or 0),
            "name": str(castle.get("name") or ""),
        }
        hbw = castle.get("hbw", previous.get("hbw"))
        if hbw is not None:
            entry["hbw"] = int(hbw)
        if existing.get(key) != entry:
            existing[key] = entry
            changed[key] = entry
    if changed:
        write_account_sources(conn, aid, existing)
    return changed


#: bot_runtime_state key holding an account's ordered commander LIDs.
COMMANDER_LIDS_KEY = "commander_lids"


def read_account_commander_lids(conn: psycopg.Connection, aid: str) -> list[int]:
    """An account's learned commander roster (ordered), or ``[]``."""

    with conn.cursor() as cur:
        cur.execute("SELECT to_regclass('bot_runtime_state')")
        if cur.fetchone()[0] is None:
            return []
        cur.execute(
            "SELECT value_text FROM bot_runtime_state WHERE aid = %s AND key = %s",
            (str(aid), COMMANDER_LIDS_KEY),
        )
        row = cur.fetchone()
    if row is None:
        return []
    try:
        data = json.loads(row[0])
    except (TypeError, json.JSONDecodeError):
        return []
    if not isinstance(data, list):
        return []
    out: list[int] = []
    for value in data:
        try:
            out.append(int(value))
        except (TypeError, ValueError):
            continue
    return out


def update_account_commander_lids(
    conn: psycopg.Connection,
    aid: str,
    lids: list[int],
) -> bool:
    """Store an account's roster. Returns True when it changed."""

    cleaned = [int(lid) for lid in lids]
    if not cleaned or read_account_commander_lids(conn, aid) == cleaned:
        return False
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO bot_runtime_state (aid, key, value_text)
            VALUES (%s, %s, %s)
            ON CONFLICT (aid, key) DO UPDATE SET value_text = EXCLUDED.value_text
            """,
            (str(aid), COMMANDER_LIDS_KEY, json.dumps(cleaned)),
        )
    conn.commit()
    return True


def list_accounts_in_db(conn: psycopg.Connection) -> list[tuple[str, int]]:
    """``(aid, rbc_rows)`` for every account that has RBC rows."""

    with conn.cursor() as cur:
        cur.execute("SELECT to_regclass('rbc')")
        if cur.fetchone()[0] is None:
            return []
        cur.execute(
            """
            SELECT aid, count(*)::int
            FROM rbc
            WHERE aid IS NOT NULL
            GROUP BY aid
            ORDER BY aid
            """
        )
        return [(str(aid), int(count)) for aid, count in cur.fetchall()]


# ---------------------------------------------------------------------------
# Database lifecycle
# ---------------------------------------------------------------------------


def database_exists(config: dict[str, str] | None = None) -> bool:
    """True when the configured database already exists."""

    config = dict(config or read_connection_config())
    target = config.get("dbname", "empire_bot")
    maintenance = {**config, "dbname": MAINTENANCE_DB}
    with connect(maintenance) as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT 1 FROM pg_database WHERE datname = %s", (target,))
            return cur.fetchone() is not None


def create_database(config: dict[str, str] | None = None) -> str:
    """Create the bot database if it does not exist. Returns the database name."""

    config = dict(config or read_connection_config())
    target = config.get("dbname", "empire_bot")

    try:
        with connect(config):
            return target
    except psycopg.OperationalError:
        pass

    maintenance = {**config, "dbname": MAINTENANCE_DB}
    with connect(maintenance) as conn:
        conn.autocommit = True
        with conn.cursor() as cur:
            cur.execute("SELECT 1 FROM pg_database WHERE datname = %s", (target,))
            if cur.fetchone() is None:
                cur.execute(f'CREATE DATABASE "{target}"')
    return target


def init_database(*, aid: str | None = None, create: bool = True) -> str:
    """Ensure the database and schema exist, then return the database name."""

    config = read_connection_config()
    target = config.get("dbname", "empire_bot")

    try:
        with connect(config) as conn:
            ensure_schema(conn, aid)
            return target
    except psycopg.OperationalError:
        if not create:
            raise

    create_database(config)
    with connect(config) as conn:
        ensure_schema(conn, aid)
    return target


__all__ = [
    "DEFAULT_CAPTURE_MAX_MB",
    "MANAGED_TABLES",
    "capture_folder_stats",
    "capture_max_bytes",
    "connection",
    "connection_config",
    "create_database",
    "database_exists",
    "database_name",
    "ensure_schema",
    "init_database",
    "list_accounts_in_db",
    "list_capture_logs",
    "migrate_account_keys",
    "all_processed_log_paths",
    "processed_log_paths",
    "prune_capture_logs",
    "read_account_commander_lids",
    "read_account_sources",
    "register_account",
    "rewrite_processed_log_paths",
    "table_counts",
    "update_account_commander_lids",
    "update_account_sources",
    "write_account_sources",
]
