"""Account-scoped persistence for learned castles and recruitment runtime state."""

from __future__ import annotations

from dataclasses import dataclass
import time
from typing import Mapping, Sequence

import psycopg

from bot.utility.recruit.recruit import (
    CastleDirectory,
    CastleLocation,
    NavigationState,
    OwnedCastle,
    RecruitReceipt,
    ResolvedRecruitment,
)


TABLES = (
    "account_castle",
    "account_navigation",
    "account_castle_unit",
    "recruit_castle_state",
)


@dataclass(frozen=True)
class PersistedNavigation:
    state: NavigationState
    last_castle_switch_at: float | None
    updated_at: float
    source_command: str


@dataclass(frozen=True)
class ScheduledRecruitment:
    resolved: ResolvedRecruitment
    queue_clear_at: float
    last_status: str

    def ready(self, now: float | None = None) -> bool:
        current = time.time() if now is None else float(now)
        return self.queue_clear_at <= current


def ensure_recruit_tables(conn: psycopg.Connection) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS account_castle (
                aid TEXT NOT NULL,
                castle_id BIGINT NOT NULL,
                kingdom_id INTEGER NOT NULL,
                area_type INTEGER,
                x_coordinate INTEGER,
                y_coordinate INTEGER,
                name TEXT NOT NULL DEFAULT '',
                is_green_main BOOLEAN NOT NULL DEFAULT FALSE,
                active BOOLEAN NOT NULL DEFAULT TRUE,
                source_command TEXT NOT NULL,
                first_seen_at DOUBLE PRECISION NOT NULL,
                last_seen_at DOUBLE PRECISION NOT NULL,
                PRIMARY KEY (aid, castle_id)
            )
            """
        )
        cur.execute(
            """
            CREATE INDEX IF NOT EXISTS account_castle_aid_kingdom_idx
            ON account_castle (aid, kingdom_id, active)
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS account_navigation (
                aid TEXT PRIMARY KEY,
                current_kingdom INTEGER,
                current_castle_id BIGINT,
                map_mode BOOLEAN NOT NULL,
                recruit_page BOOLEAN NOT NULL,
                last_castle_switch_at DOUBLE PRECISION,
                source_command TEXT NOT NULL,
                updated_at DOUBLE PRECISION NOT NULL,
                CHECK (NOT recruit_page OR NOT map_mode),
                CHECK (NOT recruit_page OR current_castle_id IS NOT NULL)
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS account_castle_unit (
                aid TEXT NOT NULL,
                castle_id BIGINT NOT NULL,
                unit_id INTEGER NOT NULL,
                quantity INTEGER NOT NULL,
                source_command TEXT NOT NULL,
                observed_at DOUBLE PRECISION NOT NULL,
                PRIMARY KEY (aid, castle_id, unit_id)
            )
            """
        )
        cur.execute(
            """
            CREATE INDEX IF NOT EXISTS account_castle_unit_lookup_idx
            ON account_castle_unit (aid, castle_id)
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS recruit_castle_state (
                aid TEXT NOT NULL,
                castle_id BIGINT NOT NULL,
                task_id TEXT NOT NULL,
                queue_clear_at DOUBLE PRECISION NOT NULL DEFAULT 0,
                last_duration_seconds INTEGER NOT NULL DEFAULT 0,
                last_request_at DOUBLE PRECISION NOT NULL DEFAULT 0,
                active_quantity INTEGER NOT NULL DEFAULT 0,
                queued_quantity INTEGER NOT NULL DEFAULT 0,
                help_active BOOLEAN NOT NULL DEFAULT FALSE,
                last_status TEXT NOT NULL DEFAULT 'idle',
                updated_at DOUBLE PRECISION NOT NULL,
                PRIMARY KEY (aid, castle_id)
            )
            """
        )
    conn.commit()


def replace_castle_inventory(
    conn: psycopg.Connection,
    aid: str,
    castle_id: int,
    inventory: Mapping[int, int],
    *,
    source_command: str = "gui",
    observed_at: float | None = None,
) -> int:
    """Replace one castle's authoritative ``gui.I`` unit/tool snapshot."""

    observed = time.time() if observed_at is None else float(observed_at)
    account = str(aid)
    castle = int(castle_id)
    with conn.cursor() as cur:
        cur.execute(
            "DELETE FROM account_castle_unit WHERE aid = %s AND castle_id = %s",
            (account, castle),
        )
        for unit_id, quantity in inventory.items():
            cur.execute(
                """
                INSERT INTO account_castle_unit
                    (aid, castle_id, unit_id, quantity, source_command, observed_at)
                VALUES (%s, %s, %s, %s, %s, %s)
                """,
                (account, castle, int(unit_id), int(quantity), source_command, observed),
            )
    conn.commit()
    return len(inventory)


def read_castle_inventory(
    conn: psycopg.Connection,
    aid: str,
    castle_id: int,
) -> dict[int, int]:
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT unit_id, quantity
            FROM account_castle_unit
            WHERE aid = %s AND castle_id = %s
            ORDER BY unit_id
            """,
            (str(aid), int(castle_id)),
        )
        return {int(unit_id): int(quantity) for unit_id, quantity in cur.fetchall()}


def upsert_owned_castles(
    conn: psycopg.Connection,
    aid: str,
    castles: Mapping[int, OwnedCastle],
    *,
    source_command: str,
    observed_at: float | None = None,
    authoritative: bool = False,
) -> int:
    """Persist learned castles; a full GBD/GCL inventory may retire stale rows."""

    observed = time.time() if observed_at is None else float(observed_at)
    account = str(aid)
    with conn.cursor() as cur:
        if authoritative:
            cur.execute("UPDATE account_castle SET active = FALSE WHERE aid = %s", (account,))
        for castle in castles.values():
            location = castle.location
            cur.execute(
                """
                INSERT INTO account_castle
                    (aid, castle_id, kingdom_id, area_type, x_coordinate, y_coordinate,
                     name, is_green_main, active, source_command, first_seen_at, last_seen_at)
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, TRUE, %s, %s, %s)
                ON CONFLICT (aid, castle_id) DO UPDATE SET
                    kingdom_id = EXCLUDED.kingdom_id,
                    area_type = COALESCE(EXCLUDED.area_type, account_castle.area_type),
                    x_coordinate = COALESCE(EXCLUDED.x_coordinate, account_castle.x_coordinate),
                    y_coordinate = COALESCE(EXCLUDED.y_coordinate, account_castle.y_coordinate),
                    name = CASE
                        WHEN EXCLUDED.name <> '' THEN EXCLUDED.name
                        ELSE account_castle.name
                    END,
                    is_green_main = EXCLUDED.is_green_main OR account_castle.is_green_main,
                    active = TRUE,
                    source_command = EXCLUDED.source_command,
                    last_seen_at = EXCLUDED.last_seen_at
                """,
                (
                    account,
                    location.castle_id,
                    location.kingdom_id,
                    castle.area_type,
                    castle.x,
                    castle.y,
                    castle.name,
                    castle.is_green_main,
                    source_command,
                    observed,
                    observed,
                ),
            )
    conn.commit()
    return len(castles)


def read_castle_directory(conn: psycopg.Connection, aid: str) -> CastleDirectory:
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT castle_id, kingdom_id
            FROM account_castle
            WHERE aid = %s AND active = TRUE
            ORDER BY kingdom_id, castle_id
            """,
            (str(aid),),
        )
        rows = cur.fetchall()
    return CastleDirectory(
        {
            int(castle_id): CastleLocation(int(kingdom_id), int(castle_id))
            for castle_id, kingdom_id in rows
        }
    )


