use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{
    RECENT_MESSAGE_LIMIT,
    account::{OwnedCastle, RbcTarget},
    event::Direction,
};

mod config;
mod ledger;
mod schema;
mod state;
mod storage;

#[cfg(test)]
mod tests;

pub use config::{AttackProfile, SubscriptionRecord, TaskRecord};
pub use ledger::{
    COMMANDER_AVAILABLE, COMMANDER_OUTBOUND, CommanderState, HEARTBEAT_FRESH_MILLIS,
    HUNT_HEARTBEAT_KEY, HuntSummary, HuntTaskSummary, MARCH_RETURNING, MARCH_SENT, MarchRecord,
};
pub use schema::SCHEMA_VERSION;
pub use state::{CastleUnit, NavigationState, RecruitCastleState};
pub use storage::{PruneOutcome, StorageReport, TableFootprint};

/// Extract the on-disk path from a sqlx SQLite URL, when there is one.
///
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
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    path: Option<PathBuf>,
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
        .bind(account_id)
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
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM owned_castle WHERE account_id = ?")
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM account_commander WHERE account_id = ?")
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        for castle in castles {
            sqlx::query(
                "INSERT INTO owned_castle (
                    account_id, castle_id, kingdom_id, area_type, x, y, name, observed_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(account_id)
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
            .bind(account_id)
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
        .bind(account_id)
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
            .bind(account_id)
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

    pub async fn account_summaries(&self) -> Result<Vec<AccountSummary>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT p.account_id, p.player_name, p.endpoint, p.server_header,
                    p.initialized_at_ms,
                    (SELECT COUNT(*) FROM owned_castle c WHERE c.account_id = p.account_id) castle_count,
                    (SELECT COUNT(*) FROM account_commander m WHERE m.account_id = p.account_id) commander_count,
                    (SELECT COUNT(*) FROM rbc_target r WHERE r.account_id = p.account_id) rbc_count
             FROM account_profile p
             WHERE p.initialized_at_ms IS NOT NULL
             ORDER BY p.player_name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
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
            })
            .collect())
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
