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
//! That is one lattice with two residue families interleaved: a slot's family is
//! decided by its residue, the two are 20 apart, and walking a diagonal they
//! alternate 20 then 19. (Naming this "two grids" is a trap — it invites two
//! sweeps, and the second one re-probes exactly what the first already answered.)
//!
//! What this buys the runner: the slots worth visiting are enumerable. A single
//! window can span *both* families, so one cursor per kingdom covers the
//! observed finite map exactly once and resumes from durable completion markers.

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

/// Slots of each residue family one request answers along an axis.
///
/// Three is the widest setting the server honours. Four was measured and is
/// **refused** — the reply comes back with an empty payload, the same tell as a
/// rejected `adi` — so this is the ceiling, not a preference. At three the window
/// spans three slots on each family, so one request answers 18 slots and a
/// kingdom is covered in 121 requests instead of 289.
///
/// Measured ladder at 2026-10-05: 23 and 62 and 101 cells honoured, 138 refused.
pub const SLOTS_PER_FAMILY_PER_AXIS: i64 = 3;

/// Distance between the two residue families: `9 + 20 = 29`.
const FAMILY_GAP: i64 = LATTICE_OFFSETS[1] - LATTICE_OFFSETS[0];

/// Distance between neighbouring blocks of a sweep.
pub const BLOCK_STEP: i64 = LATTICE_STEP * SLOTS_PER_FAMILY_PER_AXIS;

/// Distance from a block's origin to the furthest slot it answers.
pub const BLOCK_SPAN: i64 = LATTICE_STEP * (SLOTS_PER_FAMILY_PER_AXIS - 1) + FAMILY_GAP;

/// Alignment the combined sweep is anchored on.
///
/// The window reaches far enough to answer the other family as well, so a
/// kingdom needs one cursor, not one per family.
pub const SWEEP_ALIGNMENT: i64 = LATTICE_OFFSETS[0];

/// The four directions the discovery arms walk, as block steps.
pub const ARM_STEPS: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Empty windows in a row that end an arm.
///
/// One is not enough: a band of fortresses can have a gap, and stopping at the
/// first empty window would end the walk early and silently miss everything
/// beyond it. Two costs four extra requests in total and rules that out.
pub const ARM_EMPTY_LIMIT: u8 = 2;

/// A hard ceiling on one arm, so a nonsense reply cannot walk forever.
///
/// Far larger than any observed kingdom: the widest measured band spans eight
/// blocks, so 32 blocks is ~4x margin and still bounded.
pub const ARM_MAX_STEPS: u8 = 32;

/// The block origin whose window covers this coordinate.
pub fn block_origin(point: (i64, i64)) -> (i64, i64) {
    (
        point.0 - (point.0 - SWEEP_ALIGNMENT).rem_euclid(BLOCK_STEP),
        point.1 - (point.1 - SWEEP_ALIGNMENT).rem_euclid(BLOCK_STEP),
    )
}

/// The rectangle spanned by a set of block origins, inclusive.
///
/// Built from origins rather than coordinates so it always lands on the lattice
/// and always contains whole blocks — a partially covered block would leave
/// slots unprobed at the edge.
pub fn bounds_covering(origins: &[(i64, i64)]) -> Bounds {
    let left = origins.iter().map(|o| o.0).min().unwrap_or(SWEEP_ALIGNMENT);
    let top = origins.iter().map(|o| o.1).min().unwrap_or(SWEEP_ALIGNMENT);
    let right = origins.iter().map(|o| o.0).max().unwrap_or(SWEEP_ALIGNMENT);
    let bottom = origins.iter().map(|o| o.1).max().unwrap_or(SWEEP_ALIGNMENT);
    Bounds {
        left,
        top,
        right: right + BLOCK_SPAN,
        bottom: bottom + BLOCK_SPAN,
    }
}

/// Slack added around a block so no slot sits exactly on a window edge.
///
/// A 13×13 tile is what the game client asks for, but that is a client choice,
/// not a server limit — the same request is accepted with a larger span. Bounds
/// are inclusive, so zero would be safe; one is kept as cheap insurance and
/// costs two cells of width.
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

/// One block of lattice slots: the unit a single `gaa` request answers.
///
/// The origin sits on the lower residue family. The window spans far enough to
/// reach the other one, so a block answers slots from each family rather than
/// the four corners of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FortressBlock {
    /// Low slot of the block on the lower residue family.
    pub x: i64,
    pub y: i64,
}

