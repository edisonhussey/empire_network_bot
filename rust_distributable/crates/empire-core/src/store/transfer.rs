//! Stored kingdom transfers and the snapshots their checks read.
//!
//! Jobs live in `kingdom_transfer` (migration V26). Coins, kingdom state and skip stock are small
//! snapshots in `app_state`, replaced whole each time the server reports them (the server's
//! numbers are absolute, so nothing here adds or subtracts). Rules: `crate::transfer`; the session
//! part: `empire-daemon/src/direct/transfer.rs`.

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;

use super::{Store, canonical_account_id};
use crate::transfer::KingdomState;

/// Waiting for the bot to be off and the connection to be free.
pub const TRANSFER_QUEUED: &str = "queued";
/// The `kut` has gone out and no answer has come back.
pub const TRANSFER_SENT: &str = "sent";
/// Accepted; the troops are on their way (skips may be spent).
pub const TRANSFER_TRAVELLING: &str = "travelling";
pub const TRANSFER_DONE: &str = "done";
pub const TRANSFER_FAILED: &str = "failed";
pub const TRANSFER_CANCELLED: &str = "cancelled";

/// A stored transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferRow {
    /// `0` for a row not yet saved.
    pub job_id: i64,
    pub account_id: String,
    pub source_castle_id: i64,
    pub source_kingdom_id: i64,
    pub target_kingdom_id: i64,
    pub target_castle_id: i64,
    pub units: Vec<(i64, i64)>,
    pub skip_item: Option<String>,
    pub phase: String,
    pub detail: String,
    pub cost_c1: Option<i64>,
    pub skips_sent: i64,
    pub created_at_ms: i64,
    pub sent_at_ms: Option<i64>,
    pub landed_at_ms: Option<i64>,
    pub updated_at_ms: i64,
}

impl TransferRow {
    pub fn is_open(&self) -> bool {
        matches!(self.phase.as_str(), TRANSFER_QUEUED | TRANSFER_SENT | TRANSFER_TRAVELLING)
    }
}

const COLUMNS: &str = "job_id, account_id, source_castle_id, source_kingdom_id, target_kingdom_id,
    target_castle_id, units_json, skip_item, phase, detail, cost_c1, skips_sent, created_at_ms,
    sent_at_ms, landed_at_ms, updated_at_ms";

