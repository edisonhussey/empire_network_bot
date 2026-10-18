//! HTTP side of auto-station: support bots, the Overview and the job history.
//!
//! The window never sends game packets. Creating a bot only stores destinations;
//! asking for an Overview refresh only leaves a note for the session loop
//! (`direct/inventory.rs`), which decides when a `dcl` is safe to send.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use empire_core::{
    station::{UnitKind, unit_label},
    store::{OverviewCastle, STATION_KINGDOMS, StationBotKingdom, StationJobRow},
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState, now_ms, plans::CreatedId};

/// The game map is square; coordinates outside it are typing mistakes.
const MAP_LIMIT: i64 = 1_300;

#[derive(Debug, Deserialize)]
pub struct StationBotDraft {
    name: String,
    /// One destination per kingdom. A kingdom left out has none, and a castle
    /// there is never stationed.
    kingdoms: Vec<StationBotKingdom>,
}

/// The name and destinations a draft is allowed to store, or why not.
fn validated(draft: &StationBotDraft) -> Result<(String, Vec<StationBotKingdom>), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("a name is required".to_owned());
    }
    if draft.kingdoms.is_empty() {
        return Err("give at least one kingdom a destination".to_owned());
    }
    let mut seen = std::collections::HashSet::new();
    for kingdom in &draft.kingdoms {
        if !STATION_KINGDOMS.contains(&kingdom.kingdom_id) {
            return Err(format!("kingdom {} cannot be stationed from", kingdom.kingdom_id));
        }
        if !seen.insert(kingdom.kingdom_id) {
            return Err(format!("kingdom {} appears twice", kingdom.kingdom_id));
        }
        if !(0..=MAP_LIMIT).contains(&kingdom.x) || !(0..=MAP_LIMIT).contains(&kingdom.y) {
            return Err(format!(
                "kingdom {}: coordinates must be between 0 and {MAP_LIMIT}",
                kingdom.kingdom_id
            ));
        }
    }
    Ok((name.to_owned(), draft.kingdoms.clone()))
}

pub async fn create_station_bot(
    State(state): State<AppState>,
    Json(draft): Json<StationBotDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    state.licence.require("automation").await.map_err(ApiError::forbidden)?;
    let (name, kingdoms) = validated(&draft).map_err(ApiError::bad_request)?;
    let id = state
        .store
        .create_station_bot(&name, &kingdoms, now_ms())
        .await
        .map_err(|error| {
            if error.to_string().contains("UNIQUE constraint failed") {
                ApiError::bad_request("that name is already used — pick another")
            } else {
                ApiError::internal(error)
            }
        })?;
    Ok((StatusCode::CREATED, Json(CreatedId::new(id))))
}