impl FortressBlock {
    /// Every slot this block answers.
    ///
    /// Derived from the window rather than listed by hand, so it cannot drift
    /// from [`slots_in`] — the same definition of what a slot is.
    pub fn slots(self) -> Vec<(i64, i64)> {
        let window = self.window();
        slots_in(window.left, window.top, window.right, window.bottom)
    }

    /// The `gaa` rectangle containing all of them.
    pub const fn window(self) -> GaaWindow {
        GaaWindow {
            left: self.x - GAA_PAD,
            top: self.y - GAA_PAD,
            right: self.x + BLOCK_SPAN + GAA_PAD,
            bottom: self.y + BLOCK_SPAN + GAA_PAD,
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

    /// Eighteen slots per request, drawn evenly from both families, which is the
    /// point of the window being wide enough to span them.
    #[test]
    fn one_request_answers_eighteen_slots_across_both_families() {
        let block = FortressBlock { x: 9, y: 9 };
        let window = block.window();
        assert_eq!((window.left, window.top), (8, 8));
        assert_eq!((window.right, window.bottom), (108, 108));
        let slots = block.slots();
        assert_eq!(slots.len(), 18, "{slots:?}");
        let lower = slots.iter().filter(|(x, _)| x % LATTICE_STEP == 9).count();
        let upper = slots.iter().filter(|(x, _)| x % LATTICE_STEP == 29).count();
        assert_eq!((lower, upper), (9, 9), "nine from each family: {slots:?}");
        for slot in slots {
            assert!(is_slot(slot.0, slot.1), "{slot:?} is not a slot");
            // Strictly inside, never on an edge the server might exclude.
            assert!(window.left < slot.0 && slot.0 < window.right, "{slot:?}");
            assert!(window.top < slot.1 && slot.1 < window.bottom, "{slot:?}");
        }
    }

    #[test]
    fn aligning_a_sweep_lands_its_origin_on_the_lattice() {
        let bounds = Bounds::outer_kingdom();
        assert_eq!((bounds.left, bounds.top), (0, 0));
        let aligned = align_bounds(bounds, SWEEP_ALIGNMENT);
        assert_eq!(aligned.left % LATTICE_STEP, SWEEP_ALIGNMENT);
        assert_eq!(aligned.top % LATTICE_STEP, SWEEP_ALIGNMENT);
        assert!(is_slot(aligned.left, aligned.top));
        // The limits are not moved.
        assert_eq!((aligned.right, aligned.bottom), (1_285, 1_285));
        assert_eq!(aligned.block_count(), 121);
    }

    #[test]
    fn a_cursor_walks_the_kingdom_in_rows_and_wraps() {
        let bounds = align_bounds(Bounds::outer_kingdom(), SWEEP_ALIGNMENT);
        let mut cursor = FortressScanCursor::first(bounds);
        let mut seen = Vec::new();
        while let Some(block) = cursor.block(bounds) {
            seen.push((block.x, block.y));
            cursor.advance(bounds);
        }
        assert_eq!(seen.len(), 121);
        assert_eq!(seen[0], (9, 9));
        // Eleven 117-unit columns, so the twelfth block starts row two.
        assert_eq!(seen[11], (9, 126));
        assert_eq!(*seen.last().unwrap(), (1_179, 1_179));

        // Reading does not move the cursor: a lost response retries the same
        // block rather than skipping eighteen slots.
        let cursor = FortressScanCursor::first(bounds);
        assert_eq!(cursor.block(bounds), Some(FortressBlock { x: 9, y: 9 }));
        assert_eq!(cursor.block(bounds), Some(FortressBlock { x: 9, y: 9 }));
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

    /// The walk is the request count for a kingdom: 121, not 289 and not 2,178.
    #[test]
    fn one_sweep_is_121_requests_for_a_kingdom() {
        assert_eq!(
            align_bounds(Bounds::outer_kingdom(), SWEEP_ALIGNMENT).block_count(),
            121
        );
    }

    /// The discovery base is the block over the kingdom's castle, and the four
    /// arms walk outward from it until two windows in a row come back empty.
    ///
    /// Replays the geometry the live arms actually produced: from a castle at
    /// 593,613 in Sands they settled on a 7x8 block rectangle at origins
    /// x 243..945, y 126..945, and that rectangle held all 806 measured
    /// fortresses. The raw coordinates were in a database that has since been
    /// wiped, so this reproduces the shape rather than the original rows.
    #[test]
    fn the_four_arms_walk_outward_and_stop_two_empties_past_the_edge() {
        let origins_x = [243_i64, 360, 477, 594, 711, 828, 945];
        let origins_y = [126_i64, 243, 360, 477, 594, 711, 828, 945];
        // One fortress on every block of the band, which is what lets the arms
        // run to its edge instead of stopping inside it.
        let fortresses: Vec<(i64, i64)> = origins_x
            .iter()
            .flat_map(|x| origins_y.iter().map(move |y| (*x, *y)))
            .collect();
        assert_eq!(fortresses.len(), 56);

        let base = block_origin((593, 613));
        assert_eq!(base, (477, 594), "the castle's own block");

        let holds = |origin: (i64, i64)| {
            let window = FortressBlock {
                x: origin.0,
                y: origin.1,
            }
            .window();
            fortresses.iter().any(|(x, y)| {
                window.left <= *x && *x <= window.right && window.top <= *y && *y <= window.bottom
            })
        };

        let mut reached = vec![base];
        for (dx, dy) in ARM_STEPS {
            let mut origin = base;
            let mut empties = 0_u8;
            for _ in 0..ARM_MAX_STEPS {
                // The arm always advances, empty or not. Re-probing the same
                // block on an empty reply would "confirm" the same hole twice
                // and never look past it.
                origin = (origin.0 + dx * BLOCK_STEP, origin.1 + dy * BLOCK_STEP);
                if holds(origin) {
                    reached.push(origin);
                    empties = 0;
                } else {
                    empties += 1;
                    if empties >= ARM_EMPTY_LIMIT {
                        break;
                    }
                }
            }
        }

        let bounds = bounds_covering(&reached);
        assert_eq!(
            (bounds.left, bounds.top, bounds.right, bounds.bottom),
            (243, 126, 1_043, 1_043)
        );
        let missed = fortresses
            .iter()
            .filter(|(x, y)| {
                *x < bounds.left || *x > bounds.right || *y < bounds.top || *y > bounds.bottom
            })
            .count();
        assert_eq!(missed, 0, "an arm stopped before the band ended");

        // 7 x 8 blocks, against the flat sweep's 11 x 11.
        let fill = align_bounds(bounds, SWEEP_ALIGNMENT).block_count();
        assert_eq!(fill, 56);
    }

    /// The guard that makes an arm safe. Without it a single empty window inside
    /// the band ends the walk, and everything past it is silently missed.
    #[test]
    fn a_gap_inside_the_band_does_not_end_an_arm() {
        let base = (477_i64, 594_i64);
        // A band both sides of the base with one empty block in the middle.
        let band = [243_i64, 360, 477, 711, 828, 945];
        let holds = |origin: (i64, i64)| band.contains(&origin.0) && origin.1 == 594;

        let mut origin = base;
        let mut reached = vec![base];
        let mut empties = 0_u8;
        for _ in 0..ARM_MAX_STEPS {
            origin = (origin.0 + BLOCK_STEP, origin.1);
            if holds(origin) {
                reached.push(origin);
                empties = 0;
            } else {
                empties += 1;
                if empties >= ARM_EMPTY_LIMIT {
                    break;
                }
            }
        }
        assert!(
            reached.contains(&(945, 594)),
            "the arm must cross a one-block gap: {reached:?}"
        );
    }

    /// Every slot has to be answered by some block, or the walk would leave
    /// fortresses behind even though it swept the whole rectangle — and no slot
    /// may be answered twice, or the walk is doing work it has already done.
    #[test]
    fn one_sweep_covers_every_slot_exactly_once() {
        let bounds = align_bounds(Bounds::outer_kingdom(), SWEEP_ALIGNMENT);
        let mut cursor = FortressScanCursor::first(bounds);
        let mut visits = std::collections::HashMap::new();
        while let Some(block) = cursor.block(bounds) {
            for slot in block.slots() {
                *visits.entry(slot).or_insert(0_u32) += 1;
            }
            cursor.advance(bounds);
        }
        for slot in all_outer_kingdom_slots() {
            assert_eq!(
                visits.get(&slot).copied().unwrap_or(0),
                1,
                "{slot:?} is probed the wrong number of times"
            );
        }
        // At this width the windows tile the map almost exactly: 121 x 18 = 2,178,
        // which is every slot, so nothing is requested outside it either.
        assert_eq!(visits.len(), 2_178);
    }
}
