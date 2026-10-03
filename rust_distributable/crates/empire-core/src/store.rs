use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{RECENT_MESSAGE_LIMIT, event::Direction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub sequence: i64,
    pub observed_at_ms: i64,
    pub direction: Direction,
    pub command: Option<String>,
    pub payload: Value,
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
