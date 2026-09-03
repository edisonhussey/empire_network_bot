from __future__ import annotations

import time
from typing import Any

import psycopg
from psycopg.types.json import Jsonb

from bot.db_account import DEFAULT_BACKFILL_AID, ensure_account_columns
from bot.scheduler import commander_human_number


STORM_KID = 4
STORM_TTL_SECONDS = 4 * 3600
STORM_CANDIDATE_AREA_TYPES = (24, 25)
STORM_PLAYER_AREA_TYPE = 12
SYSTEM_OWNER_IDS = {-403, -1}
STORM_LEVEL_CODE_TO_LEVEL = {
    7: 60,
    8: 70,
    9: 80,
    10: 40,
    11: 50,
    12: 60,
    13: 70,
    14: 80,
}
CURRENT_AID = DEFAULT_BACKFILL_AID


def set_account_aid(aid: str) -> None:
    global CURRENT_AID
    CURRENT_AID = str(aid)


def current_aid() -> str:
    return str(CURRENT_AID)


def now_epoch() -> int:
    return int(time.time())


def ensure_storm_tables(conn: psycopg.Connection) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS attack (
                id BIGSERIAL PRIMARY KEY,
                kingdom_id INTEGER NOT NULL,
                x_coordinate INTEGER NOT NULL,
                y_coordinate INTEGER NOT NULL,
                march_id INTEGER NOT NULL UNIQUE,
                time_created BIGINT,
                troop_count INTEGER,
                duration INTEGER,
                return_duration INTEGER,
                coin_loot INTEGER,
                ruby_loot INTEGER,
                error_message TEXT
            )
            """
        )
        for statement in (
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
        ):
            cur.execute(statement)
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS storm_target (
                id BIGSERIAL PRIMARY KEY,
                kingdom_id INTEGER NOT NULL DEFAULT 4,
                area_type INTEGER NOT NULL,
                x_coordinate INTEGER NOT NULL,
                y_coordinate INTEGER NOT NULL,
                object_id BIGINT,
                owner_id BIGINT,
                storm_type INTEGER,
                display_name TEXT,
                target_level INTEGER,
                raw_ai JSONB NOT NULL,
                first_seen_at BIGINT NOT NULL,
                last_seen_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL,
                reserved_until BIGINT,
                sent_at BIGINT,
                march_id BIGINT,
                result_flag INTEGER,
                result_at BIGINT,
                raw_result JSONB,
                status TEXT NOT NULL DEFAULT 'seen',
                attack_name TEXT,
                UNIQUE (kingdom_id, area_type, x_coordinate, y_coordinate)
            )
            """
        )
        cur.execute(
            """
            CREATE INDEX IF NOT EXISTS storm_target_live_idx
            ON storm_target (kingdom_id, expires_at, sent_at, reserved_until, target_level)
            """
        )
        cur.execute(
            """
            CREATE INDEX IF NOT EXISTS storm_target_location_idx
            ON storm_target (kingdom_id, x_coordinate, y_coordinate)
            """
        )
        for statement in (
            "ALTER TABLE storm_target ADD COLUMN IF NOT EXISTS result_flag INTEGER",
            "ALTER TABLE storm_target ADD COLUMN IF NOT EXISTS result_at BIGINT",
            "ALTER TABLE storm_target ADD COLUMN IF NOT EXISTS raw_result JSONB",
        ):
            cur.execute(statement)
    conn.commit()
    ensure_account_columns(conn, current_aid())


