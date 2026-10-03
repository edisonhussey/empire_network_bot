//! User-authored configuration: attack payloads, tasks, and subscriptions.
//!
//! These records are exactly what a config JSON file carries, so they are plain
//! serde types with no behaviour. `payload` is the `A` array that goes on the
//! wire (see `empire_game::Attack::to_payload`), which means an imported profile
//! is sent exactly as authored.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use super::Store;

fn empty_object() -> Value {
    Value::Object(Default::default())
}

fn encode_tags(tags: &[String]) -> String {
    tags.iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A named attack payload the user authored or imported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttackProfile {
    pub profile_id: String,
    pub name: String,
    /// `[{L,M,R}, ...]` — one entry per wave.
    pub payload: Value,
    #[serde(default)]
    pub notes: String,
}

/// A profile plus the rules that decide which targets it attacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: String,
    pub name: String,
    /// Which target source resolves this task. `attack` today.
    pub kind: String,
    pub kingdom_id: i64,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub target_level_min: Option<i64>,
    #[serde(default)]
    pub target_level_max: Option<i64>,
    pub commander_count: i64,
    #[serde(default)]
    pub max_active: Option<i64>,
    pub priority: i64,
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

/// What a task consumes. A subscription names a target *kind* and a filter, so
/// "subscribe by kind" is data rather than a hardcoded castle list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub task_id: String,
    pub target_kind: String,
    #[serde(default = "empty_object")]
    pub filter: Value,
}

impl Store {
    pub async fn upsert_attack_profile(
        &self,
        profile: &AttackProfile,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO attack_profile (profile_id, name, payload_json, notes, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(profile_id) DO UPDATE SET
                name = excluded.name,
                payload_json = excluded.payload_json,
                notes = excluded.notes,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&profile.profile_id)
        .bind(&profile.name)
        .bind(profile.payload.to_string())
        .bind(&profile.notes)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn attack_profiles(&self) -> Result<Vec<AttackProfile>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT profile_id, name, payload_json, notes FROM attack_profile ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let payload = serde_json::from_str(row.get::<&str, _>("payload_json"))
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                Ok(AttackProfile {
                    profile_id: row.get("profile_id"),
                    name: row.get("name"),
                    payload,
                    notes: row.get("notes"),
                })
            })
            .collect()
    }

    pub async fn delete_attack_profile(&self, profile_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM attack_profile WHERE profile_id = ?")
            .bind(profile_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upsert_task(&self, task: &TaskRecord, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO task_definition (
                task_id, name, kind, kingdom_id, profile_id, target_level_min,
                target_level_max, commander_count, max_active, priority, enabled,
                tags, notes, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(task_id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                kingdom_id = excluded.kingdom_id,
                profile_id = excluded.profile_id,
                target_level_min = excluded.target_level_min,
                target_level_max = excluded.target_level_max,
                commander_count = excluded.commander_count,
                max_active = excluded.max_active,
                priority = excluded.priority,
                enabled = excluded.enabled,
                tags = excluded.tags,
                notes = excluded.notes,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&task.task_id)
        .bind(&task.name)
        .bind(&task.kind)
        .bind(task.kingdom_id)
        .bind(task.profile_id.as_deref())
        .bind(task.target_level_min)
        .bind(task.target_level_max)
        .bind(task.commander_count)
        .bind(task.max_active)
        .bind(task.priority)
        .bind(i64::from(task.enabled))
        .bind(encode_tags(&task.tags))
        .bind(&task.notes)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn tasks(&self) -> Result<Vec<TaskRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT task_id, name, kind, kingdom_id, profile_id, target_level_min,
                    target_level_max, commander_count, max_active, priority, enabled,
                    tags, notes
             FROM task_definition
             ORDER BY priority, name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TaskRecord {
                task_id: row.get("task_id"),
                name: row.get("name"),
                kind: row.get("kind"),
                kingdom_id: row.get("kingdom_id"),
                profile_id: row.get("profile_id"),
                target_level_min: row.get("target_level_min"),
                target_level_max: row.get("target_level_max"),
                commander_count: row.get("commander_count"),
                max_active: row.get("max_active"),
                priority: row.get("priority"),
                enabled: row.get::<i64, _>("enabled") != 0,
                tags: decode_tags(row.get("tags")),
                notes: row.get("notes"),
            })
            .collect())
    }

    /// Replaces a task's subscriptions wholesale, so the UI can edit them as a list.
    pub async fn replace_subscriptions(
        &self,
        task_id: &str,
        subscriptions: &[SubscriptionRecord],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM task_subscription WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
        for (position, subscription) in subscriptions.iter().enumerate() {
            sqlx::query(
                "INSERT INTO task_subscription (task_id, target_kind, filter_json, position)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(task_id)
            .bind(&subscription.target_kind)
            .bind(subscription.filter.to_string())
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_task(&self, task_id: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM task_subscription WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM task_definition WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn subscriptions(&self) -> Result<Vec<SubscriptionRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT task_id, target_kind, filter_json
             FROM task_subscription ORDER BY task_id, position",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let filter = serde_json::from_str(row.get::<&str, _>("filter_json"))
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                Ok(SubscriptionRecord {
                    task_id: row.get("task_id"),
                    target_kind: row.get("target_kind"),
                    filter,
                })
            })
            .collect()
    }
}
