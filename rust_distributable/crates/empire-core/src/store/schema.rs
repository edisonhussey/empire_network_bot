//! Forward-only schema migrations.
//!
//! The applied version lives in `schema_version`. A database created before that
//! table existed reads as version 0, which is why every migration must be safe
//! to run against an already-populated database: either `IF NOT EXISTS`, or an
//! `ALTER TABLE` that adds a column the previous schema could not have had.
//!
//! Adding a migration means appending a new `&[&str]` to [`MIGRATIONS`] and
//! bumping [`SCHEMA_VERSION`]. Never edit a released migration in place.

use sqlx::{Row, SqlitePool};

/// Highest migration index. Must equal `MIGRATIONS.len()`.
pub const SCHEMA_VERSION: i64 = 18;

/// One migration: the statements to run, in order.
pub type Migration = &'static [&'static str];

/// Schema as it shipped before the Python port began.
pub(super) const V1: Migration = &[
    "CREATE TABLE IF NOT EXISTS network_message (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,
        observed_at_ms INTEGER NOT NULL,
        direction TEXT NOT NULL,
        command TEXT,
        payload_json TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS app_state (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS licence_state (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        token TEXT NOT NULL,
        license_id TEXT NOT NULL,
        subject TEXT NOT NULL,
        revision INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        highest_seen_at INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS account_profile (
        account_id TEXT PRIMARY KEY,
        player_name TEXT NOT NULL,
        endpoint TEXT NOT NULL,
        server_header TEXT NOT NULL,
        initialized_at_ms INTEGER,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS owned_castle (
        account_id TEXT NOT NULL,
        castle_id INTEGER NOT NULL,
        kingdom_id INTEGER NOT NULL,
        area_type INTEGER NOT NULL,
        x INTEGER NOT NULL,
        y INTEGER NOT NULL,
        name TEXT NOT NULL,
        observed_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, castle_id),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS account_commander (
        account_id TEXT NOT NULL,
        ordinal INTEGER NOT NULL,
        lord_id INTEGER NOT NULL,
        observed_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, ordinal),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS rbc_target (
        account_id TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        x INTEGER NOT NULL,
        y INTEGER NOT NULL,
        level INTEGER,
        observed_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, kingdom_id, x, y),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
];

/// Tables and columns added for the Rust automation core.
///
/// Everything here is kingdom-agnostic on purpose: `kingdom_id` is a column, not
/// a table name, so a new event kingdom needs no schema change.
const V2: Migration = &[
    // Reservation bookkeeping on the target inventory.
    "ALTER TABLE rbc_target ADD COLUMN reserved_until_ms INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE rbc_target ADD COLUMN last_attacked_ms INTEGER NOT NULL DEFAULT 0",
    "CREATE INDEX IF NOT EXISTS rbc_target_pick_idx
        ON rbc_target (account_id, kingdom_id, level, reserved_until_ms)",
    // User-authored attack payloads. `payload_json` is the exact `A` array that
    // goes on the wire, so what is stored is what is sent.
    "CREATE TABLE IF NOT EXISTS attack_profile (
        profile_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        notes TEXT NOT NULL DEFAULT '',
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    // A task is a profile plus targeting rules. `kind` selects which target
    // source resolves it ('attack' today, 'recruit' later).
    "CREATE TABLE IF NOT EXISTS task_definition (
        task_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        profile_id TEXT,
        target_level_min INTEGER,
        target_level_max INTEGER,
        commander_count INTEGER NOT NULL DEFAULT 1,
        max_active INTEGER,
        priority INTEGER NOT NULL DEFAULT 100,
        enabled INTEGER NOT NULL DEFAULT 1,
        tags TEXT NOT NULL DEFAULT '',
        notes TEXT NOT NULL DEFAULT '',
        updated_at_ms INTEGER NOT NULL
    )",
    // What a task subscribes to. A subscription names a target *kind* and a
    // filter, never a hardcoded castle list, so "subscribe by kind" is data.
    "CREATE TABLE IF NOT EXISTS task_subscription (
        task_id TEXT NOT NULL,
        target_kind TEXT NOT NULL,
        filter_json TEXT NOT NULL DEFAULT '{}',
        position INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (task_id, target_kind, filter_json),
        FOREIGN KEY (task_id) REFERENCES task_definition(task_id) ON DELETE CASCADE
    )",
    // Commander availability, so return timers survive a restart.
    "CREATE TABLE IF NOT EXISTS commander_state (
        account_id TEXT NOT NULL,
        lord_id INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'available',
        available_after_ms INTEGER NOT NULL DEFAULT 0,
        march_id INTEGER,
        target_key TEXT,
        updated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, lord_id)
    )",
    "CREATE INDEX IF NOT EXISTS commander_state_ready_idx
        ON commander_state (account_id, available_after_ms)",
    // March ledger: what was sent, and what came back.
    "CREATE TABLE IF NOT EXISTS attack_ledger (
        account_id TEXT NOT NULL,
        march_id INTEGER NOT NULL,
        kingdom_id INTEGER NOT NULL,
        x INTEGER NOT NULL,
        y INTEGER NOT NULL,
        task_id TEXT,
        profile_id TEXT,
        level INTEGER,
        lord_id INTEGER,
        commander_number INTEGER,
        troop_count INTEGER,
        duration_s INTEGER,
        coin_loot INTEGER,
        ruby_loot INTEGER,
        status TEXT NOT NULL DEFAULT 'sent',
        result_flag INTEGER,
        error_message TEXT,
        sent_at_ms INTEGER NOT NULL,
        landed_at_ms INTEGER,
        result_at_ms INTEGER,
        PRIMARY KEY (account_id, march_id)
    )",
    "CREATE INDEX IF NOT EXISTS attack_ledger_recent_idx
        ON attack_ledger (account_id, sent_at_ms)",
    // Where the client currently is: map mode vs castle mode.
    "CREATE TABLE IF NOT EXISTS account_navigation (
        account_id TEXT PRIMARY KEY,
        current_kingdom_id INTEGER,
        current_castle_id INTEGER,
        map_mode INTEGER NOT NULL DEFAULT 0,
        recruit_page INTEGER NOT NULL DEFAULT 0,
        last_castle_switch_at_ms INTEGER NOT NULL DEFAULT 0,
        updated_at_ms INTEGER NOT NULL
    )",
    // Last observed troop/tool counts per castle.
    "CREATE TABLE IF NOT EXISTS account_castle_unit (
        account_id TEXT NOT NULL,
        castle_id INTEGER NOT NULL,
        unit_id INTEGER NOT NULL,
        quantity INTEGER NOT NULL,
        source_command TEXT,
        observed_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, castle_id, unit_id)
    )",
    // Per-castle recruitment queue timing.
    "CREATE TABLE IF NOT EXISTS recruit_castle_state (
        account_id TEXT NOT NULL,
        castle_id INTEGER NOT NULL,
        task_id TEXT,
        queue_clear_at_ms INTEGER NOT NULL DEFAULT 0,
        last_duration_s INTEGER NOT NULL DEFAULT 0,
        last_request_at_ms INTEGER NOT NULL DEFAULT 0,
        active_quantity INTEGER NOT NULL DEFAULT 0,
        queued_quantity INTEGER NOT NULL DEFAULT 0,
        help_active INTEGER NOT NULL DEFAULT 0,
        last_status TEXT NOT NULL DEFAULT 'idle',
        updated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, castle_id)
    )",
];

