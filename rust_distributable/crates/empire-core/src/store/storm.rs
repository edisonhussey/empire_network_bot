//! Storm Islands persistence: the forts seen on the map, what vanished, what is reserved and what this
//! account has attacked (`docs/architecture/storm-islands.md`, rules in `crate::storm`).
//!
//! One row per fort position per account. A scan window the bot read replaces what is known inside it:
//! forts in it are upserted, forts no longer in it are gone (taken, or the island moved on). Any other
//! storm `gaa` (the browser's own map, an `adi` reply) only adds. An attacked fort is never picked again
//! by this account; rows are forgotten after `storm::FORGET_MS` without a sighting.

use sqlx::Row;

use super::{ReservedTarget, Store, canonical_account_id};
use crate::storm::{FORGET_MS, FRESH_MS, STORM_KINGDOM, StormFort};

impl Store {
    /// Record forts seen on the map. With `window` (the bot's own scan, `(ax1, ay1, ax2, ay2)`), a fort
    /// known inside it but missing from `forts` has vanished and is dropped, unless it is reserved or
    /// was attacked (that memory is the per-account cooldown). Returns how many vanished.
    pub async fn record_storm_forts(
        &self,
        account_id: &str,
        window: Option<(i64, i64, i64, i64)>,
        forts: &[StormFort],
        now_ms: i64,
    ) -> Result<u64, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        let mut vanished = 0;
        if let Some((ax1, ay1, ax2, ay2)) = window {
            let rows = sqlx::query(
                "SELECT x, y FROM storm_target WHERE account_id = ? AND x BETWEEN ? AND ? AND y BETWEEN ? AND ?
                   AND attacked_at_ms IS NULL AND reserved_until_ms <= ?",
            )
            .bind(&account_id)
            .bind(ax1)
            .bind(ax2)
            .bind(ay1)
            .bind(ay2)
            .bind(now_ms)
            .fetch_all(&mut *tx)
            .await?;
            for row in rows {
                let (x, y): (i64, i64) = (row.get("x"), row.get("y"));
                if !forts.iter().any(|fort| fort.x == x && fort.y == y) {
                    sqlx::query("DELETE FROM storm_target WHERE account_id = ? AND x = ? AND y = ?")
                        .bind(&account_id)
                        .bind(x)
                        .bind(y)
                        .execute(&mut *tx)
                        .await?;
                    vanished += 1;
                }
            }
        }
        for fort in forts {
            sqlx::query(
                "INSERT INTO storm_target (account_id, x, y, level, free, first_seen_ms, last_seen_ms)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, x, y) DO UPDATE SET level = excluded.level, free = excluded.free,
                   last_seen_ms = excluded.last_seen_ms",
            )
            .bind(&account_id)
            .bind(fort.x)
            .bind(fort.y)
            .bind(fort.level)
            .bind(i64::from(fort.free))
            .bind(now_ms)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM storm_target WHERE account_id = ? AND last_seen_ms < ?")
            .bind(&account_id)
            .bind(now_ms - FORGET_MS)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(vanished)
    }

