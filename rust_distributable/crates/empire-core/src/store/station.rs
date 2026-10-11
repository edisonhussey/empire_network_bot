//! Auto-station persistence: support bots, the per-castle stock, and the job record.
//!
//! A *support bot* is only a set of destinations, one coordinate per kingdom; a
//! kingdom without one is never stationed. An account picks at most one, together
//! with the attack and recruit bots (see `plans`). The latest `dcl` stock per castle
//! feeds the Overview. A *job* is one stationed castle from detection to recall: the
//! columns answer the UI and the timing measurements, `state_json` holds the state
//! machine so an outstanding support march survives a restart.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::{Store, canonical_account_id};
use crate::station::CastleStock;

/// The kingdoms a support bot can name a destination for: the permanent ones.
pub const STATION_KINGDOMS: [i64; 5] = [0, 1, 2, 3, 4];

/// Phase of a finished job.
pub const JOB_DONE: &str = "done";
/// Jobs older than this are pruned when a session starts.
pub const JOB_RETENTION_MS: i64 = 180 * 24 * 3_600_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationBotKingdom {
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StationBot {
    pub station_bot_id: i64,
    pub name: String,
    pub kingdoms: Vec<StationBotKingdom>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountStationBot {
    pub account_id: String,
    pub station_bot_id: i64,
    pub running: bool,
    pub updated_at_ms: i64,
}

/// A running support bot, ready to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveStation {
    pub station_bot_id: i64,
    pub name: String,
    /// Where support goes, per kingdom. A kingdom not in the map is not stationed.
    pub destinations: BTreeMap<i64, (i64, i64)>,
}

/// One castle as the Overview shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverviewCastle {
    pub kingdom_id: i64,
    pub castle_id: i64,
    pub name: String,
    pub x: i64,
    pub y: i64,
    /// `None` until a `dcl` reading has been taken.
    pub observed_at_ms: Option<i64>,
    pub units: BTreeMap<i64, i64>,
    pub food: f64,
    pub mead: f64,
    pub wood: f64,
    pub stone: f64,
}

/// A stored job. `state_json` is opaque here; the daemon reads and writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StationJobRow {
    /// `0` for a job not yet saved.
    pub job_id: i64,
    pub account_id: String,
    pub kingdom_id: i64,
    pub castle_id: i64,
    pub castle_x: i64,
    pub castle_y: i64,
    pub dest: Option<(i64, i64)>,
    pub lid: Option<i64>,
    pub phase: String,
    pub outcome: Option<String>,
    pub support_mid: Option<i64>,
    pub landing_ms: i64,
    pub detected_at_ms: i64,
    pub cds_acked_at_ms: Option<i64>,
    pub recalled_at_ms: Option<i64>,
    /// How long before the estimated landing the support march was acknowledged:
    /// the figure the whole feature is judged on.
    pub lead_ms: Option<i64>,
    #[serde(skip)]
    pub state_json: String,
    pub updated_at_ms: i64,
}

