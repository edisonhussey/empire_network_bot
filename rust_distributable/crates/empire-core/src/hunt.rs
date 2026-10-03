//! The attack cycle: scan, inspect a target, then commit to a march.
//!
//! This module is pure — it builds packets and makes selections. Sending,
//! timing and persistence belong to the caller. Available without the
//! `application` feature so the command-line hunter can use it.
//!
//! Wire shapes come from `docs/network_requests.md` §4.

use serde_json::{Value, json};

use crate::protocol::{PacketError, encode_client_xt};

/// Travel option (`HBW`) accepted by the US1 live server. Captured evidence:
/// the real client sent 1004, and 1001 was rejected with status 5.
pub const DEFAULT_HBW: i64 = 1007;

/// `PTT` for a map attack. Berimond used 1; the map attack uses 0.
pub const MAP_PTT: i64 = 0;

/// Area type that marks an attackable RBC/baron tower row in a `gaa` payload.
pub const RBC_AREA_TYPE: i64 = 2;

/// Side length of one map tile. Tiles overlap by nothing: they abut.
pub const TILE_SPAN: i64 = 13;

/// One map object worth attacking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapTarget {
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    /// Resolved game level, when the kingdom's level code is known.
    pub level: Option<i64>,
}

impl MapTarget {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.kingdom_id, self.x, self.y)
    }
}

/// Sands level codes are compressed; this is the inverse the Python bot used.
pub fn sands_level(raw: i64) -> i64 {
    (1.9 * (raw.max(0) as f64).powf(0.555)).floor() as i64 + 35
}

/// RBC rows from a `gaa` response.
pub fn rbc_targets_from_map(payload: &Value) -> Vec<MapTarget> {
    let Some(kingdom_id) = payload.get("KID").and_then(Value::as_i64) else {
        return Vec::new();
    };
    payload
        .get("AI")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let row = entry.as_array()?;
            if row.first()?.as_i64()? != RBC_AREA_TYPE {
                return None;
            }
            let raw_level = row.get(4).and_then(Value::as_i64);
            Some(MapTarget {
                kingdom_id,
                x: row.get(1)?.as_i64()?,
                y: row.get(2)?.as_i64()?,
                level: raw_level.map(sands_level),
            })
        })
        .collect()
}

/// `gaa` tile request for one 13×13 square.
pub fn gaa_packet(
    server_header: &str,
    kingdom_id: i64,
    origin: (i64, i64),
) -> Result<String, PacketError> {
    encode_client_xt(
        server_header,
        "gaa",
        "1",
        &json!({
            "KID": kingdom_id,
            "AX1": origin.0,
            "AY1": origin.1,
            "AX2": origin.0 + TILE_SPAN - 1,
            "AY2": origin.1 + TILE_SPAN - 1,
        }),
    )
}

/// Tile origins covering a square of `radius` tiles around `centre`, nearest first.
///
/// `radius` 0 is just the tile containing the centre; 1 is the 3×3 block.
pub fn scan_tiles(centre: (i64, i64), radius: i64) -> Vec<(i64, i64)> {
    let radius = radius.max(0);
    let mut tiles = Vec::new();
    for row in -radius..=radius {
        for column in -radius..=radius {
            tiles.push((
                centre.0 + column * TILE_SPAN,
                centre.1 + row * TILE_SPAN,
            ));
        }
    }
    tiles.sort_by_key(|(x, y)| {
        (x - centre.0).abs() + (y - centre.1).abs()
    });
    tiles
}

/// "Attack detail" request. The reply carries the target row, the inventory
/// (`gui.I`) and the commander roster (`gli.C`).
pub fn adi_packet(
    server_header: &str,
    source: (i64, i64),
    target: &MapTarget,
) -> Result<String, PacketError> {
    encode_client_xt(
        server_header,
        "adi",
        "1",
        &json!({
            "SX": source.0,
            "SY": source.1,
            "TX": target.x,
            "TY": target.y,
            "KID": target.kingdom_id,
        }),
    )
}

