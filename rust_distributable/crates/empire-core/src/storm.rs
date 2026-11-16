//! Storm Islands rules: which map entries are storm forts, their level, whether they are free, and where to
//! scan next. Pure (no I/O). Storage: `store/storm.rs`; the attack bot: `empire-daemon/src/direct/automation.rs`
//! (`target_kind == "storm"`); flow: `docs/architecture/storm-islands.md`; packets: `docs/network/gaa.md`.
//!
//! Storm forts are not like towers: they appear, are taken by anyone and disappear. So a fort is only
//! trusted while it was seen recently ([`FRESH_MS`]), the map is read again only when a task needs a
//! target and has none fresh, and a fort that vanished before the attack landed is not an error.
//!
//! Learned from the earlier Python storm bot (`bot/event/storm/`) and the 2026-11-16 capture (a level-60
//! fort at 551:542, `[25,551,542,4,-1,12,0,8,0]`, attacked from the account's storm castle).

use serde_json::Value;

use crate::pacing::Rng;

/// The Storm Islands kingdom.
pub const STORM_KINGDOM: i64 = 4;
/// Map entry type of a storm fort (`gaa.AI[i][0]`).
pub const FORT_AREA_TYPE: i64 = 25;
/// Owners that mean "nobody" (the game's own forts).
pub const SYSTEM_OWNERS: [i64; 2] = [-1, -403];
/// A fort is attacked only if it was on the map this recently: they come and go.
pub const FRESH_MS: i64 = 120_000;
/// Forts not seen for this long are forgotten.
pub const FORGET_MS: i64 = 4 * 3_600_000;
/// Map windows are 13 x 13 tiles.
pub const WINDOW: i64 = 13;
/// How far from the storm castle the scan reaches, in tiles.
pub const SCAN_RADIUS: i64 = 96;
/// Random wait after a scan window before the next step, in seconds.
pub const SCAN_WAIT_S: (f64, f64) = (5.5, 12.5);
/// Default level range of a Storm Islands task.
pub const DEFAULT_LEVELS: (i64, i64) = (60, 80);

/// The level a fort's code stands for (`AI[i][5]`). Codes 7..9 and 12..14 are two families of the same
/// levels; 10 and 11 are the lower forts.
pub fn level_from_code(code: i64) -> Option<i64> {
    match code {
        7 | 12 => Some(60),
        8 | 13 => Some(70),
        9 | 14 => Some(80),
        10 => Some(40),
        11 => Some(50),
        _ => None,
    }
}

/// One storm fort as a map window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StormFort {
    pub x: i64,
    pub y: i64,
    pub level: Option<i64>,
    /// Both cooldown fields are 0: nobody has it on cooldown.
    pub free: bool,
}

/// The storm forts in a `gaa` reply for the Storm Islands, or `None` for another kingdom.
pub fn forts_in_gaa(payload: &Value) -> Option<Vec<StormFort>> {
    if payload.get("KID").and_then(Value::as_i64) != Some(STORM_KINGDOM) {
        return None;
    }
    let rows = payload.get("AI").and_then(Value::as_array).cloned().unwrap_or_default();
    Some(rows.iter().filter_map(fort_from_entry).collect())
}

/// A single map entry (also the `gaa.AI` that an `adi` reply carries for its target).
pub fn fort_from_entry(entry: &Value) -> Option<StormFort> {
    let row = entry.as_array()?;
    let int = |index: usize| row.get(index).and_then(Value::as_i64);
    if int(0)? != FORT_AREA_TYPE || !SYSTEM_OWNERS.contains(&int(4)?) {
        return None;
    }
    Some(StormFort {
        x: int(1)?,
        y: int(2)?,
        level: int(5).and_then(level_from_code),
        free: int(6) == Some(0) && int(8).unwrap_or(0) == 0,
    })
}

/// The 13 x 13 windows within [`SCAN_RADIUS`] of the storm castle, nearest ring first and shuffled inside a
/// ring, so a pass reads close ground first without always reading it in the same order.
pub fn scan_windows(castle: (i64, i64), rng: &mut Rng) -> Vec<(i64, i64)> {
    let origin = |value: i64| value.div_euclid(WINDOW) * WINDOW;
    let (cx, cy) = (origin(castle.0), origin(castle.1));
    let reach = SCAN_RADIUS / WINDOW;
    let mut windows = Vec::new();
    for dx in -reach..=reach {
        for dy in -reach..=reach {
            let ring = dx.abs().max(dy.abs());
            let (x, y) = (cx + dx * WINDOW, cy + dy * WINDOW);
            if x < 0 || y < 0 || (dx * dx + dy * dy) * WINDOW * WINDOW > (SCAN_RADIUS + WINDOW / 2).pow(2) {
                continue;
            }
            windows.push((ring, rng.next_u64(), (x, y)));
        }
    }
    windows.sort();
    windows.into_iter().map(|(_, _, window)| window).collect()
}

/// Seconds to wait after a scan window.
pub fn scan_wait_ms(rng: &mut Rng) -> i64 {
    (rng.uniform(SCAN_WAIT_S.0, SCAN_WAIT_S.1) * 1_000.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_captured_fort_is_a_free_level_60_and_other_entries_are_not_forts() {
        let payload = json!({"KID": 4, "AI": [
            [25, 551, 542, 4, -1, 12, 0, 8, 0],
            [25, 546, 527, 4, -1, 10, 34300, 0, 34300],
            [12, 557, 531, 4960, 16862926, 1, 3, 3, 3, 0, "Castle Ventrilo"],
            [24, 547, 521, 4940, -403, 4, "", -1, 4, 0],
            [31, 560, 531]
        ]});
        let forts = forts_in_gaa(&payload).unwrap();
        assert_eq!(forts, vec![
            StormFort { x: 551, y: 542, level: Some(60), free: true },
            StormFort { x: 546, y: 527, level: Some(40), free: false },
        ]);
        assert!(forts_in_gaa(&json!({"KID": 1, "AI": [[25, 1, 1, 4, -1, 12, 0, 8, 0]]})).is_none());
    }

    #[test]
    fn a_fort_owned_by_a_player_is_not_a_target() {
        assert!(fort_from_entry(&json!([25, 1, 1, 4, 16862926, 12, 0, 8, 0])).is_none());
    }

    #[test]
    fn levels_follow_the_codes() {
        assert_eq!((7..=14).map(level_from_code).collect::<Vec<_>>(), vec![Some(60), Some(70), Some(80), Some(40), Some(50), Some(60), Some(70), Some(80)]);
        assert_eq!(level_from_code(3), None);
    }

    #[test]
    fn scans_start_at_the_castle_and_stay_in_reach() {
        let mut rng = Rng::seeded(3);
        let windows = scan_windows((557, 531), &mut rng);
        assert_eq!(windows[0], (546, 520), "the castle's own window first");
        assert!(windows.iter().all(|(x, y)| (x - 546).abs() <= SCAN_RADIUS + WINDOW && (y - 520).abs() <= SCAN_RADIUS + WINDOW));
        assert!(windows.len() > 100 && windows.len() < 250, "{}", windows.len());
        let wait = scan_wait_ms(&mut rng);
        assert!((5_500..12_500).contains(&wait));
    }
}