impl Store {
    pub async fn create_station_bot(
        &self,
        name: &str,
        kingdoms: &[StationBotKingdom],
        now_ms: i64,
    ) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let id = sqlx::query(
            "INSERT INTO station_bot (name, created_at_ms, updated_at_ms) VALUES (?, ?, ?)",
        )
        .bind(name.trim())
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        for kingdom in kingdoms {
            sqlx::query(
                "INSERT INTO station_bot_kingdom (station_bot_id, kingdom_id, x, y) VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(kingdom.kingdom_id)
            .bind(kingdom.x)
            .bind(kingdom.y)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    pub async fn station_bots(&self) -> Result<Vec<StationBot>, sqlx::Error> {
        let bots = sqlx::query(
            "SELECT station_bot_id, name FROM station_bot ORDER BY updated_at_ms DESC, station_bot_id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut result = Vec::with_capacity(bots.len());
        for bot in bots {
            let id: i64 = bot.get("station_bot_id");
            let rows = sqlx::query(
                "SELECT kingdom_id, x, y FROM station_bot_kingdom
                 WHERE station_bot_id = ? ORDER BY kingdom_id",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
            result.push(StationBot {
                station_bot_id: id,
                name: bot.get("name"),
                kingdoms: rows
                    .into_iter()
                    .map(|row| StationBotKingdom {
                        kingdom_id: row.get("kingdom_id"),
                        x: row.get("x"),
                        y: row.get("y"),
                    })
                    .collect(),
            });
        }
        Ok(result)
    }

    /// Removes a support bot, its destinations and any account using it. The
    /// children are cleared explicitly so a row can never point at a missing bot,
    /// whether or not foreign keys are enforced on this connection.
    pub async fn delete_station_bot(&self, id: i64) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for table in ["account_station_bot", "station_bot_kingdom"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE station_bot_id = ?"))
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM station_bot WHERE station_bot_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }

    /// Choose (or clear, with `None`) the account's support bot.
    pub async fn subscribe_account_station_bot(
        &self,
        account_id: &str,
        bot_id: Option<i64>,
        running: bool,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM account_station_bot WHERE account_id = ?")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        if let Some(bot_id) = bot_id {
            sqlx::query(
                "INSERT INTO account_station_bot (account_id, station_bot_id, running, updated_at_ms)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(bot_id)
            .bind(i64::from(running))
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    pub async fn account_station_bots(&self) -> Result<Vec<AccountStationBot>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, station_bot_id, running, updated_at_ms
             FROM account_station_bot ORDER BY updated_at_ms DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| AccountStationBot {
                account_id: row.get("account_id"),
                station_bot_id: row.get("station_bot_id"),
                running: row.get::<i64, _>("running") != 0,
                updated_at_ms: row.get("updated_at_ms"),
            })
            .collect())
    }

    /// The account's support bot, when one is chosen and running.
    pub async fn active_station(
        &self,
        account_id: &str,
    ) -> Result<Option<ActiveStation>, sqlx::Error> {
        let Some(bot) = sqlx::query(
            "SELECT b.station_bot_id, b.name FROM account_station_bot a
             JOIN station_bot b ON b.station_bot_id = a.station_bot_id
             WHERE a.account_id = ? AND a.running = 1",
        )
        .bind(canonical_account_id(account_id))
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        let id: i64 = bot.get("station_bot_id");
        let rows = sqlx::query("SELECT kingdom_id, x, y FROM station_bot_kingdom WHERE station_bot_id = ?")
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
        Ok(Some(ActiveStation {
            station_bot_id: id,
            name: bot.get("name"),
            destinations: rows
                .into_iter()
                .map(|row| (row.get("kingdom_id"), (row.get("x"), row.get("y"))))
                .collect(),
        }))
    }

    /// Stop the account's station without forgetting which bot it was. A job with
    /// a support march out is unaffected: recalls go on regardless.
    pub async fn stop_account_station_bot(
        &self,
        account_id: &str,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE account_station_bot SET running = 0, updated_at_ms = ? WHERE account_id = ?")
            .bind(now_ms)
            .bind(canonical_account_id(account_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Replace the stored stock of every castle a `dcl` reply listed.
    pub async fn record_castle_inventory(
        &self,
        account_id: &str,
        castles: &[CastleStock],
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        for castle in castles {
            let units = serde_json::to_string(&castle.units).unwrap_or_else(|_| "{}".to_owned());
            sqlx::query(
                "INSERT INTO castle_inventory
                     (account_id, kingdom_id, castle_id, observed_at_ms, units_json, food, mead, wood, stone)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, castle_id) DO UPDATE SET
                     kingdom_id = excluded.kingdom_id, observed_at_ms = excluded.observed_at_ms,
                     units_json = excluded.units_json, food = excluded.food, mead = excluded.mead,
                     wood = excluded.wood, stone = excluded.stone",
            )
            .bind(&account_id)
            .bind(castle.kingdom_id)
            .bind(castle.castle_id)
            .bind(now_ms)
            .bind(units)
            .bind(castle.food)
            .bind(castle.mead)
            .bind(castle.wood)
            .bind(castle.stone)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// The stored stock of one castle, when a reading exists.
    pub async fn castle_inventory(
        &self,
        account_id: &str,
        castle_id: i64,
    ) -> Result<Option<(i64, BTreeMap<i64, i64>)>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT observed_at_ms, units_json FROM castle_inventory
             WHERE account_id = ? AND castle_id = ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(castle_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| {
            let units = serde_json::from_str(row.get::<&str, _>("units_json")).unwrap_or_default();
            (row.get("observed_at_ms"), units)
        }))
    }

    /// Every owned castle with its latest stock, castles never read included.
    pub async fn overview_castles(
        &self,
        account_id: &str,
    ) -> Result<Vec<OverviewCastle>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT o.kingdom_id, o.castle_id, o.name, o.x, o.y,
                    i.observed_at_ms, i.units_json, i.food, i.mead, i.wood, i.stone
             FROM owned_castle o
             LEFT JOIN castle_inventory i
               ON i.account_id = lower(o.account_id) AND i.castle_id = o.castle_id
             WHERE lower(o.account_id) = lower(?)
             ORDER BY o.kingdom_id, o.castle_id",
        )
        .bind(canonical_account_id(account_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| OverviewCastle {
                kingdom_id: row.get("kingdom_id"),
                castle_id: row.get("castle_id"),
                name: row.get("name"),
                x: row.get("x"),
                y: row.get("y"),
                observed_at_ms: row.get("observed_at_ms"),
                units: row
                    .get::<Option<&str>, _>("units_json")
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or_default(),
                food: row.get::<Option<f64>, _>("food").unwrap_or(0.0),
                mead: row.get::<Option<f64>, _>("mead").unwrap_or(0.0),
                wood: row.get::<Option<f64>, _>("wood").unwrap_or(0.0),
                stone: row.get::<Option<f64>, _>("stone").unwrap_or(0.0),
            })
            .collect())
    }

    /// Insert (`job_id == 0`) or update a job, returning its id.
    pub async fn save_station_job(&self, job: &StationJobRow) -> Result<i64, sqlx::Error> {
        let account_id = canonical_account_id(&job.account_id);
        if job.job_id == 0 {
            let id = sqlx::query(
                "INSERT INTO station_job (
                     account_id, kingdom_id, castle_id, castle_x, castle_y, dest_x, dest_y, lid,
                     phase, outcome, support_mid, landing_ms, detected_at_ms, cds_acked_at_ms,
                     recalled_at_ms, lead_ms, state_json, updated_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(job.kingdom_id)
            .bind(job.castle_id)
            .bind(job.castle_x)
            .bind(job.castle_y)
            .bind(job.dest.map(|dest| dest.0))
            .bind(job.dest.map(|dest| dest.1))
            .bind(job.lid)
            .bind(&job.phase)
            .bind(&job.outcome)
            .bind(job.support_mid)
            .bind(job.landing_ms)
            .bind(job.detected_at_ms)
            .bind(job.cds_acked_at_ms)
            .bind(job.recalled_at_ms)
            .bind(job.lead_ms)
            .bind(&job.state_json)
            .bind(job.updated_at_ms)
            .execute(&self.pool)
            .await?
            .last_insert_rowid();
            return Ok(id);
        }
        sqlx::query(
            "UPDATE station_job SET
                 dest_x = ?, dest_y = ?, lid = ?, phase = ?, outcome = ?, support_mid = ?,
                 landing_ms = ?, cds_acked_at_ms = ?, recalled_at_ms = ?, lead_ms = ?,
                 state_json = ?, updated_at_ms = ?
             WHERE job_id = ? AND account_id = ?",
        )
        .bind(job.dest.map(|dest| dest.0))
        .bind(job.dest.map(|dest| dest.1))
        .bind(job.lid)
        .bind(&job.phase)
        .bind(&job.outcome)
        .bind(job.support_mid)
        .bind(job.landing_ms)
        .bind(job.cds_acked_at_ms)
        .bind(job.recalled_at_ms)
        .bind(job.lead_ms)
        .bind(&job.state_json)
        .bind(job.updated_at_ms)
        .bind(job.job_id)
        .bind(&account_id)
        .execute(&self.pool)
        .await?;
        Ok(job.job_id)
    }

    /// Jobs that have not finished, oldest first: what a new session resumes.
    pub async fn open_station_jobs(
        &self,
        account_id: &str,
    ) -> Result<Vec<StationJobRow>, sqlx::Error> {
        self.station_jobs_where(account_id, "phase <> 'done'", "detected_at_ms ASC", 500)
            .await
    }

    /// Newest jobs first, finished or not.
    pub async fn recent_station_jobs(
        &self,
        account_id: &str,
        limit: i64,
    ) -> Result<Vec<StationJobRow>, sqlx::Error> {
        self.station_jobs_where(account_id, "1 = 1", "detected_at_ms DESC", limit.clamp(1, 200))
            .await
    }

    async fn station_jobs_where(
        &self,
        account_id: &str,
        condition: &str,
        order: &str,
        limit: i64,
    ) -> Result<Vec<StationJobRow>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT * FROM station_job WHERE account_id = ? AND {condition}
             ORDER BY {order} LIMIT ?"
        ))
        .bind(canonical_account_id(account_id))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(job_row).collect())
    }