def save_navigation(
    conn: psycopg.Connection,
    aid: str,
    state: NavigationState,
    *,
    source_command: str,
    observed_at: float | None = None,
    last_castle_switch_at: float | None = None,
) -> None:
    observed = time.time() if observed_at is None else float(observed_at)
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO account_navigation
                (aid, current_kingdom, current_castle_id, map_mode, recruit_page,
                 last_castle_switch_at, source_command, updated_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (aid) DO UPDATE SET
                current_kingdom = EXCLUDED.current_kingdom,
                current_castle_id = EXCLUDED.current_castle_id,
                map_mode = EXCLUDED.map_mode,
                recruit_page = EXCLUDED.recruit_page,
                last_castle_switch_at = COALESCE(
                    EXCLUDED.last_castle_switch_at,
                    account_navigation.last_castle_switch_at
                ),
                source_command = EXCLUDED.source_command,
                updated_at = EXCLUDED.updated_at
            """,
            (
                str(aid),
                state.current_kingdom,
                state.current_castle_id,
                state.map_mode,
                state.recruit_page,
                last_castle_switch_at,
                source_command,
                observed,
            ),
        )
    conn.commit()


def load_navigation(conn: psycopg.Connection, aid: str) -> PersistedNavigation | None:
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT current_kingdom, current_castle_id, map_mode, recruit_page,
                   last_castle_switch_at, updated_at, source_command
            FROM account_navigation
            WHERE aid = %s
            """,
            (str(aid),),
        )
        row = cur.fetchone()
    if row is None:
        return None
    return PersistedNavigation(
        state=NavigationState(
            current_kingdom=int(row[0]) if row[0] is not None else None,
            current_castle_id=int(row[1]) if row[1] is not None else None,
            map_mode=bool(row[2]),
            recruit_page=bool(row[3]),
        ),
        last_castle_switch_at=float(row[4]) if row[4] is not None else None,
        updated_at=float(row[5]),
        source_command=str(row[6]),
    )


