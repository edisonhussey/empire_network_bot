use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::{Store, canonical_account_id};
use super::state::RecruitCastleState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecruitmentTemplate {
    pub recruitment_id: String,
    pub name: String,
    pub troop_id: i64,
    pub quantity: i64,
    pub slot_count: i64,
    pub ask_alliance_help: bool,
    pub lane_id: i64,
    pub skill_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecruitBotCastle {
    pub account_id: String,
    pub castle_id: i64,
    pub recruitment_id: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecruitBot {
    pub recruit_bot_id: i64,
    pub name: String,
    pub algorithm: String,
    pub castles: Vec<RecruitBotCastle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRecruitBot {
    pub account_id: String,
    pub recruit_bot_id: i64,
    pub running: bool,
    /// When this bot was last chosen for the account, so the Start tab can offer
    /// the most recently used ones first.
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedCastleRecord {
    pub account_id: String,
    pub castle_id: i64,
    pub kingdom_id: i64,
    pub area_type: i64,
    pub x: i64,
    pub y: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRecruitment {
    pub recruit_bot_id: i64,
    pub algorithm: String,
    pub account_id: String,
    pub castle_id: i64,
    pub kingdom_id: i64,
    pub castle_x: i64,
    pub castle_y: i64,
    pub recruitment_id: String,
    pub troop_id: i64,
    pub quantity: i64,
    pub slot_count: i64,
    pub ask_alliance_help: bool,
    pub lane_id: i64,
    pub skill_id: i64,
    pub queue_clear_at_ms: i64,
}

impl Store {
    pub async fn upsert_recruitment(
        &self,
        value: &RecruitmentTemplate,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO recruitment_template
            (recruitment_id, name, troop_id, quantity, slot_count, ask_alliance_help, lane_id, skill_id, created_at_ms, updated_at_ms)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&value.recruitment_id).bind(value.name.trim()).bind(value.troop_id)
            .bind(value.quantity).bind(value.slot_count).bind(i64::from(value.ask_alliance_help))
            .bind(value.lane_id).bind(value.skill_id).bind(now_ms).bind(now_ms)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn recruitments(&self) -> Result<Vec<RecruitmentTemplate>, sqlx::Error> {
        let rows = sqlx::query("SELECT recruitment_id, name, troop_id, quantity, slot_count,
            ask_alliance_help, lane_id, skill_id FROM recruitment_template ORDER BY updated_at_ms DESC")
            .fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| RecruitmentTemplate {
                recruitment_id: row.get("recruitment_id"),
                name: row.get("name"),
                troop_id: row.get("troop_id"),
                quantity: row.get("quantity"),
                slot_count: row.get("slot_count"),
                ask_alliance_help: row.get::<i64, _>("ask_alliance_help") != 0,
                lane_id: row.get("lane_id"),
                skill_id: row.get("skill_id"),
            })
            .collect())
    }

    pub async fn delete_recruitment(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM recruitment_template WHERE recruitment_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_recruit_bot(
        &self,
        name: &str,
        algorithm: &str,
        castles: &[RecruitBotCastle],
        now_ms: i64,
    ) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query("INSERT INTO recruit_bot (name, algorithm, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?)")
            .bind(name.trim()).bind(algorithm).bind(now_ms).bind(now_ms).execute(&mut *tx).await?;
        let id = result.last_insert_rowid();
        for (position, castle) in castles.iter().enumerate() {
            sqlx::query("INSERT INTO recruit_bot_castle (recruit_bot_id, account_id, castle_id, recruitment_id, position) VALUES (?, ?, ?, ?, ?)")
                .bind(id).bind(&castle.account_id).bind(castle.castle_id).bind(&castle.recruitment_id)
                .bind(i64::try_from(position).unwrap_or(i64::MAX)).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    pub async fn recruit_bots(&self) -> Result<Vec<RecruitBot>, sqlx::Error> {
        let bots = sqlx::query(
            "SELECT recruit_bot_id, name, algorithm FROM recruit_bot ORDER BY updated_at_ms DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut result = Vec::with_capacity(bots.len());
        for bot in bots {
            let id: i64 = bot.get("recruit_bot_id");
            let rows = sqlx::query("SELECT account_id, castle_id, recruitment_id, position FROM recruit_bot_castle WHERE recruit_bot_id = ? ORDER BY position")
                .bind(id).fetch_all(&self.pool).await?;
            result.push(RecruitBot {
                recruit_bot_id: id,
                name: bot.get("name"),
                algorithm: bot.get("algorithm"),
                castles: rows
                    .into_iter()
                    .map(|row| RecruitBotCastle {
                        account_id: row.get("account_id"),
                        castle_id: row.get("castle_id"),
                        recruitment_id: row.get("recruitment_id"),
                        position: row.get("position"),
                    })
                    .collect(),
            });
        }
        Ok(result)
    }

    pub async fn delete_recruit_bot(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM recruit_bot WHERE recruit_bot_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn owned_castles(&self) -> Result<Vec<OwnedCastleRecord>, sqlx::Error> {
        let rows = sqlx::query("SELECT lower(account_id) account_id, castle_id, kingdom_id, area_type, x, y, name FROM owned_castle ORDER BY account_id, kingdom_id, area_type, name")
            .fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| OwnedCastleRecord {
                account_id: row.get("account_id"),
                castle_id: row.get("castle_id"),
                kingdom_id: row.get("kingdom_id"),
                area_type: row.get("area_type"),
                x: row.get("x"),
                y: row.get("y"),
                name: row.get("name"),
            })
            .collect())
    }

    pub async fn subscribe_account_recruit_bot(
        &self,
        account_id: &str,
        bot_id: Option<i64>,
        running: bool,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        sqlx::query("DELETE FROM account_recruit_bot WHERE lower(account_id) = lower(?)")
            .bind(&account_id)
            .execute(&self.pool)
            .await?;
        if let Some(bot_id) = bot_id {
            sqlx::query("INSERT INTO account_recruit_bot (account_id, recruit_bot_id, running, updated_at_ms) VALUES (?, ?, ?, ?)")
                .bind(&account_id).bind(bot_id).bind(i64::from(running)).bind(now_ms).execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn account_recruit_bots(&self) -> Result<Vec<AccountRecruitBot>, sqlx::Error> {
        let rows = sqlx::query("SELECT account_id, recruit_bot_id, running, updated_at_ms FROM account_recruit_bot ORDER BY updated_at_ms DESC").fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| AccountRecruitBot {
                account_id: row.get("account_id"),
                recruit_bot_id: row.get("recruit_bot_id"),
                running: row.get::<i64, _>("running") != 0,
                updated_at_ms: row.get("updated_at_ms"),
            })
            .collect())
    }

    /// Per-castle recruitment state for every account.
    ///
    /// The Start and recruit-bot views need the server's own queue estimate for
    /// each castle, so they can say when a castle will finish and be free again.
    pub async fn all_recruit_states(&self) -> Result<Vec<RecruitCastleState>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, castle_id, task_id, queue_clear_at_ms, last_duration_s,
                    last_request_at_ms, active_quantity, queued_quantity, help_active,
                    last_status
             FROM recruit_castle_state ORDER BY lower(account_id), castle_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RecruitCastleState {
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
            })
            .collect())
    }

    pub async fn active_recruitments(
        &self,
        account_id: &str,
    ) -> Result<Vec<ActiveRecruitment>, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query("SELECT rb.recruit_bot_id, rb.algorithm, lower(rbc.account_id) account_id,
            rbc.castle_id, c.kingdom_id, c.x castle_x, c.y castle_y, rt.recruitment_id,
            rt.troop_id, rt.quantity, rt.slot_count, rt.ask_alliance_help, rt.lane_id, rt.skill_id,
            COALESCE(rcs.queue_clear_at_ms, 0) queue_clear_at_ms
            FROM account_recruit_bot arb JOIN recruit_bot rb ON rb.recruit_bot_id = arb.recruit_bot_id
            JOIN recruit_bot_castle rbc ON rbc.recruit_bot_id = rb.recruit_bot_id AND lower(rbc.account_id) = lower(arb.account_id)
            JOIN recruitment_template rt ON rt.recruitment_id = rbc.recruitment_id
            JOIN owned_castle c ON lower(c.account_id) = lower(rbc.account_id) AND c.castle_id = rbc.castle_id
            LEFT JOIN recruit_castle_state rcs ON lower(rcs.account_id) = lower(rbc.account_id) AND rcs.castle_id = rbc.castle_id
            WHERE lower(arb.account_id) = lower(?) AND arb.running = 1 ORDER BY rbc.position")
            .bind(&account_id).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| ActiveRecruitment {
                recruit_bot_id: row.get("recruit_bot_id"),
                algorithm: row.get("algorithm"),
                account_id: row.get("account_id"),
                castle_id: row.get("castle_id"),
                kingdom_id: row.get("kingdom_id"),
                castle_x: row.get("castle_x"),
                castle_y: row.get("castle_y"),
                recruitment_id: row.get("recruitment_id"),
                troop_id: row.get("troop_id"),
                quantity: row.get("quantity"),
                slot_count: row.get("slot_count"),
                ask_alliance_help: row.get::<i64, _>("ask_alliance_help") != 0,
                lane_id: row.get("lane_id"),
                skill_id: row.get("skill_id"),
                queue_clear_at_ms: row.get("queue_clear_at_ms"),
            })
            .collect())
    }

    pub async fn stop_account_recruit_bot(
        &self,
        account_id: &str,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        sqlx::query("UPDATE account_recruit_bot SET running = 0, updated_at_ms = ? WHERE lower(account_id) = lower(?)").bind(now_ms).bind(&account_id).execute(&self.pool).await?;
        Ok(())
    }
}
