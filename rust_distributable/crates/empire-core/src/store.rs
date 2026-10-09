use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{
    RECENT_MESSAGE_LIMIT,
    account::{AttackTravel, CastleTravelOptions, FortressTarget, OwnedCastle, RbcTarget},
    event::Direction,
    planning::TravelMode,
};

mod config;
mod ledger;
mod modes;
mod recruitment;
mod schema;
mod state;
mod storage;
mod telemetry;

#[cfg(test)]
mod tests;

pub use config::{AttackProfile, SubscriptionRecord, TaskRecord};
pub use ledger::{
    COMMANDER_AVAILABLE, COMMANDER_OUTBOUND, CommanderState, DashboardPoint, DashboardSummary,
    HEARTBEAT_FRESH_MILLIS, HUNT_HEARTBEAT_KEY, HuntSummary, HuntTaskSummary, MARCH_RETURNING,
    MARCH_SENT, MarchRecord, ScanActivity,
};
pub use modes::{AccountModeRecord, ActiveModeTask, ImportModeError, ModeRecord, TaskRuntime};
pub use recruitment::{
    AccountRecruitBot, ActiveRecruitment, OwnedCastleRecord, RecruitBot, RecruitBotCastle,
    RecruitmentTemplate,
};
pub use schema::SCHEMA_VERSION;
pub use state::{CastleUnit, NavigationState, RecruitCastleState};
pub use storage::{PruneOutcome, StorageReport, TableFootprint};
pub use telemetry::{
    EVENT_RETENTION_MS, EventRecord, STOCK_SAMPLE_INTERVAL_MS, STOCK_SAMPLE_RETENTION_MS,
    StockForecast, StockSample,
};

/// The one true form of an account id.
///
/// An account id is an identity, not a label: it is the foreign key shared by
/// castles, commanders, targets and the attack ledger. Reads compare it with
/// `lower(account_id)`, so every write has to agree on one casing or the same
/// account appears twice. The player's own spelling is kept separately in
/// `player_name` for display.
pub fn canonical_account_id(account_id: &str) -> String {
    account_id.trim().to_ascii_lowercase()
}