def cleanup_expired_storm_targets(conn: psycopg.Connection, *, now: int | None = None) -> int:
    epoch = now_epoch() if now is None else int(now)
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            DELETE FROM storm_target
            WHERE aid = %s
              AND (expires_at <= %s
               OR last_seen_at <= %s)
            """,
            (aid, epoch, epoch - STORM_TTL_SECONDS),
        )
        deleted = cur.rowcount
    conn.commit()
    return int(deleted)


def _int_or_none(value: Any) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def target_level_from_storm_ai(row: list[Any]) -> int | None:
    if len(row) < 9:
        return None
    area_type = _int_or_none(row[0])
    if area_type != 25:
        return None
    if _int_or_none(row[6]) != 0 or _int_or_none(row[8]) != 0:
        return None
    return STORM_LEVEL_CODE_TO_LEVEL.get(_int_or_none(row[5]))


def storm_targets_from_gaa(payload: dict[str, Any], *, now: int | None = None) -> list[dict[str, Any]]:
    epoch = now_epoch() if now is None else int(now)
    try:
        kingdom_id = int(payload.get("KID"))
    except (TypeError, ValueError):
        return []
    if kingdom_id != STORM_KID:
        return []

    targets: list[dict[str, Any]] = []
    for row in payload.get("AI") or []:
        if not isinstance(row, list) or len(row) < 5:
            continue
        area_type = _int_or_none(row[0])
        if area_type not in STORM_CANDIDATE_AREA_TYPES:
            continue

        x_coordinate = _int_or_none(row[1])
        y_coordinate = _int_or_none(row[2])
        if x_coordinate is None or y_coordinate is None:
            continue

        object_id = _int_or_none(row[3])
        owner_id = _int_or_none(row[4])
        if area_type == 24 and owner_id not in SYSTEM_OWNER_IDS:
            continue
        if area_type == 25 and owner_id not in SYSTEM_OWNER_IDS:
            continue

        storm_type = _int_or_none(row[5]) if len(row) > 5 else None
        display_name = row[6] if len(row) > 6 and isinstance(row[6], str) and row[6] else None
        targets.append(
            {
                "kingdom_id": kingdom_id,
                "area_type": area_type,
                "x_coordinate": x_coordinate,
                "y_coordinate": y_coordinate,
                "object_id": object_id,
                "owner_id": owner_id,
                "storm_type": storm_type,
                "display_name": display_name,
                "target_level": target_level_from_storm_ai(row),
                "raw_ai": row,
                "seen_at": epoch,
                "expires_at": epoch + STORM_TTL_SECONDS,
            }
        )
    return targets


def upsert_storm_targets(conn: psycopg.Connection, targets: list[dict[str, Any]]) -> int:
    if not targets:
        return 0
    aid = current_aid()
    with conn.cursor() as cur:
        cur.executemany(
            """
            INSERT INTO storm_target (
                aid,
                kingdom_id,
                area_type,
                x_coordinate,
                y_coordinate,
                object_id,
                owner_id,
                storm_type,
                display_name,
                target_level,
                raw_ai,
                first_seen_at,
                last_seen_at,
                expires_at,
                status
            )
            VALUES (
                %(aid)s,
                %(kingdom_id)s,
                %(area_type)s,
                %(x_coordinate)s,
                %(y_coordinate)s,
                %(object_id)s,
                %(owner_id)s,
                %(storm_type)s,
                %(display_name)s,
                %(target_level)s,
                %(raw_ai)s,
                %(seen_at)s,
                %(seen_at)s,
                %(expires_at)s,
                'seen'
            )
            ON CONFLICT (aid, kingdom_id, area_type, x_coordinate, y_coordinate) DO UPDATE SET
                object_id = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.object_id
                    ELSE EXCLUDED.object_id
                END,
                owner_id = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.owner_id
                    ELSE EXCLUDED.owner_id
                END,
                storm_type = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.storm_type
                    ELSE EXCLUDED.storm_type
                END,
                display_name = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.display_name
                    ELSE EXCLUDED.display_name
                END,
                target_level = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.target_level
                    ELSE EXCLUDED.target_level
                END,
                raw_ai = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.raw_ai
                    ELSE EXCLUDED.raw_ai
                END,
                last_seen_at = EXCLUDED.last_seen_at,
                expires_at = EXCLUDED.expires_at,
                status = CASE
                    WHEN storm_target.sent_at IS NOT NULL THEN storm_target.status
                    WHEN storm_target.status LIKE 'cra_status_%%' THEN storm_target.status
                    WHEN COALESCE(storm_target.reserved_until, 0) > EXCLUDED.last_seen_at THEN storm_target.status
                    ELSE 'seen'
                END
            """,
            [
                {
                    **target,
                    "aid": aid,
                    "raw_ai": Jsonb(target["raw_ai"]),
                }
                for target in targets
            ],
        )
        changed = cur.rowcount
    conn.commit()
    return int(changed)


def reserve_storm_target(
    conn: psycopg.Connection,
    *,
    target_levels: tuple[int, ...] = (),
    reserve_seconds: int = 12 * 60,
    fresh_seconds: int | None = None,
) -> dict[str, Any] | None:
    epoch = now_epoch()
    aid = current_aid()
    level_filter = bool(target_levels)
    fresh_cutoff = 0 if fresh_seconds is None else epoch - int(fresh_seconds)
    with conn.transaction():
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT
                    id,
                    kingdom_id,
                    area_type,
                    x_coordinate,
                    y_coordinate,
                    object_id,
                    owner_id,
                    storm_type,
                    target_level,
                    raw_ai
                FROM storm_target
                WHERE aid = %s
                  AND kingdom_id = %s
                  AND expires_at > %s
                  AND last_seen_at >= %s
                  AND COALESCE(sent_at, 0) <= %s
                  AND COALESCE(reserved_until, 0) <= %s
                  AND status = 'seen'
                  AND (%s = FALSE OR target_level = ANY(%s))
                ORDER BY last_seen_at DESC, random()
                LIMIT 1
                FOR UPDATE SKIP LOCKED
                """,
                (
                    aid,
                    STORM_KID,
                    epoch,
                    fresh_cutoff,
                    epoch - STORM_TTL_SECONDS,
                    epoch,
                    level_filter,
                    list(target_levels),
                ),
            )
            row = cur.fetchone()
            if row is None:
                return None
            cur.execute(
                """
                UPDATE storm_target
                SET reserved_until = %s, status = 'reserved'
                WHERE aid = %s AND id = %s
                """,
                (epoch + int(reserve_seconds), aid, row[0]),
            )
    return {
        "id": int(row[0]),
        "kingdom_id": int(row[1]),
        "area_type": int(row[2]),
        "x": int(row[3]),
        "y": int(row[4]),
        "object_id": int(row[5]) if row[5] is not None else None,
        "owner_id": int(row[6]) if row[6] is not None else None,
        "storm_type": int(row[7]) if row[7] is not None else None,
        "target_level": int(row[8]) if row[8] is not None else None,
        "raw_ai": row[9],
    }


