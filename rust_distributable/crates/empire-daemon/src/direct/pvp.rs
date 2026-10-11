//! Feeds the PVP tab from the inbound stream: who we are (`gbd`), other players'
//! movements (`gam`) and retreats (`mrm`). All parsing and judgement lives in
//! `empire_core::pvp`; this only connects it to the store.

use empire_core::{
    protocol::XtPacket,
    pvp::{alliance_identity, classify, retreated_mid, seen_movements},
    store::Store,
};
use tracing::{debug, warn};

use super::now_ms;

pub(super) async fn observe(store: &Store, account_id: &str, packet: &XtPacket) {
    let result = match packet.command.as_str() {
        "gbd" => remember_identity(store, account_id, packet).await,
        "gam" => record_movements(store, account_id, packet).await,
        "mrm" => record_retreat(store, account_id, packet).await,
        _ => return,
    };
    if let Err(error) = result {
        warn!(%error, %account_id, command = %packet.command, "pvp persistence failed");
    }
}

async fn remember_identity(
    store: &Store,
    account_id: &str,
    packet: &XtPacket,
) -> Result<(), sqlx::Error> {
    match alliance_identity(&packet.payload) {
        Some(identity) => store.set_alliance_identity(account_id, &identity, now_ms()).await,
        None => Ok(()),
    }
}

async fn record_movements(
    store: &Store,
    account_id: &str,
    packet: &XtPacket,
) -> Result<(), sqlx::Error> {
    let seen = seen_movements(&packet.payload);
    if seen.is_empty() {
        return Ok(());
    }
    // Without the login identity nothing can be called ours or an ally's, and a
    // guess would be recorded for good. The next `gbd` makes it available.
    let Some(me) = store.alliance_identity(account_id).await? else {
        debug!(%account_id, "gam before the account identity is known; skipped");
        return Ok(());
    };
    let castles = store.owned_castle_tuples(account_id).await?;
    let classified = seen
        .into_iter()
        .filter_map(|movement| {
            let class = classify(&movement, &me, &castles)?;
            Some((movement, class))
        })
        .collect::<Vec<_>>();
    if classified.is_empty() {
        return Ok(());
    }
    let added = store
        .record_pvp_movements(account_id, &classified, now_ms())
        .await?;
    if added > 0 {
        debug!(%account_id, added, "new movements recorded");
    }
    Ok(())
}

/// A retreat frame from the server ends a movement we know. Our own outgoing
/// `mrm` requests are not read here: a request is not a confirmed retreat.
async fn record_retreat(
    store: &Store,
    account_id: &str,
    packet: &XtPacket,
) -> Result<(), sqlx::Error> {
    match retreated_mid(&packet.payload) {
        Some(mid) => store
            .retreat_pvp_movement(account_id, mid, now_ms())
            .await
            .map(|_| ()),
        None => Ok(()),
    }
}
