from __future__ import annotations

import psycopg


DEFAULT_BACKFILL_AID = "1782860727866351909"


def _drop_constraint_if_exists(cur: psycopg.Cursor, table: str, name: str) -> None:
    cur.execute(f"ALTER TABLE {table} DROP CONSTRAINT IF EXISTS {name}")


def _table_exists(cur: psycopg.Cursor, table: str) -> bool:
    cur.execute("SELECT to_regclass(%s)", (table,))
    return cur.fetchone()[0] is not None


def _column_exists(cur: psycopg.Cursor, table: str, column: str) -> bool:
    cur.execute(
        """
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = %s
          AND column_name = %s
        """,
        (table, column),
    )
    return cur.fetchone() is not None


def _column_nullable(cur: psycopg.Cursor, table: str, column: str) -> bool:
    cur.execute(
        """
        SELECT is_nullable = 'YES'
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = %s
          AND column_name = %s
        """,
        (table, column),
    )
    row = cur.fetchone()
    return bool(row and row[0])


def _constraint_def(cur: psycopg.Cursor, table: str, name: str) -> str | None:
    cur.execute(
        """
        SELECT pg_get_constraintdef(c.oid)
        FROM pg_constraint c
        JOIN pg_class t ON t.oid = c.conrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        WHERE n.nspname = 'public'
          AND t.relname = %s
          AND c.conname = %s
        """,
        (table, name),
    )
    row = cur.fetchone()
    return str(row[0]) if row else None


def _index_exists(cur: psycopg.Cursor, name: str) -> bool:
    cur.execute("SELECT to_regclass(%s)", (name,))
    return cur.fetchone()[0] is not None


def _null_count(cur: psycopg.Cursor, table: str, column: str) -> int:
    cur.execute(f"SELECT count(*) FROM {table} WHERE {column} IS NULL")
    return int(cur.fetchone()[0])


def _ensure_column(cur: psycopg.Cursor, table: str, column: str, definition: str) -> None:
    if not _column_exists(cur, table, column):
        cur.execute(f"ALTER TABLE {table} ADD COLUMN {column} {definition}")


def _set_not_null_if_needed(cur: psycopg.Cursor, table: str, column: str) -> None:
    if _column_nullable(cur, table, column):
        cur.execute(f"ALTER TABLE {table} ALTER COLUMN {column} SET NOT NULL")


def _ensure_primary_key(cur: psycopg.Cursor, table: str, name: str, definition: str) -> None:
    current = _constraint_def(cur, table, name)
    if current == definition:
        return
    if current is not None:
        _drop_constraint_if_exists(cur, table, name)
    cur.execute(f"ALTER TABLE {table} ADD PRIMARY KEY {definition.removeprefix('PRIMARY KEY ')}")


def ensure_account_columns(conn: psycopg.Connection, aid: str = DEFAULT_BACKFILL_AID) -> None:
    aid = str(aid)
    with conn.cursor() as cur:
        for table in ("attack", "rbc", "storm_target", "commander_state", "bot_runtime_state", "processed_log_file"):
            if not _table_exists(cur, table):
                continue
            _ensure_column(cur, table, "aid", "TEXT")
            if _null_count(cur, table, "aid"):
                cur.execute(f"UPDATE {table} SET aid = %s WHERE aid IS NULL", (aid,))

        if _table_exists(cur, "commander_state"):
            _set_not_null_if_needed(cur, "commander_state", "aid")
            _ensure_primary_key(cur, "commander_state", "commander_state_pkey", "PRIMARY KEY (aid, lord_id)")

        if _table_exists(cur, "bot_runtime_state"):
            _set_not_null_if_needed(cur, "bot_runtime_state", "aid")
            _ensure_primary_key(cur, "bot_runtime_state", "bot_runtime_state_pkey", "PRIMARY KEY (aid, key)")

        if _table_exists(cur, "attack"):
            _ensure_column(cur, "attack", "commander_number", "INTEGER")
            if _constraint_def(cur, "attack", "attack_march_id_key") is not None:
                _drop_constraint_if_exists(cur, "attack", "attack_march_id_key")
            if not _index_exists(cur, "attack_aid_march_id_key"):
                cur.execute("CREATE UNIQUE INDEX attack_aid_march_id_key ON attack (aid, march_id)")
            _set_not_null_if_needed(cur, "attack", "aid")

        if _table_exists(cur, "rbc"):
            if _constraint_def(cur, "rbc", "rbc_kingdom_id_x_coordinate_y_coordinate_key") is not None:
                _drop_constraint_if_exists(cur, "rbc", "rbc_kingdom_id_x_coordinate_y_coordinate_key")
            if not _index_exists(cur, "rbc_aid_location_key"):
                cur.execute("CREATE UNIQUE INDEX rbc_aid_location_key ON rbc (aid, kingdom_id, x_coordinate, y_coordinate)")
            _set_not_null_if_needed(cur, "rbc", "aid")

        if _table_exists(cur, "storm_target"):
            if _constraint_def(cur, "storm_target", "storm_target_kingdom_id_area_type_x_coordinate_y_coordinate_key") is not None:
                _drop_constraint_if_exists(cur, "storm_target", "storm_target_kingdom_id_area_type_x_coordinate_y_coordinate_key")
            if not _index_exists(cur, "storm_target_aid_location_key"):
                cur.execute(
                    "CREATE UNIQUE INDEX storm_target_aid_location_key ON storm_target "
                    "(aid, kingdom_id, area_type, x_coordinate, y_coordinate)"
                )
            _set_not_null_if_needed(cur, "storm_target", "aid")

        if _table_exists(cur, "processed_log_file"):
            _set_not_null_if_needed(cur, "processed_log_file", "aid")
            _ensure_primary_key(cur, "processed_log_file", "processed_log_file_pkey", "PRIMARY KEY (aid, path)")
    conn.commit()