def mark_storm_sent(
    conn: psycopg.Connection,
    target_id: int,
    *,
    lid: int,
    march_id: int | None = None,
    attack_name: str | None = None,
    sent_at: int | None = None,
) -> None:
    epoch = now_epoch() if sent_at is None else int(sent_at)
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE storm_target
            SET sent_at = %s,
                march_id = %s,
                attack_name = %s,
                status = 'sent',
                reserved_until = NULL
            WHERE aid = %s
              AND id = %s
            """,
            (epoch, march_id, attack_name, aid, int(target_id)),
        )
    conn.commit()


def release_storm_target(conn: psycopg.Connection, target_id: int, *, status: str = "seen") -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE storm_target
            SET reserved_until = NULL,
                status = %s
            WHERE aid = %s
              AND id = %s
              AND sent_at IS NULL
            """,
            (status, aid, int(target_id)),
        )
    conn.commit()


def record_storm_attack(
    conn: psycopg.Connection,
    target: dict[str, Any],
    *,
    march_id: int,
    sent_at: int,
    travel_seconds: int | None,
    return_seconds: int | None,
    troop_count: int | None,
    task_name: str | None,
    lid: int | None,
) -> None:
    target_id = int(target["id"])
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO attack (
                aid,
                kingdom_id,
                x_coordinate,
                y_coordinate,
                march_id,
                time_created,
                troop_count,
                duration,
                return_duration,
                target_kind,
                target_id,
                task_name,
                target_level,
                lord_id,
                commander_number,
                status
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, 'storm_target', %s, %s, %s, %s, %s, 'sent')
            ON CONFLICT (aid, march_id) DO UPDATE SET
                kingdom_id = EXCLUDED.kingdom_id,
                x_coordinate = EXCLUDED.x_coordinate,
                y_coordinate = EXCLUDED.y_coordinate,
                time_created = EXCLUDED.time_created,
                troop_count = EXCLUDED.troop_count,
                duration = EXCLUDED.duration,
                return_duration = EXCLUDED.return_duration,
                target_kind = EXCLUDED.target_kind,
                target_id = EXCLUDED.target_id,
                task_name = EXCLUDED.task_name,
                target_level = EXCLUDED.target_level,
                lord_id = EXCLUDED.lord_id,
                commander_number = EXCLUDED.commander_number,
                status = EXCLUDED.status
            """,
            (
                aid,
                int(target["kingdom_id"]),
                int(target["x"]),
                int(target["y"]),
                int(march_id),
                int(sent_at),
                troop_count,
                travel_seconds,
                return_seconds,
                target_id,
                task_name,
                int(target["target_level"]) if target.get("target_level") is not None else None,
                int(lid) if lid is not None else None,
                commander_human_number(lid),
            ),
        )
        cur.execute(
            """
            UPDATE storm_target
            SET sent_at = %s,
                march_id = %s,
                attack_name = %s,
                status = 'sent',
                reserved_until = NULL
            WHERE aid = %s
              AND id = %s
            """,
            (int(sent_at), int(march_id), task_name, aid, target_id),
        )
    conn.commit()


def mark_storm_result(
    conn: psycopg.Connection,
    *,
    march_id: int,
    result_flag: int | None,
    return_seconds: int | None,
    coin_loot: int | None,
    ruby_loot: int | None,
    raw_result: dict[str, Any],
    result_at: int | None = None,
) -> bool:
    epoch = now_epoch() if result_at is None else int(result_at)
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE attack
            SET status = 'returning',
                result_flag = %s,
                landed_at = %s,
                result_received_at = %s,
                return_duration = COALESCE(%s, return_duration),
                coin_loot = COALESCE(%s, coin_loot),
                ruby_loot = COALESCE(%s, ruby_loot),
                raw_result = %s
            WHERE aid = %s
              AND march_id = %s
            RETURNING target_kind, target_id
            """,
            (
                result_flag,
                epoch,
                epoch,
                return_seconds,
                coin_loot,
                ruby_loot,
                Jsonb(raw_result),
                aid,
                int(march_id),
            ),
        )
        row = cur.fetchone()
        if row is None:
            conn.commit()
            return False
        target_kind, target_id = row
        if target_kind == "storm_target" and target_id is not None:
            cur.execute(
                """
                UPDATE storm_target
                SET status = 'returning',
                    result_flag = %s,
                    result_at = %s,
                    raw_result = %s
                WHERE aid = %s
                  AND id = %s
                """,
                (result_flag, epoch, Jsonb(raw_result), aid, int(target_id)),
            )
    conn.commit()
    return True
