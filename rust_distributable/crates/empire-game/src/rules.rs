//! Kingdom rules and minute-skip items, by the game's own field names.
//!
//! Rows are `generated::rules::{KINGDOM_RULES, MINUTE_SKIPS}`, built by `utils/scripts/build_tables.py` from the game's items file
//! (`kingdoms` and `currencyMinutesSkipValues`); the full rows are `data/tables/kingdoms.csv` and `minute_skips.csv`. Fields are
//! the game's names in snake_case (`unitTravelTaxRate` is `unit_travel_tax_rate`). Manual: `docs/game_data/tables.md`.

use serde::Serialize;

use crate::generated::rules::{KINGDOM_RULES, MINUTE_SKIPS};

/// One row of the game's `kingdoms` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct KingdomRule {
    /// `kID`.
    pub k_id: i64,
    /// `kingdomName`, the game's internal name (`Classic`, `Dessert`, `Icecream`, `Volcano`, `Eiland`, `Faction`).
    pub kingdom_name: &'static str,
    /// The player-facing name, from the language key `kingdomName_<kingdomName>` ("The Great Empire").
    pub display_name: Option<&'static str>,
    /// The name this project's older tables used (`green_kingdom`).
    pub legacy_name: Option<&'static str>,
    /// `unitTravelTime`: seconds a troop transfer into the kingdom takes.
    pub unit_travel_time: Option<i64>,
    pub resource_travel_time: Option<i64>,
    /// `unitTravelTaxRate`: percent of each unit's `kingdomTravellingCost` charged to send troops **into** this kingdom.
    pub unit_travel_tax_rate: Option<i64>,
    pub resource_travel_tax_rate: Option<i64>,
    pub min_level: Option<i64>,
    pub skip_unit_travel_c2_cost: Option<i64>,
    pub skip_resource_travel_c2_cost: Option<i64>,
}

/// One row of the game's `currencyMinutesSkipValues`, with the `MS<n>` key the `msk` packet carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MinuteSkip {
    /// `MST` in the `msk` packet: `MS` plus `minute_skip_index + 1`.
    pub mst: &'static str,
    /// `currencyID`.
    pub currency_id: i64,
    /// `MinuteSkipIndex`.
    pub minute_skip_index: i64,
    /// `MinutesSkipValue`: the minutes one item takes off a transfer.
    pub minutes_skip_value: i64,
}

/// The rules of the kingdom with this `kID`.
pub fn kingdom_rule(k_id: i64) -> Option<&'static KingdomRule> {
    KINGDOM_RULES.iter().find(|rule| rule.k_id == k_id)
}

/// Every kingdom row, sorted by `k_id`.
pub fn kingdom_rules() -> &'static [KingdomRule] {
    KINGDOM_RULES
}

/// The minute-skip items, shortest first.
pub fn minute_skips() -> &'static [MinuteSkip] {
    MINUTE_SKIPS
}
