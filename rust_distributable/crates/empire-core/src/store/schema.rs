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
pub const SCHEMA_VERSION: i64 = 2;

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

pub const MIGRATIONS: &[Migration] = &[V1, V2];

/// Every table that holds user data, for the storage report and full wipe.
/// Order matters for deletion: children before parents.
pub const DATA_TABLES: &[&str] = &[
    "network_message",
    "app_state",
    "licence_state",
    "attack_ledger",
    "commander_state",
    "task_subscription",
    "task_definition",
    "attack_profile",
    "account_castle_unit",
    "recruit_castle_state",
    "account_navigation",
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
            sqlx::query(statement).execute(pool).await?;
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
