//! Durable record of other players' movements (the PVP tab).
//!
//! One row per `(account_id, mid)`: a movement is learned once and stays learned.
//! A later `gam` for the same MID never moves the timestamps or the classification;
//! it only fills in an army size or name that was still unknown. A retreat is final.
//!
//! Reads are paged by a `(detected_at_ms, mid)` cursor and never load the table,
//! so the tab stays cheap however long the history grows. The state of a movement
//! (active, landed, retreated) is derived from the clock at read time instead of
//! being stored, so nothing has to tick it forward.

use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, sqlite::SqliteRow};

use super::{Store, canonical_account_id};
use crate::pvp::{AllianceIdentity, Classified, SeenMovement};

/// Where a movement is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PvpState {
    /// Still travelling by the estimated landing time.
    Active,
    /// Past its estimated landing time. Based on the clock only: arrival is not
    /// confirmed by any packet.
    Landed,
    /// Called back by a retreat; final.
    Retreated,
}

impl PvpState {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "landed" => Some(Self::Landed),
            "retreated" => Some(Self::Retreated),
            _ => None,
        }
    }
}

/// One stored movement, with the state worked out for `now`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PvpRow {
    pub mid: i64,
    pub kingdom_id: i64,
    pub direction: String,
    pub owner: String,
    pub state: PvpState,
    pub sender_id: Option<i64>,
    pub sender_name: Option<String>,
    pub sender_alliance: Option<String>,
    pub sender_x: i64,
    pub sender_y: i64,
    pub sender_place: Option<String>,
    pub target_id: Option<i64>,
    pub target_name: Option<String>,
    pub target_alliance: Option<String>,
    pub target_x: i64,
    pub target_y: i64,
    pub target_place: Option<String>,
    pub army_size: Option<i64>,
    pub kind: Option<i64>,
    pub duration_s: i64,
    pub detected_at_ms: i64,
    pub land_at_ms: i64,
    pub retreated_at_ms: Option<i64>,
}

/// Which rows to page through. Every filter is optional.
#[derive(Debug, Clone, Default)]
pub struct PvpQuery {
    pub direction: Option<String>,
    pub owner: Option<String>,
    pub state: Option<PvpState>,
    pub kingdom_id: Option<i64>,
    /// Only rows strictly older than this `(detected_at_ms, mid)`.
    pub before: Option<(i64, i64)>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PvpPage {
    pub rows: Vec<PvpRow>,
    /// Pass back as `before` for the next page; `None` when this was the last.
    pub next: Option<(i64, i64)>,
}

/// How many movements fall in one direction/state bucket, over the whole history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PvpCount {
    pub direction: String,
    pub state: PvpState,
    pub count: i64,
}

const MAX_PAGE: i64 = 200;

