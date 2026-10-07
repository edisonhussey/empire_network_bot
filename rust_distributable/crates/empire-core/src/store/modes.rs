use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use thiserror::Error;
use uuid::Uuid;

use super::{Store, canonical_account_id};
use crate::planning::{
    Destination, ModeBundle, ModeTaskDraft, PlanError, SourceKind, TargetAlgorithm, TaskDraft,
    TravelMode, VENTRILO_SANDS_HBW,
};

#[derive(Debug, Error)]
pub enum ImportModeError {
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRuntime {
    pub task_id: String,
    pub source_kingdom_id: i64,
    pub source_x: i64,
    pub source_y: i64,
    pub source_kind: SourceKind,
    pub target_kingdom_id: i64,
    pub target_x: i64,
    pub target_y: i64,
    pub travel_mode: String,
    /// Legacy cache retained for database/API compatibility. The direct
    /// runner resolves the real HBW from the source castle at send time.
    pub hbw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeRecord {
    pub mode_id: i64,
    pub name: String,
    pub task_ids: Vec<String>,
    pub allocations: Vec<ModeTaskDraft>,
    pub commander_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountModeRecord {
    pub account_id: String,
    pub mode_id: i64,
    pub running: bool,
}

/// One executable task from the mode currently assigned to an account.
///
/// This is deliberately a compiled view: the network runner does not need to
/// understand the UI's normalized tables, and changes made in the editor are
/// picked up the next time it asks for work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveModeTask {
    pub mode_id: i64,
    pub task_id: String,
    pub name: String,
    pub profile_id: String,
    pub payload: serde_json::Value,
    pub kingdom_id: i64,
    pub level_min: Option<i64>,
    pub level_max: Option<i64>,
    pub source_x: i64,
    pub source_y: i64,
    pub travel_mode: TravelMode,
    pub algorithm: String,
    pub target_kind: String,
    pub commander_lids: Vec<i64>,
    /// Commanders allocated to *other* tasks of the same mode, with the task
    /// that owns each. The allocation is a guarantee, not a wall: when this
    /// task has work and its own commanders are all out, a lender with nothing
    /// to attack may lend its idle ones.
    #[serde(default)]
    pub spare_commanders: Vec<SpareCommander>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpareCommander {
    pub lord_id: i64,
    pub owner_task_id: String,
}

impl Store {
    pub async fn import_mode_bundle(
        &self,
        bundle: &ModeBundle,
        now_ms: i64,
    ) -> Result<i64, ImportModeError> {
        if bundle.name.trim().is_empty() {
            return Err(PlanError::Name.into());
        }
        if bundle.tasks.is_empty() {
            return Err(PlanError::EmptyMode.into());
        }
        let mut compiled = Vec::with_capacity(bundle.tasks.len());
        for task in &bundle.tasks {
            let payload = task.attack.payload()?;
            TaskDraft {
                name: task.name.clone(),
                attack_profile_id: "bundled".to_owned(),
                source: task.source,
                source_kind: task.source_kind,
                destination: task.destination.clone(),
                algorithm: task.algorithm,
                travel: task.travel,
                priority: task.priority,
                commander_count: task.commander_count,
            }
            .validate()?;
            compiled.push((task, payload));
        }

        let mut tx = self.pool.begin().await?;
        let mode = sqlx::query(
            "INSERT INTO automation_mode (name, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?)",
        )
        .bind(bundle.name.trim())
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
        let mode_id = mode.last_insert_rowid();

        for (position, (task, payload)) in compiled.into_iter().enumerate() {
            let profile_id = format!("attack-{}", Uuid::new_v4().simple());
            let task_id = format!("task-{}", Uuid::new_v4().simple());
            sqlx::query(
                "INSERT INTO attack_profile
                    (profile_id, name, payload_json, notes, created_at_ms, updated_at_ms)
                 VALUES (?, ?, ?, '', ?, ?)",
            )
            .bind(&profile_id)
            .bind(task.attack.name.trim())
            .bind(payload.to_string())
            .bind(now_ms)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;

            let algorithm = algorithm_name(task.algorithm);
            let (kingdom_id, minimum, maximum, target, target_kind, filter) = match task.destination
            {
                Destination::Coordinate(value) => (
                    value.kingdom_id,
                    None,
                    None,
                    value,
                    "coordinate",
                    json!({"kingdom_id": value.kingdom_id, "x": value.x, "y": value.y, "algorithm": algorithm}),
                ),
                Destination::RbcLevelRange {
                    kingdom_id,
                    minimum,
                    maximum,
                } => (
                    kingdom_id,
                    Some(minimum),
                    Some(maximum),
                    crate::planning::Coordinate {
                        kingdom_id,
                        x: 0,
                        y: 0,
                    },
                    "rbc",
                    json!({"kingdom_id": kingdom_id, "level_min": minimum, "level_max": maximum, "algorithm": algorithm}),
                ),
                Destination::FortressLevelRange {
                    kingdom_id,
                    minimum,
                    maximum,
                } => (
                    kingdom_id,
                    Some(minimum),
                    Some(maximum),
                    crate::planning::Coordinate {
                        kingdom_id,
                        x: 0,
                        y: 0,
                    },
                    "fortress",
                    json!({"kingdom_id": kingdom_id, "level_min": minimum, "level_max": maximum, "algorithm": algorithm}),
                ),
                Destination::Fortress { kingdom_id } => (
                    kingdom_id,
                    None,
                    None,
                    crate::planning::Coordinate {
                        kingdom_id,
                        x: 0,
                        y: 0,
                    },
                    "fortress",
                    json!({"kingdom_id": kingdom_id, "algorithm": algorithm}),
                ),
                Destination::BerimondCamp { kingdom_id } => (
                    kingdom_id,
                    None,
                    None,
                    crate::planning::Coordinate {
                        kingdom_id,
                        x: 0,
                        y: 0,
                    },
                    "berimond_camp",
                    json!({"kingdom_id": kingdom_id, "algorithm": algorithm}),
                ),
            };
            sqlx::query(
                "INSERT INTO task_definition (
                    task_id, name, kind, kingdom_id, profile_id, target_level_min,
                    target_level_max, commander_count, max_active, priority, enabled,
                    tags, notes, updated_at_ms
                 ) VALUES (?, ?, 'attack', ?, ?, ?, ?, ?, ?, ?, 1, ?, '', ?)",
            )
            .bind(&task_id)
            .bind(task.name.trim())
            .bind(kingdom_id)
            .bind(&profile_id)
            .bind(minimum)
            .bind(maximum)
            .bind(1)
            .bind(1)
            // A fortress is attackable only inside a one-minute window, so
            // fortress work is always the highest priority band regardless of
            // the priority chosen when the task was created.
            .bind(if task.destination.is_fortress() {
                crate::planning::Priority::ExtraHigh.scheduler_value()
            } else {
                task.priority.scheduler_value()
            })
            .bind(target_kind)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO task_runtime (
                    task_id, source_kingdom_id, source_x, source_y,
                    target_kingdom_id, target_x, target_y, travel_mode, hbw, source_kind
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&task_id)
            .bind(task.source.kingdom_id)
            .bind(task.source.x)
            .bind(task.source.y)
            .bind(target.kingdom_id)
            .bind(target.x)
            .bind(target.y)
            .bind(task.travel.as_str())
            .bind(VENTRILO_SANDS_HBW)
            .bind(match task.source_kind {
                SourceKind::Coordinate => "coordinate",
                SourceKind::MainCastle => "main_castle",
            })
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO task_subscription (task_id, target_kind, filter_json, position)
                 VALUES (?, ?, ?, 0)",
            )
            .bind(&task_id)
            .bind(target_kind)
            .bind(filter.to_string())
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO automation_mode_task (mode_id, task_id, position, commander_count)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(mode_id)
            .bind(&task_id)
            .bind(position as i64)
            .bind(task.commander_count)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(mode_id)
    }

    pub async fn upsert_task_runtime(&self, value: &TaskRuntime) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO task_runtime (
                task_id, source_kingdom_id, source_x, source_y, target_kingdom_id,
                target_x, target_y, travel_mode, hbw, source_kind
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(task_id) DO UPDATE SET
                source_kingdom_id = excluded.source_kingdom_id,
                source_x = excluded.source_x,
                source_y = excluded.source_y,
                target_kingdom_id = excluded.target_kingdom_id,
                target_x = excluded.target_x,
                target_y = excluded.target_y,
                travel_mode = excluded.travel_mode,
                hbw = excluded.hbw,
                source_kind = excluded.source_kind",
        )
        .bind(&value.task_id)
        .bind(value.source_kingdom_id)
        .bind(value.source_x)
        .bind(value.source_y)
        .bind(value.target_kingdom_id)
        .bind(value.target_x)
        .bind(value.target_y)
        .bind(&value.travel_mode)
        .bind(value.hbw)
        .bind(match value.source_kind {
            SourceKind::Coordinate => "coordinate",
            SourceKind::MainCastle => "main_castle",
        })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn task_runtimes(&self) -> Result<Vec<TaskRuntime>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT task_id, source_kingdom_id, source_x, source_y,
                    target_kingdom_id, target_x, target_y, travel_mode, hbw, source_kind
             FROM task_runtime ORDER BY task_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TaskRuntime {
                task_id: row.get("task_id"),
                source_kingdom_id: row.get("source_kingdom_id"),
                source_x: row.get("source_x"),
                source_y: row.get("source_y"),
                source_kind: match row.get::<&str, _>("source_kind") {
                    "main_castle" => SourceKind::MainCastle,
                    _ => SourceKind::Coordinate,
                },
                target_kingdom_id: row.get("target_kingdom_id"),
                target_x: row.get("target_x"),
                target_y: row.get("target_y"),
                travel_mode: row.get("travel_mode"),
                hbw: row.get("hbw"),
            })
            .collect())
    }

    pub async fn create_mode(
        &self,
        name: &str,
        allocations: &[ModeTaskDraft],
        now_ms: i64,
    ) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO automation_mode (name, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
        let mode_id = result.last_insert_rowid();
        for (position, allocation) in allocations.iter().enumerate() {
            sqlx::query(
                "INSERT INTO automation_mode_task (mode_id, task_id, position, commander_count)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(mode_id)
            .bind(&allocation.task_id)
            .bind(position as i64)
            .bind(allocation.commander_count)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(mode_id)
    }

    pub async fn modes(&self) -> Result<Vec<ModeRecord>, sqlx::Error> {
        let modes = sqlx::query(
            "SELECT m.mode_id, m.name,
                    COALESCE(SUM(mt.commander_count), 0) AS commander_count
             FROM automation_mode m
             LEFT JOIN automation_mode_task mt ON mt.mode_id = m.mode_id
             LEFT JOIN task_definition t ON t.task_id = mt.task_id
             GROUP BY m.mode_id, m.name ORDER BY m.name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut result = Vec::with_capacity(modes.len());
        for mode in modes {
            let mode_id: i64 = mode.get("mode_id");
            let allocation_rows = sqlx::query(
                "SELECT task_id, commander_count FROM automation_mode_task
                 WHERE mode_id = ? ORDER BY position",
            )
            .bind(mode_id)
            .fetch_all(&self.pool)
            .await?;
            let allocations = allocation_rows
                .into_iter()
                .map(|row| ModeTaskDraft {
                    task_id: row.get("task_id"),
                    commander_count: row.get("commander_count"),
                })
                .collect::<Vec<_>>();
            let task_ids = allocations
                .iter()
                .map(|value| value.task_id.clone())
                .collect();
            result.push(ModeRecord {
                mode_id,
                name: mode.get("name"),
                task_ids,
                allocations,
                commander_count: mode.get("commander_count"),
            });
        }
        Ok(result)
    }

    pub async fn delete_mode(&self, mode_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM automation_mode WHERE mode_id = ?")
            .bind(mode_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn subscribe_account_mode(
        &self,
        account_id: &str,
        mode_id: i64,
        running: bool,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let account_id = account_id.trim().to_ascii_lowercase();
        let available: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT lord_id) FROM account_commander
             WHERE lower(account_id) = lower(?)",
        )
        .bind(&account_id)
        .fetch_one(&self.pool)
        .await?;
        let requested: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(mt.commander_count), 0)
             FROM automation_mode_task mt
             WHERE mt.mode_id = ?",
        )
        .bind(mode_id)
        .fetch_one(&self.pool)
        .await?;
        if requested > available {
            return Err(sqlx::Error::Protocol(format!(
                "mode needs {requested} commanders but account has {available}"
            )));
        }
        let mut tx = self.pool.begin().await?;
        // Older builds preserved username casing and could create visually
        // duplicate subscriptions. Canonicalize the logical account here.
        sqlx::query("DELETE FROM account_mode WHERE lower(account_id) = lower(?)")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO account_mode (account_id, mode_id, running, updated_at_ms)
             VALUES (?, ?, ?, ?)",
        )
        .bind(&account_id)
        .bind(mode_id)
        .bind(i64::from(running))
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn account_modes(&self) -> Result<Vec<AccountModeRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, mode_id, running FROM account_mode ORDER BY account_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| AccountModeRecord {
                account_id: row.get("account_id"),
                mode_id: row.get("mode_id"),
                running: row.get::<i64, _>("running") != 0,
            })
            .collect())
    }

    /// Compile the running mode for one account into ordered executable tasks.
    pub async fn disable_account_automation(&self, account_id: &str) -> Result<(), sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE account_mode SET running = 0 WHERE lower(account_id) = lower(?)")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE account_recruit_bot SET running = 0 WHERE lower(account_id) = lower(?)")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }

    pub async fn active_mode_tasks(
        &self,
        account_id: &str,
    ) -> Result<Vec<ActiveModeTask>, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let rows = sqlx::query(
            "SELECT am.mode_id, mt.task_id, t.name, t.profile_id, p.payload_json,
                    t.kingdom_id, t.target_level_min, t.target_level_max,
                    CASE WHEN r.source_kind = 'main_castle' THEN
                        (SELECT c.x FROM owned_castle c
                         WHERE lower(c.account_id) = lower(am.account_id)
                           AND c.kingdom_id = r.source_kingdom_id
                           AND ((r.source_kingdom_id = 0 AND c.area_type = 1)
                             OR (r.source_kingdom_id <> 0 AND c.area_type = 12))
                         ORDER BY c.castle_id LIMIT 1)
                        ELSE r.source_x END AS resolved_source_x,
                    CASE WHEN r.source_kind = 'main_castle' THEN
                        (SELECT c.y FROM owned_castle c
                         WHERE lower(c.account_id) = lower(am.account_id)
                           AND c.kingdom_id = r.source_kingdom_id
                           AND ((r.source_kingdom_id = 0 AND c.area_type = 1)
                             OR (r.source_kingdom_id <> 0 AND c.area_type = 12))
                         ORDER BY c.castle_id LIMIT 1)
                        ELSE r.source_y END AS resolved_source_y,
                    r.source_kind, r.source_kingdom_id, r.travel_mode,
                    s.target_kind, s.filter_json,
                    mt.commander_count
             FROM account_mode am
             JOIN automation_mode_task mt ON mt.mode_id = am.mode_id
             JOIN task_definition t ON t.task_id = mt.task_id
             JOIN attack_profile p ON p.profile_id = t.profile_id
             JOIN task_runtime r ON r.task_id = t.task_id
             LEFT JOIN task_subscription s ON s.task_id = t.task_id
             WHERE lower(am.account_id) = lower(?) AND am.running = 1
               AND t.enabled = 1 AND t.kind = 'attack'
             GROUP BY mt.task_id
             ORDER BY mt.position, t.priority, t.name COLLATE NOCASE",
        )
        .bind(&account_id)
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let roster = self.commander_lids(&account_id).await?;
        let usable = roster
            .into_iter()
            .filter(|lid| crate::hunt::USABLE_COMMANDER_LIDS.contains(lid))
            .collect::<Vec<_>>();
        let mut cursor = 0usize;
        let mut tasks = Vec::with_capacity(rows.len());
        for row in rows {
            let count =
                usize::try_from(row.get::<i64, _>("commander_count").max(0)).unwrap_or_default();
            let end = cursor.saturating_add(count).min(usable.len());
            let commanders = usable[cursor..end].to_vec();
            cursor = end;
            if commanders.is_empty() {
                continue;
            }
            let payload = serde_json::from_str(row.get::<&str, _>("payload_json"))
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
            let filter: serde_json::Value = serde_json::from_str(row.get::<&str, _>("filter_json"))
                .unwrap_or_else(|_| json!({}));
            let source_x = row
                .try_get::<Option<i64>, _>("resolved_source_x")?
                .ok_or_else(|| {
                    sqlx::Error::Protocol(format!(
                        "{} source is unavailable for account {} in kingdom {}",
                        row.get::<&str, _>("source_kind"),
                        account_id,
                        row.get::<i64, _>("source_kingdom_id")
                    ))
                })?;
            let source_y = row
                .try_get::<Option<i64>, _>("resolved_source_y")?
                .ok_or_else(|| {
                    sqlx::Error::Protocol(format!(
                        "{} source is unavailable for account {} in kingdom {}",
                        row.get::<&str, _>("source_kind"),
                        account_id,
                        row.get::<i64, _>("source_kingdom_id")
                    ))
                })?;
            tasks.push(ActiveModeTask {
                mode_id: row.get("mode_id"),
                task_id: row.get("task_id"),
                name: row.get("name"),
                profile_id: row.get("profile_id"),
                payload,
                kingdom_id: row.get("kingdom_id"),
                level_min: row.get("target_level_min"),
                level_max: row.get("target_level_max"),
                source_x,
                source_y,
                travel_mode: TravelMode::from_stored(row.get("travel_mode")),
                algorithm: filter
                    .get("algorithm")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("advanced")
                    .to_owned(),
                target_kind: row
                    .try_get::<Option<String>, _>("target_kind")?
                    .unwrap_or_else(|| "rbc".to_owned()),
                commander_lids: commanders,
                spare_commanders: Vec::new(),
            });
        }
        let allocation = tasks
            .iter()
            .map(|task| (task.task_id.clone(), task.commander_lids.clone()))
            .collect::<Vec<_>>();
        for task in &mut tasks {
            task.spare_commanders = allocation
                .iter()
                .filter(|(owner, _)| *owner != task.task_id)
                .flat_map(|(owner, lids)| {
                    lids.iter().map(|lord_id| SpareCommander {
                        lord_id: *lord_id,
                        owner_task_id: owner.clone(),
                    })
                })
                .collect();
        }
        Ok(tasks)
    }

    pub async fn account_mode_running(&self, account_id: &str) -> Result<bool, sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let running: Option<i64> = sqlx::query_scalar(
            "SELECT running FROM account_mode WHERE lower(account_id) = lower(?)",
        )
        .bind(&account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(running.is_some_and(|value| value != 0))
    }

    pub async fn stop_account_mode(
        &self,
        account_id: &str,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        sqlx::query(
            "UPDATE account_mode SET running = 0, updated_at_ms = ?
             WHERE lower(account_id) = lower(?)",
        )
        .bind(now_ms)
        .bind(&account_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

const fn algorithm_name(value: TargetAlgorithm) -> &'static str {
    match value {
        TargetAlgorithm::Advanced => "advanced",
        TargetAlgorithm::Closest => "closest",
        TargetAlgorithm::Random => "random",
    }
}