    /// Claim the nearest fort to `source` that is free, in the level range, seen in the last
    /// `storm::FRESH_MS`, not reserved and never attacked by this account.
    pub async fn reserve_storm_target(
        &self,
        account_id: &str,
        level_min: Option<i64>,
        level_max: Option<i64>,
        source: (i64, i64),
        now_ms: i64,
        reserved_until_ms: i64,
    ) -> Result<Option<ReservedTarget>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = sqlx::query(
            "SELECT x, y, level FROM storm_target
             WHERE account_id = ? AND free = 1 AND attacked_at_ms IS NULL AND reserved_until_ms <= ?
               AND last_seen_ms >= ? AND level IS NOT NULL
               AND (? IS NULL OR level >= ?) AND (? IS NULL OR level <= ?)
             ORDER BY (x - ?) * (x - ?) + (y - ?) * (y - ?), x, y
             LIMIT 1",
        )
        .bind(&account_id)
        .bind(now_ms)
        .bind(now_ms - FRESH_MS)
        .bind(level_min)
        .bind(level_min)
        .bind(level_max)
        .bind(level_max)
        .bind(source.0)
        .bind(source.0)
        .bind(source.1)
        .bind(source.1)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let target = ReservedTarget { kingdom_id: STORM_KINGDOM, x: row.get("x"), y: row.get("y"), level: row.get("level") };
        sqlx::query("UPDATE storm_target SET reserved_until_ms = ? WHERE account_id = ? AND x = ? AND y = ?")
            .bind(reserved_until_ms)
            .bind(&account_id)
            .bind(target.x)
            .bind(target.y)
            .execute(&self.pool)
            .await?;
        Ok(Some(target))
    }

    /// The pick was not attacked (inspection dropped, short army, map changed): free it again.
    pub async fn release_storm_target(&self, account_id: &str, target: &ReservedTarget) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE storm_target SET reserved_until_ms = 0 WHERE account_id = ? AND x = ? AND y = ?")
            .bind(canonical_account_id(account_id))
            .bind(target.x)
            .bind(target.y)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The fort is gone or refused the attack (taken by someone else meanwhile): forget it. Not an error.
    pub async fn forget_storm_target(&self, account_id: &str, target: &ReservedTarget) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM storm_target WHERE account_id = ? AND x = ? AND y = ? AND attacked_at_ms IS NULL")
            .bind(canonical_account_id(account_id))
            .bind(target.x)
            .bind(target.y)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The attack left: this account never picks this fort again (its cooldown).
    pub async fn mark_storm_attacked(&self, account_id: &str, target: &ReservedTarget, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE storm_target SET attacked_at_ms = ?, reserved_until_ms = 0 WHERE account_id = ? AND x = ? AND y = ?")
            .bind(now_ms)
            .bind(canonical_account_id(account_id))
            .bind(target.x)
            .bind(target.y)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fort(x: i64, y: i64, level: i64) -> StormFort {
        StormFort { x, y, level: Some(level), free: true }
    }

    async fn store() -> Store {
        Store::open("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn the_nearest_fresh_free_fort_in_range_is_picked_once() {
        let store = store().await;
        let forts = [fort(551, 542, 60), fort(600, 600, 60), fort(552, 532, 40), StormFort { x: 556, y: 530, level: Some(70), free: false }];
        store.record_storm_forts("Ventrilo", None, &forts, 1_000).await.unwrap();
        let pick = store.reserve_storm_target("ventrilo", Some(60), Some(80), (557, 531), 1_000, 61_000).await.unwrap().unwrap();
        assert_eq!((pick.x, pick.y, pick.level), (551, 542, Some(60)));
        let next = store.reserve_storm_target("ventrilo", Some(60), Some(80), (557, 531), 1_000, 61_000).await.unwrap().unwrap();
        assert_eq!((next.x, next.y), (600, 600), "a reserved fort is not picked twice");
        assert!(store.reserve_storm_target("ventrilo", Some(60), Some(80), (557, 531), 1_000, 61_000).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_stale_sighting_is_not_trusted() {
        let store = store().await;
        store.record_storm_forts("ventrilo", None, &[fort(551, 542, 60)], 0).await.unwrap();
        assert!(store.reserve_storm_target("ventrilo", None, None, (0, 0), FRESH_MS + 1, FRESH_MS + 60_000).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_rescan_drops_vanished_forts_but_remembers_attacked_ones() {
        let store = store().await;
        store.record_storm_forts("ventrilo", None, &[fort(551, 542, 60), fort(552, 543, 60)], 0).await.unwrap();
        let attacked = ReservedTarget { kingdom_id: 4, x: 552, y: 543, level: Some(60) };
        store.mark_storm_attacked("ventrilo", &attacked, 1).await.unwrap();
        let vanished = store.record_storm_forts("ventrilo", Some((546, 533, 558, 545)), &[], 2).await.unwrap();
        assert_eq!(vanished, 1, "551:542 is gone; the attacked 552:543 stays as this account's cooldown");
        store.record_storm_forts("ventrilo", None, &[fort(552, 543, 60)], 3).await.unwrap();
        assert!(store.reserve_storm_target("ventrilo", None, None, (0, 0), 3, 60_000).await.unwrap().is_none(), "never attacked twice");
    }

    #[tokio::test]
    async fn a_released_pick_comes_back_and_a_forgotten_one_does_not() {
        let store = store().await;
        store.record_storm_forts("ventrilo", None, &[fort(551, 542, 60)], 0).await.unwrap();
        let pick = store.reserve_storm_target("ventrilo", None, None, (0, 0), 0, 60_000).await.unwrap().unwrap();
        store.release_storm_target("ventrilo", &pick).await.unwrap();
        let again = store.reserve_storm_target("ventrilo", None, None, (0, 0), 0, 60_000).await.unwrap().unwrap();
        store.forget_storm_target("ventrilo", &again).await.unwrap();
        assert!(store.reserve_storm_target("ventrilo", None, None, (0, 0), 0, 60_000).await.unwrap().is_none());
    }
}
