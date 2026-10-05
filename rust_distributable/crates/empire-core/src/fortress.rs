//! Where fortresses actually are.
//!
//! Fortresses are not scattered over the map: they sit on a fixed lattice. A
//! walk that guesses a spacing therefore finds nothing at all, however long it
//! runs, because a `gaa` window is only 13 areas wide and a probe has to land
//! *on* a slot to see what occupies it.
//!
//! Measured from every captured type-11 row — 19 distinct positions across
//! Sands, Ice and Fire, with no exceptions — a fortress only ever appears at:
//!
//! ```text
//! x ≡ y (mod 39)   and   x mod 39 ∈ {9, 29}
//! ```
//!
//! That is two interleaved 39-unit grids offset by `(20, 20)`. So neighbouring
//! fortresses are 20 and then 19 apart along the shared diagonal, and the same
//! 39 apart along a row or column. The previous implementation stepped a flat
//! ±17, which lands between slots and can never hit anything.
//!
//! What this buys the runner: the slots worth visiting are enumerable. The
//! runner can cover the observed finite map exactly and resume that walk from
//! durable completion markers.

/// Distance between two slots on the same grid.
pub const LATTICE_STEP: i64 = 39;

/// The two grid offsets a fortress slot can sit on.
pub const LATTICE_OFFSETS: [i64; 2] = [9, 29];

/// Last usable coordinate in the permanent outer-kingdom map.
///
/// Live boundary probes on US1 returned normal terrain through windows rooted
/// at 1280, partial terrain at x=1286, and no terrain at y=1286 or at 1290 on
/// either axis. The last lattice coordinate inside that observed map is 1277,
/// so 1285 is a conservative inclusive bound which contains every candidate
/// without generating requests wholly outside the world.
pub const OUTER_MAP_MAX_COORD: i64 = 1_285;

/// Is this coordinate a fortress slot?
pub fn is_slot(x: i64, y: i64) -> bool {
    let (x, y) = (x.rem_euclid(LATTICE_STEP), y.rem_euclid(LATTICE_STEP));
    x == y && LATTICE_OFFSETS.contains(&x)
}

/// Smallest value `>= from` congruent to `offset` modulo the step.
fn align_up(from: i64, offset: i64) -> i64 {
    let candidate = offset + LATTICE_STEP * (from - offset).div_euclid(LATTICE_STEP);
    if candidate < from {
        candidate + LATTICE_STEP
    } else {
        candidate
    }
}

/// Every slot inside the inclusive rectangle, in a stable order.
pub fn slots_in(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> Vec<(i64, i64)> {
    let mut slots = Vec::new();
    if min_x > max_x || min_y > max_y {
        return slots;
    }
    for offset in LATTICE_OFFSETS {
        let mut x = align_up(min_x, offset);
        while x <= max_x {
            let mut y = align_up(min_y, offset);
            while y <= max_y {
                slots.push((x, y));
                y += LATTICE_STEP;
            }
            x += LATTICE_STEP;
        }
    }
    slots
}

/// Every possible fortress coordinate in a permanent outer kingdom.
///
/// This is deliberately the complete map rectangle. Fortress bands can have
/// empty gaps wider than one lattice step, so stopping after an empty ring can
/// never prove that the map has been exhausted.
pub fn all_outer_kingdom_slots() -> Vec<(i64, i64)> {
    slots_in(0, 0, OUTER_MAP_MAX_COORD, OUTER_MAP_MAX_COORD)
}

/// Distance between two blocks on one grid: two lattice steps.
pub const BLOCK_STEP: i64 = LATTICE_STEP * 2;

/// Slack added around a block so none of its slots sits on a window edge.
///
/// A 13×13 tile is what the game client asks for, but that is a client choice,
/// not a server limit — the same request is accepted with a larger span. One
/// unit is enough to keep all four slots strictly inside, and the window should
/// stay as small as that allows.
pub const GAA_PAD: i64 = 1;

/// A rectangle in map coordinates. Both edges are inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

impl Bounds {
    /// The whole observed outer-kingdom map.
    pub const fn outer_kingdom() -> Self {
        Self {
            left: 0,
            top: 0,
            right: OUTER_MAP_MAX_COORD,
            bottom: OUTER_MAP_MAX_COORD,
        }
    }

    /// Columns a block sweep visits, zero when the bounds are empty.
    const fn columns(self) -> i64 {
        if self.left > self.right {
            0
        } else {
            (self.right - self.left) / BLOCK_STEP + 1
        }
    }

    /// Blocks a sweep of these bounds visits.
    pub const fn block_count(self) -> i64 {
        let columns = self.columns();
        if columns == 0 || self.top > self.bottom {
            0
        } else {
            columns * ((self.bottom - self.top) / BLOCK_STEP + 1)
        }
    }
}

/// The `gaa` request rectangle for one block. Both edges are inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaaWindow {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

/// One 2×2 block of lattice slots: the unit a single `gaa` request answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FortressBlock {
    /// Low slot of the block on its own grid.
    pub x: i64,
    pub y: i64,
}

