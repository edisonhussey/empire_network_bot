//! Durable, bounded telemetry: troop stock, events and march outcomes.
//!
//! The raw packet log keeps only the latest 200 rows, which is right for
//! debugging and wrong for history: the number that explained a failed run (the
//! castle's crossbow stock) lived only in packets that were pruned within
//! minutes. Everything here is keyed by `account_id`, kept small by design, and
//! documented in `docs/architecture/database.md`.
//!
//! | Data | Granularity | Kept |
//! | --- | --- | --- |
//! | `castle_stock_sample` | one row per account/castle/unit per minute at most | 72 hours |
//! | `castle_stock_hourly` | min / max / last per hour | forever |
//! | `event_log` | one row per important event | 90 days |
//! | `event_hourly` | a counter per kind per hour | forever |

use std::collections::BTreeMap;

use sqlx::Row;

use super::{Store, canonical_account_id};

/// Raw stock samples are at most this far apart per account/castle/unit.
pub const STOCK_SAMPLE_INTERVAL_MS: i64 = 60_000;
/// How long raw stock samples are kept; the hourly summary outlives them.
pub const STOCK_SAMPLE_RETENTION_MS: i64 = 72 * 3_600_000;
/// How long individual events are kept; the hourly counters outlive them.
pub const EVENT_RETENTION_MS: i64 = 90 * 24 * 3_600_000;
const HOUR_MS: i64 = 3_600_000;

/// Net drain of a unit at a castle, measured from recent samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StockForecast {
    pub home: i64,
    pub out: i64,
    /// Units lost per hour, net of recruiting. Positive means shrinking.
    pub drain_per_hour: f64,
    /// Hours until the total reaches zero at the current drain, when shrinking.
    pub hours_left: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockSample {
    pub castle_id: i64,
    pub unit_id: i64,
    pub observed_at_ms: i64,
    pub home: i64,
    pub out: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub at_ms: i64,
    pub kind: String,
    pub detail: String,
}

