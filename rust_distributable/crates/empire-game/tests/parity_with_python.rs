//! Proves the Rust port matches the Python source of truth.
//!
//! `tests/fixtures/sands_attacks.json` is produced by `scripts/gen_game_data.py`
//! from the real Python serialiser (`bot/game_data/attack.py`), so a failure
//! here means the Rust side has drifted from the bot — not that the test is
//! brittle.

use empire_game::{
    Attack, KINGDOMS, SAND_KINGDOM_ID, Side, Slot, TOOLS, TROOPS, Wave, tool_by_name, troop_by_id,
    troop_by_name,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn troop(name: &str) -> i64 {
    troop_by_name(name)
        .unwrap_or_else(|| panic!("unknown troop {name}"))
        .id
}

fn tool(name: &str) -> i64 {
    tool_by_name(name)
        .unwrap_or_else(|| panic!("unknown tool {name}"))
        .id
}

fn slot(item_id: i64, amount: i64) -> Slot {
    Slot::new(item_id, amount).unwrap()
}

/// Five scaling ladders per thirty troops is the highest ratio the server
/// accepts. Seven was rejected, so this number is not a free parameter.
const LADDERS_PER_WAVE: i64 = 5;
const TROOPS_PER_FLANK: i64 = 30;

/// Left and right flanks carrying the same unit, with optional ladders.
fn flanked_wave(unit_id: i64, ladders: i64) -> Wave {
    let tools = if ladders > 0 {
        vec![slot(tool("scaling_ladder"), ladders)]
    } else {
        Vec::new()
    };
    let flank = || Side::new(tools.clone(), vec![slot(unit_id, TROOPS_PER_FLANK)]);
    Wave::new(Some(flank()), None, Some(flank()))
}

/// `sand/config.py` has two four-wave shapes. MEAD and the Relic shortbowmen
/// carry ladders on wave 1 only; Deathly Horror and Kunai carry them on every
/// wave. Getting this wrong is invisible until the payload is compared.
fn four_wave_attack(unit_id: i64, ladders_every_wave: bool) -> Attack {
    let wave = |is_first: bool| {
        let ladders = if is_first || ladders_every_wave {
            LADDERS_PER_WAVE
        } else {
            0
        };
        flanked_wave(unit_id, ladders)
    };
    Attack::new(
        wave(true),
        Some(wave(false)),
        Some(wave(false)),
        Some(wave(false)),
    )
}

/// The five attacks defined in `bot/event/sand/config.py`.
fn sands_attacks() -> Vec<(&'static str, Attack)> {
    vec![
        (
            "SANDS_LV61",
            Attack::single_wave(Wave::new(
                Some(Side::units_only(vec![slot(troop("crossbowman"), 50)])),
                None,
                None,
            )),
        ),
        (
            "SANDS_LV36_60_MEAD",
            four_wave_attack(troop("valkyrie_ranger_10"), false),
        ),
        (
            "SANDS_BELOW_61",
            four_wave_attack(troop("relic_shortbowman_0"), false),
        ),
        (
            "SANDS_35_61_DEATHLY_HORROR",
            four_wave_attack(troop("deathly_horror"), true),
        ),
        (
            "SANDS_35_61_KUNAI",
            four_wave_attack(troop("renegade_kunai_thrower"), true),
        ),
    ]
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/sands_attacks.json"))
        .expect("fixture is valid JSON")
}

// ---------------------------------------------------------------------------
// payload parity
// ---------------------------------------------------------------------------

#[test]
fn sands_payloads_are_identical_to_the_python_serialiser() {
    let fixture = fixture();
    let attacks = sands_attacks();
    assert_eq!(attacks.len(), 5, "expected five Sands attacks");

    for (name, attack) in attacks {
        let expected = &fixture["attacks"][name]["payload"];
        assert!(!expected.is_null(), "fixture has no payload for {name}");
        let actual = serde_json::to_value(attack.to_payload().unwrap()).unwrap();
        assert_eq!(actual, *expected, "payload mismatch for {name}");
    }
}

#[test]
fn attack_json_matches_the_python_compact_encoding() {
    let fixture = fixture();
    let (_, attack) = sands_attacks().remove(0);
    let expected = serde_json::to_string(&fixture["attacks"]["SANDS_LV61"]["payload"]).unwrap();
    assert_eq!(attack.to_json().unwrap(), expected);
}

// ---------------------------------------------------------------------------
// table integrity
// ---------------------------------------------------------------------------

#[test]
fn tables_match_the_python_counts() {
    assert_eq!(TROOPS.len(), 257, "troop count drifted from troops.py");
    assert_eq!(TOOLS.len(), 294, "tool count drifted from tools.py");
    assert_eq!(KINGDOMS.len(), 6, "kingdom count drifted from kingdom.py");
}

#[test]
fn kingdom_ids_match_the_python_mapping() {
    assert_eq!(SAND_KINGDOM_ID, 1);
    let sand = empire_game::kingdom_by_id(SAND_KINGDOM_ID).unwrap();
    assert_eq!(sand.name, "sand_kingdom");
}

#[test]
fn tables_are_sorted_so_binary_search_lookups_are_valid() {
    assert!(TROOPS.windows(2).all(|pair| pair[0].name < pair[1].name));
    assert!(TOOLS.windows(2).all(|pair| pair[0].name < pair[1].name));
}

#[test]
fn lookup_by_name_and_id_agree() {
    let crossbowman = troop_by_name("crossbowman").unwrap();
    assert_eq!(crossbowman.id, 607);
    assert_eq!(troop_by_id(crossbowman.id).unwrap().name, "crossbowman");
    assert!(troop_by_name("no_such_unit").is_none());
    assert!(troop_by_id(-1).is_none());
}

#[test]
fn scaling_ladder_keeps_its_wall_reduction_and_has_no_wave_limit() {
    let ladder = tool_by_name("scaling_ladder").unwrap();
    assert_eq!(ladder.attribute("wall_reduction"), Some(10));
    assert_eq!(ladder.tool_limit_per_wave(), None);
}

#[test]
fn at_least_one_tool_declares_a_per_wave_limit() {
    assert!(
        TOOLS
            .iter()
            .any(|tool| tool.tool_limit_per_wave().is_some()),
        "tool_limit_per_wave was present in 54 tools in the Python data"
    );
}

#[test]
fn every_troop_carries_a_positive_id_and_the_core_stats() {
    for troop in TROOPS {
        assert!(troop.id > 0, "{} has no id", troop.name);
        assert!(!troop.name.is_empty());
        assert!(troop.attack_power >= 0, "{} attack_power", troop.name);
        assert!(troop.melee_defence >= 0, "{} melee_defence", troop.name);
        assert!(troop.ranged_defence >= 0, "{} ranged_defence", troop.name);
    }
}