impl FortressBlock {
    /// The four slots this block answers.
    pub const fn slots(self) -> [(i64, i64); 4] {
        [
            (self.x, self.y),
            (self.x + LATTICE_STEP, self.y),
            (self.x, self.y + LATTICE_STEP),
            (self.x + LATTICE_STEP, self.y + LATTICE_STEP),
        ]
    }

    /// Smallest practical `gaa` rectangle containing all four slots.
    pub const fn window(self) -> GaaWindow {
        GaaWindow {
            left: self.x - GAA_PAD,
            top: self.y - GAA_PAD,
            right: self.x + LATTICE_STEP + GAA_PAD,
            bottom: self.y + LATTICE_STEP + GAA_PAD,
        }
    }
}

/// A position in one grid's block sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FortressScanCursor {
    pub x: i64,
    pub y: i64,
}

impl FortressScanCursor {
    /// A cursor on the first block of an aligned rectangle.
    pub const fn first(bounds: Bounds) -> Self {
        Self {
            x: bounds.left,
            y: bounds.top,
        }
    }

    /// The block the cursor is on, without moving it.
    ///
    /// Reading and advancing are separate on purpose. The cursor only moves
    /// once a response has been seen, so a request that times out costs one
    /// repeat instead of silently skipping four slots.
    pub const fn block(self, bounds: Bounds) -> Option<FortressBlock> {
        if self.y > bounds.bottom || self.x > bounds.right {
            return None;
        }
        Some(FortressBlock {
            x: self.x,
            y: self.y,
        })
    }

    /// Move to the next block, wrapping to the first column of the next row.
    pub fn advance(&mut self, bounds: Bounds) {
        self.x += BLOCK_STEP;
        if self.x > bounds.right {
            self.x = bounds.left;
            self.y += BLOCK_STEP;
        }
    }
}

