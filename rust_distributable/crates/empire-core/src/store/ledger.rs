//! The march ledger and commander availability.
//!
//! This is the state that lets a run resume exactly where it stopped: which
//! commanders are out, when they are due back, and which marches have not yet
//! reported a result.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::Store;

/// Commander availability states, matching the vocabulary the Python bot used.
pub const COMMANDER_AVAILABLE: &str = "available";
pub const COMMANDER_OUTBOUND: &str = "outbound";

/// March states. `sent` means the server acknowledged with a march id but the
/// troops have not landed; `returning` means they are on the way back.
pub const MARCH_SENT: &str = "sent";
pub const MARCH_RETURNING: &str = "returning";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarchRecord {
    pub account_id: String,
    pub march_id: i64,
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub level: Option<i64>,
    #[serde(default)]
    pub lord_id: Option<i64>,
    #[serde(default)]
    pub commander_number: Option<i64>,
    #[serde(default)]
    pub troop_count: Option<i64>,
    #[serde(default)]
    pub duration_s: Option<i64>,
    #[serde(default)]
    pub coin_loot: Option<i64>,
    #[serde(default)]
    pub ruby_loot: Option<i64>,
    pub status: String,
    #[serde(default)]
    pub result_flag: Option<i64>,
    #[serde(default)]
    pub error_message: Option<String>,
    pub sent_at_ms: i64,
    #[serde(default)]
    pub landed_at_ms: Option<i64>,
    #[serde(default)]
    pub result_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommanderState {
    pub account_id: String,
    pub lord_id: i64,
    pub status: String,
    /// Epoch milliseconds. Zero means "ready now".
    pub available_after_ms: i64,
    #[serde(default)]
    pub march_id: Option<i64>,
    #[serde(default)]
    pub target_key: Option<String>,
}

impl Store {
    pub async fn record_march(&self, march: &MarchRecord) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO attack_ledger (
                account_id, march_id, kingdom_id, x, y, task_id, profile_id, level,
                lord_id, commander_number, troop_count, duration_s, coin_loot,
                ruby_loot, status, result_flag, error_message, sent_at_ms,
                landed_at_ms, result_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, march_id) DO UPDATE SET
                status = excluded.status,
                duration_s = excluded.duration_s,
                coin_loot = excluded.coin_loot,
                ruby_loot = excluded.ruby_loot,
                result_flag = excluded.result_flag,
                error_message = excluded.error_message,
                landed_at_ms = excluded.landed_at_ms,
                result_at_ms = excluded.result_at_ms",
        )
        .bind(&march.account_id)
        .bind(march.march_id)
        .bind(march.kingdom_id)
        .bind(march.x)
        .bind(march.y)
        .bind(march.task_id.as_deref())
        .bind(march.profile_id.as_deref())
        .bind(march.level)
        .bind(march.lord_id)
        .bind(march.commander_number)
        .bind(march.troop_count)
        .bind(march.duration_s)
        .bind(march.coin_loot)
        .bind(march.ruby_loot)
        .bind(&march.status)
        .bind(march.result_flag)
        .bind(march.error_message.as_deref())
        .bind(march.sent_at_ms)
        .bind(march.landed_at_ms)
        .bind(march.result_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Records the outcome of a march that has come back.
    /// The parameter list mirrors the ledger columns on purpose.
    #[allow(clippy::too_many_arguments)]
    pub async fn finish_march(
        &self,
        account_id: &str,
        march_id: i64,
        status: &str,
        result_flag: Option<i64>,
        coin_loot: Option<i64>,
        ruby_loot: Option<i64>,
        result_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE attack_ledger
             SET status = ?, result_flag = ?, coin_loot = COALESCE(?, coin_loot),
                 ruby_loot = COALESCE(?, ruby_loot), result_at_ms = ?
             WHERE account_id = ? AND march_id = ?",
        )
        .bind(status)
        .bind(result_flag)
        .bind(coin_loot)
        .bind(ruby_loot)
        .bind(result_at_ms)
        .bind(account_id)
        .bind(march_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Attribute a return to the march that produced it. Returns the rows updated.
    ///
    /// The `cat` return is a *distinct movement* whose `MID` differs from the
    /// `cra` acknowledgement's, so march id cannot be used to match them. Match on
    /// the attacked position plus the commander, newest in-flight march first —
    /// the same strategy as `bot/bot.py::mark_rbc_result`.
    ///
    /// A zero return means no in-flight march matched, which is the caller's cue
    /// to write a standalone row rather than drop the loot.
    #[allow(clippy::too_many_arguments)]
    pub async fn finish_march_by_target(
        &self,
        account_id: &str,
        kingdom_id: i64,
        x: i64,
        y: i64,
        lord_id: i64,
        return_seconds: Option<i64>,
        coin_loot: Option<i64>,
        ruby_loot: Option<i64>,
        result_flag: Option<i64>,
        result_at_ms: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE attack_ledger
             SET status = ?, landed_at_ms = ?, result_at_ms = ?,
                 duration_s = COALESCE(?, duration_s),
                 coin_loot = COALESCE(?, coin_loot),
                 ruby_loot = COALESCE(?, ruby_loot),
                 result_flag = COALESCE(?, result_flag)
             WHERE account_id = ? AND march_id = (
                 SELECT march_id FROM attack_ledger
                 WHERE account_id = ? AND kingdom_id = ? AND x = ? AND y = ?
                   AND status = ? AND (lord_id = ? OR lord_id IS NULL)
                   AND sent_at_ms <= ?
                 ORDER BY sent_at_ms DESC
                 LIMIT 1
             )",
        )
        .bind(MARCH_RETURNING)
        .bind(result_at_ms)
        .bind(result_at_ms)
        .bind(return_seconds)
        .bind(coin_loot)
        .bind(ruby_loot)
        .bind(result_flag)
        .bind(account_id)
        .bind(account_id)
        .bind(kingdom_id)
        .bind(x)
        .bind(y)
        .bind(MARCH_SENT)
        .bind(lord_id)
        .bind(result_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn recent_marches(
        &self,
        account_id: &str,
        limit: i64,
    ) -> Result<Vec<MarchRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, march_id, kingdom_id, x, y, task_id, profile_id, level,
                    lord_id, commander_number, troop_count, duration_s, coin_loot,
                    ruby_loot, status, result_flag, error_message, sent_at_ms,
                    landed_at_ms, result_at_ms
             FROM attack_ledger WHERE account_id = ?
             ORDER BY sent_at_ms DESC LIMIT ?",
        )
        .bind(account_id)
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_march).collect())
    }

    /// Marches with no result yet — what a resumed run has to wait on.
    pub async fn open_marches(&self) -> Result<Vec<MarchRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, march_id, kingdom_id, x, y, task_id, profile_id, level,
                    lord_id, commander_number, troop_count, duration_s, coin_loot,
                    ruby_loot, status, result_flag, error_message, sent_at_ms,
                    landed_at_ms, result_at_ms
             FROM attack_ledger WHERE status IN (?, ?)
             ORDER BY sent_at_ms",
        )
        .bind(MARCH_SENT)
        .bind(MARCH_RETURNING)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_march).collect())
    }

    pub async fn set_commander_state(&self, state: &CommanderState, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO commander_state (
                account_id, lord_id, status, available_after_ms, march_id, target_key, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, lord_id) DO UPDATE SET
                status = excluded.status,
                available_after_ms = excluded.available_after_ms,
                march_id = excluded.march_id,
                target_key = excluded.target_key,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&state.account_id)
        .bind(state.lord_id)
        .bind(&state.status)
        .bind(state.available_after_ms)
        .bind(state.march_id)
        .bind(state.target_key.as_deref())
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn commander_states(&self, account_id: &str) -> Result<Vec<CommanderState>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, lord_id, status, available_after_ms, march_id, target_key
             FROM commander_state WHERE account_id = ? ORDER BY lord_id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| CommanderState {
                account_id: row.get("account_id"),
                lord_id: row.get("lord_id"),
                status: row.get("status"),
                available_after_ms: row.get("available_after_ms"),
                march_id: row.get("march_id"),
                target_key: row.get("target_key"),
            })
            .collect())
    }

    /// Commander LIDs in roster order — the human number -> LID map.
    ///
    /// This replaces the Python module-level global that had to be rebound per
    /// account; here it is simply a query.
    pub async fn commander_lids(&self, account_id: &str) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT lord_id FROM account_commander WHERE account_id = ? ORDER BY ordinal",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.get("lord_id")).collect())
    }
}

