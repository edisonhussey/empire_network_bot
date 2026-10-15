//! A robber baron's progress in the database: its victory count, level, and the wins the
//! next level needs. The rule itself is [`crate::dungeon`]; this is only where it is kept.
//!
//! A map row sets the count (see `upsert_rbc_targets`). A landed, looted attack adds one
//! here, so the tower's level is right until the next map read without making one.

use sqlx::Row;

use super::{Store, canonical_account_id};
use crate::dungeon::{self, Progress};

impl Store {
    /// The stored progress of a tower, or `None` when the tower is unknown or its count has
    /// not been read from a map row yet.
    pub async fn rbc_progress(
        &self,
        account_id: &str,
        kingdom_id: i64,
        x: i64,
        y: i64,
    ) -> Result<Option<Progress>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let row = sqlx::query(
            "SELECT victory_count FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(x)
        .bind(y)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row
            .and_then(|row| row.get::<Option<i64>, _>("victory_count"))
            .map(|count| dungeon::progress(count, kingdom_id)))
    }

    /// Count one more won attack on a tower and bring its level and `hits_to_next` up to date.
    ///
    /// Returns the new progress, or `None` when there is nothing to advance (an unknown
    /// tower, or one whose count was never read). The read and the write share one
    /// transaction so two landings in quick succession each add exactly one.
    pub async fn record_rbc_victory(
        &self,
        account_id: &str,
        kingdom_id: i64,
        x: i64,
        y: i64,
    ) -> Result<Option<Progress>, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT victory_count FROM rbc_target
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(x)
        .bind(y)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(count) = row.and_then(|row| row.get::<Option<i64>, _>("victory_count")) else {
            return Ok(None);
        };
        let next = dungeon::progress(count, kingdom_id).after_victory(kingdom_id);
        sqlx::query(
            "UPDATE rbc_target SET victory_count = ?, level = ?, hits_to_next = ?
             WHERE lower(account_id) = lower(?) AND kingdom_id = ? AND x = ? AND y = ?",
        )
        .bind(next.victories)
        .bind(next.level)
        .bind(next.hits_to_next)
        .bind(&account_id)
        .bind(kingdom_id)
        .bind(x)
        .bind(y)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(next))
    }
}