/// "Create attack". `payload` is the `A` array from `empire_game::Attack::to_payload`.
pub fn attack_packet(
    server_header: &str,
    source: (i64, i64),
    target: &MapTarget,
    lord_id: i64,
    payload: &Value,
    hbw: i64,
    ptt: i64,
) -> Result<String, PacketError> {
    let reserved: Vec<Value> = (0..8).map(|_| json!([-1, 0])).collect();
    encode_client_xt(
        server_header,
        "cra",
        "1",
        &json!({
            "SX": source.0,
            "SY": source.1,
            "TX": target.x,
            "TY": target.y,
            "KID": target.kingdom_id,
            "LID": lord_id,
            "WT": 0,
            "HBW": hbw,
            "BPC": 0,
            "ATT": 0,
            "AV": 0,
            "LP": 0,
            "FC": 0,
            "PTT": ptt,
            "SD": 0,
            "ICA": 0,
            "CD": 99,
            "A": payload,
            "BKS": [],
            "AST": [-1, -1, -1],
            "RW": reserved,
            "ASCT": 0,
        }),
    )
}

/// Keep-alive frame. The socket is closed by the server if it goes quiet.
///
/// Deliberately raw rather than JSON-encoded: the payload is the literal token
/// `<RoundHouseKick>`, which is not valid JSON.
pub fn heartbeat_packet(server_header: &str) -> String {
    format!("%xt%{server_header}%pin%1%<RoundHouseKick>%")
}

/// Commander LIDs that are actually usable, in allocation order.
///
/// The roster the server reports is **not** the same set: LIDs 1, 4, 5, 12, 13,
/// 14, 15 and 19 are deliberately absent because they cannot be used for a
/// march. Selecting straight from the roster therefore walks onto unusable
/// lords just as often as usable ones.
///
/// Source: `bot/scheduler.py::DEFAULT_COMMANDER_LIDS_BY_HUMAN_NUMBER`, whose
/// first fifteen entries are `bot/bot.py::FIRST_13_COMMANDER_LIDS`.
pub const USABLE_COMMANDER_LIDS: &[i64] = &[
    0, 2, 3, 6, 7, 8, 9, 10, 11, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
];

/// Commander to use: the first roster entry the server says is available.
///
/// Falls back to the first roster entry when the server sent no usable list, so
/// a missing `gli.C` costs an optimistic attempt rather than skipping the attack.
pub fn choose_commander(roster: &[i64], available: &[i64]) -> Option<i64> {
    choose_free_commander(roster, available, &[])
}

/// Commander to use, walking [`USABLE_COMMANDER_LIDS`] and skipping any that are
/// already out on a march.
///
/// Two filters matter and both were learned the hard way:
///
/// * **Usable set.** Only LIDs from the curated list are considered, so the
///   unusable ones are never tried.
/// * **Not in flight.** Server status 256 is `lord_in_use`: the lord is assigned
///   to an active march. Reusing one costs a rejected request every time, and a
///   run that always picks the first entry can only ever have one march in the
///   air. With ~83 s travel each way there is room for far more.
pub fn choose_free_commander(roster: &[i64], available: &[i64], busy: &[i64]) -> Option<i64> {
    USABLE_COMMANDER_LIDS
        .iter()
        .copied()
        .filter(|lord_id| roster.is_empty() || roster.contains(lord_id))
        .filter(|lord_id| available.is_empty() || available.contains(lord_id))
        .find(|lord_id| !busy.contains(lord_id))
}

/// Lord that came back, from a return packet (`cat`), i.e. `A.UM.L.ID`.
pub fn returned_lord_id(cat_payload: &Value) -> Option<i64> {
    cat_payload.pointer("/A/UM/L/ID").and_then(Value::as_i64)
}

/// Whether a target's level falls in an inclusive `[min, max]` band.
/// An unknown level never matches: attacking blind risks the wrong profile.
pub fn accepts_level(level: Option<i64>, min: Option<i64>, max: Option<i64>) -> bool {
    let Some(level) = level else {
        return false;
    };
    if let Some(min) = min
        && level < min
    {
        return false;
    }
    if let Some(max) = max
        && level > max
    {
        return false;
    }
    true
}

