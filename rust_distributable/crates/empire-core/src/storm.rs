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
/// Side of one storm scan window, in tiles. The server answers windows up to about 107 tiles across (the
/// fortress sweep measured 101 honoured and 138 refused, `fortress.rs`), so one read covers 100 x 100.
pub const WINDOW: i64 = 100;
/// The fallback sides, in order, if the server ever refuses a window (an empty payload).
pub const FALLBACK_WINDOWS: [i64; 2] = [50, 13];
/// Rings of windows around the castle's own window: 1 ring = 3 x 3 windows = 300 x 300 tiles.
pub const SCAN_RINGS: i64 = 1;
/// A window read this recently is not read again while another one is older.
pub const RESCAN_AFTER_MS: i64 = 60_000;
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

/// The `size` x `size` windows (top-left corners) around the storm castle: the castle's own window, centred on
/// it, then [`SCAN_RINGS`] rings around it, nearest ring first and shuffled inside a ring. With a smaller
/// fallback size the rings grow so the same ground is covered.
pub fn scan_windows(castle: (i64, i64), size: i64, rng: &mut Rng) -> Vec<(i64, i64)> {
    let (cx, cy) = (castle.0 - size / 2, castle.1 - size / 2);
    // Reach the same ground as the full-size rings: half a window past them on each side.
    let span = SCAN_RINGS * WINDOW + WINDOW / 2 - size / 2;
    let reach = (span + size - 1) / size;
    let mut windows = Vec::new();
    for dx in -reach..=reach {
        for dy in -reach..=reach {
            let (x, y) = (cx + dx * size, cy + dy * size);
            if x + size <= 0 || y + size <= 0 {
                continue;
            }
            windows.push((dx.abs().max(dy.abs()), rng.next_u64(), (x.max(0), y.max(0))));
        }
    }
    windows.sort();
    windows.dedup_by_key(|(_, _, window)| *window);
    windows.into_iter().map(|(_, _, window)| window).collect()
}

/// The window to read next: the one read longest ago (never read first), the nearer ring on a tie. `None`
/// when every window was read within [`RESCAN_AFTER_MS`] (nothing new to learn yet).
pub fn next_window(windows: &[(i64, i64)], last_read: impl Fn((i64, i64)) -> Option<i64>, now_ms: i64) -> Option<(i64, i64)> {
    windows
        .iter()
        .enumerate()
        .map(|(order, window)| (last_read(*window).unwrap_or(i64::MIN), order, *window))
        .filter(|(read, _, _)| *read == i64::MIN || now_ms - *read >= RESCAN_AFTER_MS)
        .min()
        .map(|(_, _, window)| window)
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
    fn a_pass_is_nine_100_tile_windows_with_the_castle_centred_in_the_first() {
        let mut rng = Rng::seeded(3);
        let windows = scan_windows((557, 531), WINDOW, &mut rng);
        assert_eq!(windows.len(), 9);
        assert_eq!(windows[0], (507, 481), "the castle's own window first, castle in its centre");
        assert!(windows.contains(&(407, 381)) && windows.contains(&(607, 581)));
        // A smaller fallback covers the same 300 x 300 ground.
        let small = scan_windows((557, 531), 50, &mut rng);
        assert_eq!(small.len(), 49);
        let wait = scan_wait_ms(&mut rng);
        assert!((5_500..12_500).contains(&wait));
    }

    #[test]
    fn the_window_read_longest_ago_is_next_and_recent_ones_wait() {
        let windows = [(0, 0), (100, 0), (200, 0)];
        let read = |window: (i64, i64)| match window { (0, 0) => Some(1_000), (100, 0) => Some(500), _ => None };
        assert_eq!(next_window(&windows, read, 2_000), Some((200, 0)), "never read first");
        let read = |window: (i64, i64)| match window { (0, 0) => Some(1_000), _ => Some(500) };
        assert_eq!(next_window(&windows, read, 70_000), Some((100, 0)), "then the oldest");
        assert_eq!(next_window(&windows, read, 1_100), None, "all read within a minute: nothing to learn yet");
    }
}
