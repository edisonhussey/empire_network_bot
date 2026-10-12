//! The Troop overview: which commanders are out, where, with how many troops, and
//! when they are due back, read from the ledger the bot already keeps.
//!
//! Nothing is stored for this. One row per usable commander is assembled from the
//! commander states and the march ledger at the moment it is asked for; the window
//! counts down from the timestamps, so nothing here ticks.

use std::collections::BTreeMap;

use serde::Serialize;
use sqlx::Row;

use super::{COMMANDER_OUTBOUND, MarchRecord, Store, canonical_account_id, return_bound_ms};

/// What a commander is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TroopState {
    /// At home and free.
    Idle,
    /// A `cra` has left and the server has not answered yet.
    Sending,
    /// On its way to the target.
    Outbound,
    /// Fought, and on the way home (loot is known).
    Returning,
    /// Lent to a support march (auto-station).
    Station,
}

/// One commander, with the march it is on if any.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TroopRow {
    pub lord_id: i64,
    /// The human number: the commander's place in the usable roster, from 1.
    pub number: usize,
    /// The task of the running attack bot that owns this commander.
    pub task_id: Option<String>,
    pub task_name: Option<String>,
    /// Kept for the support object of the running attack bot: no task owns it, so
    /// attacks never use it and a support march always finds one free.
    pub reserved: bool,
    pub state: TroopState,
    pub march_id: Option<i64>,
    pub kingdom_id: Option<i64>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub level: Option<i64>,
    pub sent_at_ms: Option<i64>,
    /// When the troops reach the target (outbound only).
    pub lands_at_ms: Option<i64>,
    /// When the commander is free again: the return time, plus the rest the bot
    /// gives it. An estimate until the report arrives.
    pub free_at_ms: Option<i64>,
    pub troops: BTreeMap<i64, i64>,
    pub troop_total: i64,
    pub troops_lost: Option<i64>,
    pub coin_loot: Option<i64>,
    pub ruby_loot: Option<i64>,
    /// The march landed while the session was away and no return was seen: it is shown
    /// as returning, home within the time the trip out took (see
    /// [`return_bound_ms`](super::return_bound_ms)).
    pub estimated_return: bool,
}

/// A finished march, for the history under the table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentMarch {
    pub march_id: i64,
    pub lord_id: Option<i64>,
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    pub status: String,
    pub sent_at_ms: i64,
    pub troop_total: Option<i64>,
    pub coin_loot: Option<i64>,
    pub ruby_loot: Option<i64>,
    pub result_flag: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TroopOverview {
    pub rows: Vec<TroopRow>,
    pub recent: Vec<RecentMarch>,
}

/// Decide a commander's state from its ledger record.
///
/// A record whose hold has run out (`available_after_ms <= now`) is idle whatever its
/// status says: the ledger can lag the game, and a commander the bot would send again
/// is not "out". A hold with no march behind it is either a send in flight or, when
/// its key says so, a support lease.
pub fn troop_state(
    status: &str,
    available_after_ms: i64,
    march_id: Option<i64>,
    target_key: Option<&str>,
    now_ms: i64,
) -> TroopState {
    if available_after_ms <= now_ms {
        return TroopState::Idle;
    }
    if target_key.is_some_and(|key| key.starts_with("station:")) {
        return TroopState::Station;
    }
    match (status, march_id) {
        (status, None) if status == COMMANDER_OUTBOUND => TroopState::Sending,
        (status, _) if status == COMMANDER_OUTBOUND => TroopState::Outbound,
        _ => TroopState::Returning,
    }
}

