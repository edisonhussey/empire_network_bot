//! Resource bot persistence: the create-resource presets, the resource bot that carries one, the
//! account's choice, and the refinery schedule.
//!
//! * A *create-resource* preset names a building and the recipes to cycle there (today only the
//!   `refinery`, with refined wood and/or refined stone; the toolsmith will be a second building).
//! * A *resource bot* bundles the jobs a resource bot does. Today: at most one create-resource
//!   preset. Later: resource movement and transfers, as more columns or child tables.
//! * `account_resource_bot` is the control panel's choice for an account, like the support bot.
//! * `refinery_state` is the schedule, so the next visit and the wood/stone alternation survive a
//!   restart or a dropped connection (`docs/architecture/resource-bot.md`).

use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::{Store, canonical_account_id};
use crate::refinery::RefineRecipe;

/// The one building presets can name so far.
pub const REFINERY: &str = "refinery";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateResourcePreset {
    pub preset_id: i64,
    pub name: String,
    pub building: String,
    pub recipes: Vec<RefineRecipe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceBot {
    pub resource_bot_id: i64,
    pub name: String,
    /// The create-resource job, or `None` for a bot that does not craft.
    pub create_preset_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountResourceBot {
    pub account_id: String,
    pub resource_bot_id: i64,
    pub running: bool,
    pub updated_at_ms: i64,
}

/// A running resource bot's crafting job, ready to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRefinery {
    pub resource_bot_id: i64,
    pub recipes: Vec<RefineRecipe>,
}

/// The account's refinery schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct RefineryState {
    pub castle_id: i64,
    /// Not before this time is the castle visited again.
    pub next_due_ms: i64,
    /// Where the recipe cycle continues (so wood and stone alternate across visits).
    pub cycle_index: i64,
    pub last_visit_ms: i64,
    pub last_detail: String,
}

fn encode_recipes(recipes: &[RefineRecipe]) -> String {
    recipes.iter().map(|recipe| recipe.crid().to_string()).collect::<Vec<_>>().join(",")
}

fn decode_recipes(text: &str) -> Vec<RefineRecipe> {
    text.split(',').filter_map(|crid| crid.trim().parse().ok()).filter_map(RefineRecipe::from_crid).collect()
}

