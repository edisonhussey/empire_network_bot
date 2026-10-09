//! Read-only views for the Development tab: tower positions and active marches.
//!
//! Nothing here writes. The tab reads the same tables the bot already keeps (no
//! separate map database) and does the animation itself from the timestamps below,
//! so there is no backend timer per movement.

use serde::Serialize;
use sqlx::Row;

use super::{Store, canonical_account_id};

/// A tower as the map draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapTower {
    pub x: i64,
    pub y: i64,
    pub level: Option<i64>,
    /// When it can next be attacked (0 or in the past means ready now): the latest
    /// of its lease, the server-reported cooldown and our own ledger cooldown.
    pub ready_at_ms: i64,
}

/// An army that is out or coming back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Movement {
    pub march_id: i64,
    pub x: i64,
    pub y: i64,
    pub lord_id: Option<i64>,
    pub task_id: Option<String>,
    pub phase: MovementPhase,
    /// Start and expected end, epoch milliseconds. `end_ms` is `None` when the
    /// duration is not known; it is never guessed.
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementPhase {
    /// Castle to tower, from the acknowledged travel time.
    Outbound,
    /// Landed; waiting for the server's report, which carries the return time.
    Arrived,
    /// Tower to castle, from the reported return time.
    Returning,
}

/// Fraction of a journey done at `now_ms`, clamped to 0..=1. `None` when the end
/// is unknown or the interval is empty.
pub fn progress(start_ms: i64, end_ms: Option<i64>, now_ms: i64) -> Option<f64> {
    let end_ms = end_ms?;
    if end_ms <= start_ms {
        return None;
    }
    Some(((now_ms - start_ms) as f64 / (end_ms - start_ms) as f64).clamp(0.0, 1.0))
}

/// Work out what a ledger row means as a movement.
///
/// * `sent`: outbound from `sent_at` for the travel time the acknowledgement gave.
///   Past that without a report it is `Arrived` with no return time yet.
/// * `returning`: the report has arrived; the return runs from the landing for the
///   duration it reported. Without a duration the end stays unknown.
fn classify(
    status: &str,
    sent_at_ms: i64,
    duration_s: Option<i64>,
    landed_at_ms: Option<i64>,
    now_ms: i64,
) -> (MovementPhase, i64, Option<i64>) {
    let seconds = duration_s.filter(|value| *value > 0).map(|value| value * 1_000);
    if status == "returning" {
        let start = landed_at_ms.unwrap_or(sent_at_ms);
        return (MovementPhase::Returning, start, seconds.map(|value| start + value));
    }
    let end = seconds.map(|value| sent_at_ms + value);
    match end {
        Some(end) if now_ms >= end => (MovementPhase::Arrived, end, None),
        _ => (MovementPhase::Outbound, sent_at_ms, end),
    }
}