/// Snap a sweep rectangle so its first column and row land on the lattice.
///
/// Only the low edges move. The right and bottom edges are limits, and a sweep
/// that starts on the lattice lands on it for every later block anyway.
pub fn align_bounds(bounds: Bounds, offset: i64) -> Bounds {
    Bounds {
        left: align_up(bounds.left, offset),
        top: align_up(bounds.top, offset),
        right: bounds.right,
        bottom: bounds.bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fortress position seen in the captures. This is the evidence the
    /// whole discovery design rests on, so it is asserted rather than described.
    const CAPTURED: &[(i64, i64)] = &[
        // Burning Sands
        (575, 614),
        (594, 594),
        (594, 633),
        (614, 614),
        (614, 653),
        (633, 633),
        (692, 536),
        (711, 516),
        (711, 555),
        (711, 633),
        (731, 536),
        (750, 516),
        (750, 555),
        // Fire Peaks
        (692, 575),
        (672, 594),
        (711, 594),
        (731, 575),
        // Everwinter Ice
        (614, 653),
        (594, 633),
    ];

    #[test]
    fn every_captured_fortress_sits_on_the_lattice() {
        for (x, y) in CAPTURED {
            assert!(is_slot(*x, *y), "{x}:{y} is not a fortress slot");
        }
    }

    #[test]
    fn a_flat_step_lands_between_slots() {
        // Why the old ±17 seeding could never work: from a real fortress, a
        // 17-unit step is not on the lattice at all, so the probe window sees
        // empty ground no matter how many times it is repeated.
        let (x, y) = CAPTURED[0];
        assert!(!is_slot(x + 17, y));
        assert!(!is_slot(x, y + 17));
        // The real neighbours are one lattice step away on the same grid.
        assert!(is_slot(x + LATTICE_STEP, y));
        assert!(is_slot(x, y + LATTICE_STEP));
        // The other grid interleaves one shift away. Which direction depends on
        // which grid the fortress is on: 9 + 20 = 29, but 29 - 20 = 9.
        assert!(
            is_slot(594 + 20, 594 + 20),
            "a 9-slot reaches the other grid at +20"
        );
        assert!(is_slot(575 - 20, 614 - 20), "a 29-slot reaches it at -20");
        assert!(
            !is_slot(575 + 20, 614 + 20),
            "the other direction is not a slot"
        );
    }

    #[test]
    fn slots_in_enumerates_both_grids_inside_the_rectangle() {
        // A 39-unit window around one fortress holds its two immediate
        // neighbours on its own grid and its 20/20 cousin on the other.
        assert_eq!(
            slots_in(575, 594, 614, 633),
            vec![(594, 594), (594, 633), (575, 614), (614, 614)]
        );
    }

    #[test]
    fn an_empty_rectangle_yields_no_slots() {
        assert!(slots_in(10, 10, 12, 12).is_empty());
        assert!(slots_in(5, 5, 4, 9).is_empty());
    }

    #[test]
    fn a_rectangle_never_asks_for_negative_coordinates_as_slots() {
        let slots = slots_in(0, 0, 60, 60);
        assert!(slots.iter().all(|(x, y)| *x >= 0 && *y >= 0));
        assert_eq!(slots, vec![(9, 9), (9, 48), (48, 9), (48, 48), (29, 29)]);
    }

    #[test]
    fn a_complete_outer_kingdom_covers_the_observed_map_rectangle() {
        let slots = all_outer_kingdom_slots();
        assert_eq!(slots.len(), 2_178);
        assert!(slots.contains(&(9, 9)));
        assert!(slots.contains(&(1_277, 1_277)));
        assert!(slots.iter().all(|(x, y)| {
            (0..=OUTER_MAP_MAX_COORD).contains(x)
                && (0..=OUTER_MAP_MAX_COORD).contains(y)
                && is_slot(*x, *y)
        }));
    }

    /// Four slots per request, which is the whole point of the block.
    #[test]
    fn one_request_answers_a_whole_block_of_four_slots() {
        let block = FortressBlock { x: 29, y: 29 };
        let window = block.window();
        assert_eq!((window.left, window.top), (28, 28));
        assert_eq!((window.right, window.bottom), (69, 69));
        for slot in block.slots() {
            assert!(is_slot(slot.0, slot.1), "{slot:?} is not a slot");
            // Strictly inside, never on an edge the server might exclude.
            assert!(window.left < slot.0 && slot.0 < window.right, "{slot:?}");
            assert!(window.top < slot.1 && slot.1 < window.bottom, "{slot:?}");
        }
    }

    #[test]
    fn aligning_a_sweep_lands_its_origin_on_each_grid() {
        let bounds = Bounds::outer_kingdom();
        assert_eq!((bounds.left, bounds.top), (0, 0));
        for offset in LATTICE_OFFSETS {
            let aligned = align_bounds(bounds, offset);
            assert_eq!(aligned.left % LATTICE_STEP, offset);
            assert_eq!(aligned.top % LATTICE_STEP, offset);
            assert!(is_slot(aligned.left, aligned.top));
            // The limits are not moved.
            assert_eq!((aligned.right, aligned.bottom), (1_285, 1_285));
            assert_eq!(aligned.block_count(), 289);
        }
    }

    #[test]
    fn a_cursor_walks_one_grid_in_rows_and_wraps() {
        let bounds = align_bounds(Bounds::outer_kingdom(), 29);
        let mut cursor = FortressScanCursor::first(bounds);
        let mut seen = Vec::new();
        while let Some(block) = cursor.block(bounds) {
            seen.push((block.x, block.y));
            cursor.advance(bounds);
        }
        assert_eq!(seen.len(), 289);
        assert_eq!(seen[0], (29, 29));
        // Seventeen 78-unit columns, so the eighteenth block starts row two.
        assert_eq!(seen[17], (29, 107));
        assert_eq!(*seen.last().unwrap(), (1_277, 1_277));

        // Reading does not move the cursor: a lost response retries the same
        // block rather than skipping it.
        let cursor = FortressScanCursor::first(bounds);
        assert_eq!(cursor.block(bounds), Some(FortressBlock { x: 29, y: 29 }));
        assert_eq!(cursor.block(bounds), Some(FortressBlock { x: 29, y: 29 }));
    }

    #[test]
    fn a_cursor_reports_done_once_it_passes_the_bottom_edge() {
        let bounds = Bounds {
            left: 9,
            top: 9,
            right: 9,
            bottom: 9,
        };
        let mut cursor = FortressScanCursor::first(bounds);
        assert!(cursor.block(bounds).is_some());
        cursor.advance(bounds);
        assert_eq!(cursor.block(bounds), None);
    }

    /// The two sweeps are the request count for a kingdom: 578, not 2,178.
    #[test]
    fn the_two_sweeps_are_578_requests_for_a_kingdom() {
        let total: i64 = LATTICE_OFFSETS
            .iter()
            .map(|offset| align_bounds(Bounds::outer_kingdom(), *offset).block_count())
            .sum();
        assert_eq!(total, 578);
    }

    /// Every slot has to be answered by some block, or the walk would leave
    /// fortresses behind even though it swept the whole rectangle.
    #[test]
    fn the_sweeps_cover_every_slot_in_the_kingdom() {
        let mut covered = std::collections::HashSet::new();
        for offset in LATTICE_OFFSETS {
            let bounds = align_bounds(Bounds::outer_kingdom(), offset);
            let mut cursor = FortressScanCursor::first(bounds);
            while let Some(block) = cursor.block(bounds) {
                for slot in block.slots() {
                    covered.insert(slot);
                }
                cursor.advance(bounds);
            }
        }
        for slot in all_outer_kingdom_slots() {
            assert!(covered.contains(&slot), "{slot:?} is never probed");
        }
    }
}