/// User-facing plan composition. Attack payloads and task definitions remain
/// in V2; this migration adds concrete route data and ordered reusable modes.
const V3: Migration = &[
    "CREATE UNIQUE INDEX IF NOT EXISTS attack_profile_name_idx
        ON attack_profile (name COLLATE NOCASE)",
    "CREATE UNIQUE INDEX IF NOT EXISTS task_definition_name_idx
        ON task_definition (name COLLATE NOCASE)",
    "CREATE TABLE IF NOT EXISTS task_runtime (
        task_id TEXT PRIMARY KEY,
        source_kingdom_id INTEGER NOT NULL,
        source_x INTEGER NOT NULL,
        source_y INTEGER NOT NULL,
        target_kingdom_id INTEGER NOT NULL,
        target_x INTEGER NOT NULL,
        target_y INTEGER NOT NULL,
        travel_mode TEXT NOT NULL DEFAULT 'coin',
        hbw INTEGER NOT NULL DEFAULT 1007,
        FOREIGN KEY (task_id) REFERENCES task_definition(task_id) ON DELETE CASCADE
    )",
    "CREATE TABLE IF NOT EXISTS automation_mode (
        mode_id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS automation_mode_task (
        mode_id INTEGER NOT NULL,
        task_id TEXT NOT NULL,
        position INTEGER NOT NULL,
        PRIMARY KEY (mode_id, position),
        UNIQUE (mode_id, task_id),
        FOREIGN KEY (mode_id) REFERENCES automation_mode(mode_id) ON DELETE CASCADE,
        FOREIGN KEY (task_id) REFERENCES task_definition(task_id) ON DELETE RESTRICT
    )",
    "CREATE TABLE IF NOT EXISTS account_mode (
        account_id TEXT PRIMARY KEY,
        mode_id INTEGER NOT NULL,
        running INTEGER NOT NULL DEFAULT 0,
        updated_at_ms INTEGER NOT NULL,
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE,
        FOREIGN KEY (mode_id) REFERENCES automation_mode(mode_id) ON DELETE CASCADE
    )",
];