impl Store {
    /// Every usable commander of the account with what it is doing now, and the
    /// latest finished marches.
    pub async fn troop_overview(
        &self,
        account_id: &str,
        now_ms: i64,
        recent_limit: i64,
    ) -> Result<TroopOverview, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        // The account's own commanders in the order the game lists them: human number n
        // is the n-th entry.
        let roster = self.commander_lids(&account_id).await?;
        let states = self.commander_states(&account_id).await?;
        let tasks = self.active_mode_tasks(&account_id).await?;
        let reserved = self
            .running_support(&account_id)
            .await?
            .map(|(_, lids)| lids)
            .unwrap_or_default();
        let mut rows = Vec::with_capacity(roster.len());
        for (index, lid) in roster.iter().enumerate() {
            let state = states.iter().find(|state| state.lord_id == *lid);
            let owner = tasks.iter().find(|task| task.commander_lids.contains(lid));
            let mut row = TroopRow {
                lord_id: *lid,
                number: index + 1,
                task_id: owner.map(|task| task.task_id.clone()),
                task_name: owner.map(|task| task.name.clone()),
                reserved: reserved.contains(lid),
                state: TroopState::Idle,
                march_id: None,
                kingdom_id: None,
                x: None,
                y: None,
                level: None,
                sent_at_ms: None,
                lands_at_ms: None,
                free_at_ms: None,
                troops: BTreeMap::new(),
                troop_total: 0,
                troops_lost: None,
                coin_loot: None,
                ruby_loot: None,
                estimated_return: false,
            };
            if let Some(state) = state {
                row.state = troop_state(
                    &state.status,
                    state.available_after_ms,
                    state.march_id,
                    state.target_key.as_deref(),
                    now_ms,
                );
                if row.state != TroopState::Idle {
                    row.march_id = state.march_id;
                    row.free_at_ms = Some(state.available_after_ms);
                    if let Some(key) = state.target_key.as_deref() {
                        let parts = key.split(':').filter_map(|part| part.parse::<i64>().ok()).collect::<Vec<_>>();
                        if let [kingdom, x, y] = parts[..] {
                            row.kingdom_id = Some(kingdom);
                            row.x = Some(x);
                            row.y = Some(y);
                        }
                    }
                }
            }
            if let Some(march_id) = row.march_id.filter(|_| row.state != TroopState::Idle) {
                self.fill_from_ledger(&account_id, march_id, &mut row, now_ms).await?;
            }
            rows.push(row);
        }
        let recent = self
            .recent_marches(&account_id, recent_limit)
            .await?
            .into_iter()
            .filter(|march| march.result_at_ms.is_some() || march.status != "sent")
            .map(recent_march)
            .collect();
        Ok(TroopOverview { rows, recent })
    }

    async fn fill_from_ledger(
        &self,
        account_id: &str,
        march_id: i64,
        row: &mut TroopRow,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let Some(record) = sqlx::query(
            "SELECT kingdom_id, x, y, level, sent_at_ms, duration_s, troop_count, coin_loot,
                    ruby_loot, troops_lost, army_json, status
             FROM attack_ledger WHERE account_id = ? AND march_id = ?",
        )
        .bind(account_id)
        .bind(march_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(());
        };
        row.kingdom_id = Some(record.get("kingdom_id"));
        row.x = Some(record.get("x"));
        row.y = Some(record.get("y"));
        row.level = record.get("level");
        let sent: i64 = record.get("sent_at_ms");
        row.sent_at_ms = Some(sent);
        if row.state == TroopState::Outbound {
            let duration = record.get::<Option<i64>, _>("duration_s");
            row.lands_at_ms = duration
                .filter(|seconds| *seconds > 0)
                .map(|seconds| sent + seconds * 1_000);
            // Landed, and no return was ever reported (the session was interrupted):
            // it is on its way home, at most as long as the trip out took.
            if let (Some(lands), Some(bound)) = (row.lands_at_ms, return_bound_ms(sent, duration))
                && lands <= now_ms
            {
                if bound <= now_ms {
                    // Home already, as far as can be told: a hold that has outlived its
                    // march must not keep the commander looking busy.
                    row.state = TroopState::Idle;
                    row.march_id = None;
                    row.free_at_ms = None;
                    row.lands_at_ms = None;
                    return Ok(());
                }
                row.state = TroopState::Returning;
                row.estimated_return = true;
                row.free_at_ms = Some(row.free_at_ms.map_or(bound, |free| free.min(bound)));
                row.lands_at_ms = None;
            }
        }
        row.troops = record
            .get::<Option<String>, _>("army_json")
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        row.troop_total = record
            .get::<Option<i64>, _>("troop_count")
            .unwrap_or_else(|| row.troops.values().sum());
        row.troops_lost = record.get("troops_lost");
        row.coin_loot = record.get("coin_loot");
        row.ruby_loot = record.get("ruby_loot");
        Ok(())
    }
}