/// A march that has not come back within this long is treated as lost rather
/// than in flight, so a crash mid-run does not leave the count climbing forever.
pub const STALE_MARCH_MILLIS: i64 = 60 * 60 * 1_000;

/// How recently a run must have checked in for it to count as alive.
pub const HEARTBEAT_FRESH_MILLIS: i64 = 90_000;

/// Key the hunter writes to show it is still running.
pub const HUNT_HEARTBEAT_KEY: &str = "hunt.heartbeat_ms";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Totals for one task, so a run can be read at a glance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HuntTaskSummary {
    pub task_id: String,
    pub marches: i64,
    pub returned: i64,
    pub coins: i64,
    pub rubies: i64,
    /// Target band and commander allocation, when the run published a plan.
    #[serde(default)]
    pub kingdom_id: Option<i64>,
    #[serde(default)]
    pub level_min: Option<i64>,
    #[serde(default)]
    pub level_max: Option<i64>,
    #[serde(default)]
    pub commanders: Vec<i64>,
    /// True when the task belongs to the run that is current, rather than history.
    pub planned: bool,
}

/// One task exactly as a run declared it, before any march exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HuntPlannedTask {
    pub task_id: String,
    #[serde(default)]
    pub kingdom_id: Option<i64>,
    #[serde(default)]
    pub level_min: Option<i64>,
    #[serde(default)]
    pub level_max: Option<i64>,
    #[serde(default)]
    pub commanders: Vec<i64>,
}