/// Dynamic farming sources and durable map-discovery coverage.
const V4: Migration = &[
    "ALTER TABLE task_runtime ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'coordinate'",
    "CREATE TABLE IF NOT EXISTS map_scan_window (
        account_id TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        ax1 INTEGER NOT NULL,
        ay1 INTEGER NOT NULL,
        ax2 INTEGER NOT NULL,
        ay2 INTEGER NOT NULL,
        scanned_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, kingdom_id, ax1, ay1),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS map_scan_window_health_idx
        ON map_scan_window (account_id, kingdom_id, scanned_at_ms)",
];

/// Commander capacity belongs to a mode allocation, not to the reusable task.
const V5: Migration = &[
    "ALTER TABLE automation_mode_task ADD COLUMN commander_count INTEGER NOT NULL DEFAULT 1",
    "UPDATE automation_mode_task
     SET commander_count = COALESCE(
        (SELECT commander_count FROM task_definition t
         WHERE t.task_id = automation_mode_task.task_id), 1
     )",
];

/// Reusable recruitment templates and castle-independent recruit bots.
const V6: Migration = &[
    "CREATE TABLE IF NOT EXISTS recruitment_template (
        recruitment_id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        troop_id INTEGER NOT NULL,
        quantity INTEGER NOT NULL CHECK (quantity > 0),
        slot_count INTEGER NOT NULL CHECK (slot_count BETWEEN 1 AND 5),
        ask_alliance_help INTEGER NOT NULL DEFAULT 0,
        lane_id INTEGER NOT NULL DEFAULT 0,
        skill_id INTEGER NOT NULL DEFAULT 73,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS recruit_bot (
        recruit_bot_id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        algorithm TEXT NOT NULL CHECK (algorithm IN ('greedy', 'sporadic', 'advanced')),
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS recruit_bot_castle (
        recruit_bot_id INTEGER NOT NULL,
        account_id TEXT NOT NULL,
        castle_id INTEGER NOT NULL,
        recruitment_id TEXT NOT NULL,
        position INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (recruit_bot_id, castle_id),
        FOREIGN KEY (recruit_bot_id) REFERENCES recruit_bot(recruit_bot_id) ON DELETE CASCADE,
        FOREIGN KEY (recruitment_id) REFERENCES recruitment_template(recruitment_id) ON DELETE RESTRICT
    )",
    "CREATE TABLE IF NOT EXISTS account_recruit_bot (
        account_id TEXT PRIMARY KEY,
        recruit_bot_id INTEGER NOT NULL,
        running INTEGER NOT NULL DEFAULT 0,
        updated_at_ms INTEGER NOT NULL,
        FOREIGN KEY (recruit_bot_id) REFERENCES recruit_bot(recruit_bot_id) ON DELETE CASCADE
    )",
];