fn row_to_transfer(row: sqlx::sqlite::SqliteRow) -> TransferRow {
    TransferRow {
        job_id: row.get("job_id"),
        account_id: row.get("account_id"),
        source_castle_id: row.get("source_castle_id"),
        source_kingdom_id: row.get("source_kingdom_id"),
        target_kingdom_id: row.get("target_kingdom_id"),
        target_castle_id: row.get("target_castle_id"),
        units: serde_json::from_str(row.get::<&str, _>("units_json")).unwrap_or_default(),
        skip_item: row.get("skip_item"),
        phase: row.get("phase"),
        detail: row.get("detail"),
        cost_c1: row.get("cost_c1"),
        skips_sent: row.get("skips_sent"),
        created_at_ms: row.get("created_at_ms"),
        sent_at_ms: row.get("sent_at_ms"),
        landed_at_ms: row.get("landed_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    }
}

fn key(kind: &str, account_id: &str) -> String {
    format!("transfer.{kind}.{}", canonical_account_id(account_id))
}

impl Store {
    /// Save a new transfer (`job_id == 0`) or update an existing one. Returns its id.
    pub async fn save_transfer(&self, job: &TransferRow) -> Result<i64, sqlx::Error> {
        let account_id = canonical_account_id(&job.account_id);
        let units = serde_json::to_string(&job.units).unwrap_or_else(|_| "[]".to_owned());
        if job.job_id == 0 {
            let id = sqlx::query(
                "INSERT INTO kingdom_transfer (
                     account_id, source_castle_id, source_kingdom_id, target_kingdom_id,
                     target_castle_id, units_json, skip_item, phase, detail, cost_c1, skips_sent,
                     created_at_ms, sent_at_ms, landed_at_ms, updated_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(job.source_castle_id)
            .bind(job.source_kingdom_id)
            .bind(job.target_kingdom_id)
            .bind(job.target_castle_id)
            .bind(units)
            .bind(&job.skip_item)
            .bind(&job.phase)
            .bind(&job.detail)
            .bind(job.cost_c1)
            .bind(job.skips_sent)
            .bind(job.created_at_ms)
            .bind(job.sent_at_ms)
            .bind(job.landed_at_ms)
            .bind(job.updated_at_ms)
            .execute(&self.pool)
            .await?
            .last_insert_rowid();
            return Ok(id);
        }
        sqlx::query(
            "UPDATE kingdom_transfer SET phase = ?, detail = ?, cost_c1 = ?, skips_sent = ?,
                    sent_at_ms = ?, landed_at_ms = ?, updated_at_ms = ?
             WHERE job_id = ? AND account_id = ?",
        )
        .bind(&job.phase)
        .bind(&job.detail)
        .bind(job.cost_c1)
        .bind(job.skips_sent)
        .bind(job.sent_at_ms)
        .bind(job.landed_at_ms)
        .bind(job.updated_at_ms)
        .bind(job.job_id)
        .bind(&account_id)
        .execute(&self.pool)
        .await?;
        Ok(job.job_id)
    }

    pub async fn transfer(&self, account_id: &str, job_id: i64) -> Result<Option<TransferRow>, sqlx::Error> {
        let row = sqlx::query(&format!("SELECT {COLUMNS} FROM kingdom_transfer WHERE account_id = ? AND job_id = ?"))
            .bind(canonical_account_id(account_id))
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(row_to_transfer))
    }

    /// Newest first.
    pub async fn recent_transfers(&self, account_id: &str, limit: i64) -> Result<Vec<TransferRow>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM kingdom_transfer WHERE account_id = ? ORDER BY job_id DESC LIMIT ?"
        ))
        .bind(canonical_account_id(account_id))
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_transfer).collect())
    }

    /// Transfers not yet finished, oldest first (the one to run next is the first).
    pub async fn open_transfers(&self, account_id: &str) -> Result<Vec<TransferRow>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM kingdom_transfer
             WHERE account_id = ? AND phase IN (?, ?, ?) ORDER BY job_id"
        ))
        .bind(canonical_account_id(account_id))
        .bind(TRANSFER_QUEUED)
        .bind(TRANSFER_SENT)
        .bind(TRANSFER_TRAVELLING)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_transfer).collect())
    }

    /// Cancel a transfer that has not been sent. `false` when it is already on its way or finished:
    /// nothing is recalled once the `kut` is out.
    pub async fn cancel_queued_transfer(&self, account_id: &str, job_id: i64, now_ms: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE kingdom_transfer SET phase = ?, detail = 'Cancelled before it was sent', updated_at_ms = ?
             WHERE account_id = ? AND job_id = ? AND phase = ?",
        )
        .bind(TRANSFER_CANCELLED)
        .bind(now_ms)
        .bind(canonical_account_id(account_id))
        .bind(job_id)
        .bind(TRANSFER_QUEUED)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// When a transfer from this castle was last sent. A castle reading must be newer than this to
    /// be trusted for the next one.
    pub async fn last_transfer_sent_from(&self, account_id: &str, castle_id: i64) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT MAX(sent_at_ms) FROM kingdom_transfer WHERE account_id = ? AND source_castle_id = ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(castle_id)
        .fetch_one(&self.pool)
        .await
    }

    /// Whether an attack bot or a recruitment bot is running for the account: a transfer only runs
    /// while both are off, so they never share the connection or the castle's troops.
    pub async fn account_bot_running(&self, account_id: &str) -> Result<bool, sqlx::Error> {
        if self.account_mode_running(account_id).await? {
            return Ok(true);
        }
        Ok(!self.active_recruitments(account_id).await?.is_empty())
    }

    /// Everything the checks need, read from what the server last told us. `held` is the set of
    /// `(kingdom, x, y)` castles whose troops are out as support (known only to the session; the
    /// API passes an empty list and the session checks again before sending).
    pub async fn transfer_snapshot(
        &self,
        account_id: &str,
        plan: &crate::transfer::TransferPlan,
        held: &[(i64, i64, i64)],
    ) -> Result<crate::transfer::Snapshot, sqlx::Error> {
        let castle = self
            .owned_castles()
            .await?
            .into_iter()
            .find(|castle| {
                castle.account_id.eq_ignore_ascii_case(account_id)
                    && castle.castle_id == plan.source_castle_id
                    && castle.kingdom_id == plan.source_kingdom_id
            });
        Ok(crate::transfer::Snapshot {
            stock: self.castle_inventory(account_id, plan.source_castle_id).await?,
            last_sent_from_castle_ms: self.last_transfer_sent_from(account_id, plan.source_castle_id).await?,
            coins: self.currency(account_id).await?.map(|(coins, _, _)| coins),
            kingdoms: self.kingdom_state(account_id).await?.map(|(_, state)| state),
            skip_stock: self.skip_stock(account_id).await?,
            bot_running: self.account_bot_running(account_id).await?,
            castle_held: castle
                .as_ref()
                .is_some_and(|castle| held.contains(&(castle.kingdom_id, castle.x, castle.y))),
            owns_source: castle.is_some(),
        })
    }

    // ---- snapshots (replace whole) ----

    pub async fn set_kingdom_state(&self, account_id: &str, state: &KingdomState, now_ms: i64) -> Result<(), sqlx::Error> {
        let value = json!({"state": state, "observed_at_ms": now_ms});
        self.set_app_state(&key("kingdoms", account_id), &value, now_ms).await
    }

    pub async fn kingdom_state(&self, account_id: &str) -> Result<Option<(i64, KingdomState)>, sqlx::Error> {
        Ok(self.app_state(&key("kingdoms", account_id)).await?.and_then(|value| {
            let state = serde_json::from_value(value.get("state")?.clone()).ok()?;
            Some((value.get("observed_at_ms")?.as_i64()?, state))
        }))
    }

    pub async fn set_currency(&self, account_id: &str, coins: i64, rubies: i64, now_ms: i64) -> Result<(), sqlx::Error> {
        let value = json!({"c1": coins, "c2": rubies, "observed_at_ms": now_ms});
        self.set_app_state(&key("currency", account_id), &value, now_ms).await
    }

    /// `(coins, rubies, observed_at_ms)`.
    pub async fn currency(&self, account_id: &str) -> Result<Option<(i64, i64, i64)>, sqlx::Error> {
        Ok(self.app_state(&key("currency", account_id)).await?.and_then(|value| {
            Some((value.get("c1")?.as_i64()?, value.get("c2")?.as_i64()?, value.get("observed_at_ms")?.as_i64()?))
        }))
    }

    /// Set the counts the server reported (`gbd` lists every skip item it has; `sce` after a spend
    /// lists the one spent). Each key is an absolute count; keys not mentioned are kept.
    pub async fn merge_skip_stock(
        &self,
        account_id: &str,
        reported: &BTreeMap<String, i64>,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let mut stock = self.skip_stock(account_id).await?;
        stock.extend(reported.iter().map(|(key, count)| (key.clone(), *count)));
        self.set_app_state(&key("skips", account_id), &json!(stock), now_ms).await
    }

    pub async fn skip_stock(&self, account_id: &str) -> Result<BTreeMap<String, i64>, sqlx::Error> {
        Ok(self
            .app_state(&key("skips", account_id))
            .await?
            .and_then(|value: Value| serde_json::from_value(value).ok())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::InFlight;

    const NOW: i64 = 1_800_000_000_000;

    async fn store() -> Store {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store.upsert_account_profile("ventrilo", "Ventrilo", "wss://x/", "EmpireEx_21", 1).await.unwrap();
        store.upsert_account_profile("other", "Other", "wss://x/", "EmpireEx_21", 1).await.unwrap();
        store
    }

    fn job(account: &str) -> TransferRow {
        TransferRow {
            job_id: 0,
            account_id: account.to_owned(),
            source_castle_id: 16_011_862,
            source_kingdom_id: 0,
            target_kingdom_id: 3,
            target_castle_id: -1,
            units: vec![(655, 20)],
            skip_item: Some("MS5".to_owned()),
            phase: TRANSFER_QUEUED.to_owned(),
            detail: "Waiting".to_owned(),
            cost_c1: Some(2_500),
            skips_sent: 0,
            created_at_ms: NOW,
            sent_at_ms: None,
            landed_at_ms: None,
            updated_at_ms: NOW,
        }
    }

    #[tokio::test]
    async fn a_transfer_is_saved_updated_and_read_back_with_its_units() {
        let store = store().await;
        let id = store.save_transfer(&job("Ventrilo")).await.unwrap();
        let mut row = store.transfer("ventrilo", id).await.unwrap().unwrap();
        assert_eq!((row.units.clone(), row.skip_item.clone(), row.phase.as_str()), (vec![(655, 20)], Some("MS5".into()), "queued"));
        assert!(row.is_open());
        row.phase = TRANSFER_TRAVELLING.to_owned();
        row.sent_at_ms = Some(NOW + 5);
        row.skips_sent = 1;
        store.save_transfer(&row).await.unwrap();
        let again = store.transfer("VENTRILO", id).await.unwrap().unwrap();
        assert_eq!((again.phase.as_str(), again.sent_at_ms, again.skips_sent), ("travelling", Some(NOW + 5), 1));
        row.phase = TRANSFER_DONE.to_owned();
        store.save_transfer(&row).await.unwrap();
        assert!(!store.transfer("ventrilo", id).await.unwrap().unwrap().is_open());
        assert!(store.open_transfers("ventrilo").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accounts_never_see_each_others_transfers() {
        let store = store().await;
        let mine = store.save_transfer(&job("ventrilo")).await.unwrap();
        let theirs = store.save_transfer(&job("other")).await.unwrap();
        assert_eq!(store.recent_transfers("ventrilo", 10).await.unwrap().len(), 1);
        assert!(store.transfer("ventrilo", theirs).await.unwrap().is_none());
        assert!(!store.cancel_queued_transfer("ventrilo", theirs, NOW).await.unwrap());
        assert!(store.cancel_queued_transfer("ventrilo", mine, NOW).await.unwrap());
    }

    #[tokio::test]
    async fn only_a_queued_transfer_can_be_cancelled() {
        let store = store().await;
        let id = store.save_transfer(&job("ventrilo")).await.unwrap();
        let mut row = store.transfer("ventrilo", id).await.unwrap().unwrap();
        row.phase = TRANSFER_SENT.to_owned();
        store.save_transfer(&row).await.unwrap();
        assert!(!store.cancel_queued_transfer("ventrilo", id, NOW).await.unwrap(), "already out");
        let queued = store.save_transfer(&job("ventrilo")).await.unwrap();
        assert!(store.cancel_queued_transfer("ventrilo", queued, NOW).await.unwrap());
        assert_eq!(store.transfer("ventrilo", queued).await.unwrap().unwrap().phase, "cancelled");
        let open = store.open_transfers("ventrilo").await.unwrap();
        assert_eq!(open.iter().map(|row| row.job_id).collect::<Vec<_>>(), vec![id]);
    }

    #[tokio::test]
    async fn the_last_send_from_a_castle_is_the_newest_sent_time() {
        let store = store().await;
        assert_eq!(store.last_transfer_sent_from("ventrilo", 16_011_862).await.unwrap(), None);
        for sent in [NOW + 10, NOW + 50, NOW + 30] {
            let mut row = job("ventrilo");
            row.sent_at_ms = Some(sent);
            store.save_transfer(&row).await.unwrap();
        }
        assert_eq!(store.last_transfer_sent_from("ventrilo", 16_011_862).await.unwrap(), Some(NOW + 50));
        assert_eq!(store.last_transfer_sent_from("ventrilo", 1).await.unwrap(), None);
    }

    #[tokio::test]
    async fn snapshots_replace_whole_and_skip_stock_merges_by_key() {
        let store = store().await;
        assert!(store.kingdom_state("ventrilo").await.unwrap().is_none());
        let mut state = KingdomState::default();
        state.unlocked.insert(3, true);
        state.transfers.push(InFlight { kingdom_id: 3, remaining_s: 7_200, units: BTreeMap::from([(655, 20)]) });
        store.set_kingdom_state("Ventrilo", &state, NOW).await.unwrap();
        let (at, read) = store.kingdom_state("ventrilo").await.unwrap().unwrap();
        assert_eq!((at, read.clone()), (NOW, state));
        store.set_kingdom_state("ventrilo", &KingdomState::default(), NOW + 1).await.unwrap();
        assert!(store.kingdom_state("ventrilo").await.unwrap().unwrap().1.transfers.is_empty(), "a snapshot replaces, never merges");

        store.set_currency("ventrilo", 2_147_480_937, 150_163, NOW).await.unwrap();
        assert_eq!(store.currency("ventrilo").await.unwrap(), Some((2_147_480_937, 150_163, NOW)));

        store.merge_skip_stock("ventrilo", &BTreeMap::from([("MS1".to_owned(), 3_786), ("MS5".to_owned(), 996)]), NOW).await.unwrap();
        store.merge_skip_stock("ventrilo", &BTreeMap::from([("MS5".to_owned(), 995)]), NOW + 1).await.unwrap();
        assert_eq!(
            store.skip_stock("ventrilo").await.unwrap(),
            BTreeMap::from([("MS1".to_owned(), 3_786), ("MS5".to_owned(), 995)])
        );
        assert!(store.skip_stock("other").await.unwrap().is_empty());
    }
}
