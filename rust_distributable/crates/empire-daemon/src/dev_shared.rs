//! What every signed-in account on one world is doing in one kingdom, for the
//! Development tab's colour-coded maps.
//!
//! Accounts that share a world see each other's marches, threats and support sends on
//! the map; accounts on different worlds never do. The set is the viewed account plus
//! every account with a live session on the same game endpoint. Everything here is read
//! from state that already exists (the ledger, the PVP record, each session's snapshot);
//! nothing is sent to the game and nothing is stored.

use empire_core::store::{Movement, PvpQuery, PvpRow, PvpState};
use serde::Serialize;

use crate::{ApiError, AppState, direct::{JobView, SpatialView}, now_ms};

/// Armies older than this are ignored, as in the single-account view.
const MOVEMENT_WINDOW_MS: i64 = 4 * 3_600_000;
const PVP_LIMIT: i64 = 100;

#[derive(Debug, Serialize)]
pub struct SharedView {
    pub kingdom_id: i64,
    pub now_ms: i64,
    /// The world, as the game endpoint. `None` when the viewed account has no profile.
    pub world: Option<String>,
    /// The kingdom the viewed account's own bot is working in, if it is running.
    pub focus_kingdom: Option<i64>,
    pub accounts: Vec<SharedAccount>,
}

#[derive(Debug, Serialize)]
pub struct SharedAccount {
    pub account_id: String,
    pub player_name: Option<String>,
    pub player_id: Option<i64>,
    pub alliance_name: Option<String>,
    /// The account the view was asked for (the others are its world-mates).
    pub viewed: bool,
    pub connected: bool,
    /// Its main castle in this kingdom.
    pub castle: Option<[i64; 2]>,
    /// Its armies out or coming back.
    pub movements: Vec<Movement>,
    /// Other players' movements it has seen that are still travelling.
    pub pvp: Vec<PvpRow>,
    /// Its stationed castles in this kingdom.
    pub station: Vec<JobView>,
    /// Where its own bot's spotlight is, when it is working in this kingdom.
    pub spatial: Option<SpatialView>,
}