/// Canonicalise the account identity.
///
/// The account id is an identity, but it was written with whatever casing the
/// caller happened to hold: the desktop session lowercases it, the command line
/// hunter used the name as typed. Reads compare with `lower(account_id)`, so the
/// two copies collapsed into one account with every castle, commander and target
/// listed twice.
///
/// The profile is repaired first because children carry a foreign key to it.
/// Where a lowercase row already describes the same thing, the duplicate is
/// dropped; anything else is relabelled, so ledger history and learned targets
/// survive.
pub const V7: Migration = &[
    "DELETE FROM account_profile WHERE account_id <> lower(account_id)
       AND EXISTS (SELECT 1 FROM account_profile x
                    WHERE x.account_id = lower(account_profile.account_id))",
    "UPDATE account_profile SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "DELETE FROM owned_castle WHERE account_id <> lower(account_id)
       AND EXISTS (SELECT 1 FROM owned_castle x
                    WHERE x.account_id = lower(owned_castle.account_id)
                      AND x.castle_id = owned_castle.castle_id)",
    "DELETE FROM account_commander WHERE account_id <> lower(account_id)
       AND EXISTS (SELECT 1 FROM account_commander x
                    WHERE x.account_id = lower(account_commander.account_id)
                      AND x.lord_id = account_commander.lord_id)",
    "UPDATE owned_castle SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE account_commander SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE rbc_target SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)
         AND NOT EXISTS (SELECT 1 FROM rbc_target x
                          WHERE x.account_id = lower(rbc_target.account_id)
                            AND x.kingdom_id = rbc_target.kingdom_id
                            AND x.x = rbc_target.x AND x.y = rbc_target.y)",
    "DELETE FROM rbc_target WHERE account_id <> lower(account_id)",
    "UPDATE attack_ledger SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE commander_state SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE account_navigation SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE account_castle_unit SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE recruit_castle_state SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE account_mode SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE map_scan_window SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE recruit_bot_castle SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
    "UPDATE account_recruit_bot SET account_id = lower(account_id)
       WHERE account_id <> lower(account_id)",
];

/// Removes rows whose parent is gone.
///
/// The schema declares `ON DELETE CASCADE` where a child cannot outlive its
/// parent, but a foreign key is only enforced by the connection that runs the
/// statement. A connection that was opened without the pragma turned a cascade
/// into a no-op: the parent went, the children stayed, and because every read
/// joins through the parent they became invisible rather than obviously broken.
///
/// This sweeps that debris. Only bookkeeping that is derived from a parent is
/// removed - which is exactly why it is unreachable. Observations the account
/// cannot re-derive are deliberately left alone: `rbc_target` is learned map
/// knowledge and `owned_castle` is the map itself, so an orphaned row there is
/// kept on the chance that a later bootstrap re-adopts it.
pub const V8: Migration = &[
    "DELETE FROM recruit_bot_castle WHERE recruit_bot_id NOT IN (SELECT recruit_bot_id FROM recruit_bot)",
    "DELETE FROM recruit_bot_castle WHERE recruitment_id NOT IN (SELECT recruitment_id FROM recruitment_template)",
    "DELETE FROM account_recruit_bot WHERE recruit_bot_id NOT IN (SELECT recruit_bot_id FROM recruit_bot)",
    "DELETE FROM task_subscription WHERE task_id NOT IN (SELECT task_id FROM task_definition)",
    "DELETE FROM task_runtime WHERE task_id NOT IN (SELECT task_id FROM task_definition)",
    "DELETE FROM automation_mode_task WHERE mode_id NOT IN (SELECT mode_id FROM automation_mode)",
    "DELETE FROM automation_mode_task WHERE task_id NOT IN (SELECT task_id FROM task_definition)",
    "DELETE FROM account_mode WHERE mode_id NOT IN (SELECT mode_id FROM automation_mode)",
];

/// Per-castle horse/travel unlocks learned from the authoritative GBD payload.
/// They cannot be a mode/task constant because each castle may have a different
/// stable level, even inside the same account.
pub const V9: Migration = &[
    "CREATE TABLE IF NOT EXISTS account_castle_travel (
        account_id TEXT NOT NULL,
        castle_id INTEGER NOT NULL,
        kingdom_id INTEGER NOT NULL,
        unlocked_hbw_json TEXT NOT NULL,
        coin_hbw INTEGER NOT NULL,
        observed_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, castle_id),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS idx_castle_travel_kingdom
       ON account_castle_travel(account_id, kingdom_id)",
];

/// A stage-0 licence has no player id. This immutable record is created only
/// after an authenticated GBD matches its signed server and bootstrap castle.
pub const V10: Migration = &["CREATE TABLE IF NOT EXISTS licence_activation (
        license_id TEXT PRIMARY KEY,
        server TEXT NOT NULL,
        player_id INTEGER NOT NULL,
        activated_at INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )"];