impl Store {
    /// Every learned tower of one kingdom with the time it is next ready.
    pub async fn map_towers(
        &self,
        account_id: &str,
        kingdom_id: i64,
    ) -> Result<Vec<MapTower>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT x, y, level,
                    MAX(reserved_until_ms, server_free_at_ms, COALESCE((
                        SELECT MAX(CASE WHEN a.landed_at_ms IS NOT NULL
                                        THEN MIN(a.landed_at_ms + 10800000, a.sent_at_ms + 14400000)
                                        ELSE a.sent_at_ms + 14400000 END) + 300000
                        FROM attack_ledger a
                        WHERE lower(a.account_id) = lower(rbc_target.account_id)
                          AND a.kingdom_id = rbc_target.kingdom_id
                          AND a.x = rbc_target.x AND a.y = rbc_target.y
                          AND rbc_target.kingdom_id != 10
                    ), 0)) AS ready_at_ms
             FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(kingdom_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MapTower {
                x: row.get("x"),
                y: row.get("y"),
                level: row.get("level"),
                ready_at_ms: row.get("ready_at_ms"),
            })
            .collect())
    }

    /// The account's main castle in one kingdom: the first castle in Green, the
    /// kingdom castle elsewhere. This is where marches start and end.
    pub async fn main_castle(
        &self,
        account_id: &str,
        kingdom_id: i64,
    ) -> Result<Option<(i64, i64)>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT x, y FROM owned_castle
             WHERE lower(account_id) = lower(?) AND kingdom_id = ?
               AND ((kingdom_id = 0 AND area_type = 1) OR (kingdom_id <> 0 AND area_type = 12))
             ORDER BY castle_id LIMIT 1",
        )
        .bind(canonical_account_id(account_id))
        .bind(kingdom_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| (row.get("x"), row.get("y"))))
    }

    /// Marches that are out or coming back, newest first, for one kingdom. Rows
    /// older than `since_ms` are ignored so a forgotten `sent` row (a report that
    /// never arrived) does not draw a path forever.
    pub async fn open_movements(
        &self,
        account_id: &str,
        kingdom_id: i64,
        since_ms: i64,
        now_ms: i64,
        limit: i64,
    ) -> Result<Vec<Movement>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT march_id, x, y, lord_id, task_id, status, sent_at_ms, duration_s, landed_at_ms
             FROM attack_ledger
             WHERE account_id = ? AND kingdom_id = ? AND status IN ('sent', 'returning')
               AND sent_at_ms >= ?
             ORDER BY sent_at_ms DESC LIMIT ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(kingdom_id)
        .bind(since_ms)
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let (phase, start_ms, end_ms) = classify(
                    row.get::<&str, _>("status"),
                    row.get("sent_at_ms"),
                    row.get("duration_s"),
                    row.get("landed_at_ms"),
                    now_ms,
                );
                Movement {
                    march_id: row.get("march_id"),
                    x: row.get("x"),
                    y: row.get("y"),
                    lord_id: row.get("lord_id"),
                    task_id: row.get("task_id"),
                    phase,
                    start_ms,
                    end_ms,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_the_clamped_fraction_of_the_journey() {
        assert_eq!(progress(1_000, Some(3_000), 1_000), Some(0.0));
        assert_eq!(progress(1_000, Some(3_000), 2_000), Some(0.5));
        assert_eq!(progress(1_000, Some(3_000), 3_000), Some(1.0));
        assert_eq!(progress(1_000, Some(3_000), 99_999), Some(1.0));
        assert_eq!(progress(1_000, Some(3_000), 0), Some(0.0), "not started yet");
    }

    #[test]
    fn an_unknown_or_empty_journey_has_no_progress_rather_than_a_guess() {
        assert_eq!(progress(1_000, None, 2_000), None);
        assert_eq!(progress(2_000, Some(2_000), 2_500), None);
        assert_eq!(progress(2_000, Some(1_000), 2_500), None);
    }

    #[test]
    fn an_outbound_march_runs_for_the_acknowledged_travel_time() {
        let (phase, start, end) = classify("sent", 10_000, Some(300), None, 100_000);
        assert_eq!((phase, start, end), (MovementPhase::Outbound, 10_000, Some(310_000)));
        assert_eq!(progress(start, end, 160_000), Some(0.5));
    }

    #[test]
    fn past_its_travel_time_without_a_report_it_has_arrived_with_no_return_time() {
        let (phase, start, end) = classify("sent", 10_000, Some(300), None, 400_000);
        assert_eq!(phase, MovementPhase::Arrived);
        assert_eq!((start, end), (310_000, None));
    }

    #[test]
    fn a_returning_march_uses_the_reported_return_time_from_the_landing() {
        let (phase, start, end) = classify("returning", 10_000, Some(280), Some(320_000), 400_000);
        assert_eq!((phase, start, end), (MovementPhase::Returning, 320_000, Some(600_000)));
        assert_eq!(progress(start, end, 460_000), Some(0.5));
    }

    #[test]
    fn missing_timestamps_never_invent_a_duration() {
        let (_, _, end) = classify("sent", 10_000, None, None, 50_000);
        assert_eq!(end, None);
        let (phase, start, end) = classify("returning", 10_000, None, Some(40_000), 50_000);
        assert_eq!((phase, start, end), (MovementPhase::Returning, 40_000, None));
        let (_, _, end) = classify("sent", 10_000, Some(0), None, 50_000);
        assert_eq!(end, None, "a zero duration is not a duration");
    }
}