/// A whole run, summarised for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HuntSummary {
    /// The operator's name for the run, as passed with `--label`.
    pub label: Option<String>,
    pub account_id: Option<String>,
    pub marches: i64,
    pub returned: i64,
    pub in_flight: i64,
    pub coins: i64,
    pub rubies: i64,
    /// Whether a run has checked in recently. Without it, marches left over from
    /// a previous session would be reported as still flying when nothing is
    /// running at all.
    pub active: bool,
    pub tasks: Vec<HuntTaskSummary>,
    /// Most recent marches, newest first.
    pub recent: Vec<MarchRecord>,
}

impl Store {
    /// Aggregate the ledger into something a UI can render directly.
    ///
    /// Kept in the store rather than the daemon so the query and the shape it
    /// produces live next to the table they read.
    pub async fn hunt_summary(&self, recent_limit: i64) -> Result<HuntSummary, sqlx::Error> {
        let totals = sqlx::query(
            "SELECT COUNT(*) marches,
                    COALESCE(SUM(status = ?), 0) returned,
                    COALESCE(SUM(status = ? AND sent_at_ms > ?), 0) in_flight,
                    COALESCE(SUM(coin_loot), 0) coins,
                    COALESCE(SUM(ruby_loot), 0) rubies
             FROM attack_ledger",
        )
        .bind(MARCH_RETURNING)
        .bind(MARCH_SENT)
        .bind(now_ms() - STALE_MARCH_MILLIS)
        .fetch_one(&self.pool)
        .await?;
        let task_rows = sqlx::query(
            "SELECT COALESCE(task_id, '(earlier runs)') task_id,
                    COUNT(*) marches,
                    COALESCE(SUM(status = ?), 0) returned,
                    COALESCE(SUM(coin_loot), 0) coins,
                    COALESCE(SUM(ruby_loot), 0) rubies
             FROM attack_ledger
             GROUP BY task_id
             ORDER BY marches DESC",
        )
        .bind(MARCH_RETURNING)
        .fetch_all(&self.pool)
        .await?;

        // Only a run that has checked in recently can have marches in the air.
        let now = now_ms();
        let active = self
            .app_state(HUNT_HEARTBEAT_KEY)
            .await?
            .and_then(|value| value.as_i64())
            .is_some_and(|beat| beat > now - HEARTBEAT_FRESH_MILLIS);

        let account_row = sqlx::query(
            "SELECT account_id FROM attack_ledger
             ORDER BY sent_at_ms DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(HuntSummary {
            label: self
                .app_state("hunt.label")
                .await?
                .and_then(|value| value.as_str().map(str::to_owned)),
            account_id: account_row.map(|row| row.get("account_id")),
            marches: totals.get("marches"),
            returned: totals.get("returned"),
            in_flight: if active { totals.get("in_flight") } else { 0 },
            coins: totals.get("coins"),
            rubies: totals.get("rubies"),
            active,
            tasks: merge_plan_with_ledger(
                self.app_state("hunt.plan").await?,
                task_rows,
            ),
            recent: self.recent_marches_all(recent_limit).await?,
        })
    }

    /// The most recent marches across every account.
    pub async fn recent_marches_all(&self, limit: i64) -> Result<Vec<MarchRecord>, sqlx::Error> {        let rows = sqlx::query(
            "SELECT account_id, march_id, kingdom_id, x, y, task_id, profile_id, level,
                    lord_id, commander_number, troop_count, duration_s, coin_loot,
                    ruby_loot, status, result_flag, error_message, sent_at_ms,
                    landed_at_ms, result_at_ms
             FROM attack_ledger ORDER BY sent_at_ms DESC LIMIT ?",
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_march).collect())
    }
}

