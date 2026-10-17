//! HTTP side of manual kingdom transfers: what the widget shows, the check, the request, the cancel.
//!
//! The window never sends game packets. `POST /v1/transfers` stores a request after the same checks
//! the session repeats before sending (`empire_core::transfer::check`); the session
//! (`direct/transfer.rs`) runs it only while the bot is off. Design: `docs/architecture/kingdom-transfer.md`.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use empire_core::{
    store::{TRANSFER_QUEUED, TransferRow},
    transfer::{self, ANY_CASTLE, Check, InFlight, TransferPlan},
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState, now_ms};

#[derive(Debug, Deserialize)]
pub struct AccountParam {
    account_id: Option<String>,
}

/// What the operator asked for. The source kingdom is looked up from the castle, never taken on trust.
#[derive(Debug, Deserialize)]
pub struct TransferDraft {
    source_castle_id: i64,
    target_kingdom_id: i64,
    /// `[unit id, count]`.
    units: Vec<(i64, i64)>,
    /// A skip item key such as `MS5`, or none to wait the journey out.
    #[serde(default)]
    skip: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KingdomRow {
    kingdom_id: i64,
    unlocked: bool,
    tax_percent: i64,
    travel_s: i64,
    /// Troops travelling there now, with the seconds left when the state was read.
    in_flight: Option<InFlight>,
}

#[derive(Debug, Serialize)]
pub struct SkipRow {
    key: &'static str,
    seconds: i64,
    /// `None` until the server has reported this item.
    stock: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Coins {
    coins: i64,
    rubies: i64,
    observed_at_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct TransferOverview {
    account_id: String,
    connected: bool,
    bot_running: bool,
    now_ms: i64,
    coins: Option<Coins>,
    /// Empty until the kingdom state has been read once.
    kingdoms: Vec<KingdomRow>,
    kingdoms_observed_at_ms: Option<i64>,
    skips: Vec<SkipRow>,
    /// Newest first.
    jobs: Vec<TransferRow>,
}

pub async fn overview(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
) -> Result<Json<TransferOverview>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    let store = &state.store;
    let connected = state.sessions.status(&account_id).await.is_some_and(|status| status.connected);
    let kingdom_state = store.kingdom_state(&account_id).await?;
    let kingdoms = kingdom_state
        .as_ref()
        .map(|(_, snapshot)| {
            snapshot
                .unlocked
                .iter()
                .filter(|(id, _)| **id != 10)
                .filter_map(|(id, unlocked)| {
                    Some(KingdomRow {
                        kingdom_id: *id,
                        unlocked: *unlocked,
                        tax_percent: transfer::tax_percent(*id)?,
                        travel_s: transfer::travel_seconds(*id)?,
                        in_flight: snapshot.in_flight(*id).cloned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let stock = store.skip_stock(&account_id).await?;
    Ok(Json(TransferOverview {
        connected,
        bot_running: store.account_bot_running(&account_id).await?,
        now_ms: now_ms(),
        coins: store
            .currency(&account_id)
            .await?
            .map(|(coins, rubies, observed_at_ms)| Coins { coins, rubies, observed_at_ms }),
        kingdoms,
        kingdoms_observed_at_ms: kingdom_state.map(|(at, _)| at),
        skips: transfer::skip_items()
            .into_iter()
            .map(|item| SkipRow { key: item.key, seconds: item.seconds, stock: stock.get(item.key).copied() })
            .collect(),
        jobs: store.recent_transfers(&account_id, 12).await?,
        account_id,
    }))
}

/// Merge repeats and drop empty rows, so the plan holds each unit once.
fn tidy(units: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut merged = BTreeMap::new();
    for (unit, count) in units {
        *merged.entry(*unit).or_insert(0) += count;
    }
    merged.into_iter().filter(|(_, count)| *count > 0).collect()
}

async fn plan_from(state: &AppState, account_id: &str, draft: &TransferDraft) -> Result<TransferPlan, ApiError> {
    let castle = state
        .store
        .owned_castles()
        .await?
        .into_iter()
        .find(|castle| castle.account_id.eq_ignore_ascii_case(account_id) && castle.castle_id == draft.source_castle_id)
        .ok_or_else(|| ApiError::bad_request("that castle is not one of this account's castles"))?;
    Ok(TransferPlan {
        source_castle_id: castle.castle_id,
        source_kingdom_id: castle.kingdom_id,
        target_kingdom_id: draft.target_kingdom_id,
        target_castle_id: ANY_CASTLE,
        units: tidy(&draft.units),
        skip: draft.skip.clone().filter(|key| !key.trim().is_empty()),
    })
}

async fn run_check(state: &AppState, account_id: &str, plan: &TransferPlan) -> Result<Check, ApiError> {
    // The API cannot see which castles are out as support; the session checks that before sending.
    let snapshot = state.store.transfer_snapshot(account_id, plan, &[]).await?;
    Ok(transfer::check(plan, &snapshot, now_ms()))
}

/// The checks only: the cost, the journey time and every reason it would not go. Changes nothing.
pub async fn check(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
    Json(draft): Json<TransferDraft>,
) -> Result<Json<Check>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    let plan = plan_from(&state, &account_id, &draft).await?;
    Ok(Json(run_check(&state, &account_id, &plan).await?))
}

/// Store the request. Refused here, with the reasons, when a check already fails.
pub async fn create(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
    Json(draft): Json<TransferDraft>,
) -> Result<(StatusCode, Json<TransferRow>), ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    if !state.sessions.status(&account_id).await.is_some_and(|status| status.connected) {
        return Err(ApiError::bad_request("connect this account first"));
    }
    if !state.store.open_transfers(&account_id).await?.is_empty() {
        return Err(ApiError::bad_request("a transfer is already in progress; wait for it to land"));
    }
    let plan = plan_from(&state, &account_id, &draft).await?;
    let result = run_check(&state, &account_id, &plan).await?;
    if !result.ok() {
        let reasons = result.problems.iter().map(|problem| problem.message.as_str()).collect::<Vec<_>>().join(" ");
        return Err(ApiError::bad_request(reasons));
    }
    let now = now_ms();
    let mut row = TransferRow {
        job_id: 0,
        account_id: account_id.clone(),
        source_castle_id: plan.source_castle_id,
        source_kingdom_id: plan.source_kingdom_id,
        target_kingdom_id: plan.target_kingdom_id,
        target_castle_id: plan.target_castle_id,
        units: plan.units,
        skip_item: plan.skip,
        phase: TRANSFER_QUEUED.to_owned(),
        detail: "Waiting to be sent".to_owned(),
        cost_c1: result.cost_c1,
        skips_sent: 0,
        created_at_ms: now,
        sent_at_ms: None,
        landed_at_ms: None,
        updated_at_ms: now,
    };
    row.job_id = state.store.save_transfer(&row).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn cancel(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
    Path(job_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    if state.store.cancel_queued_transfer(&account_id, job_id, now_ms()).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::bad_request("it has already been sent (or is finished); a transfer on its way cannot be recalled"))
    }
}

/// Ask the session to read the kingdoms (`kpi`) and the castles (`dcl`) when the connection is
/// free, so the widget's numbers are current. Never more often than every fifteen seconds.
pub async fn refresh(
    State(state): State<AppState>,
    Query(params): Query<AccountParam>,
) -> Result<StatusCode, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    if !state.sessions.status(&account_id).await.is_some_and(|status| status.connected) {
        return Err(ApiError::bad_request("connect this account to read its kingdoms"));
    }
    let now = now_ms();
    let value = serde_json::json!(now);
    state.store.set_app_state(&crate::direct::transfer_refresh_key(&account_id), &value, now).await?;
    state.store.set_app_state(&crate::direct::overview_refresh_key(&account_id), &value, now).await?;
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use empire_core::{
        account::OwnedCastle,
        station::CastleStock,
        store::{Store, TRANSFER_QUEUED},
        transfer::KingdomState,
    };
    use tokio::sync::RwLock;

    use super::*;
    use crate::sessions::Sessions;

    const ACCOUNT: &str = "ventrilo";
    const SOURCE: i64 = 16_011_862;

    fn draft(units: Vec<(i64, i64)>, skip: Option<&str>) -> TransferDraft {
        TransferDraft { source_castle_id: SOURCE, target_kingdom_id: 3, units, skip: skip.map(str::to_owned) }
    }

    async fn app(connected: bool) -> AppState {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store.upsert_account_profile(ACCOUNT, "Ventrilo", "wss://x/", "EmpireEx_21", 1).await.unwrap();
        store
            .replace_account_bootstrap(
                ACCOUNT,
                &[OwnedCastle { kingdom_id: 0, castle_id: SOURCE, area_type: 1, x: 643, y: 637, name: "Home".into() }],
                empire_core::hunt::USABLE_COMMANDER_LIDS,
                now_ms(),
            )
            .await
            .unwrap();
        store
            .record_castle_inventory(
                ACCOUNT,
                &[CastleStock {
                    kingdom_id: 0,
                    castle_id: SOURCE,
                    units: BTreeMap::from([(655, 20)]),
                    food: 0.0,
                    mead: 0.0,
                    wood: 0.0,
                    stone: 0.0,
                }],
                now_ms(),
            )
            .await
            .unwrap();
        store.set_currency(ACCOUNT, 2_147_483_437, 150_163, now_ms()).await.unwrap();
        store
            .set_kingdom_state(
                ACCOUNT,
                &KingdomState { unlocked: BTreeMap::from([(0, true), (3, true)]), transfers: Vec::new() },
                now_ms(),
            )
            .await
            .unwrap();
        store.merge_skip_stock(ACCOUNT, &BTreeMap::from([("MS5".to_owned(), 996)]), now_ms()).await.unwrap();
        let sessions = Sessions::default();
        if connected {
            sessions
                .replace(ACCOUNT, |cells| {
                    tokio::spawn(async move {
                        cells.status.write().await.connected = true;
                        std::future::pending::<()>().await;
                    })
                })
                .await;
            tokio::task::yield_now().await;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        AppState {
            licence: crate::licence::LicenceGate::new(store.clone()).unwrap(),
            store,
            sessions,
            relay_transport: Arc::new(RwLock::new(None)),
        }
    }

    fn account() -> Query<AccountParam> {
        Query(AccountParam { account_id: Some(ACCOUNT.to_owned()) })
    }

    #[tokio::test]
    async fn the_check_reports_the_live_run_cost_and_needs_no_session() {
        let state = app(false).await;
        let Json(result) = check(State(state), account(), Json(draft(vec![(655, 20)], Some("MS5")))).await.unwrap();
        assert!(result.ok(), "{:?}", result.problems);
        assert_eq!((result.cost_c1, result.travel_s, result.skips_needed), (Some(2_500), Some(7_200), Some(2)));
    }

    #[tokio::test]
    async fn the_check_names_what_is_wrong_and_repeats_are_merged() {
        let state = app(false).await;
        let Json(result) = check(State(state), account(), Json(draft(vec![(655, 15), (655, 10)], None))).await.unwrap();
        assert!(result.problems.iter().any(|problem| problem.message.contains("needs 25, the castle has 20")));
    }

    #[tokio::test]
    async fn a_castle_that_is_not_the_accounts_is_refused() {
        let state = app(false).await;
        let mut stranger = draft(vec![(655, 1)], None);
        stranger.source_castle_id = 999;
        let error = check(State(state), account(), Json(stranger)).await.unwrap_err();
        assert!(error.message.contains("not one of this account's castles"));
    }

    #[tokio::test]
    async fn a_request_is_stored_once_checked_and_only_one_can_be_open() {
        let state = app(true).await;
        let (status, Json(row)) = create(State(state.clone()), account(), Json(draft(vec![(655, 20)], Some("MS5")))).await.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!((row.phase.as_str(), row.cost_c1, row.source_kingdom_id), (TRANSFER_QUEUED, Some(2_500), 0));
        let again = create(State(state.clone()), account(), Json(draft(vec![(655, 1)], None))).await.unwrap_err();
        assert!(again.message.contains("already in progress"));

        let Json(shown) = overview(State(state.clone()), account()).await.unwrap();
        assert_eq!(shown.jobs.len(), 1);
        assert!(shown.connected && !shown.bot_running);
        assert_eq!(shown.coins.as_ref().map(|coins| coins.coins), Some(2_147_483_437));
        assert!(shown.kingdoms.iter().any(|kingdom| kingdom.kingdom_id == 3 && kingdom.unlocked && kingdom.tax_percent == 50));
        assert_eq!(shown.skips.iter().find(|item| item.key == "MS5").and_then(|item| item.stock), Some(996));

        assert_eq!(cancel(State(state.clone()), account(), Path(row.job_id)).await.unwrap(), StatusCode::NO_CONTENT);
        let late = cancel(State(state), account(), Path(row.job_id)).await.unwrap_err();
        assert!(late.message.contains("already been sent"));
    }

    #[tokio::test]
    async fn nothing_is_stored_when_the_account_is_not_connected_or_a_check_fails() {
        let offline = app(false).await;
        let error = create(State(offline), account(), Json(draft(vec![(655, 20)], None))).await.unwrap_err();
        assert!(error.message.contains("connect this account"));

        let online = app(true).await;
        online.store.set_currency(ACCOUNT, 100, 0, now_ms()).await.unwrap();
        let error = create(State(online.clone()), account(), Json(draft(vec![(655, 20)], None))).await.unwrap_err();
        assert!(error.message.contains("Needs 2500 coins"), "{}", error.message);
        assert!(online.store.open_transfers(ACCOUNT).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_running_bot_blocks_the_request_with_the_reason() {
        let state = app(true).await;
        let mode = state.store.import_mode_bundle(&empire_core::planning::ventrilo_sands_bundle(), now_ms()).await.unwrap();
        state.store.subscribe_account_mode(ACCOUNT, mode, true, now_ms()).await.unwrap();
        let error = create(State(state.clone()), account(), Json(draft(vec![(655, 20)], None))).await.unwrap_err();
        assert!(error.message.contains("Stop the bot first"), "{}", error.message);
        let Json(shown) = overview(State(state), account()).await.unwrap();
        assert!(shown.bot_running);
    }

    #[tokio::test]
    async fn the_refresh_asks_the_session_for_both_readings_and_needs_a_connection() {
        let offline = app(false).await;
        assert!(refresh(State(offline), account()).await.is_err());
        let state = app(true).await;
        assert_eq!(refresh(State(state.clone()), account()).await.unwrap(), StatusCode::ACCEPTED);
        for key in [crate::direct::transfer_refresh_key(ACCOUNT), crate::direct::overview_refresh_key(ACCOUNT)] {
            assert!(state.store.app_state(&key).await.unwrap().and_then(|value| value.as_i64()).is_some(), "{key}");
        }
    }
}
