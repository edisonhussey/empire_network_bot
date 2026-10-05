use super::*;

/// How often the runner re-checks which sweeps the running bot still needs.
pub(super) const FORTRESS_PLAN_INTERVAL_MS: i64 = 2_000;

/// Delay after a confirmed fortress probe.
///
/// One request is in flight at a time, so this is the whole walk's tempo. The
/// server answers a `gaa` in roughly 0.1-0.5 s (measured, and it varies by
/// ground rather than by window size). The agreed policy is a 0.7-second floor
/// plus one uniformly random fractional second.
pub(super) fn fortress_probe_delay_seconds(rng: &mut Rng) -> f64 {
    0.7 + rng.uniform(0.0, 1.0)
}

/// Extra pause when the walk moves to the next kingdom, so a kingdom boundary
/// is not a burst of requests.
pub(super) fn fortress_kingdom_switch_delay_seconds(rng: &mut Rng) -> f64 {
    rng.uniform(3.4, 4.8)
}

/// Measures the rectangle a kingdom's fortresses actually occupy.
///
/// The walk used to be a flat sweep of a rectangle hard-coded from one server's
/// map, which pays 121 requests on every kingdom and is simply wrong on a server
/// whose map is a different size. This finds the extent instead: start on the
/// block over the kingdom's own castle and walk four arms outward, one window at
/// a time, until an arm has seen [`ARM_EMPTY_LIMIT`] empty windows in a row. The
/// rectangle those stops bound is then filled row by row.
///
/// An arm advances on an empty window as well as on a full one. The guard is
/// meant to survive a hole *inside* the band - a block with no fortress in it
/// between two that have some - so the walk has to look past the hole to decide,
/// not re-ask about it.
pub(super) struct FortressDiscovery {
    pub(super) kingdom_id: i64,
    /// The block over the kingdom's castle. Its window spans the castle, so it
    /// is always full and is where every arm starts.
    pub(super) base: (i64, i64),
    /// The block in flight, waiting for its reply.
    pub(super) probe: (i64, i64),
    /// Which of [`ARM_STEPS`] is being walked.
    pub(super) arm: usize,
    /// Consecutive empty windows seen by the current arm.
    pub(super) empties: u8,
    /// Windows asked about by the current arm, so a kingdom with a fortress in
    /// every direction cannot walk forever.
    pub(super) steps: u8,
    /// Origins whose window held at least one fortress.
    pub(super) reached: Vec<(i64, i64)>,
    /// Set once the base has been answered, after which the arms run.
    pub(super) seen_base: bool,
    /// Answered discovery windows. Before the boundary is known the UI treats
    /// each answer as one estimated percentage point (capped below complete).
    pub(super) probes_answered: u64,
    pub(super) done: bool,
}

impl FortressDiscovery {
    pub(super) fn new(kingdom_id: i64, base: (i64, i64)) -> Self {
        Self {
            kingdom_id,
            base,
            probe: base,
            arm: 0,
            empties: 0,
            steps: 0,
            reached: Vec::new(),
            seen_base: false,
            probes_answered: 0,
            done: false,
        }
    }

    /// The window to ask about next, or `None` once the rectangle is measured.
    pub(super) fn next(&self) -> Option<FortressBlock> {
        (!self.done).then_some(FortressBlock {
            x: self.probe.0,
            y: self.probe.1,
        })
    }

    /// Record what the window in flight held.
    ///
    /// `hit` is false both for a window with no fortress and for a refused one.
    /// A refusal is a window we did not really look at, so treating it as empty
    /// ends the arm early and costs coverage, not correctness - the opposite
    /// choice would extend the rectangle over ground we never saw.
    pub(super) fn observe(&mut self, hit: bool) {
        if self.done {
            return;
        }
        self.probes_answered = self.probes_answered.saturating_add(1);
        if !self.seen_base {
            self.seen_base = true;
            if hit {
                self.reached.push(self.base);
            }
            self.probe = self.base;
            self.step_arm();
            return;
        }
        if hit {
            self.reached.push(self.probe);
            self.empties = 0;
        } else {
            self.empties += 1;
        }
        self.steps += 1;
        if self.empties >= ARM_EMPTY_LIMIT || self.steps >= ARM_MAX_STEPS {
            self.arm += 1;
            self.empties = 0;
            self.steps = 0;
            if self.arm >= ARM_STEPS.len() {
                self.done = true;
                return;
            }
            self.probe = self.base;
        }
        self.step_arm();
    }

    pub(super) fn estimated_percent(&self) -> u64 {
        self.probes_answered.min(99)
    }

    pub(super) fn step_arm(&mut self) {
        let (dx, dy) = ARM_STEPS[self.arm];
        self.probe = (
            self.probe.0 + dx * BLOCK_STEP,
            self.probe.1 + dy * BLOCK_STEP,
        );
    }

    /// The measured rectangle, or `None` if no window ever held a fortress.
    pub(super) fn bounds(&self) -> Option<Bounds> {
        (!self.reached.is_empty()).then(|| bounds_covering(&self.reached))
    }
}