impl Store {
    pub async fn create_resource_preset(
        &self,
        name: &str,
        building: &str,
        recipes: &[RefineRecipe],
        now_ms: i64,
    ) -> Result<i64, sqlx::Error> {
        Ok(sqlx::query(
            "INSERT INTO create_resource_preset (name, building, recipes, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(name.trim())
        .bind(building)
        .bind(encode_recipes(recipes))
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?
        .last_insert_rowid())
    }

    pub async fn create_resource_presets(&self) -> Result<Vec<CreateResourcePreset>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT preset_id, name, building, recipes FROM create_resource_preset
             ORDER BY updated_at_ms DESC, preset_id DESC",
        )
        .fetch_all(self.reader())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| CreateResourcePreset {
                preset_id: row.get("preset_id"),
                name: row.get("name"),
                building: row.get("building"),
                recipes: decode_recipes(&row.get::<String, _>("recipes")),
            })
            .collect())
    }

    /// Refused (with the bots' names) while a resource bot still uses the preset.
    pub async fn delete_resource_preset(&self, id: i64) -> Result<Vec<String>, sqlx::Error> {
        let users: Vec<String> = sqlx::query_scalar("SELECT name FROM resource_bot WHERE create_preset_id = ?")
            .bind(id)
            .fetch_all(self.reader())
            .await?;
        if users.is_empty() {
            sqlx::query("DELETE FROM create_resource_preset WHERE preset_id = ?").bind(id).execute(&self.pool).await?;
        }
        Ok(users)
    }

    pub async fn create_resource_bot(
        &self,
        name: &str,
        create_preset_id: Option<i64>,
        now_ms: i64,
    ) -> Result<i64, sqlx::Error> {
        Ok(sqlx::query(
            "INSERT INTO resource_bot (name, create_preset_id, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(name.trim())
        .bind(create_preset_id)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await?
        .last_insert_rowid())
    }

    pub async fn resource_bots(&self) -> Result<Vec<ResourceBot>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT resource_bot_id, name, create_preset_id FROM resource_bot
             ORDER BY updated_at_ms DESC, resource_bot_id DESC",
        )
        .fetch_all(self.reader())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ResourceBot {
                resource_bot_id: row.get("resource_bot_id"),
                name: row.get("name"),
                create_preset_id: row.get("create_preset_id"),
            })
            .collect())
    }

    /// Removes the bot and any account using it.
    pub async fn delete_resource_bot(&self, id: i64) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM account_resource_bot WHERE resource_bot_id = ?").bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM resource_bot WHERE resource_bot_id = ?").bind(id).execute(&mut *tx).await?;
        tx.commit().await
    }

    /// Choose (or clear, with `None`) the account's resource bot.
    pub async fn subscribe_account_resource_bot(
        &self,
        account_id: &str,
        bot_id: Option<i64>,
        running: bool,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM account_resource_bot WHERE account_id = ?").bind(&account_id).execute(&mut *tx).await?;
        if let Some(bot_id) = bot_id {
            sqlx::query(
                "INSERT INTO account_resource_bot (account_id, resource_bot_id, running, updated_at_ms) VALUES (?, ?, ?, ?)",
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

    pub async fn account_resource_bots(&self) -> Result<Vec<AccountResourceBot>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, resource_bot_id, running, updated_at_ms FROM account_resource_bot ORDER BY updated_at_ms DESC",
        )
        .fetch_all(self.reader())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| AccountResourceBot {
                account_id: row.get("account_id"),
                resource_bot_id: row.get("resource_bot_id"),
                running: row.get::<i64, _>("running") != 0,
                updated_at_ms: row.get("updated_at_ms"),
            })
            .collect())
    }

    /// Stop the account's resource bot without forgetting which one it was.
    pub async fn stop_account_resource_bot(&self, account_id: &str, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE account_resource_bot SET running = 0, updated_at_ms = ? WHERE account_id = ?")
            .bind(now_ms)
            .bind(canonical_account_id(account_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The crafting job of the account's running resource bot, if it has one with recipes.
    pub async fn active_refinery(&self, account_id: &str) -> Result<Option<ActiveRefinery>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT b.resource_bot_id, p.recipes FROM account_resource_bot a
             JOIN resource_bot b ON b.resource_bot_id = a.resource_bot_id
             JOIN create_resource_preset p ON p.preset_id = b.create_preset_id
             WHERE a.account_id = ? AND a.running = 1 AND p.building = ?",
        )
        .bind(canonical_account_id(account_id))
        .bind(REFINERY)
        .fetch_optional(self.reader())
        .await?;
        Ok(row.and_then(|row| {
            let recipes = decode_recipes(&row.get::<String, _>("recipes"));
            (!recipes.is_empty()).then(|| ActiveRefinery { resource_bot_id: row.get("resource_bot_id"), recipes })
        }))
    }

    /// The account's main castle: Great Empire (kingdom 0), area type 1. Only it has the refinery the bot uses.
    pub async fn green_main_castle(&self, account_id: &str) -> Result<Option<(i64, i64, i64, String)>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT castle_id, x, y, name FROM owned_castle WHERE account_id = ? AND kingdom_id = 0 AND area_type = 1
             ORDER BY castle_id LIMIT 1",
        )
        .bind(canonical_account_id(account_id))
        .fetch_optional(self.reader())
        .await?;
        Ok(row.map(|row| (row.get("castle_id"), row.get("x"), row.get("y"), row.get("name"))))
    }

    pub async fn refinery_state(&self, account_id: &str) -> Result<Option<RefineryState>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT castle_id, next_due_ms, cycle_index, last_visit_ms, last_detail FROM refinery_state WHERE account_id = ?",
        )
        .bind(canonical_account_id(account_id))
        .fetch_optional(self.reader())
        .await?;
        Ok(row.map(|row| RefineryState {
            castle_id: row.get("castle_id"),
            next_due_ms: row.get("next_due_ms"),
            cycle_index: row.get("cycle_index"),
            last_visit_ms: row.get("last_visit_ms"),
            last_detail: row.get("last_detail"),
        }))
    }

    pub async fn all_refinery_states(&self) -> Result<Vec<(String, RefineryState)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, castle_id, next_due_ms, cycle_index, last_visit_ms, last_detail FROM refinery_state",
        )
        .fetch_all(self.reader())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get("account_id"),
                    RefineryState {
                        castle_id: row.get("castle_id"),
                        next_due_ms: row.get("next_due_ms"),
                        cycle_index: row.get("cycle_index"),
                        last_visit_ms: row.get("last_visit_ms"),
                        last_detail: row.get("last_detail"),
                    },
                )
            })
            .collect())
    }

    pub async fn set_refinery_state(&self, account_id: &str, state: &RefineryState, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO refinery_state (account_id, castle_id, next_due_ms, cycle_index, last_visit_ms, last_detail, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET castle_id = excluded.castle_id, next_due_ms = excluded.next_due_ms,
               cycle_index = excluded.cycle_index, last_visit_ms = excluded.last_visit_ms,
               last_detail = excluded.last_detail, updated_at_ms = excluded.updated_at_ms",
        )
        .bind(canonical_account_id(account_id))
        .bind(state.castle_id)
        .bind(state.next_due_ms)
        .bind(state.cycle_index)
        .bind(state.last_visit_ms)
        .bind(&state.last_detail)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RefineRecipe::*;

    async fn store() -> Store {
        Store::open("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn a_running_bot_with_a_refinery_preset_is_active_and_stops_without_forgetting() {
        let store = store().await;
        let preset = store.create_resource_preset("both", REFINERY, &[RefinedWood, RefinedStone], 1).await.unwrap();
        let bot = store.create_resource_bot("res", Some(preset), 1).await.unwrap();
        assert!(store.active_refinery("Ventrilo").await.unwrap().is_none());
        store.subscribe_account_resource_bot("Ventrilo", Some(bot), true, 2).await.unwrap();
        let active = store.active_refinery("ventrilo").await.unwrap().unwrap();
        assert_eq!(active.recipes, vec![RefinedWood, RefinedStone]);
        store.stop_account_resource_bot("ventrilo", 3).await.unwrap();
        assert!(store.active_refinery("ventrilo").await.unwrap().is_none());
        assert_eq!(store.account_resource_bots().await.unwrap()[0].resource_bot_id, bot);
    }

    #[tokio::test]
    async fn a_preset_in_use_is_not_deleted_and_a_deleted_bot_takes_its_choice_with_it() {
        let store = store().await;
        let preset = store.create_resource_preset("wood", REFINERY, &[RefinedWood], 1).await.unwrap();
        let bot = store.create_resource_bot("res", Some(preset), 1).await.unwrap();
        store.subscribe_account_resource_bot("ventrilo", Some(bot), true, 2).await.unwrap();
        assert_eq!(store.delete_resource_preset(preset).await.unwrap(), vec!["res".to_owned()]);
        store.delete_resource_bot(bot).await.unwrap();
        assert!(store.account_resource_bots().await.unwrap().is_empty());
        assert!(store.delete_resource_preset(preset).await.unwrap().is_empty());
        assert!(store.create_resource_presets().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_schedule_survives_as_written() {
        let store = store().await;
        assert!(store.refinery_state("ventrilo").await.unwrap().is_none());
        let state = RefineryState { castle_id: 7, next_due_ms: 99, cycle_index: 1, last_visit_ms: 5, last_detail: "ok".into() };
        store.set_refinery_state("Ventrilo", &state, 6).await.unwrap();
        assert_eq!(store.refinery_state("ventrilo").await.unwrap(), Some(state));
    }
}
