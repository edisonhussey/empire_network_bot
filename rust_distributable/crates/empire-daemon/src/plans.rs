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
        AccountModeRecord, AccountRecruitBot, AttackProfile, ModeRecord, OwnedCastleRecord,
        RecruitBot, RecruitBotCastle, RecruitCastleState, RecruitmentTemplate, SubscriptionRecord,
        TaskRecord, TaskRuntime,
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
    recruitments: Vec<RecruitmentTemplate>,
    recruit_bots: Vec<RecruitBot>,
    account_recruit_bots: Vec<AccountRecruitBot>,
    recruit_states: Vec<RecruitCastleState>,
    castles: Vec<OwnedCastleRecord>,
}

#[derive(Serialize)]
pub struct CreatedId {
    id: String,
}

/// Names are unique, so a repeat is a mistake the operator can fix rather than a
/// database fault worth showing them raw.
fn duplicate_name_error(error: sqlx::Error) -> ApiError {
    if error.to_string().contains("UNIQUE constraint failed") {
        return ApiError::bad_request("that name is already used — pick another");
    }
    ApiError::internal(error)
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

#[derive(Deserialize)]
pub struct RecruitmentDraft {
    name: String,
    troop_id: i64,
    quantity: i64,
    slot_count: i64,
    #[serde(default)]
    ask_alliance_help: bool,
}

#[derive(Deserialize)]
pub struct RecruitBotDraft {
    name: String,
    /// Request cadence. Optional, so leaving it out selects `advanced`.
    #[serde(default)]
    algorithm: String,
    castles: Vec<RecruitBotCastle>,
}

#[derive(Deserialize)]
pub struct StartBotsRequest {
    account_id: String,
    mode_id: i64,
    recruit_bot_id: Option<i64>,
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
        recruitments: state
            .store
            .recruitments()
            .await
            .map_err(ApiError::internal)?,
        recruit_bots: state
            .store
            .recruit_bots()
            .await
            .map_err(ApiError::internal)?,
        account_recruit_bots: state
            .store
            .account_recruit_bots()
            .await
            .map_err(ApiError::internal)?,
        recruit_states: state
            .store
            .all_recruit_states()
            .await
            .map_err(ApiError::internal)?,
        castles: state
            .store
            .owned_castles()
            .await
            .map_err(ApiError::internal)?,
    }))
}

pub async fn create_recruitment(
    State(state): State<AppState>,
    Json(draft): Json<RecruitmentDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    require_plans(&state).await?;
    if draft.name.trim().is_empty() || draft.quantity <= 0 || !(1..=5).contains(&draft.slot_count) {
        return Err(ApiError::bad_request(
            "name, positive quantity, and 1–5 slots are required",
        ));
    }
    if !catalog().iter().any(|item| {
        item.id == draft.troop_id && matches!(item.kind, empire_core::planning::CatalogKind::Troop)
    }) {
        return Err(ApiError::bad_request(
            "selected troop is not in the game catalog",
        ));
    }
    let id = format!("recruit-{}", Uuid::new_v4().simple());
    state
        .store
        .upsert_recruitment(
            &RecruitmentTemplate {
                recruitment_id: id.clone(),
                name: draft.name.trim().to_owned(),
                troop_id: draft.troop_id,
                quantity: draft.quantity,
                slot_count: draft.slot_count,
                ask_alliance_help: draft.ask_alliance_help,
                lane_id: 0,
                skill_id: 73,
            },
            now_ms(),
        )
        .await
        .map_err(duplicate_name_error)?;
    Ok((StatusCode::CREATED, Json(CreatedId { id })))
}

pub async fn create_recruit_bot(
    State(state): State<AppState>,
    Json(draft): Json<RecruitBotDraft>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    require_plans(&state).await?;
    // The cadence is the only choice here; leaving it out selects the default,
    // which is the recorded client's randomised timing.
    let algorithm = if draft.algorithm.trim().is_empty() {
        empire_core::pacing::RecruitTempo::default()
    } else {
        match draft.algorithm.trim().to_ascii_lowercase().as_str() {
            "greedy" => empire_core::pacing::RecruitTempo::Greedy,
            "sporadic" => empire_core::pacing::RecruitTempo::Sporadic,
            "advanced" => empire_core::pacing::RecruitTempo::Advanced,
            _ => {
                return Err(ApiError::bad_request(
                    "request timing must be greedy, sporadic or advanced",
                ));
            }
        }
    };
    if draft.name.trim().is_empty() {
        return Err(ApiError::bad_request("a name is required"));
    }
    if draft.castles.is_empty() {
        return Err(ApiError::bad_request("subscribe at least one castle"));
    }
    let mut unique = std::collections::HashSet::new();
    if draft
        .castles
        .iter()
        .any(|value| !unique.insert(value.castle_id))
    {
        return Err(ApiError::bad_request(
            "each castle can subscribe to only one recruitment",
        ));
    }
    let id = state
        .store
        .create_recruit_bot(
            draft.name.trim(),
            algorithm.name(),
            &draft.castles,
            now_ms(),
        )
        .await
        .map_err(duplicate_name_error)?;
    Ok((StatusCode::CREATED, Json(CreatedId { id: id.to_string() })))
}

pub async fn start_bots(
    State(state): State<AppState>,
    Json(request): Json<StartBotsRequest>,
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
                "start the connection and wait until the account is ready",
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
    state
        .store
        .subscribe_account_recruit_bot(
            request.account_id.trim(),
            request.recruit_bot_id,
            request.running && request.recruit_bot_id.is_some(),
            now_ms(),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_recruitment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    let blocking = state
        .store
        .delete_recruitment(&id)
        .await
        .map_err(ApiError::bad_request)?;
    if !blocking.is_empty() {
        // The delete is refused rather than cascaded: a recruit bot exists to
        // run this recruitment, so quietly changing what that bot does would be
        // a worse answer than saying which bot to edit.
        let noun = if blocking.len() == 1 { "bot" } else { "bots" };
        return Err(ApiError::bad_request(format!(
            "Still used by recruit {noun} {}. Remove it from that bot first.",
            blocking.join(", ")
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_recruit_bot(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_plans(&state).await?;
    state
        .store
        .delete_recruit_bot(id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
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
