//! Every unit and tool the game defines, by the game's own ids and field names.
//!
//! The rows are `generated::units::UNITS`, built by `utils/scripts/build_tables.py` from the game's items file (the `units`
//! section: troops and tools are one table there). The full row, with every column, is `data/tables/units.csv`; this struct carries
//! the fields the code reads. **Field names are the game's, in snake_case** (`kingdomTravellingCost` is
//! `kingdom_travelling_cost`); `type` is `type_name` because `type` is a Rust keyword. Manual: `docs/game_data/tables.md`.
//!
//! This is the complete table. The older, smaller `generated::troops` / `generated::tools` tables are still what decides which units
//! the bot builds attacks from and sends as support (see the manual, "What still uses the older tables").

use serde::Serialize;

use crate::generated::units::UNITS;

/// The game's `role` field. Troops have one; tools have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Melee,
    Ranged,
}

/// One row of the game's `units` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Unit {
    /// `wodID`: the id every packet uses (`AC` in `dcl`, `A` in `kut`, ...).
    pub wod_id: i64,
    /// `type`: the game's internal name, for example `MeadRanger` or `Eventknight`.
    pub type_name: &'static str,
    /// What the player reads: the language key `<type>_name` ("Valkyrie ranger"). Absent for about a sixth of the rows.
    pub display_name: Option<&'static str>,
    /// The name this project's older tables used (`valkyrie_ranger_9`), kept so old names can be found.
    pub legacy_name: Option<&'static str>,
    /// `role`: `Some` for a troop (melee or ranged), `None` for a tool.
    pub role: Option<Role>,
    /// `comment1`: where it comes from ("Relic Barracks").
    pub comment1: &'static str,
    /// `comment2`: what it is ("Ranged Attacker").
    pub comment2: &'static str,
    pub level: Option<i64>,
    pub speed: Option<i64>,
    pub melee_attack: Option<i64>,
    pub range_attack: Option<i64>,
    pub melee_defence: Option<i64>,
    pub range_defence: Option<i64>,
    pub loot_value: Option<i64>,
    pub might_value: Option<i64>,
    pub food_supply: Option<i64>,
    /// `kingdomTravellingCost`: coins before the target kingdom's tax, when the unit can be sent between kingdoms.
    pub kingdom_travelling_cost: Option<i64>,
    pub upgrade_wod_id: Option<i64>,
    pub downgrade_wod_id: Option<i64>,
}

impl Unit {
    /// The game's own test: a unit with a `role` is a troop, one without is a tool.
    pub fn is_troop(&self) -> bool {
        self.role.is_some()
    }

    /// The name to show: the player-facing one, else the game's internal type name.
    pub fn label(&self) -> &'static str {
        self.display_name.unwrap_or(self.type_name)
    }
}

/// The unit with this `wodID`, if the game defines it.
pub fn unit_by_id(wod_id: i64) -> Option<&'static Unit> {
    UNITS.binary_search_by_key(&wod_id, |unit| unit.wod_id).ok().map(|index| &UNITS[index])
}

/// Every unit, sorted by `wod_id`.
pub fn units() -> &'static [Unit] {
    UNITS
}

/// A unit's `kingdomTravellingCost`, when it has a positive one (the price of sending it to another kingdom).
pub fn kingdom_travelling_cost(wod_id: i64) -> Option<i64> {
    unit_by_id(wod_id)?.kingdom_travelling_cost.filter(|cost| *cost > 0)
}