/// Extract the on-disk path from a sqlx SQLite URL, when there is one.///
/// `sqlite::memory:` has no file, so the storage report and prune operations
/// degrade to row counts only.
fn database_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("sqlite://")?;
    let path = rest.split('?').next().unwrap_or(rest);
    if path.is_empty() || path == ":memory:" {
        return None;
    }
    Some(PathBuf::from(path))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub sequence: i64,
    pub observed_at_ms: i64,
    pub direction: Direction,
    pub command: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct StoredLicence {
    pub token: String,
    pub license_id: String,
    pub subject: String,
    pub revision: i64,
    pub expires_at: i64,
    pub highest_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenceActivation {
    pub license_id: String,
    pub server: String,
    pub player_id: i64,
    pub activated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account_id: String,
    pub player_name: String,
    pub endpoint: String,
    pub server_header: String,
    pub initialized_at_ms: Option<i64>,
    pub castle_count: i64,
    pub commander_count: i64,
    pub rbc_count: i64,
    pub kingdom_health: Vec<KingdomHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KingdomHealth {
    pub castle_id: i64,
    pub castle_name: String,
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    pub target_count: i64,
    pub scan_window_count: i64,
    pub last_scanned_at_ms: Option<i64>,
    /// Fortresses observed in this kingdom. Green never has any, so zero there
    /// is a fact rather than a sign that scanning has not happened yet.
    pub fortress_count: i64,
    /// Discovery windows in this kingdom that have not been probed yet.
    pub fortress_probes_pending: i64,
    /// Every discovery window in this kingdom, probed or not. One window is one
    /// `gaa` request, so this is the denominator of the walk's progress.
    pub fortress_probes_total: i64,
    /// Whether a fortress task is assigned here. Only these kingdoms are ever
    /// walked, so the UI must not promise fortress scanning anywhere else.
    pub fortress_task: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedTarget {
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    pub level: Option<i64>,
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    path: Option<PathBuf>,
}

pub const SCAN_REFRESH_BASE_MS: i64 = 12 * 60 * 60 * 1_000;

/// A fortress is never re-read until its last observation is at least this old.
///
/// Without a floor a read schedules the next read: the response re-observes the
/// row and clears `refresh_due_ms`, so the queue would never drain and the runner
/// would spend every cycle confirming the same cooldown.
const FORTRESS_RECHECK_MIN_MS: i64 = 5 * 60 * 1_000;

/// Confirm a cooldown once its window is this close.
///
/// The projected time is already authoritative — the server handed it to us — so
/// a read is only worth making to catch a fortress somebody else took, which
/// resets it to 24 hours.
const FORTRESS_CONFIRM_LEAD_MS: i64 = 10 * 60 * 1_000;

/// One background read for a fortress nobody has looked at in this long, so a
/// stale row cannot stay wrong forever.
const FORTRESS_SANITY_INTERVAL_MS: i64 = 6 * 60 * 60 * 1_000;

/// A stable per-window offset prevents every stored map window becoming due at
/// once after a restart while keeping tests and scheduling reproducible.
pub fn scan_refresh_after_ms(kingdom_id: i64, ax1: i64, ay1: i64) -> i64 {
    let mixed = kingdom_id
        .wrapping_mul(73_856_093)
        .wrapping_add(ax1.wrapping_mul(19_349_663))
        .wrapping_add(ay1.wrapping_mul(83_492_791));
    let jitter = mixed.rem_euclid(60 * 60 * 1_000) - 30 * 60 * 1_000;
    SCAN_REFRESH_BASE_MS + jitter
}

impl Store {
    pub async fn open(url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await?;
        let store = Self {
            pool,
            path: database_path(url),
        };
        store.migrate().await?;
        Ok(store)
    }

    /// Path of the database file, when it is file-backed rather than in-memory.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await?;
        schema::apply(&self.pool).await
    }

    pub async fn upsert_account_profile(
        &self,
        account_id: &str,
        player_name: &str,
        endpoint: &str,
        server_header: &str,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // The display name keeps its casing; the identity does not. Every read
        // compares with `lower(account_id)`, so storing the caller's casing here
        // would create a second copy of the same account.
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "INSERT INTO account_profile (
                account_id, player_name, endpoint, server_header, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                player_name = excluded.player_name,
                endpoint = excluded.endpoint,
                server_header = excluded.server_header,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&account_id)
        .bind(player_name)
        .bind(endpoint)
        .bind(server_header)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn replace_account_bootstrap(
        &self,
        account_id: &str,
        castles: &[OwnedCastle],
        commanders: &[i64],
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM owned_castle WHERE account_id = ?")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM account_commander WHERE account_id = ?")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        for castle in castles {
            sqlx::query(
                "INSERT INTO owned_castle (
                    account_id, castle_id, kingdom_id, area_type, x, y, name, observed_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(castle.castle_id)
            .bind(castle.kingdom_id)
            .bind(castle.area_type)
            .bind(castle.x)
            .bind(castle.y)
            .bind(&castle.name)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }
        for (ordinal, lord_id) in commanders.iter().enumerate() {
            sqlx::query(
                "INSERT INTO account_commander (account_id, ordinal, lord_id, observed_at_ms)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(i64::try_from(ordinal + 1).unwrap_or(i64::MAX))
            .bind(lord_id)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "UPDATE account_profile SET initialized_at_ms = ?, updated_at_ms = ?
             WHERE account_id = ?",
        )
        .bind(now_ms)
        .bind(now_ms)
        .bind(&account_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_rbc_targets(
        &self,
        account_id: &str,
        targets: &[RbcTarget],
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        // A 101x101 map response can contain hundreds of RBC rows. Inserting
        // each one as a separate SQLite statement held the socket task for
        // several seconds. Bounded multi-row UPSERTs preserve every row while
        // reducing that work to a handful of statements.
        for chunk in targets.chunks(150) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT INTO rbc_target (
                    account_id, kingdom_id, x, y, level, observed_at_ms, server_free_at_ms
                 ) ",
            );
            query.push_values(chunk, |mut row, target| {
                row.push_bind(&account_id)
                    .push_bind(target.kingdom_id)
                    .push_bind(target.x)
                    .push_bind(target.y)
                    .push_bind(target.level)
                    .push_bind(now_ms)
                    .push_bind(server_free_at_ms(target, now_ms));
            });
            query.push(
                " ON CONFLICT(account_id, kingdom_id, x, y) DO UPDATE SET
                    level = excluded.level,
                    observed_at_ms = excluded.observed_at_ms,
                    server_free_at_ms = excluded.server_free_at_ms",
            );
            query.build().execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Apply the cooldowns a map response reports for towers we already know.
    ///
    /// Unlike [`Self::upsert_rbc_targets`] this never adds a row, so any map
    /// response the client happens to receive (a tile read after a landing, a
    /// refresh after a refusal) can correct the cooldowns without enlarging the
    /// catalogue the operator chose to scan.
    pub async fn refresh_rbc_cooldowns(
        &self,
        account_id: &str,
        targets: &[RbcTarget],
        observed_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        if targets.is_empty() {
            return Ok(());
        }
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        for target in targets {
            sqlx::query(
                "UPDATE rbc_target SET server_free_at_ms = ?, observed_at_ms = ?
                 WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
            )
            .bind(server_free_at_ms(target, observed_at_ms))
            .bind(observed_at_ms)
            .bind(&account_id)
            .bind(target.kingdom_id)
            .bind(target.x)
            .bind(target.y)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// When the server last said this tower becomes hittable (0 = ready).
    pub async fn rbc_server_free_at(
        &self,
        account_id: &str,
        target: &ReservedTarget,
    ) -> Result<Option<i64>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query_scalar(
            "SELECT MAX(server_free_at_ms) FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .fetch_one(&self.pool)
        .await
    }

    /// Give back a target that was leased but never attacked or refused.
    ///
    /// A lease lasts twelve minutes. Without this, every handshake abandoned
    /// before the attack (no commander free, map context lost) quietly removed
    /// a perfectly good tower from the pool for that long.
    pub async fn release_rbc_target(
        &self,
        account_id: &str,
        target: &ReservedTarget,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE rbc_target SET reserved_until_ms = 0
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The earliest moment any tower in the task's level range can be selected.
    ///
    /// A value at or before `now` means a target is ready. This is what lets the
    /// scheduler sleep exactly until work exists instead of polling.
    pub async fn next_rbc_ready_ms(
        &self,
        account_id: &str,
        kingdom_id: i64,
        level_min: Option<i64>,
        level_max: Option<i64>,
    ) -> Result<Option<i64>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query_scalar(
            "SELECT MIN(ready) FROM (
                 SELECT MAX(reserved_until_ms, server_free_at_ms, COALESCE((
                     SELECT MAX(CASE WHEN a.landed_at_ms IS NOT NULL
                                     THEN MIN(a.landed_at_ms + 10800000, a.sent_at_ms + 14400000)
                                     ELSE a.sent_at_ms + 14400000 END) + 300000
                     FROM attack_ledger a
                     WHERE lower(a.account_id) = lower(rbc_target.account_id)
                       AND a.kingdom_id = rbc_target.kingdom_id
                       AND a.x = rbc_target.x AND a.y = rbc_target.y
                       AND rbc_target.kingdom_id != 10
                 ), 0)) AS ready
                 FROM rbc_target
                 WHERE lower(account_id) = lower(?) AND kingdom_id = ?
                   AND (? IS NULL OR level >= ?)
                   AND (? IS NULL OR level <= ?)
             )",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(level_min)
        .bind(level_min)
        .bind(level_max)
        .bind(level_max)
        .fetch_one(&self.pool)
        .await
    }

    /// Remove RBC observations outside the initialization square for each
    /// owned permanent kingdom. Fortress discovery may read the whole kingdom,
    /// but those opportunistic responses must not silently enlarge the user's
    /// configured farming radius — and a kingdom that is not being scanned at
    /// all keeps nothing. The radius travels with each kingdom because the
    /// account sets it per kingdom.
    pub async fn prune_rbc_targets_outside_radius(
        &self,
        account_id: &str,
        origins: &[(i64, i64, i64, i64)],
    ) -> Result<u64, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        let mut deleted = 0;
        for (kingdom_id, origin_x, origin_y, radius) in origins {
            let radius = (*radius).max(0);
            deleted += sqlx::query(
                "DELETE FROM rbc_target
                 WHERE lower(account_id) = lower(?) AND kingdom_id = ?
                   AND (abs(x - ?) > ? OR abs(y - ?) > ?)",
            )
            .bind(&account_id)
            .bind(kingdom_id)
            .bind(origin_x)
            .bind(radius)
            .bind(origin_y)
            .bind(radius)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        tx.commit().await?;
        Ok(deleted)
    }

    pub async fn upsert_fortress_targets(
        &self,
        account_id: &str,
        targets: &[FortressTarget],
        observed_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        for target in targets {
            let available_at_ms =
                observed_at_ms.saturating_add(target.cooldown_remaining_s.saturating_mul(1_000));
            sqlx::query(
                "INSERT INTO fortress_target (
                    account_id, kingdom_id, x, y, level, cooldown_remaining_s,
                    available_at_ms, occupier_player_id, observed_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, kingdom_id, x, y) DO UPDATE SET
                    level = excluded.level,
                    cooldown_remaining_s = excluded.cooldown_remaining_s,
                    available_at_ms = excluded.available_at_ms,
                    occupier_player_id = excluded.occupier_player_id,
                    refresh_due_ms = CASE
                        WHEN excluded.cooldown_remaining_s = 0
                         AND fortress_target.available_at_ms < excluded.observed_at_ms - 60000
                        THEN excluded.observed_at_ms + 1800000
                        ELSE 0
                    END,
                    observed_at_ms = excluded.observed_at_ms",
            )
            .bind(&account_id)
            .bind(target.kingdom_id)
            .bind(target.x)
            .bind(target.y)
            .bind(target.level)
            .bind(target.cooldown_remaining_s)
            .bind(available_at_ms)
            .bind(target.occupier_player_id)
            .bind(observed_at_ms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Persist the travel choices advertised for each castle by `gbd.gpc.A`.
    pub async fn upsert_castle_travel_options(
        &self,
        account_id: &str,
        options: &[CastleTravelOptions],
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        for option in options {
            let Some(coin_hbw) = option.coin_hbw() else {
                continue;
            };
            let encoded = serde_json::to_string(&option.unlocked_hbw)
                .expect("serializing integer travel ids cannot fail");
            sqlx::query(
                "INSERT INTO account_castle_travel (
                    account_id, castle_id, kingdom_id, unlocked_hbw_json,
                    coin_hbw, observed_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, castle_id) DO UPDATE SET
                    kingdom_id = excluded.kingdom_id,
                    unlocked_hbw_json = excluded.unlocked_hbw_json,
                    coin_hbw = excluded.coin_hbw,
                    observed_at_ms = excluded.observed_at_ms",
            )
            .bind(&account_id)
            .bind(option.castle_id)
            .bind(option.kingdom_id)
            .bind(encoded)
            .bind(coin_hbw)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Resolve a semantic travel mode for the exact attack source castle.
    /// Coordinates prevent two castles in one kingdom from being confused.
    pub async fn travel_for_source(
        &self,
        account_id: &str,
        kingdom_id: i64,
        source_x: i64,
        source_y: i64,
        mode: TravelMode,
    ) -> Result<Option<AttackTravel>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = sqlx::query(
            "SELECT castle.castle_id, travel.unlocked_hbw_json
             FROM owned_castle castle
             JOIN account_castle_travel travel
               ON travel.account_id = castle.account_id
              AND travel.castle_id = castle.castle_id
             WHERE castle.account_id = ? AND castle.kingdom_id = ?
               AND castle.x = ? AND castle.y = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(source_x)
        .bind(source_y)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let unlocked_hbw = serde_json::from_str(row.get("unlocked_hbw_json"))
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        Ok(CastleTravelOptions {
            castle_id: row.get("castle_id"),
            kingdom_id,
            unlocked_hbw,
        }
        .resolve(mode))
    }

    pub async fn record_scan_window(
        &self,
        account_id: &str,
        kingdom_id: i64,
        bounds: (i64, i64, i64, i64),
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "INSERT INTO map_scan_window (
                account_id, kingdom_id, ax1, ay1, ax2, ay2, scanned_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, kingdom_id, ax1, ay1) DO UPDATE SET
                ax2 = excluded.ax2,
                ay2 = excluded.ay2,
                scanned_at_ms = excluded.scanned_at_ms",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(bounds.0)
        .bind(bounds.1)
        .bind(bounds.2)
        .bind(bounds.3)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn scan_window_is_fresh(
        &self,
        account_id: &str,
        kingdom_id: i64,
        bounds: (i64, i64, i64, i64),
        now_ms: i64,
    ) -> Result<bool, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let scanned_at: Option<i64> = sqlx::query_scalar(
            "SELECT scanned_at_ms FROM map_scan_window
             WHERE account_id = ? AND kingdom_id = ?
               AND ax1 = ? AND ay1 = ? AND ax2 = ? AND ay2 = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(bounds.0)
        .bind(bounds.1)
        .bind(bounds.2)
        .bind(bounds.3)
        .fetch_optional(&self.pool)
        .await?;
        Ok(scanned_at.is_some_and(|timestamp| {
            now_ms.saturating_sub(timestamp) < scan_refresh_after_ms(kingdom_id, bounds.0, bounds.1)
        }))
    }

    pub async fn account_has_targets(
        &self,
        account_id: &str,
        kingdom_id: i64,
    ) -> Result<bool, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn account_has_fortresses(
        &self,
        account_id: &str,
        kingdom_id: i64,
    ) -> Result<bool, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fortress_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    /// `(done, pending)` blocks across both grids of one kingdom.
    pub async fn fortress_probe_progress(
        &self,
        account_id: &str,
        kingdom_id: i64,
    ) -> Result<(u64, u64), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = sqlx::query(
            "SELECT COALESCE(SUM(blocks_done), 0) done,
                    COALESCE(SUM(blocks_total - blocks_done), 0) pending
             FROM fortress_scan_state
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .fetch_one(&self.pool)
        .await?;
        let done = row.try_get::<i64, _>("done")?.max(0);
        let pending = row.try_get::<i64, _>("pending")?.max(0);
        Ok((done as u64, pending as u64))
    }

    /// Claim one eligible target before its handshake starts. The reservation
    /// survives a runner restart and prevents two tasks choosing the same tower.
    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_rbc_target(
        &self,
        account_id: &str,
        kingdom_id: i64,
        level_min: Option<i64>,
        level_max: Option<i64>,
        source: (i64, i64),
        algorithm: &str,
        now_ms: i64,
        reserved_until_ms: i64,
    ) -> Result<Option<ReservedTarget>, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query(
            "SELECT kingdom_id, x, y, level, last_attacked_ms
             FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?
               AND reserved_until_ms <= ?
               AND server_free_at_ms <= ?
               AND NOT EXISTS (
                   SELECT 1 FROM attack_ledger a
                   WHERE lower(a.account_id) = lower(rbc_target.account_id)
                     AND a.kingdom_id = rbc_target.kingdom_id
                     AND a.x = rbc_target.x AND a.y = rbc_target.y
                     AND rbc_target.kingdom_id != 10
                     AND (CASE WHEN a.landed_at_ms IS NOT NULL
                               THEN MIN(a.landed_at_ms + 10800000, a.sent_at_ms + 14400000)
                               ELSE a.sent_at_ms + 14400000 END) + 300000 > ?
               )
               AND (? IS NULL OR level >= ?)
               AND (? IS NULL OR level <= ?)",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .bind(level_min)
        .bind(level_min)
        .bind(level_max)
        .bind(level_max)
        .fetch_all(&self.pool)
        .await?;
        let chosen = match algorithm {
            "random" => rows.get((now_ms.unsigned_abs() as usize) % rows.len().max(1)),
            "closest" => rows.iter().min_by_key(|row| {
                (row.get::<i64, _>("x") - source.0).abs()
                    + (row.get::<i64, _>("y") - source.1).abs()
            }),
            _ => rows.iter().min_by_key(|row| {
                (
                    row.get::<i64, _>("last_attacked_ms"),
                    (row.get::<i64, _>("x") - source.0).abs()
                        + (row.get::<i64, _>("y") - source.1).abs(),
                )
            }),
        };
        let Some(row) = chosen else { return Ok(None) };
        let target = ReservedTarget {
            kingdom_id: row.get("kingdom_id"),
            x: row.get("x"),
            y: row.get("y"),
            level: row.get("level"),
        };
        let updated = sqlx::query(
            "UPDATE rbc_target SET reserved_until_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?
               AND reserved_until_ms <= ?",
        )
        .bind(reserved_until_ms)
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        // Pre-canonical databases may contain the same logical account under
        // different username casing. Reserve every matching copy together and
        // treat any update as one successful logical target claim.
        Ok((updated.rows_affected() > 0).then_some(target))
    }

    /// Reserve the fortress whose one-minute dispatch window expires first.
    /// A fortress that has been available for over a minute is intentionally
    /// ignored until a fresh server observation gives it a new cooldown.
    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_fortress_target(
        &self,
        account_id: &str,
        kingdom_id: i64,
        level_min: Option<i64>,
        level_max: Option<i64>,
        source: (i64, i64),
        now_ms: i64,
        reserved_until_ms: i64,
    ) -> Result<Option<ReservedTarget>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = sqlx::query(
            "SELECT kingdom_id, x, y, level
             FROM fortress_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?
               AND available_at_ms <= ? AND available_at_ms >= ?
               AND reserved_until_ms <= ?
               AND (? IS NULL OR level >= ?)
               AND (? IS NULL OR level <= ?)
             ORDER BY available_at_ms,
                      abs(x - ?) + abs(y - ?), x, y
             LIMIT 1",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(now_ms)
        .bind(now_ms.saturating_sub(60_000))
        .bind(now_ms)
        .bind(level_min)
        .bind(level_min)
        .bind(level_max)
        .bind(level_max)
        .bind(source.0)
        .bind(source.1)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let target = ReservedTarget {
            kingdom_id: row.get("kingdom_id"),
            x: row.get("x"),
            y: row.get("y"),
            level: Some(row.get("level")),
        };
        let updated = sqlx::query(
            "UPDATE fortress_target SET reserved_until_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?
               AND reserved_until_ms <= ?
               AND available_at_ms <= ? AND available_at_ms >= ?",
        )
        .bind(reserved_until_ms)
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms.saturating_sub(60_000))
        .execute(&self.pool)
        .await?;
        Ok((updated.rows_affected() > 0).then_some(target))
    }

    /// Read the bounds of a kingdom's sweep out of a state row.
    fn scan_bounds(row: &sqlx::sqlite::SqliteRow) -> crate::fortress::Bounds {
        crate::fortress::Bounds {
            left: row.get("left_bound"),
            top: row.get("top_bound"),
            right: row.get("right_bound"),
            bottom: row.get("bottom_bound"),
        }
    }

    /// Start the sweep of one grid in one kingdom, or leave a running one alone.
    ///
    /// The bounds are the map rectangle this kingdom is walked over and the
    /// cursor starts at its low corner. `INSERT OR IGNORE` is what makes the
    /// sweep resumable: a second call never resets a cursor that has moved.
    pub async fn start_fortress_scan(
        &self,
        account_id: &str,
        kingdom_id: i64,
        lattice_offset: i64,
    ) -> Result<bool, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let bounds =
            crate::fortress::align_bounds(crate::fortress::Bounds::outer_kingdom(), lattice_offset);
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO fortress_scan_state (
                account_id, kingdom_id, lattice_offset,
                left_bound, top_bound, right_bound, bottom_bound,
                next_x, next_y, blocks_total
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(lattice_offset)
        .bind(bounds.left)
        .bind(bounds.top)
        .bind(bounds.right)
        .bind(bounds.bottom)
        .bind(bounds.left)
        .bind(bounds.top)
        .bind(bounds.block_count())
        .execute(&self.pool)
        .await?;
        Ok(inserted.rows_affected() > 0)
    }

    /// The block the sweep is on, or `None` once the kingdom is finished.
    ///
    /// This only reads. The cursor moves in [`Self::advance_fortress_scan`],
    /// which the runner calls once the server has answered, so a request that
    /// never comes back is retried instead of skipping four slots.
    pub async fn next_fortress_block(
        &self,
        account_id: &str,
        kingdom_id: i64,
        lattice_offset: i64,
    ) -> Result<Option<crate::fortress::FortressBlock>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = self
            .fortress_scan_row(&account_id, kingdom_id, lattice_offset)
            .await?;
        let Some(row) = row else { return Ok(None) };
        let cursor = crate::fortress::FortressScanCursor {
            x: row.get("next_x"),
            y: row.get("next_y"),
        };
        Ok(cursor.block(Self::scan_bounds(&row)))
    }

    /// Which of `sweeps` still have blocks left to visit.
    ///
    /// A sweep is outstanding when it has no state row (never started) or its
    /// cursor has not yet passed the last block. Fortress coordinates never
    /// change, so a kingdom walked in an earlier session answers empty here and
    /// the runner can skip the walk instead of pacing a map it has already
    /// covered. An empty input returns an empty result, so "nothing to do" and
    /// "nothing asked for" never have to be told apart by the caller.
    pub async fn outstanding_fortress_sweeps(
        &self,
        account_id: &str,
        sweeps: &[(i64, i64)],
    ) -> Result<Vec<(i64, i64)>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut outstanding = Vec::new();
        for &(kingdom_id, lattice_offset) in sweeps {
            let row = self
                .fortress_scan_row(&account_id, kingdom_id, lattice_offset)
                .await?;
            let Some(row) = row else {
                outstanding.push((kingdom_id, lattice_offset));
                continue;
            };
            let cursor = crate::fortress::FortressScanCursor {
                x: row.get("next_x"),
                y: row.get("next_y"),
            };
            if cursor.block(Self::scan_bounds(&row)).is_some() {
                outstanding.push((kingdom_id, lattice_offset));
            }
        }
        Ok(outstanding)
    }

    /// Kingdoms with a fortress task in the attack bot that is running right now.
    ///
    /// Discovery is work done *for* a fortress task, so it waits until such a task
    /// is actually being run. A robber-baron bot, or a fortress task sitting in a
    /// bot nobody started, must not cost a walk — and because the walk is
    /// persisted, each kingdom is paid for once and never again.
    pub async fn active_fortress_kingdoms(
        &self,
        account_id: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query(
            "SELECT DISTINCT task.kingdom_id kingdom_id
             FROM account_mode mode
             JOIN automation_mode_task assignment ON assignment.mode_id = mode.mode_id
             JOIN task_definition task ON task.task_id = assignment.task_id
             JOIN task_subscription subscription ON subscription.task_id = task.task_id
             WHERE lower(mode.account_id) = lower(?) AND mode.running = 1
               AND task.enabled = 1 AND subscription.target_kind = 'fortress'
             ORDER BY task.kingdom_id",
        )
        .bind(&account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.get("kingdom_id")).collect())
    }

    /// The bounds of a kingdom's sweep, or `None` when it has not been walked yet.
    ///
    /// The discovery walk needs to tell "never started" apart from "started and
    /// finished", which `next_fortress_block` cannot: it answers `None` for both.
    pub async fn fortress_scan_bounds(
        &self,
        account_id: &str,
        kingdom_id: i64,
        lattice_offset: i64,
    ) -> Result<Option<crate::fortress::Bounds>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        Ok(self
            .fortress_scan_row(&account_id, kingdom_id, lattice_offset)
            .await?
            .map(|row| Self::scan_bounds(&row)))
    }

    /// Record the rectangle a discovery walk found and begin its row-major fill.
    ///
    /// `INSERT OR REPLACE` rather than `INSERT OR IGNORE`: this is called once,
    /// when the arms have measured the extent, and it has to overwrite the
    /// placeholder that said the kingdom was merely outstanding. The cursor is
    /// reset to the rectangle's low corner because that is where the fill starts.
    pub async fn set_fortress_scan_bounds(
        &self,
        account_id: &str,
        kingdom_id: i64,
        lattice_offset: i64,
        bounds: crate::fortress::Bounds,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let bounds = crate::fortress::align_bounds(bounds, lattice_offset);
        sqlx::query(
            "INSERT OR REPLACE INTO fortress_scan_state (
                account_id, kingdom_id, lattice_offset,
                left_bound, top_bound, right_bound, bottom_bound,
                next_x, next_y, blocks_total, blocks_done
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(lattice_offset)
        .bind(bounds.left)
        .bind(bounds.top)
        .bind(bounds.right)
        .bind(bounds.bottom)
        .bind(bounds.left)
        .bind(bounds.top)
        .bind(bounds.block_count())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// An explicit user scan reuses known geometry but walks it again to obtain
    /// fresh variable cooldowns. Ordinary reconnects never call this, so the
    /// coordinate-discovery cost remains a one-time operation.
    pub async fn reset_fortress_scans(
        &self,
        account_id: &str,
        sweeps: &[(i64, i64)],
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        for (kingdom_id, lattice_offset) in sweeps {
            sqlx::query(
                "UPDATE fortress_scan_state
                 SET next_x = left_bound, next_y = top_bound, blocks_done = 0
                 WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND lattice_offset = ?",
            )
            .bind(&account_id)
            .bind(kingdom_id)
            .bind(lattice_offset)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Move the sweep on by one block, after the server has answered.
    pub async fn advance_fortress_scan(
        &self,
        account_id: &str,
        kingdom_id: i64,
        lattice_offset: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = self
            .fortress_scan_row(&account_id, kingdom_id, lattice_offset)
            .await?;
        let Some(row) = row else { return Ok(()) };
        let mut cursor = crate::fortress::FortressScanCursor {
            x: row.get("next_x"),
            y: row.get("next_y"),
        };
        cursor.advance(Self::scan_bounds(&row));
        sqlx::query(
            "UPDATE fortress_scan_state
             SET next_x = ?, next_y = ?, blocks_done = blocks_done + 1
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND lattice_offset = ?",
        )
        .bind(cursor.x)
        .bind(cursor.y)
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(lattice_offset)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn fortress_scan_row(
        &self,
        account_id: &str,
        kingdom_id: i64,
        lattice_offset: i64,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query(
            "SELECT left_bound, top_bound, right_bound, bottom_bound, next_x, next_y
             FROM fortress_scan_state
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND lattice_offset = ?",
        )
        .bind(account_id)
        .bind(kingdom_id)
        .bind(lattice_offset)
        .fetch_optional(&self.pool)
        .await
    }

    /// Queue a cooldown re-read for each fortress in this kingdom that is worth
    /// re-reading.
    ///
    /// Initialization learns *where* the fortresses are and, from the same
    /// responses, *when* each one opens. After that a request is only justified
    /// to confirm a window that is about to open, or to sanity-check a row that
    /// has not been looked at for hours. Both are gated behind
    /// [`FORTRESS_RECHECK_MIN_MS`], which is what keeps this idempotent: a read
    /// refreshes the observation, so it cannot schedule its own successor. (The
    /// response clears `refresh_due_ms` again, so gating on that flag alone makes
    /// the runner read the same cooldown forever.)
    pub async fn queue_fortress_recheck(
        &self,
        account_id: &str,
        kingdom_id: i64,
        now_ms: i64,
    ) -> Result<u64, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let queued = sqlx::query(
            "UPDATE fortress_target SET refresh_due_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?
               AND refresh_due_ms = 0
               AND observed_at_ms <= ? - ?
               AND (available_at_ms - ? <= ? OR observed_at_ms <= ? - ?)",
        )
        .bind(now_ms)
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(now_ms)
        .bind(FORTRESS_RECHECK_MIN_MS)
        .bind(now_ms)
        .bind(FORTRESS_CONFIRM_LEAD_MS)
        .bind(now_ms)
        .bind(FORTRESS_SANITY_INTERVAL_MS)
        .execute(&self.pool)
        .await?;
        Ok(queued.rows_affected())
    }

    pub async fn reserve_due_fortress_refresh(
        &self,
        account_id: &str,
        kingdom_id: i64,
        now_ms: i64,
    ) -> Result<Option<ReservedTarget>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        // Once the strict dispatch minute is gone, drop that opportunity but
        // keep a lightweight truth refresh so a later attack by another player
        // and its new 24-hour cooldown are eventually learned.
        sqlx::query(
            "UPDATE fortress_target SET refresh_due_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?
               AND available_at_ms < ? AND refresh_due_ms = 0",
        )
        .bind(now_ms)
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(now_ms.saturating_sub(60_000))
        .execute(&self.pool)
        .await?;
        let row = sqlx::query(
            "SELECT kingdom_id, x, y, level FROM fortress_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?
               AND refresh_due_ms > 0 AND refresh_due_ms <= ?
             ORDER BY refresh_due_ms LIMIT 1",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(now_ms)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let target = ReservedTarget {
            kingdom_id: row.get("kingdom_id"),
            x: row.get("x"),
            y: row.get("y"),
            level: Some(row.get("level")),
        };
        sqlx::query(
            "UPDATE fortress_target SET refresh_due_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(now_ms.saturating_add(10 * 60 * 1_000))
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(Some(target))
    }

    /// Enforce the one-minute dispatch window derived from the last broad scan.
    pub async fn fortress_is_dispatchable(
        &self,
        account_id: &str,
        target: &ReservedTarget,
        now_ms: i64,
        observed_since_ms: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fortress_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?
               AND available_at_ms <= ? AND available_at_ms >= ?
               AND (? IS NULL OR observed_at_ms >= ?)",
        )
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .bind(now_ms)
        .bind(now_ms.saturating_sub(60_000))
        .bind(observed_since_ms)
        .bind(observed_since_ms)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn release_fortress_target(
        &self,
        account_id: &str,
        target: &ReservedTarget,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE fortress_target SET reserved_until_ms = 0
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Quarantine a server-refused fortress without deleting its observed
    /// cooldown. This prevents one bad opening from consuming the whole
    /// one-minute dispatch window through immediate retries.
    /// A tower's cooldown once its landing is observed:
    /// `min(landed + 3h, sent + 4h) + buffer`. Overwrites (does not `MAX`) the
    /// provisional `sent + 4h` upper bound written when the attack was sent.
    pub async fn refine_rbc_cooldown(
        &self,
        account_id: &str,
        kingdom_id: i64,
        x: i64,
        y: i64,
        landed_at_ms: i64,
        buffer_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE rbc_target SET reserved_until_ms = MIN(?, COALESCE((
                 SELECT MAX(sent_at_ms) FROM attack_ledger a
                 WHERE lower(a.account_id) = lower(rbc_target.account_id)
                   AND a.kingdom_id = rbc_target.kingdom_id
                   AND a.x = rbc_target.x AND a.y = rbc_target.y
             ), ?) + 14400000) + ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(landed_at_ms.saturating_add(10_800_000))
        .bind(landed_at_ms)
        .bind(buffer_ms)
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(x)
        .bind(y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_rbc_target(
        &self,
        account_id: &str,
        kingdom_id: i64,
        x: i64,
        y: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "DELETE FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(x)
        .bind(y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn defer_rbc_target(
        &self,
        account_id: &str,
        target: &ReservedTarget,
        until_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE rbc_target SET reserved_until_ms = MAX(reserved_until_ms, ?)
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(until_ms)
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn defer_fortress_target(
        &self,
        account_id: &str,
        target: &ReservedTarget,
        until_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE fortress_target SET reserved_until_ms = MAX(reserved_until_ms, ?)
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(until_ms)
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_fortress_attack_sent(
        &self,
        account_id: &str,
        target: &ReservedTarget,
        sent_at_ms: i64,
        expected_landing_ms: i64,
        refresh_delay_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE fortress_target
             SET last_attacked_ms = ?, reserved_until_ms = 0, refresh_due_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(sent_at_ms)
        .bind(expected_landing_ms.saturating_add(refresh_delay_ms))
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Rubies prove our fortress hit succeeded; otherwise retain the scheduled
    /// live refresh because another player may have won the race.
    pub async fn record_fortress_result(
        &self,
        account_id: &str,
        kingdom_id: i64,
        x: i64,
        y: i64,
        ruby_loot: Option<i64>,
        landed_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        if ruby_loot.is_some_and(|rubies| rubies > 0) {
            let cooldown_s = 100 * 60 * 60;
            sqlx::query(
                "UPDATE fortress_target
                 SET cooldown_remaining_s = ?, available_at_ms = ?, refresh_due_ms = 0,
                     reserved_until_ms = 0, observed_at_ms = ?
                 WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
            )
            .bind(cooldown_s)
            .bind(landed_at_ms.saturating_add(cooldown_s * 1_000))
            .bind(landed_at_ms)
            .bind(&account_id)
            .bind(kingdom_id)
            .bind(x)
            .bind(y)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn mark_target_attacked(
        &self,
        account_id: &str,
        target: &ReservedTarget,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE rbc_target SET last_attacked_ms = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(now_ms)
        .bind(&account_id)
        .bind(target.kingdom_id)
        .bind(target.x)
        .bind(target.y)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn account_summaries(&self) -> Result<Vec<AccountSummary>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT lower(p.account_id) account_id,
                    upper(substr(MAX(p.player_name), 1, 1)) || substr(MAX(p.player_name), 2) player_name,
                    MAX(p.endpoint) endpoint, MAX(p.server_header) server_header,
                    MAX(p.initialized_at_ms) initialized_at_ms,
                    (SELECT COUNT(DISTINCT printf('%d:%d', c.kingdom_id, c.castle_id))
                     FROM owned_castle c WHERE lower(c.account_id) = lower(p.account_id)) castle_count,
                    (SELECT COUNT(DISTINCT m.lord_id)
                     FROM account_commander m WHERE lower(m.account_id) = lower(p.account_id)) commander_count,
                    (SELECT COUNT(DISTINCT printf('%d:%d:%d', r.kingdom_id, r.x, r.y))
                     FROM rbc_target r WHERE lower(r.account_id) = lower(p.account_id)) rbc_count
             FROM account_profile p
             WHERE p.initialized_at_ms IS NOT NULL
             GROUP BY lower(p.account_id)
             ORDER BY p.player_name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut accounts: Vec<AccountSummary> = rows
            .into_iter()
            .map(|row| AccountSummary {
                account_id: row.get("account_id"),
                player_name: row.get("player_name"),
                endpoint: row.get("endpoint"),
                server_header: row.get("server_header"),
                initialized_at_ms: row.get("initialized_at_ms"),
                castle_count: row.get("castle_count"),
                commander_count: row.get("commander_count"),
                rbc_count: row.get("rbc_count"),
                kingdom_health: Vec::new(),
            })
            .collect();
        for account in &mut accounts {
            let rows = sqlx::query(
                "SELECT MAX(c.castle_id) castle_id, MAX(c.name) name, c.kingdom_id,
                        MAX(c.x) x, MAX(c.y) y,
                        (SELECT COUNT(DISTINCT printf('%d:%d', r.x, r.y)) FROM rbc_target r
                         WHERE lower(r.account_id) = lower(c.account_id) AND r.kingdom_id = c.kingdom_id) target_count,
                        (SELECT COUNT(DISTINCT printf('%d:%d:%d:%d', s.ax1, s.ay1, s.ax2, s.ay2)) FROM map_scan_window s
                         WHERE lower(s.account_id) = lower(c.account_id) AND s.kingdom_id = c.kingdom_id) scan_window_count,
                        (SELECT MAX(scanned_at_ms) FROM map_scan_window s
                         WHERE lower(s.account_id) = lower(c.account_id) AND s.kingdom_id = c.kingdom_id) last_scanned_at_ms,
                        (SELECT COUNT(*) FROM fortress_target f
                         WHERE lower(f.account_id) = lower(c.account_id) AND f.kingdom_id = c.kingdom_id) fortress_count,
                        (SELECT COALESCE(SUM(s.blocks_total - s.blocks_done), 0) FROM fortress_scan_state s
                         WHERE lower(s.account_id) = lower(c.account_id) AND s.kingdom_id = c.kingdom_id) fortress_probes_pending,
                        (SELECT COALESCE(SUM(s.blocks_total), 0) FROM fortress_scan_state s
                         WHERE lower(s.account_id) = lower(c.account_id) AND s.kingdom_id = c.kingdom_id) fortress_probes_total,
                        (SELECT COUNT(*) FROM task_definition t
                         JOIN task_subscription s ON s.task_id = t.task_id
                         WHERE s.target_kind = 'fortress' AND t.enabled = 1
                           AND t.kingdom_id = c.kingdom_id) fortress_task
                 FROM owned_castle c
                 WHERE lower(c.account_id) = lower(?)
                   AND ((c.kingdom_id = 0 AND c.area_type = 1)
                     OR (c.kingdom_id != 0 AND c.area_type = 12))
                 GROUP BY c.kingdom_id
                 ORDER BY c.kingdom_id",
            )
            .bind(&account.account_id)
            .fetch_all(&self.pool)
            .await?;
            account.kingdom_health = rows
                .into_iter()
                .map(|row| KingdomHealth {
                    castle_id: row.get("castle_id"),
                    castle_name: row.get("name"),
                    kingdom_id: row.get("kingdom_id"),
                    x: row.get("x"),
                    y: row.get("y"),
                    target_count: row.get("target_count"),
                    scan_window_count: row.get("scan_window_count"),
                    last_scanned_at_ms: row.get("last_scanned_at_ms"),
                    fortress_count: row.get("fortress_count"),
                    fortress_probes_pending: row.get("fortress_probes_pending"),
                    fortress_probes_total: row.get("fortress_probes_total"),
                    fortress_task: row.get::<i64, _>("fortress_task") > 0,
                })
                .collect();
        }
        Ok(accounts)
    }

    /// Every installed licence, one per game account.
    pub async fn licences(&self) -> Result<Vec<StoredLicence>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT token, license_id, subject, revision, expires_at, highest_seen_at
             FROM licence ORDER BY license_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredLicence {
                token: row.get("token"),
                license_id: row.get("license_id"),
                subject: row.get("subject"),
                revision: row.get("revision"),
                expires_at: row.get("expires_at"),
                highest_seen_at: row.get("highest_seen_at"),
            })
            .collect())
    }

    /// Add a licence, or renew the one with the same `license_id`. Other
    /// licences are untouched.
    pub async fn save_licence(
        &self,
        licence: &StoredLicence,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO licence (
                license_id, token, subject, revision, expires_at, highest_seen_at, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(license_id) DO UPDATE SET
                token = excluded.token,
                subject = excluded.subject,
                revision = excluded.revision,
                expires_at = excluded.expires_at,
                highest_seen_at = MAX(licence.highest_seen_at, excluded.highest_seen_at),
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&licence.license_id)
        .bind(&licence.token)
        .bind(&licence.subject)
        .bind(licence.revision)
        .bind(licence.expires_at)
        .bind(licence.highest_seen_at)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Remember the latest clock reading for every licence, so winding the system
    /// clock back cannot revive an expired one.
    pub async fn advance_highest_seen(&self, now: i64, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE licence
             SET highest_seen_at = MAX(highest_seen_at, ?), updated_at_ms = ?",
        )
        .bind(now)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn licence_activation(
        &self,
        license_id: &str,
    ) -> Result<Option<LicenceActivation>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT license_id, server, player_id, activated_at
             FROM licence_activation WHERE license_id = ?",
        )
        .bind(license_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(activation_from_row))
    }

    /// The licence already bound to this game account, if any.
    pub async fn licence_activation_for_player(
        &self,
        server: &str,
        player_id: i64,
    ) -> Result<Option<LicenceActivation>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT license_id, server, player_id, activated_at
             FROM licence_activation WHERE server = ? AND player_id = ?",
        )
        .bind(server)
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(activation_from_row))
    }

    pub async fn licence_activations(&self) -> Result<Vec<LicenceActivation>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT license_id, server, player_id, activated_at FROM licence_activation",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(activation_from_row).collect())
    }

    /// First binding is immutable. Renewals reuse the same licence id and
    /// therefore the same permanent game identity.
    pub async fn bind_licence(
        &self,
        activation: &LicenceActivation,
        now_ms: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO licence_activation (
                license_id, server, player_id, activated_at, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&activation.license_id)
        .bind(&activation.server)
        .bind(activation.player_id)
        .bind(activation.activated_at)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Write a small piece of cross-run state, such as the current run label.
    pub async fn set_app_state(
        &self,
        key: &str,
        value: &Value,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO app_state (key, value_json, updated_at_ms) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(key)
        .bind(value.to_string())
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn app_state(&self, key: &str) -> Result<Option<Value>, sqlx::Error> {
        let row = sqlx::query("SELECT value_json FROM app_state WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => serde_json::from_str(row.get::<&str, _>("value_json"))
                .map(Some)
                .map_err(|error| sqlx::Error::Decode(Box::new(error))),
            None => Ok(None),
        }
    }

    pub async fn record_message(
        &self,
        observed_at_ms: i64,
        direction: Direction,
        command: Option<&str>,
        payload: &Value,
    ) -> Result<i64, sqlx::Error> {
        self.record_message_for(None, observed_at_ms, direction, command, payload)
            .await
    }

    /// Like [`Self::record_message`], tagging the packet with the account whose
    /// session it belongs to, so captures from two accounts can be told apart.
    pub async fn record_message_for(
        &self,
        account_id: Option<&str>,
        observed_at_ms: i64,
        direction: Direction,
        command: Option<&str>,
        payload: &Value,
    ) -> Result<i64, sqlx::Error> {
        let direction_text = serde_json::to_value(&direction)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let result = sqlx::query(
            "INSERT INTO network_message (observed_at_ms, direction, command, payload_json, account_id)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(observed_at_ms)
        .bind(direction_text)
        .bind(command)
        .bind(payload.to_string())
        .bind(account_id.map(canonical_account_id))
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "DELETE FROM network_message WHERE sequence <= (
                SELECT COALESCE(MAX(sequence), 0) - ? FROM network_message
            )",
        )
        .bind(RECENT_MESSAGE_LIMIT)
        .execute(&self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn recent_messages(&self, limit: i64) -> Result<Vec<StoredMessage>, sqlx::Error> {
        let requested = limit.clamp(1, RECENT_MESSAGE_LIMIT);
        let rows = sqlx::query(
            "SELECT sequence, observed_at_ms, direction, command, payload_json
             FROM network_message ORDER BY sequence DESC LIMIT ?",
        )
        .bind(requested)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let direction: Direction =
                    serde_json::from_value(Value::String(row.get("direction")))
                        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                let payload = serde_json::from_str(row.get::<&str, _>("payload_json"))
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                Ok(StoredMessage {
                    sequence: row.get("sequence"),
                    observed_at_ms: row.get("observed_at_ms"),
                    direction,
                    command: row.get("command"),
                    payload,
                })
            })
            .collect()
    }
}

/// A tower's free time from its reported cooldown, with a little slack for the
/// time the response spent in flight so we never fire a hair too early.
fn server_free_at_ms(target: &RbcTarget, observed_at_ms: i64) -> i64 {
    if target.cooldown_remaining_s <= 0 {
        return 0;
    }
    observed_at_ms
        .saturating_add(target.cooldown_remaining_s.saturating_mul(1_000))
        .saturating_add(2_000)
}

fn activation_from_row(row: sqlx::sqlite::SqliteRow) -> LicenceActivation {
    LicenceActivation {
        license_id: row.get("license_id"),
        server: row.get("server"),
        player_id: row.get("player_id"),
        activated_at: row.get("activated_at"),
    }
}