/// Fortress knowledge remains distinct from RBC knowledge because the two map
/// objects have different wire types and cooldown behavior.
pub const V11: Migration = &[
    "CREATE TABLE IF NOT EXISTS fortress_target (
        account_id TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        x INTEGER NOT NULL,
        y INTEGER NOT NULL,
        level INTEGER NOT NULL,
        cooldown_remaining_s INTEGER NOT NULL,
        available_at_ms INTEGER NOT NULL,
        occupier_player_id INTEGER NOT NULL,
        reserved_until_ms INTEGER NOT NULL DEFAULT 0,
        refresh_due_ms INTEGER NOT NULL DEFAULT 0,
        last_attacked_ms INTEGER NOT NULL DEFAULT 0,
        observed_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, kingdom_id, x, y),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS fortress_target_ready_idx
        ON fortress_target (account_id, kingdom_id, available_at_ms, reserved_until_ms)",
    "CREATE INDEX IF NOT EXISTS fortress_target_refresh_idx
        ON fortress_target (account_id, refresh_due_ms)",
    // Legacy fortress work queue, superseded by `fortress_scan_state` (V15).
    // Kept because this list is applied on every launch and V13/V14 reference
    // it, so removing it would break a fresh database. Nothing reads it.
    "CREATE TABLE IF NOT EXISTS fortress_scan_frontier (
        account_id TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        center_x INTEGER NOT NULL,
        center_y INTEGER NOT NULL,
        claimed_until_ms INTEGER NOT NULL DEFAULT 0,
        completed_at_ms INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (account_id, kingdom_id, center_x, center_y),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS fortress_scan_frontier_pending_idx
        ON fortress_scan_frontier (account_id, kingdom_id, completed_at_ms, claimed_until_ms)",
    "CREATE TABLE IF NOT EXISTS fortress_scan_state (
        account_id TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        lattice_offset INTEGER NOT NULL,
        left_bound INTEGER NOT NULL,
        top_bound INTEGER NOT NULL,
        right_bound INTEGER NOT NULL,
        bottom_bound INTEGER NOT NULL,
        next_x INTEGER NOT NULL,
        next_y INTEGER NOT NULL,
        blocks_total INTEGER NOT NULL,
        blocks_done INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (account_id, kingdom_id, lattice_offset),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )",
];

