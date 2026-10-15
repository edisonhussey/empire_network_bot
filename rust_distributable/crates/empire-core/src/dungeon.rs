//! Robber baron (dungeon) levels: the rule the game client uses, for every kingdom.
//!
//! The server never sends a level. A type-2 map row carries the **victory count**
//! (`[2, x, y, -1, count, cooldown_s, kid]`), and the client turns it into a level with
//! `DungeonConst.getLevel`. This is that rule, pure and kingdom-agnostic, so nothing else
//! in the crate needs to know a kingdom's constants. See
//! `docs/game_infrastructure/robber_baron_level_formula.md` for where it was read from,
//! the offsets, and the readings it was checked against.
//!
//! ```text
//! level(count, kid)  = floor(1.9 * |count| ^ 0.555) + offset(kid)
//! hits_to_next       = (first count with a higher level) - count
//! ```
//!
//! A successful attack adds one to the count, so after a landing the new level and the
//! hits still needed follow from the stored count with no further map read
//! ([`progress`] of `count + 1`).

/// `DungeonConst.FACTOR_VICTORY_LEVEL`.
pub const FACTOR: f64 = 1.9;
/// `DungeonConst.POWER_VICTORY_LEVEL`.
pub const POWER: f64 = 0.555;

/// `DungeonConst.getKingdomOffset`: where a kingdom's levels start. Kingdoms the client
/// has no offset for (Storm Islands, factions) start at 0.
pub fn kingdom_offset(kingdom_id: i64) -> i64 {
    match kingdom_id {
        0 => 1,  // Great Empire (green)
        2 => 20, // Everwinter
        1 => 35, // Burning Sands
        3 => 45, // Fire Peaks
        _ => 0,
    }
}

/// `DungeonConst.getLevel`.
pub fn level_for(victories: i64, kingdom_id: i64) -> i64 {
    let base = (FACTOR * (victories.unsigned_abs() as f64).powf(POWER)).floor() as i64;
    base + kingdom_offset(kingdom_id)
}

/// How many more victories until the level rises, `>= 1`.
///
/// Found by walking the count forward on [`level_for`] itself, so it can never disagree
/// with the level the same count shows. (The client's tooltip uses the algebraic inverse,
/// `getVictories(level) + 1 - count`; the two agree except where that inverse lands on an
/// exact integer, which no count in the first thousand does.)
pub fn hits_to_next(victories: i64, kingdom_id: i64) -> i64 {
    let level = level_for(victories, kingdom_id);
    let mut next = victories.saturating_add(1);
    // The curve is concave but every level is reached long before this bound.
    while level_for(next, kingdom_id) <= level && next - victories < 100_000 {
        next += 1;
    }
    next - victories
}

/// Where a tower stands: its count, level, and what the next level needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub victories: i64,
    pub level: i64,
    pub hits_to_next: i64,
}

pub fn progress(victories: i64, kingdom_id: i64) -> Progress {
    Progress {
        victories,
        level: level_for(victories, kingdom_id),
        hits_to_next: hits_to_next(victories, kingdom_id),
    }
}

impl Progress {
    /// The same tower after one more won attack.
    pub fn after_victory(self, kingdom_id: i64) -> Progress {
        progress(self.victories.saturating_add(1), kingdom_id)
    }
}

/// `DungeonConst.getVictories`: the count at which `level` is last reached, per the client.
/// Kept to cross-check [`hits_to_next`] against the tooltip's own arithmetic.
pub fn client_victories(level: i64, kingdom_id: i64) -> i64 {
    let base = (level - kingdom_offset(kingdom_id) + 1) as f64 / FACTOR;
    base.powf(1.0 / POWER).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Readings taken in game on 2026-10-14 (Great Empire), `(count, level, hits to go)`.
    const GREEN_READINGS: [(i64, i64, i64); 7] =
        [(0, 1, 1), (4, 5, 2), (7, 6, 1), (9, 7, 2), (11, 8, 3), (13, 8, 1), (14, 9, 3)];

    #[test]
    fn the_formula_reproduces_every_in_game_reading() {
        for (count, level, to_go) in GREEN_READINGS {
            let p = progress(count, 0);
            assert_eq!((p.level, p.hits_to_next), (level, to_go), "count {count}");
        }
        // Fire Peaks castle 698:581 at count 7, and 654:645 in green at count 5.
        assert_eq!(progress(7, 3), Progress { victories: 7, level: 50, hits_to_next: 1 });
        assert_eq!(progress(5, 0), Progress { victories: 5, level: 5, hits_to_next: 1 });
        // An untouched Everwinter tower starts at its offset.
        assert_eq!(progress(0, 2), Progress { victories: 0, level: 20, hits_to_next: 1 });
    }

    #[test]
    fn offsets_follow_the_client_and_default_to_zero() {
        assert_eq!(
            [0, 1, 2, 3, 4, 10, 99].map(kingdom_offset),
            [1, 35, 20, 45, 0, 0, 0]
        );
    }

    #[test]
    fn sands_keeps_its_documented_levels() {
        // The raw codes the Sands code path has always used (raw 112 is level 61).
        assert_eq!(level_for(112, 1), 61);
        assert_eq!(level_for(111, 1), 60);
        assert_eq!(level_for(0, 1), 35);
    }

    #[test]
    fn level_start_counts_match_the_documented_table() {
        let first_count = |level: i64| (0..2_000).find(|c| level_for(*c, 0) == level).unwrap();
        let starts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 20, 40, 60, 80].map(first_count);
        assert_eq!(starts, [0, 1, 2, 3, 4, 6, 8, 11, 14, 17, 20, 24, 64, 232, 489, 826]);
    }

    #[test]
    fn a_victory_walks_the_tower_up_exactly_as_the_server_would() {
        for kingdom in [0, 1, 2, 3] {
            let mut p = progress(0, kingdom);
            for count in 1..=600 {
                p = p.after_victory(kingdom);
                assert_eq!(p, progress(count, kingdom));
                assert!(p.hits_to_next >= 1);
            }
        }
    }

    #[test]
    fn hits_to_next_matches_the_clients_tooltip_arithmetic() {
        for kingdom in [0, 1, 2, 3, 4] {
            for count in 0..1_500 {
                let level = level_for(count, kingdom);
                let tooltip = client_victories(level, kingdom) + 1 - count;
                assert_eq!(hits_to_next(count, kingdom), tooltip, "kingdom {kingdom} count {count}");
            }
        }
    }

    #[test]
    fn the_level_never_goes_down_as_the_count_rises() {
        for kingdom in [0, 1, 2, 3] {
            let mut last = 0;
            for count in 0..5_000 {
                let level = level_for(count, kingdom);
                assert!(level >= last);
                last = level;
            }
        }
    }
}
