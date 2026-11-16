//! Refinery rules: which crafts to start, whether the castle can afford them, when to look again.
//!
//! Pure (no I/O). The daemon's `direct/refinery.rs` runs the packets; the store keeps the presets and
//! the schedule. Packets: `docs/network/crst.md`, `docs/network/crin.md`. Flow:
//! `docs/architecture/resource-bot.md`.
//!
//! The refinery has one production slot and one queue slot, so one visit starts at most two crafts.
//! The selected recipes are cycled across those places: wood + stone gives one of each, wood alone
//! gives two lots of wood. Every craft is checked against the castle's stock before anything is sent,
//! so a sequence never starts that the server would refuse half way.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pacing::Rng;

/// A recipe the refinery can run (`craftingRecipes` in the items file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefineRecipe {
    /// CRID 1: 54,400 wood to 100 refined wood, one hour.
    RefinedWood,
    /// CRID 34: 54,400 stone to 100 refined stone, one hour.
    RefinedStone,
}

/// Which raw resource a recipe spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Wood,
    Stone,
}

impl RefineRecipe {
    pub const ALL: [RefineRecipe; 2] = [RefineRecipe::RefinedWood, RefineRecipe::RefinedStone];

    pub fn crid(self) -> i64 {
        match self {
            Self::RefinedWood => 1,
            Self::RefinedStone => 34,
        }
    }

    pub fn from_crid(crid: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|recipe| recipe.crid() == crid)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::RefinedWood => "refined wood",
            Self::RefinedStone => "refined stone",
        }
    }

    pub fn resource(self) -> Resource {
        match self {
            Self::RefinedWood => Resource::Wood,
            Self::RefinedStone => Resource::Stone,
        }
    }

    /// Raw resource spent per craft (items v787.03).
    pub fn cost(self) -> f64 {
        54_400.0
    }

    pub fn duration_s(self) -> i64 {
        3_600
    }
}

/// Places in the refinery: one producing, one waiting (`craftingQueues`, queue type 1).
pub const REFINERY_PLACES: usize = 2;
/// The refinery's queue in `crin` (`CBI[].CQID`).
pub const REFINERY_QUEUE_ID: i64 = 1;
/// At least this long between two visits (the diagram's "at least 2 hours elapsed").
pub const MIN_INTERVAL_MS: i64 = 2 * 3_600_000;
/// Look again this soon when the castle could not afford a single craft.
pub const SHORT_RETRY_MS: i64 = 30 * 60_000;
/// Bounds of the random gap before each `crst`, in seconds ("not both at the same second", under 10 s).
pub const STAGGER_S: (f64, f64) = (1.5, 9.0);

/// The refinery as `crin` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineryView {
    pub kingdom_id: i64,
    pub castle_id: i64,
    pub object_id: i64,
    /// Crafts producing now (`PS.CRID`), with their seconds left (`PS.RCT`).
    pub producing: Vec<(i64, i64)>,
    /// Crafts waiting (`QS.CRID`).
    pub queued: Vec<i64>,
}

impl RefineryView {
    pub fn free_places(&self) -> usize {
        REFINERY_PLACES.saturating_sub(self.producing.len() + self.queued.len())
    }

    /// Seconds until every craft now in the building has finished.
    pub fn busy_for_s(&self) -> i64 {
        let producing = self.producing.iter().map(|(_, left)| *left).max().unwrap_or(0);
        let queued: i64 = self
            .queued
            .iter()
            .map(|crid| RefineRecipe::from_crid(*crid).map_or(3_600, RefineRecipe::duration_s))
            .sum();
        producing + queued
    }
}

/// The castle's refinery from a `crin` reply (or a `crst` reply, which has the same building shape).
pub fn parse_refinery(payload: &Value, castle_id: i64) -> Option<RefineryView> {
    let buildings: Vec<&Value> = match payload.get("CAI") {
        Some(kingdoms) => kingdoms
            .as_array()?
            .iter()
            .flat_map(|kingdom| kingdom.get("CBI").and_then(Value::as_array).into_iter().flatten())
            .collect(),
        None => vec![payload],
    };
    let building = buildings.into_iter().find(|building| {
        building.get("CQID").and_then(Value::as_i64) == Some(REFINERY_QUEUE_ID)
            && building.get("AID").and_then(Value::as_i64) == Some(castle_id)
    })?;
    let ints = |value: Option<&Value>| -> Vec<i64> {
        value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_i64).collect()
    };
    let crids = ints(building.pointer("/PS/CRID"));
    let left = ints(building.pointer("/PS/RCT"));
    Some(RefineryView {
        kingdom_id: building.get("KID").and_then(Value::as_i64)?,
        castle_id,
        object_id: building.get("OID").and_then(Value::as_i64)?,
        producing: crids.iter().enumerate().map(|(i, crid)| (*crid, left.get(i).copied().unwrap_or(3_600))).collect(),
        queued: ints(building.pointer("/QS/CRID")),
    })
}

