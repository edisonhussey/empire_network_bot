//! HTTP side of the Licence tab: the licence of the account in view, and replacing it.
//!
//! The rules (which licence an account has, what may replace it) live in `licence.rs`; this only
//! finds the account's server, player and main castle and hands them over. Both calls are on
//! demand (the tab opening, the account changing, a token dropped), so nothing here polls.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState, licence::AccountLicence};

#[derive(Debug, Deserialize)]
pub struct AccountParams {
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceBody {
    account_id: Option<String>,
    token: String,
}

#[derive(Debug, Serialize)]
pub struct AccountLicenceResponse {
    account_id: String,
    player_name: String,
    server: String,
    /// Learned at the account's first login; until then a licence waits unbound.
    player_id: Option<i64>,
    main_castle: Option<(i64, i64)>,
    licence: Option<AccountLicence>,
}

struct Account {
    account_id: String,
    player_name: String,
    server: String,
    player_id: Option<i64>,
    main_castles: Vec<(i64, i64)>,
}

async fn account(state: &AppState, requested: Option<String>) -> Result<Account, ApiError> {
    let account_id = crate::focus_account(state, requested).await?;
    let profile = state
        .store
        .account_summaries()
        .await?
        .into_iter()
        .find(|profile| profile.account_id == account_id)
        .ok_or_else(|| ApiError::bad_request(format!("unknown account {account_id}")))?;
    let server = crate::direct::trusted_server_name(&profile.endpoint, &profile.server_header)
        .ok_or_else(|| ApiError::bad_request("the account's server is not a known Goodgame world"))?;
    let player_id = state.store.alliance_identity(&account_id).await?.map(|identity| identity.player_id);
    // Main castles in the green kingdom (area type 1): what a licence is issued for.
    let main_castles = state
        .store
        .owned_castles()
        .await?
        .into_iter()
        .filter(|castle| castle.account_id == account_id && castle.kingdom_id == 0 && castle.area_type == 1)
        .map(|castle| (castle.x, castle.y))
        .collect();
    Ok(Account { account_id, player_name: profile.player_name, server: server.to_owned(), player_id, main_castles })
}

fn response(account: Account, licence: Option<AccountLicence>) -> AccountLicenceResponse {
    AccountLicenceResponse {
        main_castle: account.main_castles.first().copied(),
        account_id: account.account_id,
        player_name: account.player_name,
        server: account.server,
        player_id: account.player_id,
        licence,
    }
}

pub async fn account_licence(
    State(state): State<AppState>,
    Query(params): Query<AccountParams>,
) -> Result<Json<AccountLicenceResponse>, ApiError> {
    let account = account(&state, params.account_id).await?;
    let licence = state
        .licence
        .for_account(&account.server, account.player_id, &account.main_castles)
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(response(account, licence)))
}

pub async fn replace_account_licence(
    State(state): State<AppState>,
    Json(body): Json<ReplaceBody>,
) -> Result<Json<AccountLicenceResponse>, ApiError> {
    let account = account(&state, body.account_id).await?;
    let licence = state
        .licence
        .replace_for_account(&account.server, account.player_id, &account.main_castles, &body.token)
        .await
        .map_err(|error| ApiError::bad_request(format!("{error:#}")))?;
    Ok(Json(response(account, Some(licence))))
}