impl Store {
    /// Remember who the account is, so a movement can be called incoming or outgoing.
    pub async fn set_alliance_identity(
        &self,
        account_id: &str,
        identity: &AllianceIdentity,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO account_alliance
                 (account_id, player_id, player_name, alliance_id, alliance_name, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                 player_id = excluded.player_id, player_name = excluded.player_name,
                 alliance_id = excluded.alliance_id, alliance_name = excluded.alliance_name,
                 updated_at_ms = excluded.updated_at_ms",
        )
        .bind(canonical_account_id(account_id))
        .bind(identity.player_id)
        .bind(&identity.player_name)
        .bind(identity.alliance_id)
        .bind(&identity.alliance_name)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn alliance_identity(
        &self,
        account_id: &str,
    ) -> Result<Option<AllianceIdentity>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT player_id, player_name, alliance_id, alliance_name
             FROM account_alliance WHERE account_id = ?",
        )
        .bind(canonical_account_id(account_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| AllianceIdentity {
            player_id: row.get("player_id"),
            player_name: row.get("player_name"),
            alliance_id: row.get("alliance_id"),
            alliance_name: row.get("alliance_name"),
        }))
    }

    /// The account whose identity was recorded most recently: the one to show
    /// when nothing is connected.
    pub async fn latest_alliance_account(&self) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT account_id FROM account_alliance ORDER BY updated_at_ms DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await
    }

    /// The `(kingdom, x, y)` of every castle found when the account was
    /// initialised: what decides whether a target is the account's own.
    pub async fn owned_castle_tuples(
        &self,
        account_id: &str,
    ) -> Result<Vec<(i64, i64, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT kingdom_id, x, y FROM owned_castle WHERE lower(account_id) = lower(?)",
        )
        .bind(canonical_account_id(account_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("kingdom_id"), row.get("x"), row.get("y")))
            .collect())
    }

    /// Record classified movements first seen at `seen_at_ms`; returns how many
    /// were new. A movement already on record keeps its timestamps and
    /// classification and only gains details it was missing.
    pub async fn record_pvp_movements(
        &self,
        account_id: &str,
        movements: &[(SeenMovement, Classified)],
        seen_at_ms: i64,
    ) -> Result<u64, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        let mut added = 0;
        for (movement, class) in movements {
            let remaining_ms = (movement.duration_s - movement.progress_s).max(0) * 1_000;
            let inserted = sqlx::query(
                "INSERT OR IGNORE INTO pvp_movement (
                     account_id, mid, kingdom_id, direction, owner,
                     sender_id, sender_name, sender_alliance_id, sender_alliance,
                     sender_x, sender_y, sender_place,
                     target_id, target_name, target_alliance_id, target_alliance,
                     target_x, target_y, target_place,
                     army_size, kind, flag, duration_s, detected_at_ms, land_at_ms
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(movement.mid)
            .bind(movement.kingdom_id)
            .bind(class.direction.as_str())
            .bind(class.owner.as_str())
            .bind(movement.sender.player_id)
            .bind(&movement.sender.name)
            .bind(movement.sender.alliance_id)
            .bind(&movement.sender.alliance_name)
            .bind(movement.sender.x)
            .bind(movement.sender.y)
            .bind(&movement.sender.place)
            .bind(movement.target.player_id)
            .bind(&movement.target.name)
            .bind(movement.target.alliance_id)
            .bind(&movement.target.alliance_name)
            .bind(movement.target.x)
            .bind(movement.target.y)
            .bind(&movement.target.place)
            .bind(movement.army_size)
            .bind(movement.kind)
            .bind(movement.flag)
            .bind(movement.duration_s)
            .bind(seen_at_ms)
            .bind(seen_at_ms + remaining_ms)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if inserted > 0 {
                added += inserted;
                continue;
            }
            sqlx::query(
                "UPDATE pvp_movement SET
                     army_size = COALESCE(army_size, ?),
                     sender_name = COALESCE(sender_name, ?),
                     sender_alliance = COALESCE(sender_alliance, ?),
                     target_name = COALESCE(target_name, ?),
                     target_alliance = COALESCE(target_alliance, ?)
                 WHERE account_id = ? AND mid = ?",
            )
            .bind(movement.army_size)
            .bind(&movement.sender.name)
            .bind(&movement.sender.alliance_name)
            .bind(&movement.target.name)
            .bind(&movement.target.alliance_name)
            .bind(&account_id)
            .bind(movement.mid)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(added)
    }

    /// Mark a recorded movement retreated. Final: a second retreat changes nothing.
    /// `false` when the MID is unknown (for instance one of our own marches).
    pub async fn retreat_pvp_movement(
        &self,
        account_id: &str,
        mid: i64,
        at_ms: i64,
    ) -> Result<bool, sqlx::Error> {
        let changed = sqlx::query(
            "UPDATE pvp_movement SET retreated_at_ms = ?
             WHERE account_id = ? AND mid = ? AND retreated_at_ms IS NULL",
        )
        .bind(at_ms)
        .bind(canonical_account_id(account_id))
        .bind(mid)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(changed > 0)
    }

    /// One page of movements, newest detected first.
    pub async fn pvp_page(
        &self,
        account_id: &str,
        query: &PvpQuery,
        now_ms: i64,
    ) -> Result<PvpPage, sqlx::Error> {
        let limit = query.limit.clamp(1, MAX_PAGE);
        let mut sql = QueryBuilder::<Sqlite>::new("SELECT *, ");
        push_state(&mut sql, now_ms);
        sql.push(" AS state FROM pvp_movement WHERE account_id = ");
        sql.push_bind(canonical_account_id(account_id));
        if let Some(direction) = &query.direction {
            sql.push(" AND direction = ").push_bind(direction.clone());
        }
        if let Some(owner) = &query.owner {
            sql.push(" AND owner = ").push_bind(owner.clone());
        }
        if let Some(kingdom_id) = query.kingdom_id {
            sql.push(" AND kingdom_id = ").push_bind(kingdom_id);
        }
        match query.state {
            Some(PvpState::Active) => {
                sql.push(" AND retreated_at_ms IS NULL AND land_at_ms > ")
                    .push_bind(now_ms);
            }
            Some(PvpState::Landed) => {
                sql.push(" AND retreated_at_ms IS NULL AND land_at_ms <= ")
                    .push_bind(now_ms);
            }
            Some(PvpState::Retreated) => {
                sql.push(" AND retreated_at_ms IS NOT NULL");
            }
            None => {}
        }
        if let Some((detected_at_ms, mid)) = query.before {
            sql.push(" AND (detected_at_ms < ")
                .push_bind(detected_at_ms)
                .push(" OR (detected_at_ms = ")
                .push_bind(detected_at_ms)
                .push(" AND mid < ")
                .push_bind(mid)
                .push("))");
        }
        // One extra row says whether another page exists without a second query.
        sql.push(" ORDER BY detected_at_ms DESC, mid DESC LIMIT ")
            .push_bind(limit + 1);
        let mut rows = sql
            .build()
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(pvp_row)
            .collect::<Vec<_>>();
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next = more
            .then(|| rows.last().map(|row| (row.detected_at_ms, row.mid)))
            .flatten();
        Ok(PvpPage { rows, next })
    }

    /// Counts per direction and state over the whole history, without loading rows.
    pub async fn pvp_counts(
        &self,
        account_id: &str,
        now_ms: i64,
    ) -> Result<Vec<PvpCount>, sqlx::Error> {
        let mut sql = QueryBuilder::<Sqlite>::new("SELECT direction, ");
        push_state(&mut sql, now_ms);
        sql.push(" AS state, COUNT(*) AS count FROM pvp_movement WHERE account_id = ")
            .push_bind(canonical_account_id(account_id))
            .push(" GROUP BY direction, state ORDER BY direction, state");
        Ok(sql
            .build()
            .fetch_all(&self.pool)
            .await?
            .iter()
            .filter_map(|row| {
                Some(PvpCount {
                    direction: row.get("direction"),
                    state: PvpState::parse(row.get::<&str, _>("state"))?,
                    count: row.get("count"),
                })
            })
            .collect())
    }
}