pub async fn delete_station_bot(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.licence.require("automation").await.map_err(ApiError::forbidden)?;
    // An attack bot that carries this support would be left pointing at nothing.
    let users = state.store.modes_using_support(id).await?;
    if !users.is_empty() {
        return Err(ApiError::bad_request(format!(
            "Still used by attack bot {}. Remove it from that bot first.",
            users.join(", ")
        )));
    }
    state.store.delete_station_bot(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AccountParam {
    account_id: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct UnitCount {
    id: i64,
    name: String,
    count: i64,
}

#[derive(Debug, Serialize)]
pub struct OverviewRow {
    kingdom_id: i64,
    castle_id: i64,
    name: String,
    x: i64,
    y: i64,
    /// `None` until the castle has been read once.
    observed_at_ms: Option<i64>,
    troops: Vec<UnitCount>,
    tools: Vec<UnitCount>,
    /// Ids in the stock the game tables do not know.
    other: Vec<UnitCount>,
    food: f64,
    mead: f64,
    wood: f64,
    stone: f64,
}

#[derive(Debug, Serialize)]
pub struct OverviewResponse {
    account_id: String,
    connected: bool,
    now_ms: i64,
    /// When the window last asked for a reading that has not arrived yet.
    refresh_pending_since_ms: Option<i64>,
    castles: Vec<OverviewRow>,
}

/// Split a castle's stock into troops, tools and unknown ids, largest first,
/// dropping zero counts (they are not available).
fn overview_row(castle: OverviewCastle) -> OverviewRow {
    let mut troops = Vec::new();
    let mut tools = Vec::new();
    let mut other = Vec::new();
    for (id, count) in castle.units.iter().filter(|(_, count)| **count > 0) {
        let (kind, name) = unit_label(*id);
        let entry = UnitCount { id: *id, name, count: *count };
        match kind {
            UnitKind::Troop => troops.push(entry),
            UnitKind::Tool => tools.push(entry),
            UnitKind::Unknown => other.push(entry),
        }
    }
    for list in [&mut troops, &mut tools, &mut other] {
        list.sort_by(|a, b| b.count.cmp(&a.count).then(a.id.cmp(&b.id)));
    }
    OverviewRow {
        kingdom_id: castle.kingdom_id,
        castle_id: castle.castle_id,
        name: castle.name,
        x: castle.x,
        y: castle.y,
        observed_at_ms: castle.observed_at_ms,
        troops,
        tools,
        other,
        food: castle.food,
        mead: castle.mead,
        wood: castle.wood,
        stone: castle.stone,
    }
}

pub async fn overview(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
) -> Result<Json<OverviewResponse>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    let connected = state
        .sessions
        .status(&account_id)
        .await
        .is_some_and(|status| status.connected);
    let castles = state.store.overview_castles(&account_id).await?;
    let newest = castles.iter().filter_map(|castle| castle.observed_at_ms).max().unwrap_or(0);
    let requested = state
        .store
        .app_state(&crate::direct::overview_refresh_key(&account_id))
        .await?
        .and_then(|value| value.as_i64())
        .filter(|requested| *requested > newest);
    Ok(Json(OverviewResponse {
        account_id,
        connected,
        now_ms: now_ms(),
        refresh_pending_since_ms: requested,
        castles: castles.into_iter().map(overview_row).collect(),
    }))
}

/// Ask the session loop for a fresh reading. It sends a `dcl` when the connection
/// is free, never more often than every fifteen seconds. The request is per account:
/// each session looks only at its own key.
pub async fn refresh_overview(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
) -> Result<StatusCode, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    if !state.sessions.status(&account_id).await.is_some_and(|status| status.connected) {
        return Err(ApiError::bad_request("connect this account to read its castles"));
    }
    let now = now_ms();
    state
        .store
        .set_app_state(&crate::direct::overview_refresh_key(&account_id), &serde_json::json!(now), now)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn jobs(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
) -> Result<Json<Vec<StationJobRow>>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    Ok(Json(
        state
            .store
            .recent_station_jobs(&account_id, params.limit.unwrap_or(30))
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn draft(kingdoms: &[(i64, i64, i64)]) -> StationBotDraft {
        StationBotDraft {
            name: " Quiet corner ".into(),
            kingdoms: kingdoms
                .iter()
                .map(|(kingdom_id, x, y)| StationBotKingdom { kingdom_id: *kingdom_id, x: *x, y: *y })
                .collect(),
        }
    }

    #[test]
    fn a_bot_needs_a_name_and_a_destination_somewhere() {
        let (name, kingdoms) = validated(&draft(&[(0, 468, 507)])).unwrap();
        assert_eq!(name, "Quiet corner");
        assert_eq!(kingdoms.len(), 1);
        assert!(validated(&draft(&[])).is_err());
        let mut nameless = draft(&[(0, 1, 1)]);
        nameless.name = "  ".into();
        assert!(validated(&nameless).is_err());
    }

    #[test]
    fn only_the_permanent_kingdoms_one_destination_each_inside_the_map() {
        assert!(validated(&draft(&[(10, 5, 5)])).is_err(), "Berimond has no stations");
        assert!(validated(&draft(&[(1, 5, 5), (1, 6, 6)])).is_err(), "twice");
        assert!(validated(&draft(&[(1, -1, 5)])).is_err());
        assert!(validated(&draft(&[(1, 5, 1_301)])).is_err());
        assert!(validated(&draft(&[(0, 0, 0), (1, 1_300, 1_300), (4, 600, 600)])).is_ok());
    }

    #[test]
    fn the_overview_splits_troops_from_tools_largest_first_and_drops_empty_rows() {
        let castle = OverviewCastle {
            kingdom_id: 0,
            castle_id: 437391,
            name: "Castle great em".into(),
            x: 543,
            y: 520,
            observed_at_ms: Some(5),
            units: BTreeMap::from([(602, 9), (637, 0), (614, 5), (660, 3), (999_999, 2)]),
            food: 637.14,
            mead: 0.0,
            wood: 1.0,
            stone: 2.0,
        };
        let row = overview_row(castle);
        assert_eq!(row.troops.len(), 1);
        assert_eq!((row.troops[0].id, row.troops[0].count), (602, 9));
        assert_eq!(row.tools.iter().map(|tool| tool.id).collect::<Vec<_>>(), vec![614, 660], "637 has none");
        // Names are the ones the player reads, from the game's own unit table (it was `scaling_ladder`).
        assert_eq!(row.tools[0].name, "Scaling ladder");
        assert_eq!(row.troops[0].name, "Spearman");
        assert_eq!(row.other.len(), 1, "an id the game does not define stays in Other");
        assert!((row.food - 637.14).abs() < 1e-9);
    }
}
