//! Where the client currently is, and per-castle inventory/recruitment state.
//!
//! Navigation is persisted because the automation must know whether the client
//! is on the map or inside a castle before it sends anything: map mode is where
//! targets are attacked from, castle mode is where recruitment happens.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::Store;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationState {
    pub account_id: String,
    #[serde(default)]
    pub current_kingdom_id: Option<i64>,
    #[serde(default)]
    pub current_castle_id: Option<i64>,
    /// True while the client is on the map, which is the attack context.
    pub map_mode: bool,
    /// True while the recruitment screen is open.
    pub recruit_page: bool,
    pub last_castle_switch_at_ms: i64,
}

impl NavigationState {
    pub fn new(account_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            current_kingdom_id: None,
            current_castle_id: None,
            map_mode: false,
            recruit_page: false,
            last_castle_switch_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CastleUnit {
    pub account_id: String,
    pub castle_id: i64,
    pub unit_id: i64,
    pub quantity: i64,
    #[serde(default)]
    pub source_command: Option<String>,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecruitCastleState {
    pub account_id: String,
    pub castle_id: i64,
    #[serde(default)]
    pub task_id: Option<String>,
    pub queue_clear_at_ms: i64,
    pub last_duration_s: i64,
    pub last_request_at_ms: i64,
    pub active_quantity: i64,
    pub queued_quantity: i64,
    pub help_active: bool,
    pub last_status: String,
}

impl RecruitCastleState {
    pub fn idle(account_id: impl Into<String>, castle_id: i64) -> Self {
        Self {
            account_id: account_id.into(),
            castle_id,
            task_id: None,
            queue_clear_at_ms: 0,
            last_duration_s: 0,
            last_request_at_ms: 0,
            active_quantity: 0,
            queued_quantity: 0,
            help_active: false,
            last_status: "idle".to_owned(),
        }
    }
}

impl Store {
    pub async fn navigation(
        &self,
        account_id: &str,
    ) -> Result<Option<NavigationState>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT account_id, current_kingdom_id, current_castle_id, map_mode,
                    recruit_page, last_castle_switch_at_ms
             FROM account_navigation WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| NavigationState {
            account_id: row.get("account_id"),
            current_kingdom_id: row.get("current_kingdom_id"),
            current_castle_id: row.get("current_castle_id"),
            map_mode: row.get::<i64, _>("map_mode") != 0,
            recruit_page: row.get::<i64, _>("recruit_page") != 0,
            last_castle_switch_at_ms: row.get("last_castle_switch_at_ms"),
        }))
    }

    pub async fn set_navigation(
        &self,
        state: &NavigationState,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO account_navigation (
                account_id, current_kingdom_id, current_castle_id, map_mode,
                recruit_page, last_castle_switch_at_ms, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                current_kingdom_id = excluded.current_kingdom_id,
                current_castle_id = excluded.current_castle_id,
                map_mode = excluded.map_mode,
                recruit_page = excluded.recruit_page,
                last_castle_switch_at_ms = excluded.last_castle_switch_at_ms,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&state.account_id)
        .bind(state.current_kingdom_id)
        .bind(state.current_castle_id)
        .bind(i64::from(state.map_mode))
        .bind(i64::from(state.recruit_page))
        .bind(state.last_castle_switch_at_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_castle_units(&self, units: &[CastleUnit]) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for unit in units {
            sqlx::query(
                "INSERT INTO account_castle_unit (
                    account_id, castle_id, unit_id, quantity, source_command, observed_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, castle_id, unit_id) DO UPDATE SET
                    quantity = excluded.quantity,
                    source_command = excluded.source_command,
                    observed_at_ms = excluded.observed_at_ms",
            )
            .bind(&unit.account_id)
            .bind(unit.castle_id)
            .bind(unit.unit_id)
            .bind(unit.quantity)
            .bind(unit.source_command.as_deref())
            .bind(unit.observed_at_ms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Troop and tool counts last seen in one castle.
    pub async fn castle_units(
        &self,
        account_id: &str,
        castle_id: i64,
    ) -> Result<Vec<CastleUnit>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, castle_id, unit_id, quantity, source_command, observed_at_ms
             FROM account_castle_unit WHERE account_id = ? AND castle_id = ?
             ORDER BY unit_id",
        )
        .bind(account_id)
        .bind(castle_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| CastleUnit {
                account_id: row.get("account_id"),
                castle_id: row.get("castle_id"),
                unit_id: row.get("unit_id"),
                quantity: row.get("quantity"),
                source_command: row.get("source_command"),
                observed_at_ms: row.get("observed_at_ms"),
            })
            .collect())
    }

    pub async fn recruit_states(
        &self,
        account_id: &str,
    ) -> Result<Vec<RecruitCastleState>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, castle_id, task_id, queue_clear_at_ms, last_duration_s,
                    last_request_at_ms, active_quantity, queued_quantity, help_active,
                    last_status
             FROM recruit_castle_state WHERE account_id = ? ORDER BY castle_id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_recruit_state).collect())
    }

    pub async fn set_recruit_state(
        &self,
        state: &RecruitCastleState,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO recruit_castle_state (
                account_id, castle_id, task_id, queue_clear_at_ms, last_duration_s,
                last_request_at_ms, active_quantity, queued_quantity, help_active,
                last_status, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, castle_id) DO UPDATE SET
                task_id = excluded.task_id,
                queue_clear_at_ms = excluded.queue_clear_at_ms,
                last_duration_s = excluded.last_duration_s,
                last_request_at_ms = excluded.last_request_at_ms,
                active_quantity = excluded.active_quantity,
                queued_quantity = excluded.queued_quantity,
                help_active = excluded.help_active,
                last_status = excluded.last_status,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&state.account_id)
        .bind(state.castle_id)
        .bind(state.task_id.as_deref())
        .bind(state.queue_clear_at_ms)
        .bind(state.last_duration_s)
        .bind(state.last_request_at_ms)
        .bind(state.active_quantity)
        .bind(state.queued_quantity)
        .bind(i64::from(state.help_active))
        .bind(&state.last_status)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn row_to_recruit_state(row: sqlx::sqlite::SqliteRow) -> RecruitCastleState {
    RecruitCastleState {
        account_id: row.get("account_id"),
        castle_id: row.get("castle_id"),
        task_id: row.get("task_id"),
        queue_clear_at_ms: row.get("queue_clear_at_ms"),
        last_duration_s: row.get("last_duration_s"),
        last_request_at_ms: row.get("last_request_at_ms"),
        active_quantity: row.get("active_quantity"),
        queued_quantity: row.get("queued_quantity"),
        help_active: row.get::<i64, _>("help_active") != 0,
        last_status: row.get("last_status"),
    }
}
