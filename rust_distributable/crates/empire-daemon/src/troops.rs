//! `GET /v1/troops`: the Troop overview. Read-only, built from the ledger and the
//! commander states the bot already keeps (see `empire-core/src/store/troops.rs`).

use axum::{
    Json,
    extract::{Query, State},
};
use empire_core::store::{RecentMarch, TroopRow};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState, now_ms};

#[derive(Debug, Deserialize)]
pub struct TroopParams {
    account_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Unit {
    id: i64,
    name: String,
    count: i64,
}

#[derive(Debug, Serialize)]
pub struct TroopRowView {
    #[serde(flatten)]
    row: TroopRow,
    /// The army by name, largest first.
    army: Vec<Unit>,
}

#[derive(Debug, Serialize)]
pub struct TroopsResponse {
    account_id: String,
    connected: bool,
    now_ms: i64,
    rows: Vec<TroopRowView>,
    recent: Vec<RecentMarch>,
}

fn named(row: TroopRow) -> TroopRowView {
    let mut army = row
        .troops
        .iter()
        .map(|(id, count)| Unit {
            id: *id,
            name: empire_core::station::unit_label(*id).1,
            count: *count,
        })
        .collect::<Vec<_>>();
    army.sort_by(|a, b| b.count.cmp(&a.count).then(a.id.cmp(&b.id)));
    TroopRowView { row, army }
}

pub async fn troops(
    State(state): State<AppState>,
    Query(params): Query<TroopParams>,
) -> Result<Json<TroopsResponse>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    let now = now_ms();
    let overview = state.store.troop_overview(&account_id, now, 15).await?;
    let connected = state
        .sessions
        .status(&account_id)
        .await
        .is_some_and(|status| status.connected);
    Ok(Json(TroopsResponse {
        connected,
        now_ms: now,
        rows: overview.rows.into_iter().map(named).collect(),
        recent: overview.recent,
        account_id,
    }))
}