fn push_state(sql: &mut QueryBuilder<'_, Sqlite>, now_ms: i64) {
    sql.push("CASE WHEN retreated_at_ms IS NOT NULL THEN 'retreated' WHEN land_at_ms > ")
        .push_bind(now_ms)
        .push(" THEN 'active' ELSE 'landed' END");
}

fn pvp_row(row: &SqliteRow) -> PvpRow {
    PvpRow {
        mid: row.get("mid"),
        kingdom_id: row.get("kingdom_id"),
        direction: row.get("direction"),
        owner: row.get("owner"),
        state: PvpState::parse(row.get::<&str, _>("state")).unwrap_or(PvpState::Landed),
        sender_id: row.get("sender_id"),
        sender_name: row.get("sender_name"),
        sender_alliance: row.get("sender_alliance"),
        sender_x: row.get("sender_x"),
        sender_y: row.get("sender_y"),
        sender_place: row.get("sender_place"),
        target_id: row.get("target_id"),
        target_name: row.get("target_name"),
        target_alliance: row.get("target_alliance"),
        target_x: row.get("target_x"),
        target_y: row.get("target_y"),
        target_place: row.get("target_place"),
        army_size: row.get("army_size"),
        kind: row.get("kind"),
        duration_s: row.get("duration_s"),
        detected_at_ms: row.get("detected_at_ms"),
        land_at_ms: row.get("land_at_ms"),
        retreated_at_ms: row.get("retreated_at_ms"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pvp::{Direction, Owner, Party};

    fn movement(mid: i64, army: Option<i64>) -> (SeenMovement, Classified) {
        let party = |id: i64, x: i64| Party {
            player_id: Some(id),
            name: Some(format!("player {id}")),
            alliance_id: Some(7),
            alliance_name: Some("Electi".into()),
            x,
            y: 520,
            ..Party::default()
        };
        (
            SeenMovement {
                mid,
                kingdom_id: 0,
                sender: party(1, 543),
                target: party(2, 546),
                progress_s: 4,
                duration_s: 57,
                army_size: army,
                kind: Some(0),
                flag: Some(0),
            },
            Classified {
                direction: Direction::Incoming,
                owner: Owner::Alliance,
            },
        )
    }

    async fn store() -> Store {
        Store::open("sqlite::memory:").await.unwrap()
    }

    fn query(limit: i64) -> PvpQuery {
        PvpQuery {
            limit,
            ..PvpQuery::default()
        }
    }

    #[tokio::test]
    async fn a_movement_is_learned_once_and_keeps_its_first_timestamps() {
        let store = store().await;
        let first = store
            .record_pvp_movements("Ventrilo", &[movement(10, None)], 1_000_000)
            .await
            .unwrap();
        let again = store
            .record_pvp_movements("ventrilo", &[movement(10, None)], 9_000_000)
            .await
            .unwrap();
        assert_eq!((first, again), (1, 0));
        let page = store.pvp_page("ventrilo", &query(10), 1_000_000).await.unwrap();
        assert_eq!(page.rows.len(), 1);
        let row = &page.rows[0];
        assert_eq!(row.detected_at_ms, 1_000_000);
        assert_eq!(row.land_at_ms, 1_000_000 + 53_000, "(TT - PT) seconds after first sight");
    }

    #[tokio::test]
    async fn a_later_packet_fills_in_an_unknown_army_but_never_changes_a_known_one() {
        let store = store().await;
        store
            .record_pvp_movements("a", &[movement(10, None)], 1_000)
            .await
            .unwrap();
        store
            .record_pvp_movements("a", &[movement(10, Some(40))], 2_000)
            .await
            .unwrap();
        store
            .record_pvp_movements("a", &[movement(10, Some(99))], 3_000)
            .await
            .unwrap();
        let rows = store.pvp_page("a", &query(10), 0).await.unwrap().rows;
        assert_eq!(rows[0].army_size, Some(40));
        assert_eq!(rows[0].detected_at_ms, 1_000);
    }

    #[tokio::test]
    async fn state_follows_the_clock_and_a_retreat_is_final() {
        let store = store().await;
        store
            .record_pvp_movements("a", &[movement(1, Some(5)), movement(2, Some(5))], 0)
            .await
            .unwrap();
        let state = |now: i64, mid: i64| {
            let store = store.clone();
            async move {
                let rows = store.pvp_page("a", &query(10), now).await.unwrap().rows;
                rows.into_iter().find(|row| row.mid == mid).unwrap().state
            }
        };
        assert_eq!(state(10_000, 1).await, PvpState::Active);
        assert_eq!(state(60_000, 1).await, PvpState::Landed);
        assert!(store.retreat_pvp_movement("a", 1, 20_000).await.unwrap());
        assert!(!store.retreat_pvp_movement("a", 1, 30_000).await.unwrap(), "already final");
        assert!(!store.retreat_pvp_movement("a", 999, 30_000).await.unwrap(), "unknown MID");
        assert_eq!(state(10_000, 1).await, PvpState::Retreated);
        assert_eq!(state(10_000, 2).await, PvpState::Active);
        let row = store.pvp_page("a", &query(10), 0).await.unwrap().rows;
        let retreated = row.iter().find(|row| row.mid == 1).unwrap();
        assert_eq!(retreated.retreated_at_ms, Some(20_000), "first retreat time stands");
    }

    #[tokio::test]
    async fn pages_walk_the_history_without_repeating_or_skipping_rows() {
        let store = store().await;
        let batch = (1..=7).map(|mid| movement(mid, Some(1))).collect::<Vec<_>>();
        // Two share a detection instant, so the MID has to break the tie.
        store.record_pvp_movements("a", &batch[..3], 1_000).await.unwrap();
        store.record_pvp_movements("a", &batch[3..], 2_000).await.unwrap();
        let mut seen = Vec::new();
        let mut before = None;
        loop {
            let page = store
                .pvp_page("a", &PvpQuery { before, limit: 3, ..PvpQuery::default() }, 0)
                .await
                .unwrap();
            seen.extend(page.rows.iter().map(|row| row.mid));
            match page.next {
                Some(cursor) => before = Some(cursor),
                None => break,
            }
        }
        assert_eq!(seen, vec![7, 6, 5, 4, 3, 2, 1]);
    }

    #[tokio::test]
    async fn filters_and_counts_are_per_account_and_cover_the_whole_history() {
        let store = store().await;
        let mut outgoing = movement(2, Some(1));
        outgoing.1.direction = Direction::Outgoing;
        store
            .record_pvp_movements("a", &[movement(1, Some(1)), outgoing], 0)
            .await
            .unwrap();
        store
            .record_pvp_movements("b", &[movement(1, Some(1))], 0)
            .await
            .unwrap();
        let only_out = PvpQuery {
            direction: Some("outgoing".into()),
            limit: 10,
            ..PvpQuery::default()
        };
        assert_eq!(store.pvp_page("a", &only_out, 0).await.unwrap().rows.len(), 1);
        let counts = store.pvp_counts("a", 1_000_000).await.unwrap();
        assert_eq!(
            counts,
            vec![
                PvpCount { direction: "incoming".into(), state: PvpState::Landed, count: 1 },
                PvpCount { direction: "outgoing".into(), state: PvpState::Landed, count: 1 },
            ]
        );
    }

    #[tokio::test]
    async fn identity_and_owned_castles_round_trip() {
        let store = store().await;
        assert!(store.alliance_identity("a").await.unwrap().is_none());
        let identity = AllianceIdentity {
            player_id: 5,
            player_name: Some("Ventrilo".into()),
            alliance_id: Some(9),
            alliance_name: Some("Sons".into()),
        };
        store.set_alliance_identity("A", &identity, 1).await.unwrap();
        assert_eq!(store.alliance_identity("a").await.unwrap(), Some(identity));
        assert!(store.owned_castle_tuples("a").await.unwrap().is_empty());
    }
}
