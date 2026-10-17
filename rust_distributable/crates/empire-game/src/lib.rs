//! Static Goodgame Empire game data and the attack payload codec.
//!
//! Nothing here performs I/O or holds mutable state: it is the shared vocabulary
//! the rest of the application builds on (task definitions, the attack editor,
//! and the event runner all reference these tables).
//!
//! The tables in [`generated`] are produced from the Python bot's own data
//! modules by `scripts/gen_game_data.py` at the repository root, so the two
//! implementations cannot drift. Re-run that script after editing
//! `bot/game_data/*.py`.

pub mod attack;
pub mod generated;

pub use attack::{
    Attack, AttackError, EMPTY_SLOT, FlankSlots, LEFT_SLOTS, MIDDLE_SLOTS, RIGHT_SLOTS, Side, Slot,
    Wave,
};
pub use generated::kingdoms::{
    BERIMOND_KINGDOM_ID, FIRE_KINGDOM_ID, GREEN_KINGDOM_ID, ICE_KINGDOM_ID, KINGDOMS, Kingdom,
    SAND_KINGDOM_ID, STORM_KINGDOM_ID, kingdom_by_id,
};
pub use generated::tools::{TOOLS, ToolStats, tool_by_id, tool_by_name};
pub use generated::travel_costs::{KINGDOM_TRANSFER_RATES, MINUTE_SKIPS, TRAVEL_COSTS};
pub use generated::troops::{TROOPS, TroopStats, troop_by_id, troop_by_name};

/// A unit's `kingdomTravellingCost` (coins before the target kingdom's tax), when it has one.
pub fn kingdom_travelling_cost(unit_id: i64) -> Option<i64> {
    TRAVEL_COSTS
        .binary_search_by_key(&unit_id, |(id, _)| *id)
        .ok()
        .map(|index| TRAVEL_COSTS[index].1)
}