/// Best target for an inclusive level band, nearest to `source` first.
pub fn pick_target<'a>(
    targets: &'a [MapTarget],
    kingdom_id: i64,
    min: Option<i64>,
    max: Option<i64>,
    source: (i64, i64),
    attempted: &[String],
) -> Option<&'a MapTarget> {
    let mut candidates: Vec<&MapTarget> = targets
        .iter()
        .filter(|target| target.kingdom_id == kingdom_id)
        .filter(|target| accepts_level(target.level, min, max))
        .filter(|target| !attempted.contains(&target.key()))
        .collect();
    candidates.sort_by_key(|target| {
        (target.x - source.0).abs() + (target.y - source.1).abs()
    });
    candidates.first().copied()
}

/// Commander LIDs named by an `adi` reply, if it carries a roster.
pub fn available_commanders(adi_reply: &Value) -> Vec<i64> {
    adi_reply
        .pointer("/gli/C")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("ID").and_then(Value::as_i64))
        .collect()
}

/// March id from a `cra` acknowledgement (`AAM.M`).
pub fn march_id_from_ack(cra_reply: &Value) -> Option<i64> {
    cra_reply
        .pointer("/AAM/M")
        .and_then(|march| march.get("MID"))
        .and_then(Value::as_i64)
}

/// Travel time in seconds from a `cra` acknowledgement, when present.
pub fn travel_seconds_from_ack(cra_reply: &Value) -> Option<i64> {
    cra_reply
        .pointer("/AAM/M")
        .and_then(|march| march.get("TT"))
        .and_then(Value::as_i64)
}

/// `[coins, rubies]` from a return packet (`cat`), when it carries loot.
pub fn loot_from_return(cat_payload: &Value) -> Option<(i64, i64)> {
    let rows = cat_payload
        .pointer("/A/G")
        .and_then(Value::as_array)?
        .clone();
    let mut coins = 0;
    let mut rubies = 0;
    for row in rows {
        let Some(pair) = row.as_array() else {
            continue;
        };
        let Some(currency) = pair.first().and_then(Value::as_str) else {
            continue;
        };
        let amount = pair.get(1).and_then(Value::as_i64).unwrap_or_default();
        match currency {
            "C1" => coins = amount,
            "C2" => rubies = amount,
            _ => {}
        }
    }
    Some((coins, rubies))
}

/// Result flag from a return packet (`cat`), i.e. `A.S`.
pub fn result_flag_from_return(cat_payload: &Value) -> Option<i64> {
    cat_payload.pointer("/A/S").and_then(Value::as_i64)
}

/// March id from a return packet (`cat`), i.e. `A.M.MID`.
pub fn returned_march_id(cat_payload: &Value) -> Option<i64> {
    cat_payload
        .pointer("/A/M/MID")
        .and_then(Value::as_i64)
}

/// Where a returning RBC march went, from a return packet: `(kingdom, x, y)`.
///
/// Reads `A.M.SA`, not `TA`. On the return leg the areas are swapped, so `SA` is
/// the attacked RBC and `TA` is the castle it set out from. The area type must be
/// the RBC type, which is what rejects a castle-to-castle movement.
///
/// The `cra` acknowledgement's `AAM.M.MID` does **not** match the `A.M.MID` the
/// return carries — observed live, every time. `bot/rbc_proxy_listener.py` says so
/// and attributes returns by position plus commander instead, which is what
/// [`crate::store::Store::finish_march_by_target`] implements.
pub fn return_target(cat_payload: &Value) -> Option<(i64, i64, i64)> {
    let row = cat_payload.pointer("/A/M/SA")?.as_array()?;
    if row.first()?.as_i64()? != RBC_AREA_TYPE {
        return None;
    }
    let kingdom_id = cat_payload
        .pointer("/A/M/KID")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    Some((
        kingdom_id,
        row.get(1)?.as_i64()?,
        row.get(2)?.as_i64()?,
    ))
}

/// Return-trip duration from a return packet (`cat`), i.e. `A.M.TT`.
pub fn return_seconds_from_return(cat_payload: &Value) -> Option<i64> {
    cat_payload.pointer("/A/M/TT").and_then(Value::as_i64)
}

