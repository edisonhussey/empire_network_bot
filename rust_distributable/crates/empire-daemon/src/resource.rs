//! HTTP side of the resource bot: create-resource presets, resource bots and the refinery schedule.
//!
//! The window never sends game packets. These routes only store what the bot should do; the session
//! loop (`direct/refinery.rs`) decides when it is safe to visit the castle. The control panel picks a
//! resource bot through `POST /v1/plans/start` (`resource_bot_id`), with the other bots.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use empire_core::{
    refinery::RefineRecipe,
    store::{AccountResourceBot, CreateResourcePreset, REFINERY, RefineryState, ResourceBot},
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState, now_ms, plans::{CreatedId, require_plans}};

#[derive(Serialize)]
pub struct ResourceLibrary {
    create_presets: Vec<CreateResourcePreset>,
    resource_bots: Vec<ResourceBot>,
    account_resource_bots: Vec<AccountResourceBot>,
    /// Per account: the next refinery visit and what the last one did.
    refinery_states: Vec<AccountRefineryState>,
}

#[derive(Serialize)]
pub struct AccountRefineryState {
    account_id: String,
    #[serde(flatten)]
    state: RefineryState,
}

#[derive(Deserialize)]
pub struct CreatePresetDraft {
    name: String,
    /// Only `refinery` today; the toolsmith comes later with its own recipes.
    #[serde(default = "refinery")]
    building: String,
    recipes: Vec<RefineRecipe>,
}

fn refinery() -> String {
    REFINERY.to_owned()
}

#[derive(Deserialize)]
pub struct ResourceBotDraft {
    name: String,
    create_preset_id: Option<i64>,
}

fn duplicate_name(error: sqlx::Error) -> ApiError {
    if error.to_string().contains("UNIQUE constraint failed") {
        return ApiError::bad_request("that name is already used — pick another");
    }
    ApiError::internal(error)
}

pub async fn library(State(state): State<AppState>) -> Result<Json<ResourceLibrary>, ApiError> {
    require_plans(&state).await?;
    let store = &state.store;
    Ok(Json(ResourceLibrary {
        create_presets: store.create_resource_presets().await.map_err(ApiError::internal)?,
        resource_bots: store.resource_bots().await.map_err(ApiError::internal)?,
        account_resource_bots: store.account_resource_bots().await.map_err(ApiError::internal)?,
        refinery_states: store
            .all_refinery_states()
            .await
            .map_err(ApiError::internal)?
            .into_iter()
            .map(|(account_id, state)| AccountRefineryState { account_id, state })
            .collect(),
    }))
}

pub async fn create_preset(
    State(state): State<AppState>,
    Json(draft): Json<CreatePresetDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    require_plans(&state).await?;
    if draft.name.trim().is_empty() {
        return Err(ApiError::bad_request("a name is required"));
    }
    if draft.building != REFINERY {
        return Err(ApiError::bad_request("only the refinery can be chosen for now"));
    }
    let mut recipes = Vec::new();
    for recipe in draft.recipes {
        if !recipes.contains(&recipe) {
            recipes.push(recipe);
        }
    }
    if recipes.is_empty() {
        return Err(ApiError::bad_request("tick refined wood, refined stone or both"));
    }
    let id = state
        .store
        .create_resource_preset(&draft.name, REFINERY, &recipes, now_ms())
        .await
        .map_err(duplicate_name)?;
    Ok((StatusCode::CREATED, Json(CreatedId::new(id))))
}

pub async fn delete_preset(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    let users = state.store.delete_resource_preset(id).await.map_err(ApiError::internal)?;
    if !users.is_empty() {
        return Err(ApiError::bad_request(format!(
            "Still used by resource bot {}. Remove it from that bot first.",
            users.join(", ")
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_bot(
    State(state): State<AppState>,
    Json(draft): Json<ResourceBotDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    require_plans(&state).await?;
    if draft.name.trim().is_empty() {
        return Err(ApiError::bad_request("a name is required"));
    }
    let Some(preset_id) = draft.create_preset_id else {
        return Err(ApiError::bad_request("choose a create-resource preset"));
    };
    let presets = state.store.create_resource_presets().await.map_err(ApiError::internal)?;
    if !presets.iter().any(|preset| preset.preset_id == preset_id) {
        return Err(ApiError::bad_request("that create-resource preset no longer exists"));
    }
    let id = state.store.create_resource_bot(&draft.name, Some(preset_id), now_ms()).await.map_err(duplicate_name)?;
    Ok((StatusCode::CREATED, Json(CreatedId::new(id))))
}

pub async fn delete_bot(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    state.store.delete_resource_bot(id).await.map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