/// The crafts to start now: the selected recipes cycled over the free places (continuing from
/// `cycle_start`, so wood and stone alternate across visits), keeping only those the stock still
/// covers after the ones before them. Order is kept: the crafts are sent in this order.
pub fn plan(selected: &[RefineRecipe], free_places: usize, cycle_start: usize, wood: f64, stone: f64) -> Vec<RefineRecipe> {
    if selected.is_empty() {
        return Vec::new();
    }
    let (mut wood, mut stone) = (wood, stone);
    let mut crafts = Vec::new();
    for index in 0..free_places {
        let recipe = selected[(cycle_start + index) % selected.len()];
        let left = match recipe.resource() {
            Resource::Wood => &mut wood,
            Resource::Stone => &mut stone,
        };
        if *left >= recipe.cost() {
            *left -= recipe.cost();
            crafts.push(recipe);
        }
    }
    crafts
}

/// When to visit the castle again after a visit at `now_ms`: once the building is empty again
/// (`busy_for_s` after the crafts just started), never sooner than [`MIN_INTERVAL_MS`], plus up to
/// two minutes of jitter so visits do not land on the same second every cycle. A visit that could
/// afford nothing retries after [`SHORT_RETRY_MS`].
pub fn next_visit_ms(now_ms: i64, busy_for_s: i64, started: usize, rng: &mut Rng) -> i64 {
    let jitter = (rng.uniform(5.0, 120.0) * 1_000.0) as i64;
    if started == 0 && busy_for_s == 0 {
        return now_ms + SHORT_RETRY_MS + jitter;
    }
    now_ms + (busy_for_s * 1_000).max(MIN_INTERVAL_MS) + jitter
}

/// The random gap before the next `crst`, in milliseconds.
pub fn stagger_ms(rng: &mut Rng) -> i64 {
    (rng.uniform(STAGGER_S.0, STAGGER_S.1) * 1_000.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use RefineRecipe::*;

    #[test]
    fn crin_from_the_capture_finds_the_refinery() {
        let payload = json!({"CAI":[{"CBI":[
            {"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[]},"PS":{"CRID":[],"RCT":[]},"S":4,"WID":2174},
            {"KID":0,"AID":16011862,"OID":394,"CQID":2,"QS":{"CRID":[]},"PS":{"CRID":[],"RCT":[]},"S":12,"WID":2175}]}]});
        let view = parse_refinery(&payload, 16011862).unwrap();
        assert_eq!((view.object_id, view.kingdom_id, view.free_places(), view.busy_for_s()), (393, 0, 2, 0));
    }

    #[test]
    fn a_crst_reply_reads_as_a_busy_refinery() {
        let payload = json!({"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[34]},"PS":{"CRID":[1],"RCT":[3599]},"S":4});
        let view = parse_refinery(&payload, 16011862).unwrap();
        assert_eq!((view.free_places(), view.busy_for_s()), (0, 3599 + 3600));
    }

    #[test]
    fn two_recipes_alternate_and_one_recipe_fills_both_places() {
        assert_eq!(plan(&[RefinedWood, RefinedStone], 2, 0, 1e6, 1e6), vec![RefinedWood, RefinedStone]);
        assert_eq!(plan(&[RefinedWood, RefinedStone], 1, 1, 1e6, 1e6), vec![RefinedStone]);
        assert_eq!(plan(&[RefinedWood], 2, 0, 1e6, 1e6), vec![RefinedWood, RefinedWood]);
    }

    #[test]
    fn the_stock_is_checked_before_the_sequence_for_every_craft() {
        // Enough wood for one lot only: the second wood lot is dropped, not refused by the server.
        assert_eq!(plan(&[RefinedWood], 2, 0, 60_000.0, 0.0), vec![RefinedWood]);
        assert_eq!(plan(&[RefinedWood, RefinedStone], 2, 0, 10.0, 54_400.0), vec![RefinedStone]);
        assert!(plan(&[RefinedWood, RefinedStone], 2, 0, 0.0, 0.0).is_empty());
    }

    #[test]
    fn visits_wait_for_the_queue_and_at_least_two_hours() {
        let mut rng = Rng::seeded(7);
        let next = next_visit_ms(0, 7_200, 2, &mut rng);
        assert!((7_205_000..=7_320_000).contains(&next));
        let next = next_visit_ms(0, 3_000, 1, &mut rng);
        assert!(next >= MIN_INTERVAL_MS);
        let next = next_visit_ms(0, 0, 0, &mut rng);
        assert!(next < MIN_INTERVAL_MS && next >= SHORT_RETRY_MS);
        for _ in 0..200 {
            let gap = stagger_ms(&mut rng);
            assert!((1_500..9_000).contains(&gap));
        }
    }
}