impl Store {
    /// Record an important, rare event (connection, stop, refusal).
    pub async fn record_event(
        &self,
        account_id: &str,
        kind: &str,
        detail: &str,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query("INSERT INTO event_log (account_id, at_ms, kind, detail) VALUES (?, ?, ?, ?)")
            .bind(&account_id)
            .bind(now_ms)
            .bind(kind)
            .bind(detail)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM event_log WHERE account_id = ? AND at_ms < ?")
            .bind(&account_id)
            .bind(now_ms - EVENT_RETENTION_MS)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Count a routine, possibly frequent event. One row per kind per hour, so
    /// a retry loop cannot grow the database.
    pub async fn bump_event(
        &self,
        account_id: &str,
        kind: &str,
        detail: &str,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "INSERT INTO event_hourly (account_id, hour_ms, kind, count, first_at_ms, last_at_ms, last_detail)
             VALUES (?, ?, ?, 1, ?, ?, ?)
             ON CONFLICT(account_id, hour_ms, kind) DO UPDATE SET
                count = count + 1,
                last_at_ms = excluded.last_at_ms,
                last_detail = excluded.last_detail",
        )
        .bind(&account_id)
        .bind(now_ms - now_ms.rem_euclid(HOUR_MS))
        .bind(kind)
        .bind(now_ms)
        .bind(now_ms)
        .bind(detail)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Newest-first important events for one account.
    pub async fn recent_events(
        &self,
        account_id: &str,
        limit: i64,
    ) -> Result<Vec<EventRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT at_ms, kind, detail FROM event_log
             WHERE account_id = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(limit.clamp(1, 1_000))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| EventRecord {
                at_ms: row.get("at_ms"),
                kind: row.get("kind"),
                detail: row.get("detail"),
            })
            .collect())
    }

    /// Newest-first raw stock samples for one account.
    pub async fn recent_stock_samples(
        &self,
        account_id: &str,
        limit: i64,
    ) -> Result<Vec<StockSample>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT castle_id, unit_id, observed_at_ms, home, out_count FROM castle_stock_sample
             WHERE account_id = ? ORDER BY observed_at_ms DESC LIMIT ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(limit.clamp(1, 10_000))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StockSample {
                castle_id: row.get("castle_id"),
                unit_id: row.get("unit_id"),
                observed_at_ms: row.get("observed_at_ms"),
                home: row.get("home"),
                out: row.get("out_count"),
            })
            .collect())
    }

    /// Store the home / out counts of the units an attack uses.
    ///
    /// `rows` are `(unit_id, home, out)`. The hourly summary is always updated;
    /// the raw row is skipped when one was written for the same unit in the last
    /// minute, which is what keeps a busy run from producing a row per attack.
    pub async fn record_stock_sample(
        &self,
        account_id: &str,
        castle_id: i64,
        rows: &[(i64, i64, i64)],
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        for (unit_id, home, out) in rows {
            let newest: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(observed_at_ms) FROM castle_stock_sample
                 WHERE account_id = ? AND castle_id = ? AND unit_id = ?",
            )
            .bind(&account_id)
            .bind(castle_id)
            .bind(unit_id)
            .fetch_one(&self.pool)
            .await?;
            if newest.is_none_or(|at| now_ms - at >= STOCK_SAMPLE_INTERVAL_MS) {
                sqlx::query(
                    "INSERT OR REPLACE INTO castle_stock_sample
                        (account_id, castle_id, unit_id, observed_at_ms, home, out_count)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&account_id)
                .bind(castle_id)
                .bind(unit_id)
                .bind(now_ms)
                .bind(home)
                .bind(out)
                .execute(&self.pool)
                .await?;
            }
            sqlx::query(
                "INSERT INTO castle_stock_hourly
                    (account_id, castle_id, unit_id, hour_ms, samples, min_home, max_home,
                     last_home, last_out, last_at_ms)
                 VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, castle_id, unit_id, hour_ms) DO UPDATE SET
                    samples = samples + 1,
                    min_home = MIN(min_home, excluded.min_home),
                    max_home = MAX(max_home, excluded.max_home),
                    last_home = excluded.last_home,
                    last_out = excluded.last_out,
                    last_at_ms = excluded.last_at_ms",
            )
            .bind(&account_id)
            .bind(castle_id)
            .bind(unit_id)
            .bind(now_ms - now_ms.rem_euclid(HOUR_MS))
            .bind(home)
            .bind(home)
            .bind(home)
            .bind(out)
            .bind(now_ms)
            .execute(&self.pool)
            .await?;
        }
        sqlx::query("DELETE FROM castle_stock_sample WHERE account_id = ? AND observed_at_ms < ?")
            .bind(&account_id)
            .bind(now_ms - STOCK_SAMPLE_RETENTION_MS)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Net drain of one unit over the last hour of samples, once they span at
    /// least ten minutes. Uses home plus out, so troops merely on a march do not
    /// look like a loss; only permanent losses (net of recruiting) do.
    pub async fn stock_forecast(
        &self,
        account_id: &str,
        castle_id: i64,
        unit_id: i64,
        now_ms: i64,
    ) -> Result<Option<StockForecast>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT observed_at_ms, home, out_count FROM castle_stock_sample
             WHERE account_id = ? AND castle_id = ? AND unit_id = ? AND observed_at_ms >= ?
             ORDER BY observed_at_ms",
        )
        .bind(canonical_account_id(account_id))
        .bind(castle_id)
        .bind(unit_id)
        .bind(now_ms - HOUR_MS)
        .fetch_all(&self.pool)
        .await?;
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
            return Ok(None);
        };
        let home: i64 = last.get("home");
        let out: i64 = last.get("out_count");
        let span_ms = last.get::<i64, _>("observed_at_ms") - first.get::<i64, _>("observed_at_ms");
        if span_ms < 10 * 60_000 {
            return Ok(Some(StockForecast {
                home,
                out,
                drain_per_hour: 0.0,
                hours_left: None,
            }));
        }
        let before = first.get::<i64, _>("home") + first.get::<i64, _>("out_count");
        let after = home + out;
        let drain_per_hour = (before - after) as f64 / (span_ms as f64 / HOUR_MS as f64);
        Ok(Some(StockForecast {
            home,
            out,
            drain_per_hour,
            hours_left: (drain_per_hour > 0.0).then(|| after as f64 / drain_per_hour),
        }))
    }

    /// What a march took with it, recorded when the attack is acknowledged so a
    /// later `cat` can work out the losses.
    pub async fn set_march_army(
        &self,
        account_id: &str,
        march_id: i64,
        army: &BTreeMap<i64, i64>,
    ) -> Result<(), sqlx::Error> {
        let army_json = serde_json::to_string(army).unwrap_or_else(|_| "{}".to_owned());
        sqlx::query(
            "UPDATE attack_ledger SET army_json = ?, troop_count = ?
             WHERE account_id = ? AND march_id = ?",
        )
        .bind(army_json)
        .bind(army.values().sum::<i64>())
        .bind(canonical_account_id(account_id))
        .bind(march_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Work out what a returned march lost. `returned` maps unit id to the count
    /// that came home (the `cat` payload's `A.A`).
    pub(super) async fn record_march_losses(
        &self,
        account_id: &str,
        march_id: i64,
        returned: &BTreeMap<i64, i64>,
    ) -> Result<(), sqlx::Error> {
        let army_json: Option<String> = sqlx::query_scalar(
            "SELECT army_json FROM attack_ledger WHERE account_id = ? AND march_id = ?",
        )
        .bind(account_id)
        .bind(march_id)
        .fetch_one(&self.pool)
        .await?;
        let Some(sent) = army_json
            .and_then(|json| serde_json::from_str::<BTreeMap<i64, i64>>(&json).ok())
        else {
            return Ok(());
        };
        let mut came_back = 0;
        let mut lost = 0;
        for (unit, count) in &sent {
            let back = returned.get(unit).copied().unwrap_or(0).clamp(0, *count);
            came_back += back;
            lost += count - back;
        }
        sqlx::query(
            "UPDATE attack_ledger SET troops_returned = ?, troops_lost = ?
             WHERE account_id = ? AND march_id = ?",
        )
        .bind(came_back)
        .bind(lost)
        .bind(account_id)
        .bind(march_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