pub async fn build(state: &AppState, viewed: &str, kingdom_id: i64) -> Result<SharedView, ApiError> {
    let viewed = empire_core::store::canonical_account_id(viewed);
    let world = state.store.account_world(&viewed).await?;
    let mut members = vec![viewed.clone()];
    if let Some(world) = &world {
        for (account, cells) in state.sessions.all().await {
            let status = cells.status.read().await;
            if account != viewed && status.connected && status.endpoint.as_deref() == Some(world.endpoint.as_str()) {
                members.push(account.clone());
            }
        }
    }
    let now = now_ms();
    let mut accounts = Vec::with_capacity(members.len());
    for account in members {
        let identity = state.store.alliance_identity(&account).await?;
        let profile = state.store.account_world(&account).await?;
        let snapshot = state.sessions.dev(&account).await.unwrap_or_default();
        let connected = state
            .sessions
            .status(&account)
            .await
            .is_some_and(|status| status.connected);
        let query = PvpQuery {
            state: Some(PvpState::Active),
            kingdom_id: Some(kingdom_id),
            limit: PVP_LIMIT,
            ..PvpQuery::default()
        };
        accounts.push(SharedAccount {
            viewed: account == viewed,
            connected,
            player_name: profile.map(|profile| profile.player_name),
            player_id: identity.as_ref().map(|identity| identity.player_id),
            alliance_name: identity.and_then(|identity| identity.alliance_name),
            castle: state
                .store
                .main_castle(&account, kingdom_id)
                .await?
                .map(|(x, y)| [x, y]),
            movements: state
                .store
                .open_movements(&account, kingdom_id, now - MOVEMENT_WINDOW_MS, now, 200)
                .await?,
            pvp: state.store.pvp_page(&account, &query, now).await?.rows,
            station: snapshot
                .station
                .jobs
                .into_iter()
                .filter(|job| job.kingdom_id == kingdom_id)
                .collect(),
            spatial: snapshot.spatial.filter(|spatial| spatial.kingdom_id == kingdom_id),
            account_id: account,
        });
    }
    let focus_kingdom = state
        .sessions
        .dev(&viewed)
        .await
        .and_then(|snapshot| snapshot.spatial.map(|spatial| spatial.kingdom_id));
    Ok(SharedView {
        kingdom_id,
        now_ms: now,
        world: world.map(|world| world.endpoint),
        focus_kingdom,
        accounts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{licence::LicenceGate, sessions::Sessions};
    use empire_core::{
        account::OwnedCastle,
        pvp::{AllianceIdentity, Classified, Direction, Owner, Party, SeenMovement},
        store::Store,
    };
    use std::time::Duration;

    const US1: &str = "wss://ep-live-us1-game.goodgamestudios.com/";
    const WORLD2: &str = "wss://ep-live-world2-game.goodgamestudios.com/";

    async fn state() -> AppState {
        let store = Store::open("sqlite::memory:").await.unwrap();
        AppState {
            licence: LicenceGate::new(store.clone()).unwrap(),
            store,
            sessions: Sessions::default(),
            relay_transport: Default::default(),
        }
    }

    /// An account with a main castle in Green, signed in (or not) on `endpoint`.
    async fn account(state: &AppState, id: &str, endpoint: &str, signed_in: bool, player_id: i64) {
        state
            .store
            .upsert_account_profile(id, id, endpoint, "EmpireEx_21", 1)
            .await
            .unwrap();
        state
            .store
            .replace_account_bootstrap(
                id,
                &[OwnedCastle { kingdom_id: 0, castle_id: player_id, area_type: 1, x: 500 + player_id, y: 500, name: id.into() }],
                &[1],
                1,
            )
            .await
            .unwrap();
        state
            .store
            .set_alliance_identity(
                id,
                &AllianceIdentity { player_id, player_name: Some(id.into()), alliance_id: Some(7), alliance_name: Some("top secret".into()) },
                1,
            )
            .await
            .unwrap();
        let cells = state
            .sessions
            .replace(id, |_| tokio::spawn(async { tokio::time::sleep(Duration::from_secs(3_600)).await }))
            .await;
        let mut status = cells.status.write().await;
        status.connected = signed_in;
        status.endpoint = Some(endpoint.to_owned());
    }

    fn threat(mid: i64) -> (SeenMovement, Classified) {
        let party = |id: i64, x: i64| Party { player_id: Some(id), x, y: 500, ..Party::default() };
        (
            SeenMovement {
                mid,
                kingdom_id: 0,
                sender: party(99, 100),
                target: party(1, 501),
                progress_s: 0,
                duration_s: 600,
                army_size: Some(3),
                kind: None,
                flag: None,
            },
            Classified { direction: Direction::Incoming, owner: Owner::Mine },
        )
    }

    fn ids(view: &SharedView) -> Vec<&str> {
        view.accounts.iter().map(|account| account.account_id.as_str()).collect()
    }

    #[tokio::test]
    async fn accounts_on_the_same_world_see_each_other_and_other_worlds_never_do() {
        let state = state().await;
        account(&state, "ventrilo", US1, true, 1).await;
        account(&state, "pingpoko", US1, true, 2).await;
        account(&state, "elsewhere", WORLD2, true, 3).await;
        let view = build(&state, "ventrilo", 0).await.unwrap();
        assert_eq!(ids(&view), vec!["ventrilo", "pingpoko"], "the viewed account first, its world-mates after");
        assert!(view.accounts[0].viewed && !view.accounts[1].viewed);
        let from_the_other_world = build(&state, "elsewhere", 0).await.unwrap();
        assert_eq!(ids(&from_the_other_world), vec!["elsewhere"]);
        assert_eq!(view.world.as_deref(), Some(US1));
    }

    #[tokio::test]
    async fn only_signed_in_accounts_are_shared_but_the_viewed_one_is_always_there() {
        let state = state().await;
        account(&state, "ventrilo", US1, false, 1).await;
        account(&state, "pingpoko", US1, false, 2).await;
        let view = build(&state, "ventrilo", 0).await.unwrap();
        assert_eq!(ids(&view), vec!["ventrilo"], "a signed-out account is not shown to others");
        assert!(!view.accounts[0].connected);
        assert_eq!(view.accounts[0].castle, Some([501, 500]), "stored data is still shown");
    }

    #[tokio::test]
    async fn each_account_carries_its_own_castle_marches_threats_and_identity() {
        let state = state().await;
        account(&state, "ventrilo", US1, true, 1).await;
        account(&state, "pingpoko", US1, true, 2).await;
        let now = crate::now_ms();
        state.store.record_pvp_movements("ventrilo", &[threat(11)], now).await.unwrap();
        let view = build(&state, "pingpoko", 0).await.unwrap();
        let ventrilo = view.accounts.iter().find(|account| account.account_id == "ventrilo").unwrap();
        let pingpoko = view.accounts.iter().find(|account| account.account_id == "pingpoko").unwrap();
        assert_eq!(ventrilo.pvp.len(), 1, "its threat is visible from the other account's map");
        assert!(pingpoko.pvp.is_empty());
        assert_eq!(ventrilo.player_id, Some(1));
        assert_eq!(ventrilo.alliance_name.as_deref(), Some("top secret"));
        assert_eq!(ventrilo.castle, Some([501, 500]));
        assert_eq!(pingpoko.castle, Some([502, 500]));
        let other_kingdom = build(&state, "pingpoko", 1).await.unwrap();
        assert!(other_kingdom.accounts.iter().all(|account| account.pvp.is_empty() && account.castle.is_none()));
    }
}