/// A fortress is attackable only inside a one-minute window, so fortress work
/// must never queue behind routine farming. New tasks are promoted as they are
/// written; this repairs the rows that predate that rule. `10` is the
/// `Priority::ExtraHigh` scheduler band, spelled out because a released
/// migration must not depend on code that can still change.
pub const V12: Migration = &["UPDATE task_definition SET priority = 10
     WHERE priority > 10 AND task_id IN (
        SELECT task_id FROM task_subscription WHERE target_kind = 'fortress'
     )"];

/// Discard discovery windows that are not on the fortress lattice.
///
/// The first discovery implementation stepped a flat ±17, which lands between
/// slots, so those rows can never find anything and only consume requests.
/// Fortresses sit where `x ≡ y (mod 39)` and `x mod 39 ∈ {9, 29}`; anything else
/// is provably empty ground.
pub const V13: Migration = &["DELETE FROM fortress_scan_frontier
     WHERE (center_x % 39) <> (center_y % 39)
        OR (center_x % 39) NOT IN (9, 29)"];

pub const V14: Migration = &["DELETE FROM fortress_scan_frontier"];

/// Fortress discovery stops being a queue of one row per coordinate and becomes
/// one cursor row per (kingdom, grid): the bounds of the sweep and where it has
/// reached.
///
/// `fortress_scan_frontier` is left in place rather than dropped. The
/// declarative list above re-creates every table it names on each launch, so a
/// drop here would only resurrect an empty table on the next start, and V13/V14
/// still reference it, so it has to exist for a fresh database. Nothing reads
/// it any more; the fortresses it found are in `fortress_target`.
pub const V15: Migration = &["CREATE TABLE IF NOT EXISTS fortress_scan_state (
        account_id TEXT NOT NULL,
        kingdom_id INTEGER NOT NULL,
        lattice_offset INTEGER NOT NULL,
        left_bound INTEGER NOT NULL,
        top_bound INTEGER NOT NULL,
        right_bound INTEGER NOT NULL,
        bottom_bound INTEGER NOT NULL,
        next_x INTEGER NOT NULL,
        next_y INTEGER NOT NULL,
        blocks_total INTEGER NOT NULL,
        blocks_done INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (account_id, kingdom_id, lattice_offset),
        FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
    )"];

/// Fortress discovery stops being two sweeps per kingdom and becomes one.
///
/// A window now spans both residue families, so one request answers eight slots
/// and a cursor no longer means the position it used to: a 2×2 block on a single
/// family is not a sub-rectangle of the new window grid, so resuming from an old
/// cursor would silently skip ground. Clearing the rows re-walks each kingdom
/// once — 289 requests at the new tempo — and is the only correct option, since
/// the old positions cannot be translated.
pub const V16: Migration = &["DELETE FROM fortress_scan_state"];

/// Bounds are measured per kingdom now, from the block over that kingdom's own
/// castle, rather than filled in from one hard-coded map rectangle. Every row
/// written before V17 therefore holds the wrong rectangle, and `blocks_total`
/// counts the wrong number of blocks, so a stored cursor cannot be resumed
/// against them. The rows are dropped and each kingdom is measured again on its
/// next walk: a few probes per kingdom, once.
pub const V17: Migration = &["DELETE FROM fortress_scan_state"];

/// The server's own word on when a tower can be hit again.
///
/// Every RBC row in a map response carries its remaining cooldown, and the bot
/// used to throw that away and learn about cooldowns by being refused (status
/// 95). `server_free_at_ms` is the last server-reported free time; 0 means the
/// server last reported the tower as free.
pub const V18: Migration = &[
    "ALTER TABLE rbc_target ADD COLUMN server_free_at_ms INTEGER NOT NULL DEFAULT 0",
];

pub const MIGRATIONS: &[Migration] = &[
    V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18,
];

/// Every table that holds user data, for the storage report and full wipe.
/// Order matters for deletion: children before parents.
pub const DATA_TABLES: &[&str] = &[
    "network_message",
    "app_state",
    "licence_state",
    "licence_activation",
    "attack_ledger",
    "commander_state",
    "task_subscription",
    "account_recruit_bot",
    "recruit_bot_castle",
    "recruit_bot",
    "recruitment_template",
    "account_mode",
    "automation_mode_task",
    "automation_mode",
    "task_runtime",
    "task_definition",
    "attack_profile",
    "account_castle_unit",
    "account_castle_travel",
    "recruit_castle_state",
    "account_navigation",
    "map_scan_window",
    "fortress_scan_state",
    "fortress_target",
    "rbc_target",
    "account_commander",
    "owned_castle",
    "account_profile",
];

/// Apply every migration newer than the recorded version.
pub async fn apply(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_version (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            version INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let recorded = sqlx::query("SELECT version FROM schema_version WHERE singleton = 1")
        .fetch_optional(pool)
        .await?
        .map(|row| row.get::<i64, _>("version"))
        .unwrap_or(0);

    for (index, migration) in MIGRATIONS.iter().enumerate() {
        let version = index as i64 + 1;
        if version <= recorded {
            continue;
        }
        for statement in migration.iter() {
            if let Err(error) = sqlx::query(statement).execute(pool).await {
                // SQLite has no `ADD COLUMN IF NOT EXISTS`. A column that is
                // already there means this statement already ran (a rewound
                // version, or a restore), which is the state it aims for.
                let already_applied = statement
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("ALTER TABLE")
                    && error.to_string().contains("duplicate column name");
                if !already_applied {
                    return Err(error);
                }
            }
        }
        sqlx::query(
            "INSERT INTO schema_version (singleton, version) VALUES (1, ?)
             ON CONFLICT(singleton) DO UPDATE SET version = excluded.version",
        )
        .bind(version)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// The version currently applied to this database.
///
/// A database created before versioning existed has no `schema_version` table
/// and reports 0, which is exactly what [`apply`] needs in order to upgrade it.
pub async fn version(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let present: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
    )
    .fetch_optional(pool)
    .await?;
    if present.is_none() {
        return Ok(0);
    }
    let row = sqlx::query("SELECT version FROM schema_version WHERE singleton = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| row.get::<i64, _>("version")).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_matches_the_migration_count() {
        assert_eq!(
            SCHEMA_VERSION,
            MIGRATIONS.len() as i64,
            "SCHEMA_VERSION must be bumped with every new migration"
        );
    }

    #[test]
    fn every_data_table_is_created_by_a_migration() {
        let all_sql = MIGRATIONS
            .iter()
            .flat_map(|migration| migration.iter().copied())
            .collect::<Vec<&str>>()
            .join("\n");
        for table in DATA_TABLES {
            assert!(
                all_sql.contains(&format!("CREATE TABLE IF NOT EXISTS {table}"))
                    || all_sql.contains(&format!("CREATE TABLE {table}")),
                "DATA_TABLES lists {table} but no migration creates it"
            );
        }
    }
}
