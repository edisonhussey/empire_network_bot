use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{
    RECENT_MESSAGE_LIMIT,
    account::{AttackTravel, CastleTravelOptions, OwnedCastle, RbcTarget},
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
        for target in targets {
            sqlx::query(
                "INSERT INTO rbc_target (
                    account_id, kingdom_id, x, y, level, observed_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, kingdom_id, x, y) DO UPDATE SET
                    level = excluded.level,
                    observed_at_ms = excluded.observed_at_ms",
            )
            .bind(&account_id)
            .bind(target.kingdom_id)
            .bind(target.x)
            .bind(target.y)
            .bind(target.level)
            .bind(now_ms)
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
        ax1: i64,
        ay1: i64,
        now_ms: i64,
    ) -> Result<bool, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let scanned_at: Option<i64> = sqlx::query_scalar(
            "SELECT scanned_at_ms FROM map_scan_window
             WHERE account_id = ? AND kingdom_id = ? AND ax1 = ? AND ay1 = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(ax1)
        .bind(ay1)
        .fetch_optional(&self.pool)
        .await?;
        Ok(scanned_at.is_some_and(|timestamp| {
            now_ms.saturating_sub(timestamp) < scan_refresh_after_ms(kingdom_id, ax1, ay1)
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
               AND (? IS NULL OR level >= ?)
               AND (? IS NULL OR level <= ?)",
        )
        .bind(&account_id)
        .bind(kingdom_id)
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
                         WHERE lower(s.account_id) = lower(c.account_id) AND s.kingdom_id = c.kingdom_id) last_scanned_at_ms
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
                })
                .collect();
        }
        Ok(accounts)
    }

    pub async fn licence(&self) -> Result<Option<StoredLicence>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT token, license_id, subject, revision, expires_at, highest_seen_at
             FROM licence_state WHERE singleton = 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| StoredLicence {
            token: row.get("token"),
            license_id: row.get("license_id"),
            subject: row.get("subject"),
            revision: row.get("revision"),
            expires_at: row.get("expires_at"),
            highest_seen_at: row.get("highest_seen_at"),
        }))
    }

    pub async fn save_licence(
        &self,
        licence: &StoredLicence,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO licence_state (
                singleton, token, license_id, subject, revision, expires_at,
                highest_seen_at, updated_at_ms
             ) VALUES (1, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(singleton) DO UPDATE SET
                token = excluded.token,
                license_id = excluded.license_id,
                subject = excluded.subject,
                revision = excluded.revision,
                expires_at = excluded.expires_at,
                highest_seen_at = MAX(licence_state.highest_seen_at, excluded.highest_seen_at),
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&licence.token)
        .bind(&licence.license_id)
        .bind(&licence.subject)
        .bind(licence.revision)
        .bind(licence.expires_at)
        .bind(licence.highest_seen_at)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn advance_highest_seen(&self, now: i64, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE licence_state
             SET highest_seen_at = MAX(highest_seen_at, ?), updated_at_ms = ?
             WHERE singleton = 1",
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
        Ok(row.map(|row| LicenceActivation {
            license_id: row.get("license_id"),
            server: row.get("server"),
            player_id: row.get("player_id"),
            activated_at: row.get("activated_at"),
        }))
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
        let direction_text = serde_json::to_value(&direction)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let result = sqlx::query(
            "INSERT INTO network_message (observed_at_ms, direction, command, payload_json)
             VALUES (?, ?, ?, ?)",
        )
        .bind(observed_at_ms)
        .bind(direction_text)
        .bind(command)
        .bind(payload.to_string())
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