def record_recruitment(
    conn: psycopg.Connection,
    aid: str,
    resolved: ResolvedRecruitment,
    receipt: RecruitReceipt,
    *,
    received_at: float | None = None,
) -> float:
    received = time.time() if received_at is None else float(received_at)
    event = resolved.task.event
    clear_at = float(
        receipt.queue_clear_at(
            received,
            buffer_seconds=event.ready_buffer_seconds,
        )
    )
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO recruit_castle_state
                (aid, castle_id, task_id, queue_clear_at, last_duration_seconds,
                 last_request_at, active_quantity, queued_quantity, help_active,
                 last_status, updated_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, 'queued', %s)
            ON CONFLICT (aid, castle_id) DO UPDATE SET
                task_id = EXCLUDED.task_id,
                queue_clear_at = EXCLUDED.queue_clear_at,
                last_duration_seconds = EXCLUDED.last_duration_seconds,
                last_request_at = EXCLUDED.last_request_at,
                active_quantity = EXCLUDED.active_quantity,
                queued_quantity = EXCLUDED.queued_quantity,
                help_active = EXCLUDED.help_active,
                last_status = EXCLUDED.last_status,
                updated_at = EXCLUDED.updated_at
            """,
            (
                str(aid),
                resolved.location.castle_id,
                resolved.task_id,
                clear_at,
                receipt.total_remaining_seconds,
                received,
                receipt.active_quantity,
                receipt.queued_quantity,
                receipt.help_active,
                received,
            ),
        )
    conn.commit()
    return clear_at


def sync_recruitment_schedule(
    conn: psycopg.Connection,
    aid: str,
    resolved: Sequence[ResolvedRecruitment],
    *,
    observed_at: float | None = None,
) -> int:
    """Ensure every subscribed castle has one account-scoped queue row."""

    observed = time.time() if observed_at is None else float(observed_at)
    with conn.cursor() as cur:
        for item in resolved:
            cur.execute(
                """
                INSERT INTO recruit_castle_state
                    (aid, castle_id, task_id, queue_clear_at, last_duration_seconds,
                     last_request_at, active_quantity, queued_quantity, help_active,
                     last_status, updated_at)
                VALUES (%s, %s, %s, 0, 0, 0, 0, 0, FALSE, 'idle', %s)
                ON CONFLICT (aid, castle_id) DO UPDATE SET
                    task_id = EXCLUDED.task_id
                """,
                (str(aid), item.location.castle_id, item.task_id, observed),
            )
    conn.commit()
    return len(resolved)


def recruitment_schedule(
    conn: psycopg.Connection,
    aid: str,
    resolved: Sequence[ResolvedRecruitment],
) -> tuple[ScheduledRecruitment, ...]:
    """Return subscribed castles ordered by the time their lane becomes free."""

    sync_recruitment_schedule(conn, aid, resolved)
    by_castle = {item.location.castle_id: item for item in resolved}
    if not by_castle:
        return ()
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT castle_id, queue_clear_at, last_status
            FROM recruit_castle_state
            WHERE aid = %s AND castle_id = ANY(%s)
            ORDER BY queue_clear_at, castle_id
            """,
            (str(aid), list(by_castle)),
        )
        rows = cur.fetchall()
    return tuple(
        ScheduledRecruitment(
            resolved=by_castle[int(castle_id)],
            queue_clear_at=float(queue_clear_at),
            last_status=str(last_status),
        )
        for castle_id, queue_clear_at, last_status in rows
    )


def next_recruitment(
    conn: psycopg.Connection,
    aid: str,
    resolved: Sequence[ResolvedRecruitment],
) -> ScheduledRecruitment | None:
    schedule = recruitment_schedule(conn, aid, resolved)
    return schedule[0] if schedule else None