/// How a task's attack is shaped, so the payload can be built without the task
/// carrying a live `Attack` (this module stays free of `empire-game`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackShape {
    /// One wave, left flank, units only — the level-61 crossbow attack.
    SingleFlank { unit_id: i64, count: i64 },
    /// Four waves, both outer flanks, units *and* tools on every wave.
    ///
    /// Deathly Horror and Kunai both put scaling ladders on all four waves, which
    /// is easy to get wrong: transcribing it by hand produced a payload that
    /// matched only one wave.
    FourWaveFlanks {
        unit_id: i64,
        count: i64,
        tool_id: i64,
        tool_count: i64,
    },
}

/// A task: a level band, its share of the commanders, and how it attacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDefinition {
    pub name: &'static str,
    pub kingdom_id: i64,
    /// Inclusive level band this task accepts.
    pub level_min: i64,
    pub level_max: i64,
    /// How many commanders this task owns, taken in declaration order.
    pub commander_count: usize,
    /// Lower wins when two bands overlap.
    pub priority: i64,
    pub attack: AttackShape,
}

/// A task with its commanders handed out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTask {
    pub definition: TaskDefinition,
    /// This task's commanders, and only this task's.
    pub commander_lids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("tasks need {needed} commanders but only {available} are usable")]
    NotEnoughCommanders { needed: usize, available: usize },
    #[error("a task must own at least one commander")]
    NoCommanders,
}

/// Allocate commanders across tasks **in declaration order**.
///
/// Task 1 takes the first `commander_count` usable LIDs, task 2 the next, and so
/// on — the Python's `allocate_task_definitions` walking consecutive human
/// numbers. A commander therefore belongs to exactly one task, so two tasks can
/// never mix their commanders: a kunai commander is never sent at a level-61
/// crossbow target, or the reverse.
pub fn allocate_tasks(definitions: &[TaskDefinition]) -> Result<Vec<PlannedTask>, PlanError> {
    let needed: usize = definitions
        .iter()
        .map(|definition| definition.commander_count)
        .sum();
    if definitions
        .iter()
        .any(|definition| definition.commander_count == 0)
    {
        return Err(PlanError::NoCommanders);
    }
    if needed > USABLE_COMMANDER_LIDS.len() {
        return Err(PlanError::NotEnoughCommanders {
            needed,
            available: USABLE_COMMANDER_LIDS.len(),
        });
    }

    let mut plan = Vec::with_capacity(definitions.len());
    let mut cursor = 0usize;
    for definition in definitions {
        let end = cursor + definition.commander_count;
        plan.push(PlannedTask {
            commander_lids: USABLE_COMMANDER_LIDS[cursor..end].to_vec(),
            definition: definition.clone(),
        });
        cursor = end;
    }
    Ok(plan)
}

/// The task whose band contains `level`, lowest priority first.
pub fn task_for_level(plan: &[PlannedTask], kingdom_id: i64, level: i64) -> Option<&PlannedTask> {
    plan.iter()
        .filter(|task| task.definition.kingdom_id == kingdom_id)
        .filter(|task| (task.definition.level_min..=task.definition.level_max).contains(&level))
        .min_by_key(|task| task.definition.priority)
}

/// A commander for this task, drawn from **its own allocation only**.
///
/// `offered` is the roster the server listed in the `adi` reply. A commander the
/// server did not offer, or that is already out, is skipped.
pub fn choose_task_commander(
    task: &PlannedTask,
    offered: &[i64],
    busy: &[i64],
) -> Option<i64> {
    task.commander_lids
        .iter()
        .copied()
        .filter(|lord_id| offered.is_empty() || offered.contains(lord_id))
        .find(|lord_id| !busy.contains(lord_id))
}

/// True when every one of this task's commanders is out on a march.
pub fn task_exhausted(task: &PlannedTask, busy: &[i64]) -> bool {
    task.commander_lids
        .iter()
        .all(|lord_id| busy.contains(lord_id))
}