/// Fold the run's declared tasks together with what the ledger actually recorded.
///
/// A task that has been declared but has not been given a target yet still has to
/// appear, and rows written before tasks were labelled need somewhere to go.
fn merge_plan_with_ledger(
    plan: Option<serde_json::Value>,
    ledger_rows: Vec<sqlx::sqlite::SqliteRow>,
) -> Vec<HuntTaskSummary> {
    let planned: Vec<HuntPlannedTask> = plan
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();

    let recorded: Vec<HuntTaskSummary> = ledger_rows
        .into_iter()
        .map(|row| HuntTaskSummary {
            task_id: row.get("task_id"),
            marches: row.get("marches"),
            returned: row.get("returned"),
            coins: row.get("coins"),
            rubies: row.get("rubies"),
            kingdom_id: None,
            level_min: None,
            level_max: None,
            commanders: Vec::new(),
            planned: false,
        })
        .collect();

    let mut tasks: Vec<HuntTaskSummary> = planned
        .into_iter()
        .map(|task| {
            let totals = recorded.iter().find(|row| row.task_id == task.task_id);
            HuntTaskSummary {
                marches: totals.map_or(0, |row| row.marches),
                returned: totals.map_or(0, |row| row.returned),
                coins: totals.map_or(0, |row| row.coins),
                rubies: totals.map_or(0, |row| row.rubies),
                task_id: task.task_id,
                kingdom_id: task.kingdom_id,
                level_min: task.level_min,
                level_max: task.level_max,
                commanders: task.commanders,
                planned: true,
            }
        })
        .collect();

    let planned_ids: std::collections::HashSet<String> =
        tasks.iter().map(|task| task.task_id.clone()).collect();
    tasks.extend(
        recorded
            .into_iter()
            .filter(|row| !planned_ids.contains(&row.task_id)),
    );
    tasks
}

fn row_to_march(row: sqlx::sqlite::SqliteRow) -> MarchRecord {
    MarchRecord {
        account_id: row.get("account_id"),
        march_id: row.get("march_id"),
        kingdom_id: row.get("kingdom_id"),
        x: row.get("x"),
        y: row.get("y"),
        task_id: row.get("task_id"),
        profile_id: row.get("profile_id"),
        level: row.get("level"),
        lord_id: row.get("lord_id"),
        commander_number: row.get("commander_number"),
        troop_count: row.get("troop_count"),
        duration_s: row.get("duration_s"),
        coin_loot: row.get("coin_loot"),
        ruby_loot: row.get("ruby_loot"),
        status: row.get("status"),
        result_flag: row.get("result_flag"),
        error_message: row.get("error_message"),
        sent_at_ms: row.get("sent_at_ms"),
        landed_at_ms: row.get("landed_at_ms"),
        result_at_ms: row.get("result_at_ms"),
    }
}
