use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{
    RECENT_MESSAGE_LIMIT,
    account::{OwnedCastle, RbcTarget},
    event::Direction,
};

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
}

impl Store {
    pub async fn open(url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS network_message (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                observed_at_ms INTEGER NOT NULL,
                direction TEXT NOT NULL,
                command TEXT,
                payload_json TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS app_state (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at_ms INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
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
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS account_profile (
                account_id TEXT PRIMARY KEY,
                player_name TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                server_header TEXT NOT NULL,
                initialized_at_ms INTEGER,
                updated_at_ms INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
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
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS account_commander (
                account_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                lord_id INTEGER NOT NULL,
                observed_at_ms INTEGER NOT NULL,
                PRIMARY KEY (account_id, ordinal),
                FOREIGN KEY (account_id) REFERENCES account_profile(account_id) ON DELETE CASCADE
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
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
        )
        .execute(&self.pool)
        .await?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn retention_is_bounded_to_recent_messages() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        for index in 0..205 {
            store
                .record_message(
                    index,
                    Direction::Injected,
                    Some("gbl"),
                    &json!({"i": index}),
                )
                .await
                .unwrap();
        }
        let messages = store.recent_messages(500).await.unwrap();
        assert_eq!(messages.len(), RECENT_MESSAGE_LIMIT as usize);
        assert_eq!(messages.first().unwrap().payload, json!({"i": 204}));
        assert_eq!(messages.last().unwrap().payload, json!({"i": 5}));
    }
}