fn recent_march(march: MarchRecord) -> RecentMarch {
    RecentMarch {
        march_id: march.march_id,
        lord_id: march.lord_id,
        kingdom_id: march.kingdom_id,
        x: march.x,
        y: march.y,
        status: march.status,
        sent_at_ms: march.sent_at_ms,
        troop_total: march.troop_count,
        coin_loot: march.coin_loot,
        ruby_loot: march.ruby_loot,
        result_flag: march.result_flag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ledger::RETURN_BOUND_MARGIN_MS;
    use crate::store::{COMMANDER_AVAILABLE, COMMANDER_RETURNING, CommanderState, MARCH_SENT};

    #[test]
    fn a_commander_is_idle_once_its_hold_has_run_out_whatever_the_ledger_says() {
        assert_eq!(troop_state("outbound", 100, Some(1), Some("1:5:5"), 100), TroopState::Idle);
        assert_eq!(troop_state("returning", 50, Some(1), None, 100), TroopState::Idle);
        assert_eq!(troop_state(COMMANDER_AVAILABLE, 0, None, None, 100), TroopState::Idle);
    }

    #[test]
    fn a_hold_says_what_the_commander_is_doing() {
        assert_eq!(troop_state("outbound", 500, None, Some("1:5:5"), 100), TroopState::Sending);
        assert_eq!(troop_state("outbound", 500, Some(7), Some("1:5:5"), 100), TroopState::Outbound);
        assert_eq!(troop_state("returning", 500, Some(7), None, 100), TroopState::Returning);
        assert_eq!(troop_state("outbound", 500, None, Some("station:437391"), 100), TroopState::Station, "a support lease is not an attack");
    }

    async fn store() -> Store {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store.upsert_account_profile("ventrilo", "Ventrilo", "wss://x/", "EmpireEx_21", 1).await.unwrap();
        let lids = crate::hunt::USABLE_COMMANDER_LIDS[..4].to_vec();
        store.replace_account_bootstrap("ventrilo", &[], &lids, 1).await.unwrap();
        store
    }

    #[tokio::test]
    async fn the_overview_lists_every_usable_commander_with_its_march_troops_and_times() {
        let store = store().await;
        let lids = &crate::hunt::USABLE_COMMANDER_LIDS[..4];
        let now = 1_000_000;
        // Commander 1 is out, with a recorded army; 2 is back and carrying loot; 3, 4 idle.
        let march = MarchRecord {
            account_id: "ventrilo".into(), march_id: 11, kingdom_id: 1, x: 636, y: 574, task_id: Some("t".into()),
            profile_id: None, level: Some(43), lord_id: Some(lids[0]), commander_number: None, troop_count: None,
            duration_s: Some(300), coin_loot: None, ruby_loot: None, status: MARCH_SENT.into(), result_flag: None,
            error_message: None, sent_at_ms: now - 100_000, landed_at_ms: None, result_at_ms: None,
        };
        store.record_march(&march).await.unwrap();
        store.set_march_army("ventrilo", 11, &BTreeMap::from([(602, 50)])).await.unwrap();
        store.set_commander_state(&CommanderState { account_id: "ventrilo".into(), lord_id: lids[0], status: COMMANDER_OUTBOUND.into(), available_after_ms: now + 700_000, march_id: Some(11), target_key: Some("1:636:574".into()) }, now).await.unwrap();
        let back = MarchRecord { march_id: 12, lord_id: Some(lids[1]), x: 700, y: 700, status: "returning".into(), coin_loot: Some(1_200), ruby_loot: Some(3), ..march.clone() };
        store.record_march(&back).await.unwrap();
        store.set_commander_state(&CommanderState { account_id: "ventrilo".into(), lord_id: lids[1], status: COMMANDER_RETURNING.into(), available_after_ms: now + 90_000, march_id: Some(12), target_key: Some("1:700:700".into()) }, now).await.unwrap();

        let overview = store.troop_overview("Ventrilo", now, 10).await.unwrap();
        assert_eq!(overview.rows.len(), 4);
        let out = &overview.rows[0];
        assert_eq!((out.number, out.state), (1, TroopState::Outbound));
        assert_eq!((out.kingdom_id, out.x, out.y, out.level), (Some(1), Some(636), Some(574), Some(43)));
        assert_eq!(out.lands_at_ms, Some(now - 100_000 + 300_000));
        assert_eq!(out.free_at_ms, Some(now + 700_000));
        assert_eq!((out.troop_total, out.troops.get(&602)), (50, Some(&50)));
        let home = &overview.rows[1];
        assert_eq!(home.state, TroopState::Returning);
        assert_eq!((home.coin_loot, home.ruby_loot), (Some(1_200), Some(3)));
        assert_eq!(home.lands_at_ms, None, "it has already landed");
        assert!(overview.rows[2..].iter().all(|row| row.state == TroopState::Idle && row.march_id.is_none()));
        assert_eq!(overview.rows[3].number, 4);
    }

    #[tokio::test]
    async fn a_march_that_landed_unseen_is_returning_by_a_bound_and_then_idle() {
        let store = store().await;
        let lid = crate::hunt::USABLE_COMMANDER_LIDS[0];
        let sent = 1_000_000;
        let march = MarchRecord {
            account_id: "ventrilo".into(), march_id: 21, kingdom_id: 1, x: 5, y: 6, task_id: None, profile_id: None,
            level: None, lord_id: Some(lid), commander_number: None, troop_count: Some(50), duration_s: Some(300),
            coin_loot: None, ruby_loot: None, status: MARCH_SENT.into(), result_flag: None, error_message: None,
            sent_at_ms: sent, landed_at_ms: None, result_at_ms: None,
        };
        store.record_march(&march).await.unwrap();
        // The hold runs far beyond the real return (an hour), as after an interruption.
        store.set_commander_state(&CommanderState { account_id: "ventrilo".into(), lord_id: lid, status: COMMANDER_OUTBOUND.into(), available_after_ms: sent + 3_600_000, march_id: Some(21), target_key: Some("1:5:6".into()) }, sent).await.unwrap();
        // Landed 100 s ago (sent + 300 s), return never seen.
        let landed = store.troop_overview("ventrilo", sent + 400_000, 5).await.unwrap().rows.remove(0);
        assert_eq!(landed.state, TroopState::Returning);
        assert!(landed.estimated_return);
        assert_eq!(landed.free_at_ms, Some(sent + 600_000 + RETURN_BOUND_MARGIN_MS), "bounded by the trip out, not by the stale hour-long hold");
        // Past the bound it has certainly returned: the commander is free again.
        let later = store.troop_overview("ventrilo", sent + 700_000, 5).await.unwrap().rows.remove(0);
        assert_eq!((later.state, later.march_id, later.estimated_return), (TroopState::Idle, None, false));
    }

    #[tokio::test]
    async fn lapsing_closes_only_marches_past_their_bound_and_frees_only_their_own_commander() {
        let store = store().await;
        let lids = &crate::hunt::USABLE_COMMANDER_LIDS[..2];
        let sent = 1_000_000;
        let march = |id: i64, lid: i64, duration: i64| MarchRecord {
            account_id: "ventrilo".into(), march_id: id, kingdom_id: 1, x: 5, y: 6, task_id: None, profile_id: None,
            level: None, lord_id: Some(lid), commander_number: None, troop_count: None, duration_s: Some(duration),
            coin_loot: None, ruby_loot: None, status: MARCH_SENT.into(), result_flag: None, error_message: None,
            sent_at_ms: sent, landed_at_ms: None, result_at_ms: None,
        };
        store.record_march(&march(1, lids[0], 100)).await.unwrap();
        store.record_march(&march(2, lids[1], 900)).await.unwrap();
        for (lid, id) in [(lids[0], 1), (lids[1], 2)] {
            store.set_commander_state(&CommanderState { account_id: "ventrilo".into(), lord_id: lid, status: COMMANDER_OUTBOUND.into(), available_after_ms: sent + 9_000_000, march_id: Some(id), target_key: None }, sent).await.unwrap();
        }
        // 300 s later: march 1 (100 s each way) is long home, march 2 (900 s) is not.
        let now = sent + 300_000;
        assert_eq!(store.lapse_stale_marches("Ventrilo", now).await.unwrap(), 1);
        assert_eq!(store.lapse_stale_marches("ventrilo", now).await.unwrap(), 0, "once");
        let states = store.commander_states("ventrilo").await.unwrap();
        assert_eq!(states.iter().find(|s| s.lord_id == lids[0]).unwrap().status, COMMANDER_AVAILABLE);
        assert!(states.iter().find(|s| s.lord_id == lids[0]).unwrap().available_after_ms <= now);
        assert_eq!(states.iter().find(|s| s.lord_id == lids[1]).unwrap().status, COMMANDER_OUTBOUND);
        assert!(store.open_marches().await.unwrap().iter().all(|march| march.march_id == 2), "the lapsed one is closed");
    }

    #[tokio::test]
    async fn another_accounts_commanders_are_not_listed() {
        let store = store().await;
        assert!(store.troop_overview("pingpoko", 0, 5).await.unwrap().rows.is_empty());
    }
}