    /// Forget finished jobs older than the retention. Returns how many went.
    pub async fn prune_station_jobs(&self, now_ms: i64) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query("DELETE FROM station_job WHERE phase = 'done' AND detected_at_ms < ?")
            .bind(now_ms - JOB_RETENTION_MS)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }
}

fn job_row(row: &sqlx::sqlite::SqliteRow) -> StationJobRow {
    let dest = row
        .get::<Option<i64>, _>("dest_x")
        .zip(row.get::<Option<i64>, _>("dest_y"));
    StationJobRow {
        job_id: row.get("job_id"),
        account_id: row.get("account_id"),
        kingdom_id: row.get("kingdom_id"),
        castle_id: row.get("castle_id"),
        castle_x: row.get("castle_x"),
        castle_y: row.get("castle_y"),
        dest,
        lid: row.get("lid"),
        phase: row.get("phase"),
        outcome: row.get("outcome"),
        support_mid: row.get("support_mid"),
        landing_ms: row.get("landing_ms"),
        detected_at_ms: row.get("detected_at_ms"),
        cds_acked_at_ms: row.get("cds_acked_at_ms"),
        recalled_at_ms: row.get("recalled_at_ms"),
        lead_ms: row.get("lead_ms"),
        state_json: row.get("state_json"),
        updated_at_ms: row.get("updated_at_ms"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> Store {
        Store::open("sqlite::memory:").await.unwrap()
    }

    fn kingdoms() -> Vec<StationBotKingdom> {
        vec![
            StationBotKingdom { kingdom_id: 0, x: 468, y: 507 },
            StationBotKingdom { kingdom_id: 1, x: 600, y: 600 },
        ]
    }

    fn job(castle_id: i64) -> StationJobRow {
        StationJobRow {
            job_id: 0,
            account_id: "Ventrilo".into(),
            kingdom_id: 0,
            castle_id,
            castle_x: 543,
            castle_y: 520,
            dest: Some((468, 507)),
            lid: Some(7),
            phase: "planned".into(),
            outcome: None,
            support_mid: None,
            landing_ms: 10_000,
            detected_at_ms: 5_000,
            cds_acked_at_ms: None,
            recalled_at_ms: None,
            lead_ms: None,
            state_json: "{}".into(),
            updated_at_ms: 5_000,
        }
    }

    #[tokio::test]
    async fn a_support_bot_keeps_one_destination_per_kingdom() {
        let store = store().await;
        let id = store.create_station_bot("Green and Sands", &kingdoms(), 1).await.unwrap();
        let bots = store.station_bots().await.unwrap();
        assert_eq!(bots.len(), 1);
        assert_eq!(bots[0].station_bot_id, id);
        assert_eq!(bots[0].kingdoms, kingdoms());
        assert!(store.create_station_bot("green and sands", &[], 2).await.is_err(), "names are unique");
    }

    #[tokio::test]
    async fn the_account_uses_at_most_one_bot_and_only_while_running() {
        let store = store().await;
        let first = store.create_station_bot("first", &kingdoms(), 1).await.unwrap();
        let second = store.create_station_bot("second", &kingdoms()[..1], 2).await.unwrap();
        assert!(store.active_station("Ventrilo").await.unwrap().is_none());
        store.subscribe_account_station_bot("Ventrilo", Some(first), true, 3).await.unwrap();
        store.subscribe_account_station_bot("ventrilo", Some(second), true, 4).await.unwrap();
        let active = store.active_station("VENTRILO").await.unwrap().unwrap();
        assert_eq!(active.name, "second");
        assert_eq!(active.destinations, BTreeMap::from([(0, (468, 507))]));
        assert_eq!(store.account_station_bots().await.unwrap().len(), 1);
        store.stop_account_station_bot("ventrilo", 5).await.unwrap();
        assert!(store.active_station("ventrilo").await.unwrap().is_none());
        assert_eq!(store.account_station_bots().await.unwrap()[0].station_bot_id, second, "the choice is remembered");
        store.subscribe_account_station_bot("ventrilo", None, false, 6).await.unwrap();
        assert!(store.account_station_bots().await.unwrap().is_empty(), "null clears it");
    }

    #[tokio::test]
    async fn deleting_a_bot_takes_its_destinations_and_subscriptions_with_it() {
        let store = store().await;
        let id = store.create_station_bot("gone", &kingdoms(), 1).await.unwrap();
        store.subscribe_account_station_bot("ventrilo", Some(id), true, 2).await.unwrap();
        store.delete_station_bot(id).await.unwrap();
        assert!(store.station_bots().await.unwrap().is_empty());
        assert!(store.account_station_bots().await.unwrap().is_empty());
        assert!(store.active_station("ventrilo").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn the_latest_reading_replaces_the_last_and_the_overview_lists_every_castle() {
        let store = store().await;
        store
            .upsert_account_profile("ventrilo", "Ventrilo", "wss://example/", "EmpireEx_21", 1)
            .await
            .unwrap();
        store
            .replace_account_bootstrap(
                "ventrilo",
                &[crate::account::OwnedCastle {
                    kingdom_id: 0,
                    castle_id: 437391,
                    area_type: 1,
                    x: 543,
                    y: 520,
                    name: "Castle great em".into(),
                }],
                &[1],
                1,
            )
            .await
            .unwrap();
        let before = store.overview_castles("ventrilo").await.unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].observed_at_ms, None, "never read");
        let stock = CastleStock {
            kingdom_id: 0,
            castle_id: 437391,
            units: BTreeMap::from([(602, 9), (614, 5)]),
            food: 637.5,
            mead: 2.0,
            wood: 1.0,
            stone: 2.0,
        };
        store.record_castle_inventory("Ventrilo", std::slice::from_ref(&stock), 100).await.unwrap();
        let newer = CastleStock { units: BTreeMap::from([(602, 4)]), ..stock };
        store.record_castle_inventory("ventrilo", &[newer], 200).await.unwrap();
        let after = store.overview_castles("ventrilo").await.unwrap();
        assert_eq!(after[0].observed_at_ms, Some(200));
        assert_eq!(after[0].units, BTreeMap::from([(602, 4)]));
        assert_eq!(after[0].name, "Castle great em");
        assert_eq!(store.castle_inventory("ventrilo", 437391).await.unwrap().unwrap().0, 200);
    }

    #[tokio::test]
    async fn jobs_are_saved_updated_resumed_and_pruned() {
        let store = store().await;
        let mut first = job(1);
        first.job_id = store.save_station_job(&first).await.unwrap();
        let second = job(2);
        let second_id = store.save_station_job(&second).await.unwrap();
        assert_ne!(first.job_id, second_id);
        assert_eq!(store.open_station_jobs("ventrilo").await.unwrap().len(), 2);

        first.phase = JOB_DONE.into();
        first.outcome = Some("saved".into());
        first.support_mid = Some(156018970);
        first.lead_ms = Some(2_400);
        first.updated_at_ms = 9_000;
        store.save_station_job(&first).await.unwrap();
        let open = store.open_station_jobs("ventrilo").await.unwrap();
        assert_eq!(open.len(), 1, "a finished job is not resumed");
        assert_eq!(open[0].job_id, second_id);
        let recent = store.recent_station_jobs("VENTRILO", 10).await.unwrap();
        let done = recent.iter().find(|row| row.job_id == first.job_id).unwrap();
        assert_eq!((done.support_mid, done.lead_ms), (Some(156018970), Some(2_400)));
        assert_eq!(done.dest, Some((468, 507)));
        assert_eq!(store.prune_station_jobs(10_000 + JOB_RETENTION_MS).await.unwrap(), 1);
        assert_eq!(store.recent_station_jobs("ventrilo", 10).await.unwrap().len(), 1, "open jobs are never pruned");
    }
}