/// Best target for a task: inside its band, nearest to `source`, unleased.
pub fn pick_target_for_task<'a>(
    targets: &'a [MapTarget],
    task: &PlannedTask,
    source: (i64, i64),
    skip: &[String],
) -> Option<&'a MapTarget> {
    let mut candidates: Vec<&MapTarget> = targets
        .iter()
        .filter(|target| target.kingdom_id == task.definition.kingdom_id)
        .filter(|target| {
            target.level.is_some_and(|level| {
                (task.definition.level_min..=task.definition.level_max).contains(&level)
            })
        })
        .filter(|target| !skip.contains(&target.key()))
        .collect();
    candidates.sort_by_key(|target| (target.x - source.0).abs() + (target.y - source.1).abs());
    candidates.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_codes_map_to_the_documented_band() {
        // The formula the Python bot used. 112 is the first raw code that
        // reaches level 61; 111 still reads as 60.
        assert_eq!(sands_level(112), 61);
        assert_eq!(sands_level(111), 60);
        assert!(sands_level(50) > 35 && sands_level(50) < 61);
        assert_eq!(sands_level(0), 35);
    }

    #[test]
    fn only_rbc_rows_are_learned() {
        let payload = json!({"KID": 1, "AI": [
            [2, 600, 610, -1, 112],
            [12, 593, 613, 16366514, 1],
            [1, 601, 611, -1, 112],
            [2, 590, 600, -1, 112]
        ]});
        let targets = rbc_targets_from_map(&payload);
        assert_eq!(targets.len(), 2);
        assert_eq!((targets[0].x, targets[0].y), (600, 610));
        assert_eq!(targets[0].level, Some(61));
    }

    #[test]
    fn tiles_are_spaced_one_span_apart_and_nearest_first() {
        let tiles = scan_tiles((593, 613), 1);
        assert_eq!(tiles.len(), 9);
        assert_eq!(tiles[0], (593, 613), "the centre tile comes first");
        // The four orthogonal neighbours sit one span out and come next; the
        // four corners follow at two spans.
        let distance = |(x, y): &(i64, i64)| (x - 593).abs() + (y - 613).abs();
        assert!(tiles[1..5].iter().all(|tile| distance(tile) == TILE_SPAN));
        assert!(tiles[5..].iter().all(|tile| distance(tile) == 2 * TILE_SPAN));
        assert!(tiles.contains(&(593 - 13, 613)));
        assert!(tiles.contains(&(593 + 13, 613 + 13)));
    }

    #[test]
    fn a_zero_radius_covers_only_the_centre_tile() {
        assert_eq!(scan_tiles((10, 20), 0), vec![(10, 20)]);
    }

    #[test]
    fn gaa_carries_a_full_thirteen_by_thirteen_window() {
        let packet = gaa_packet("EmpireEx_21", 1, (572, 598)).unwrap();
        assert!(packet.contains("%gaa%"));
        assert!(packet.contains("\"AX1\":572"));
        assert!(packet.contains("\"AX2\":584"));
        assert!(packet.contains("\"AY1\":598"));
        assert!(packet.contains("\"AY2\":610"));
    }

    #[test]
    fn adi_carries_source_target_and_kingdom() {
        let target = MapTarget {
            kingdom_id: 1,
            x: 600,
            y: 610,
            level: Some(61),
        };
        let packet = adi_packet("EmpireEx_21", (593, 613), &target).unwrap();
        assert!(packet.contains("%adi%"));
        assert!(packet.contains("\"SX\":593"));
        assert!(packet.contains("\"TX\":600"));
        assert!(packet.contains("\"KID\":1"));
    }

    #[test]
    fn the_attack_envelope_matches_the_captured_shape() {
        let target = MapTarget {
            kingdom_id: 1,
            x: 600,
            y: 610,
            level: Some(61),
        };
        let payload = json!([{"L": {"T": [[-1, 0], [-1, 0]], "U": [[607, 50], [-1, 0]]}}]);
        let packet = attack_packet("EmpireEx_21", (593, 613), &target, 17, &payload, DEFAULT_HBW, MAP_PTT)
            .unwrap();
        assert!(packet.contains("%cra%"));
        for field in [
            "\"LID\":17",
            "\"HBW\":1007",
            "\"PTT\":0",
            "\"CD\":99",
            "\"ASCT\":0",
            "\"AST\":[-1,-1,-1]",
        ] {
            assert!(packet.contains(field), "envelope is missing {field}");
        }
        // Eight reserved wave slots, padded.
        assert!(packet.matches("[-1,0]").count() >= 8);
        assert!(packet.contains("\"BKS\":[]"));
        assert!(packet.contains("\"A\":["));
    }

    #[test]
    fn commander_choice_prefers_the_roster_order() {
        let roster = [0, 2, 17, 22];
        assert_eq!(choose_commander(&roster, &[22, 17]), Some(17));
        assert_eq!(choose_commander(&roster, &[]), Some(0));
        assert_eq!(choose_commander(&roster, &[99]), None);
        assert_eq!(choose_commander(&[], &[1]), None);
    }

    // -- per-task allocation ------------------------------------------------

    fn crossbow_61(commanders: usize) -> TaskDefinition {
        TaskDefinition {
            name: "crossbow_61",
            kingdom_id: 1,
            level_min: 61,
            level_max: 61,
            commander_count: commanders,
            priority: 20,
            attack: AttackShape::SingleFlank {
                unit_id: 607,
                count: 50,
            },
        }
    }

    fn kunai_35_60(commanders: usize) -> TaskDefinition {
        TaskDefinition {
            name: "kunai_35_60",
            kingdom_id: 1,
            level_min: 35,
            level_max: 60,
            commander_count: commanders,
            priority: 10,
            attack: AttackShape::FourWaveFlanks {
                unit_id: 700,
                count: 30,
                tool_id: 614,
                tool_count: 5,
            },
        }
    }

    #[test]
    fn commanders_are_allocated_per_task_in_declaration_order() {
        let plan = allocate_tasks(&[crossbow_61(17), kunai_35_60(18)]).unwrap();
        assert_eq!(plan[0].commander_lids.len(), 17);
        assert_eq!(plan[1].commander_lids.len(), 18);
        // Task 1 takes the first seventeen usable LIDs, task 2 the rest.
        assert_eq!(plan[0].commander_lids[0], USABLE_COMMANDER_LIDS[0]);
        assert_eq!(plan[0].commander_lids[16], USABLE_COMMANDER_LIDS[16]);
        assert_eq!(plan[1].commander_lids[0], USABLE_COMMANDER_LIDS[17]);
        assert_eq!(
            plan[1].commander_lids[17],
            *USABLE_COMMANDER_LIDS.last().unwrap()
        );
        // 17 + 18 covers the whole pool exactly.
        assert_eq!(plan[0].commander_lids.len() + plan[1].commander_lids.len(), 35);
    }

    #[test]
    fn the_two_tasks_cannot_mix_commanders() {
        let plan = allocate_tasks(&[crossbow_61(17), kunai_35_60(18)]).unwrap();
        // Disjoint allocations, whichever order the server offers them in.
        for lord_id in &plan[0].commander_lids {
            assert!(!plan[1].commander_lids.contains(lord_id));
        }
        let offered = USABLE_COMMANDER_LIDS;
        let crossbow = choose_task_commander(&plan[0], offered, &[]).unwrap();
        let kunai = choose_task_commander(&plan[1], offered, &[]).unwrap();
        assert!(plan[0].commander_lids.contains(&crossbow));
        assert!(plan[1].commander_lids.contains(&kunai));
        assert_ne!(crossbow, kunai);
        // A task never reaches outside its own allocation, even when offered one.
        assert_eq!(choose_task_commander(&plan[0], &[25, 26, 27], &[]), None);
        assert_eq!(choose_task_commander(&plan[1], &[0, 2, 3], &[]), None);
    }

    #[test]
    fn a_busy_commander_is_passed_over_within_the_same_task() {
        let plan = allocate_tasks(&[crossbow_61(3), kunai_35_60(2)]).unwrap();
        let offered = USABLE_COMMANDER_LIDS;
        let first = plan[0].commander_lids[0];
        let second = plan[0].commander_lids[1];
        assert_eq!(choose_task_commander(&plan[0], offered, &[first]), Some(second));
        // Whole allocation out: this task has to wait, it cannot borrow.
        assert_eq!(
            choose_task_commander(&plan[0], offered, &plan[0].commander_lids),
            None
        );
        assert!(task_exhausted(&plan[0], &plan[0].commander_lids));
        assert!(!task_exhausted(&plan[0], &[first]));
    }

    #[test]
    fn levels_route_to_exactly_one_task() {
        let plan = allocate_tasks(&[crossbow_61(17), kunai_35_60(18)]).unwrap();
        assert_eq!(
            task_for_level(&plan, 1, 61).unwrap().definition.name,
            "crossbow_61"
        );
        for level in [35, 45, 60] {
            assert_eq!(
                task_for_level(&plan, 1, level).unwrap().definition.name,
                "kunai_35_60",
                "level {level} should be the kunai task"
            );
        }
        // Outside both bands, and the wrong kingdom, select nothing.
        assert!(task_for_level(&plan, 1, 34).is_none());
        assert!(task_for_level(&plan, 1, 62).is_none());
        assert!(task_for_level(&plan, 4, 61).is_none());
    }

    #[test]
    fn target_picking_respects_the_task_band() {
        let plan = allocate_tasks(&[crossbow_61(17), kunai_35_60(18)]).unwrap();
        let targets = vec![
            MapTarget { kingdom_id: 1, x: 600, y: 610, level: Some(61) },
            MapTarget { kingdom_id: 1, x: 601, y: 611, level: Some(45) },
            MapTarget { kingdom_id: 1, x: 602, y: 612, level: None },
        ];
        let crossbow = pick_target_for_task(&targets, &plan[0], (593, 613), &[]).unwrap();
        assert_eq!(crossbow.level, Some(61));
        let kunai = pick_target_for_task(&targets, &plan[1], (593, 613), &[]).unwrap();
        assert_eq!(kunai.level, Some(45));
        // An unknown level is never claimed by either task.
        let leased = vec!["1:600:610".to_owned(), "1:601:611".to_owned()];
        assert!(pick_target_for_task(&targets, &plan[0], (593, 613), &leased).is_none());
        assert!(pick_target_for_task(&targets, &plan[1], (593, 613), &leased).is_none());
    }

    #[test]
    fn allocation_refuses_to_overrun_the_pool() {
        assert_eq!(
            allocate_tasks(&[crossbow_61(17), kunai_35_60(18)]).unwrap().len(),
            2
        );
        assert_eq!(
            allocate_tasks(&[crossbow_61(20), kunai_35_60(20)]).unwrap_err(),
            PlanError::NotEnoughCommanders {
                needed: 40,
                available: 35
            }
        );
        assert_eq!(
            allocate_tasks(&[crossbow_61(0)]).unwrap_err(),
            PlanError::NoCommanders
        );
    }

    #[test]
    fn the_usable_pool_excludes_the_reserved_lids() {
        // These are present in the server roster but cannot be sent on a march.
        for unusable in [1, 4, 5, 12, 13, 14, 15, 19] {
            assert!(
                !USABLE_COMMANDER_LIDS.contains(&unusable),
                "lid {unusable} must not be in the usable pool"
            );
        }
        assert_eq!(USABLE_COMMANDER_LIDS.len(), 35);
        // `FIRST_13_COMMANDER_LIDS` is the first fifteen entries.
        assert_eq!(
            &USABLE_COMMANDER_LIDS[..15],
            &[0, 2, 3, 6, 7, 8, 9, 10, 11, 16, 17, 18, 20, 21, 22]
        );
    }

    #[test]
    fn a_roster_lid_outside_the_usable_pool_is_never_chosen() {
        // Even when the server offers it and the roster lists it.
        assert_eq!(choose_free_commander(&[1, 4, 5], &[1, 4, 5], &[]), None);
        assert_eq!(choose_free_commander(&[1, 2], &[1, 2], &[]), Some(2));
    }

    #[test]
    fn a_busy_commander_is_skipped_rather_than_reused() {
        let roster = USABLE_COMMANDER_LIDS;
        assert_eq!(choose_free_commander(roster, roster, &[0]), Some(2));
        assert_eq!(choose_free_commander(roster, roster, &[0, 2]), Some(3));
        assert_eq!(choose_free_commander(roster, roster, &[0, 2, 3]), Some(6));
        // All out: report none rather than provoking a rejection.
        assert_eq!(choose_free_commander(roster, roster, roster), None);
    }

    #[test]
    fn the_returning_lord_is_read_from_the_return_packet() {
        let back = json!({"A": {"S": 0, "UM": {"L": {"ID": 17}}}});
        assert_eq!(returned_lord_id(&back), Some(17));
        assert_eq!(returned_lord_id(&json!({"A": {}})), None);
    }

    #[test]
    fn a_return_carries_the_attacked_rbc_in_its_source_area() {
        // On the return leg SA is the RBC (area type 2) and TA is the castle.
        let back = json!({"A": {
            "S": 0,
            "M": {"MID": 101091817, "KID": 1, "TT": 61,
                  "SA": [2, 597, 613, -1, 112], "TA": [12, 593, 613, 16657787]},
            "UM": {"L": {"ID": 0}},
            "G": [["C1", 39612], ["C2", 14]]
        }});
        assert_eq!(return_target(&back), Some((1, 597, 613)));
        assert_eq!(return_seconds_from_return(&back), Some(61));
        assert_eq!(returned_lord_id(&back), Some(0));
        assert_eq!(loot_from_return(&back), Some((39612, 14)));

        // A castle-to-castle movement (both areas type 12) is not an RBC result.
        let castle_move = json!({"A": {"M": {"KID": 1,
            "SA": [12, 593, 613], "TA": [12, 600, 600]}}});
        assert_eq!(return_target(&castle_move), None);
        assert_eq!(return_target(&json!({"A": {"M": {}}})), None);
    }

    #[test]
    fn an_unknown_level_never_matches_a_band() {
        assert!(accepts_level(Some(61), Some(61), Some(61)));
        assert!(!accepts_level(Some(60), Some(61), Some(61)));
        assert!(!accepts_level(None, None, None));
        assert!(accepts_level(Some(45), Some(35), Some(60)));
    }

    #[test]
    fn target_picking_is_nearest_first_and_skips_attempts() {
        let targets = vec![
            MapTarget { kingdom_id: 1, x: 900, y: 900, level: Some(61) },
            MapTarget { kingdom_id: 1, x: 600, y: 610, level: Some(61) },
            MapTarget { kingdom_id: 1, x: 601, y: 611, level: Some(45) },
            MapTarget { kingdom_id: 0, x: 593, y: 613, level: Some(61) },
        ];
        let picked = pick_target(&targets, 1, Some(61), Some(61), (593, 613), &[]).unwrap();
        assert_eq!((picked.x, picked.y), (600, 610), "nearest level 61 wins");

        let attempted = vec!["1:600:610".to_owned()];
        let picked = pick_target(&targets, 1, Some(61), Some(61), (593, 613), &attempted).unwrap();
        assert_eq!((picked.x, picked.y), (900, 900));

        // A different kingdom's targets are never returned.
        assert!(pick_target(&targets, 2, Some(61), Some(61), (593, 613), &[]).is_none());
    }

    #[test]
    fn acknowledgements_and_results_are_read_from_the_right_places() {
        let ack = json!({"AAM": {"M": {"MID": 100838963, "TT": 774, "TA": [12, 557, 527]}}});
        assert_eq!(march_id_from_ack(&ack), Some(100_838_963));
        assert_eq!(travel_seconds_from_ack(&ack), Some(774));

        let back = json!({"A": {"S": 0, "M": {"MID": 100838963}, "G": [["C1", 120], ["C2", 3]]}});
        assert_eq!(returned_march_id(&back), Some(100_838_963));
        assert_eq!(result_flag_from_return(&back), Some(0));
        assert_eq!(loot_from_return(&back), Some((120, 3)));
    }

    #[test]
    fn a_return_with_no_loot_reports_zero_rather_than_failing() {
        let back = json!({"A": {"S": 3, "G": []}});
        assert_eq!(loot_from_return(&back), Some((0, 0)));
    }

    #[test]
    fn heartbeat_is_a_pin_frame_with_its_literal_token() {
        assert_eq!(
            heartbeat_packet("EmpireEx_21"),
            "%xt%EmpireEx_21%pin%1%<RoundHouseKick>%"
        );
    }
}
