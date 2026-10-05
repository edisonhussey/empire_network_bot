//! The march ledger and commander availability.
//!
//! This is the state that lets a run resume exactly where it stopped: which
//! commanders are out, when they are due back, and which marches have not yet
//! reported a result.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::{Store, canonical_account_id};

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
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
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
        .bind(&account_id)
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
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
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
        .bind(&account_id)
        .bind(&account_id)
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
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query(
            "SELECT account_id, march_id, kingdom_id, x, y, task_id, profile_id, level,
                    lord_id, commander_number, troop_count, duration_s, coin_loot,
                    ruby_loot, status, result_flag, error_message, sent_at_ms,
                    landed_at_ms, result_at_ms
             FROM attack_ledger WHERE account_id = ?
             ORDER BY sent_at_ms DESC LIMIT ?",
        )
        .bind(&account_id)
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

    pub async fn set_commander_state(
        &self,
        state: &CommanderState,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
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

    pub async fn commander_states(
        &self,
        account_id: &str,
    ) -> Result<Vec<CommanderState>, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query(
            "SELECT account_id, lord_id, status, available_after_ms, march_id, target_key
             FROM commander_state WHERE account_id = ? ORDER BY lord_id",
        )
        .bind(&account_id)
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
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query(
            "SELECT lord_id FROM account_commander WHERE lower(account_id) = lower(?)
             GROUP BY lord_id ORDER BY MIN(ordinal)",
        )
        .bind(&account_id)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardPoint {
    pub at_ms: i64,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanActivity {
    pub at_ms: i64,
    pub kingdom_id: i64,
    pub windows: i64,
}

/// Minute-cached dashboard aggregates. These queries are intentionally separate
/// from the live connection status so UI polling cannot make ledger work hot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub generated_at_ms: i64,
    pub attacks_last_hour: i64,
    pub returns_last_hour: i64,
    pub rubies_last_hour: i64,
    pub coins_last_hour: i64,
    pub ruby_series: Vec<DashboardPoint>,
    pub scan_activity: Vec<ScanActivity>,
    /// When the next fortress opens, if any has been observed. Fortresses are the
    /// one target the server gives a cooldown for, so this is a countdown to a
    /// real time rather than an estimate.
    pub fortress_next_available_at_ms: Option<i64>,
    pub fortress_count: i64,
    /// Discovery probes still outstanding. The map walk is long, so the dashboard
    /// can say it is still looking instead of implying there are none.
    pub fortress_probes_pending: i64,
}

impl Store {
    pub async fn dashboard_summary(&self) -> Result<DashboardSummary, sqlx::Error> {
        self.dashboard_summary_for(None).await
    }

    pub async fn dashboard_summary_for(
        &self,
        account_id: Option<&str>,
    ) -> Result<DashboardSummary, sqlx::Error> {
        const HOUR_MS: i64 = 60 * 60 * 1_000;
        const DAY_MS: i64 = 24 * HOUR_MS;
        let now = now_ms();
        let hour_ago = now - HOUR_MS;
        let account_id = account_id.map(canonical_account_id);

        let hourly = sqlx::query(
            "SELECT
                COALESCE(SUM(sent_at_ms >= ?), 0) attacks,
                COALESCE(SUM(result_at_ms >= ?), 0) returns,
                COALESCE(SUM(CASE WHEN result_at_ms >= ? THEN ruby_loot ELSE 0 END), 0) rubies,
                COALESCE(SUM(CASE WHEN result_at_ms >= ? THEN coin_loot ELSE 0 END), 0) coins
             FROM attack_ledger
             WHERE (? IS NULL OR account_id = ?)",
        )
        .bind(hour_ago)
        .bind(hour_ago)
        .bind(hour_ago)
        .bind(hour_ago)
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_one(&self.pool)
        .await?;

        let first_at: Option<i64> = sqlx::query_scalar(
            "SELECT MIN(COALESCE(result_at_ms, sent_at_ms)) FROM attack_ledger
             WHERE (? IS NULL OR account_id = ?)",
        )
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_one(&self.pool)
        .await?;
        let span = first_at.map_or(0, |first| now.saturating_sub(first));
        let bucket_ms = if span > 365 * DAY_MS {
            7 * DAY_MS
        } else if span > 30 * DAY_MS {
            DAY_MS
        } else if span > 2 * DAY_MS {
            HOUR_MS
        } else {
            15 * 60 * 1_000
        };
        let lifetime_start = first_at
            .unwrap_or(now)
            .div_euclid(bucket_ms)
            .saturating_mul(bucket_ms);
        let ruby_rows = sqlx::query(
            "SELECT (COALESCE(result_at_ms, sent_at_ms) / ?) * ? bucket,
                    COALESCE(SUM(ruby_loot), 0) rubies
             FROM attack_ledger
             WHERE (? IS NULL OR account_id = ?)
             GROUP BY bucket ORDER BY bucket",
        )
        .bind(bucket_ms)
        .bind(bucket_ms)
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_all(&self.pool)
        .await?;
        let bucket_values = ruby_rows
            .into_iter()
            .map(|row| (row.get::<i64, _>("bucket"), row.get::<i64, _>("rubies")))
            .collect::<std::collections::HashMap<_, _>>();
        let mut cumulative = 0;
        let capacity = usize::try_from(span.div_euclid(bucket_ms).saturating_add(2)).unwrap_or(2);
        let mut ruby_series = Vec::with_capacity(capacity);
        let mut bucket = lifetime_start;
        while bucket <= now {
            cumulative += bucket_values.get(&bucket).copied().unwrap_or(0);
            ruby_series.push(DashboardPoint {
                at_ms: bucket,
                value: cumulative,
            });
            bucket += bucket_ms;
        }

        let scan_activity = sqlx::query(
            "SELECT (scanned_at_ms / 60000) * 60000 bucket, kingdom_id,
                    COUNT(DISTINCT printf('%d:%d:%d:%d', ax1, ay1, ax2, ay2)) windows
             FROM map_scan_window
             WHERE scanned_at_ms >= ? AND (? IS NULL OR account_id = ?)
             GROUP BY bucket, kingdom_id
             ORDER BY bucket DESC LIMIT 30",
        )
        .bind(now - DAY_MS)
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| ScanActivity {
            at_ms: row.get("bucket"),
            kingdom_id: row.get("kingdom_id"),
            windows: row.get("windows"),
        })
        .collect();

        // Fortress cooldowns come from the server, so the dashboard can count
        // down to the next window instead of estimating one. A kingdom with no
        // observation yet reports nothing rather than zero.
        //
        // Scoped to kingdoms that actually have a fortress task: the runner can
        // only walk and attack tasked kingdoms, so an observation elsewhere (an
        // Ice fortress while only Sands is tasked) must never be presented as
        // the next target.
        let fortress = sqlx::query(
            "SELECT COUNT(*) observed, MIN(available_at_ms) soonest
             FROM fortress_target
             WHERE (? IS NULL OR account_id = ?)
               AND kingdom_id IN (SELECT t.kingdom_id FROM task_definition t
                                  JOIN task_subscription s ON s.task_id = t.task_id
                                  WHERE s.target_kind = 'fortress' AND t.enabled = 1)",
        )
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_one(&self.pool)
        .await?;
        let fortress_probes_pending: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(scan.blocks_total - scan.blocks_done), 0)
             FROM fortress_scan_state scan
             WHERE (? IS NULL OR scan.account_id = ?)
               AND scan.kingdom_id IN (SELECT t.kingdom_id FROM task_definition t
                                  JOIN task_subscription s ON s.task_id = t.task_id
                                  WHERE s.target_kind = 'fortress' AND t.enabled = 1)",
        )
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_one(&self.pool)
        .await?;

        Ok(DashboardSummary {
            generated_at_ms: now,
            attacks_last_hour: hourly.get("attacks"),
            returns_last_hour: hourly.get("returns"),
            rubies_last_hour: hourly.get("rubies"),
            coins_last_hour: hourly.get("coins"),
            ruby_series,
            scan_activity,
            fortress_next_available_at_ms: fortress.get("soonest"),
            fortress_count: fortress.get("observed"),
            fortress_probes_pending,
        })
    }

    /// Aggregate the ledger into something a UI can render directly.
    ///
    /// Kept in the store rather than the daemon so the query and the shape it
    /// produces live next to the table they read.
    pub async fn hunt_summary(&self, recent_limit: i64) -> Result<HuntSummary, sqlx::Error> {
        self.hunt_summary_for(None, recent_limit).await
    }

    pub async fn hunt_summary_for(
        &self,
        account_id: Option<&str>,
        recent_limit: i64,
    ) -> Result<HuntSummary, sqlx::Error> {
        let account_id = account_id.map(canonical_account_id);
        let totals = sqlx::query(
            "SELECT COUNT(*) marches,
                    COALESCE(SUM(status = ?), 0) returned,
                    COALESCE(SUM(status = ? AND sent_at_ms > ?), 0) in_flight,
                    COALESCE(SUM(coin_loot), 0) coins,
                    COALESCE(SUM(ruby_loot), 0) rubies
             FROM attack_ledger
             WHERE (? IS NULL OR account_id = ?)",
        )
        .bind(MARCH_RETURNING)
        .bind(MARCH_SENT)
        .bind(now_ms() - STALE_MARCH_MILLIS)
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_one(&self.pool)
        .await?;
        let task_rows = sqlx::query(
            "SELECT COALESCE(task_id, '(earlier runs)') task_id,
                    COUNT(*) marches,
                    COALESCE(SUM(status = ?), 0) returned,
                    COALESCE(SUM(coin_loot), 0) coins,
                    COALESCE(SUM(ruby_loot), 0) rubies
             FROM attack_ledger
             WHERE (? IS NULL OR account_id = ?)
             GROUP BY task_id
             ORDER BY marches DESC",
        )
        .bind(MARCH_RETURNING)
        .bind(account_id.as_deref())
        .bind(account_id.as_deref())
        .fetch_all(&self.pool)
        .await?;

        // Only a run that has checked in recently can have marches in the air.
        let now = now_ms();
        let active = self
            .app_state(HUNT_HEARTBEAT_KEY)
            .await?
            .and_then(|value| value.as_i64())
            .is_some_and(|beat| beat > now - HEARTBEAT_FRESH_MILLIS);

        let account_row = if account_id.is_none() {
            sqlx::query("SELECT account_id FROM attack_ledger ORDER BY sent_at_ms DESC LIMIT 1")
                .fetch_optional(&self.pool)
                .await?
        } else {
            None
        };

        Ok(HuntSummary {
            label: self
                .app_state("hunt.label")
                .await?
                .and_then(|value| value.as_str().map(str::to_owned)),
            account_id: account_id
                .clone()
                .or_else(|| account_row.map(|row| row.get("account_id"))),
            marches: totals.get("marches"),
            returned: totals.get("returned"),
            in_flight: if active { totals.get("in_flight") } else { 0 },
            coins: totals.get("coins"),
            rubies: totals.get("rubies"),
            active,
            tasks: merge_plan_with_ledger(self.app_state("hunt.plan").await?, task_rows),
            recent: if let Some(account_id) = account_id.as_deref() {
                self.recent_marches(account_id, recent_limit).await?
            } else {
                self.recent_marches_all(recent_limit).await?
            },
        })
    }

    /// The most recent marches across every account.
    pub async fn recent_marches_all(&self, limit: i64) -> Result<Vec<MarchRecord>, sqlx::Error> {
        let rows = sqlx::query(
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
