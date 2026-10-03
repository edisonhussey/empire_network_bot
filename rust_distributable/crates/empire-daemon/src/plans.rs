use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use empire_core::{
    planning::{
        AttackDraft, Destination, KingdomOption, ModeBundle, ModeDraft, TaskDraft,
        VENTRILO_SANDS_HBW, catalog, kingdoms, ventrilo_sands_bundle,
    },
    store::{
        AccountModeRecord, AttackProfile, ModeRecord, SubscriptionRecord, TaskRecord, TaskRuntime,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{ApiError, AppState, now_ms};

#[derive(Serialize)]
pub struct PlanLibrary {
    catalog: Vec<empire_core::planning::CatalogItem>,
    kingdoms: Vec<KingdomOption>,
    attacks: Vec<AttackProfile>,
    tasks: Vec<TaskRecord>,
    task_runtimes: Vec<TaskRuntime>,
    subscriptions: Vec<SubscriptionRecord>,
    modes: Vec<ModeRecord>,
    account_modes: Vec<AccountModeRecord>,
}

#[derive(Serialize)]
pub struct CreatedId {
    id: String,
}

#[derive(Serialize)]
pub struct CreatedMode {
    mode_id: i64,
}

#[derive(Deserialize)]
pub struct AccountModeRequest {
    account_id: String,
    mode_id: i64,
    #[serde(default)]
    running: bool,
}

pub async fn library(State(state): State<AppState>) -> Result<Json<PlanLibrary>, ApiError> {
    require_plans(&state).await?;
    Ok(Json(PlanLibrary {
        catalog: catalog(),
        kingdoms: kingdoms(),
        attacks: state
            .store
            .attack_profiles()
            .await
            .map_err(ApiError::internal)?,
        tasks: state.store.tasks().await.map_err(ApiError::internal)?,
        task_runtimes: state
            .store
            .task_runtimes()
            .await
            .map_err(ApiError::internal)?,
        subscriptions: state
            .store
            .subscriptions()
            .await
            .map_err(ApiError::internal)?,
        modes: state.store.modes().await.map_err(ApiError::internal)?,
        account_modes: state
            .store
            .account_modes()
            .await
            .map_err(ApiError::internal)?,
    }))
}

pub async fn example(State(state): State<AppState>) -> Result<Json<ModeBundle>, ApiError> {
    require_plans(&state).await?;
    Ok(Json(ventrilo_sands_bundle()))
}

pub async fn create_attack(
    State(state): State<AppState>,
    Json(draft): Json<AttackDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    require_plans(&state).await?;
    let payload = draft.payload().map_err(ApiError::bad_request)?;
    let id = format!("attack-{}", Uuid::new_v4().simple());
    state
        .store
        .upsert_attack_profile(
            &AttackProfile {
                profile_id: id.clone(),
                name: draft.name.trim().to_owned(),
                payload,
                notes: String::new(),
            },
            now_ms(),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(CreatedId { id })))
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(draft): Json<TaskDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    require_plans(&state).await?;
    draft.validate().map_err(ApiError::bad_request)?;
    if !state
        .store
        .attack_profiles()
        .await
        .map_err(ApiError::internal)?
        .iter()
        .any(|profile| profile.profile_id == draft.attack_profile_id)
    {
        return Err(ApiError::bad_request("selected attack does not exist"));
    }
    let id = format!("task-{}", Uuid::new_v4().simple());
    let algorithm = match draft.algorithm {
        empire_core::planning::TargetAlgorithm::Advanced => "advanced",
        empire_core::planning::TargetAlgorithm::Closest => "closest",
        empire_core::planning::TargetAlgorithm::Random => "random",
    };
    let (kingdom_id, level_min, level_max, target, target_kind, filter) = match draft.destination {
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
            empire_core::planning::Coordinate {
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
            empire_core::planning::Coordinate {
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
            empire_core::planning::Coordinate {
                kingdom_id,
                x: 0,
                y: 0,
            },
            "fortress",
            json!({"kingdom_id": kingdom_id, "algorithm": algorithm}),
        ),
    };
    state
        .store
        .upsert_task(
            &TaskRecord {
                task_id: id.clone(),
                name: draft.name.trim().to_owned(),
                kind: "attack".to_owned(),
                kingdom_id,
                profile_id: Some(draft.attack_profile_id),
                target_level_min: level_min,
                target_level_max: level_max,
                commander_count: 1,
                max_active: Some(1),
                priority: draft.priority.scheduler_value(),
                enabled: true,
                tags: vec!["user".to_owned(), target_kind.to_owned()],
                notes: String::new(),
            },
            now_ms(),
        )
        .await
        .map_err(ApiError::bad_request)?;
    state
        .store
        .upsert_task_runtime(&TaskRuntime {
            task_id: id.clone(),
            source_kingdom_id: draft.source.kingdom_id,
            source_x: draft.source.x,
            source_y: draft.source.y,
            source_kind: draft.source_kind,
            target_kingdom_id: target.kingdom_id,
            target_x: target.x,
            target_y: target.y,
            travel_mode: "coin".to_owned(),
            hbw: VENTRILO_SANDS_HBW,
        })
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .replace_subscriptions(
            &id,
            &[SubscriptionRecord {
                task_id: id.clone(),
                target_kind: target_kind.to_owned(),
                filter,
            }],
        )
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(CreatedId { id })))
}

pub async fn create_mode(
    State(state): State<AppState>,
    Json(draft): Json<ModeDraft>,
) -> Result<(StatusCode, Json<CreatedMode>), ApiError> {
    require_plans(&state).await?;
    draft.validate().map_err(ApiError::bad_request)?;
    let allocations = draft.normalized_allocations();
    let mode_id = state
        .store
        .create_mode(draft.name.trim(), &allocations, now_ms())
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(CreatedMode { mode_id })))
}

pub async fn import_mode(
    State(state): State<AppState>,
    Json(bundle): Json<ModeBundle>,
) -> Result<(StatusCode, Json<CreatedMode>), ApiError> {
    require_plans(&state).await?;
    if bundle.schema != 1 {
        return Err(ApiError::bad_request("unsupported mode JSON schema"));
    }
    let mode_id = state
        .store
        .import_mode_bundle(&bundle, now_ms())
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(CreatedMode { mode_id })))
}

pub async fn subscribe_mode(
    State(state): State<AppState>,
    Json(request): Json<AccountModeRequest>,
) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    if request.running {
        let direct = state.direct_status.read().await;
        if !direct.connected
            || direct.phase != empire_core::session::SessionPhase::SandsReady
            || !direct
                .account_id
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(request.account_id.trim()))
        {
            return Err(ApiError::bad_request(
                "log in to this account and wait for Sands readiness before running a bot",
            ));
        }
    }
    state
        .store
        .subscribe_account_mode(
            request.account_id.trim(),
            request.mode_id,
            request.running,
            now_ms(),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_attack(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    state
        .store
        .delete_attack_profile(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    state
        .store
        .delete_task(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_mode(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    state
        .store
        .delete_mode(id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_plans(state: &AppState) -> Result<(), ApiError> {
    state
        .licence
        .require("automation")
        .await
        .map(|_| ())
        .map_err(ApiError::forbidden)
}
