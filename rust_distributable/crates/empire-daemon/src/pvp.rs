//! Read-only endpoint behind the PVP tab and the Development tab's arrows.
//!
//! `GET /v1/pvp` pages through the recorded movements; nothing is loaded that was
//! not asked for. The first page (no `before`) also carries the whole-history
//! counts and who the account is, so the tab can show a true summary while only a
//! page of rows is in memory.

use axum::{
    Json,
    extract::{Query, State},
};
use empire_core::store::{PvpCount, PvpPage, PvpQuery, PvpRow, PvpState};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState, now_ms};

const DEFAULT_PAGE: i64 = 50;

#[derive(Debug, Deserialize)]
pub struct PvpParams {
    account_id: Option<String>,
    /// `incoming`, `outgoing`, `internal` or `other`; absent for all.
    direction: Option<String>,
    /// `mine` or `alliance`; absent for all.
    owner: Option<String>,
    /// `active`, `landed` or `retreated`; absent for all.
    state: Option<String>,
    kingdom: Option<i64>,
    /// Cursor from a previous page's `next`: `<detected_at_ms>:<mid>`.
    before: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PvpResponse {
    account_id: String,
    now_ms: i64,
    player_name: Option<String>,
    alliance_name: Option<String>,
    rows: Vec<PvpRow>,
    next: Option<String>,
    /// Whole-history counts per direction and state; first page only.
    counts: Option<Vec<PvpCount>>,
}

pub async fn pvp(
    State(state): State<AppState>,
    Query(params): Query<PvpParams>,
) -> Result<Json<PvpResponse>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id.clone()).await?;
    let query = parse_query(&params)?;
    let now = now_ms();
    let first_page = query.before.is_none();
    let PvpPage { rows, next } = state.store.pvp_page(&account_id, &query, now).await?;
    let identity = state.store.alliance_identity(&account_id).await?;
    let counts = if first_page {
        Some(state.store.pvp_counts(&account_id, now).await?)
    } else {
        None
    };
    Ok(Json(PvpResponse {
        account_id,
        now_ms: now,
        player_name: identity.as_ref().and_then(|identity| identity.player_name.clone()),
        alliance_name: identity.and_then(|identity| identity.alliance_name),
        rows,
        next: next.map(|(detected_at_ms, mid)| format!("{detected_at_ms}:{mid}")),
        counts,
    }))
}

fn parse_query(params: &PvpParams) -> Result<PvpQuery, ApiError> {
    let choice = |value: &Option<String>, allowed: &[&str], name: &str| match value.as_deref() {
        None | Some("" | "all") => Ok(None),
        Some(value) if allowed.contains(&value) => Ok(Some(value.to_owned())),
        Some(value) => Err(ApiError::bad_request(format!("unknown {name} '{value}'"))),
    };
    let state = match params.state.as_deref() {
        None | Some("" | "all") => None,
        Some(value) => Some(
            PvpState::parse(value)
                .ok_or_else(|| ApiError::bad_request(format!("unknown state '{value}'")))?,
        ),
    };
    let before = params
        .before
        .as_deref()
        .map(|cursor| {
            cursor
                .split_once(':')
                .and_then(|(at, mid)| Some((at.parse().ok()?, mid.parse().ok()?)))
                .ok_or_else(|| ApiError::bad_request("malformed cursor"))
        })
        .transpose()?;
    Ok(PvpQuery {
        direction: choice(
            &params.direction,
            &["incoming", "outgoing", "internal", "other"],
            "direction",
        )?,
        owner: choice(&params.owner, &["mine", "alliance"], "owner")?,
        state,
        kingdom_id: params.kingdom,
        before,
        limit: params.limit.unwrap_or(DEFAULT_PAGE),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(before: Option<&str>, state: Option<&str>) -> PvpParams {
        PvpParams {
            account_id: None,
            direction: Some("incoming".into()),
            owner: None,
            state: state.map(str::to_owned),
            kingdom: Some(0),
            before: before.map(str::to_owned),
            limit: None,
        }
    }

    #[test]
    fn a_cursor_and_filters_become_a_store_query() {
        let Ok(query) = parse_query(&params(Some("1700:42"), Some("active"))) else {
            panic!("valid parameters were rejected");
        };
        assert_eq!(query.before, Some((1700, 42)));
        assert_eq!(query.state, Some(PvpState::Active));
        assert_eq!(query.direction.as_deref(), Some("incoming"));
        assert_eq!(query.limit, DEFAULT_PAGE);
    }

    #[test]
    fn unknown_filters_and_bad_cursors_are_rejected_not_ignored() {
        assert!(parse_query(&params(Some("nonsense"), None)).is_err());
        assert!(parse_query(&params(None, Some("sleeping"))).is_err());
        let mut bad = params(None, None);
        bad.direction = Some("sideways".into());
        assert!(parse_query(&bad).is_err());
    }
}
