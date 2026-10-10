//! The action scheduler: when the next logical action may happen.
//!
//! One timing layer for the existing attack sequence, not a second scheduler. The
//! sequence itself (`adi`, wait, `cra`, acknowledgement) is unchanged and still
//! driven by [`super::automation`]; this decides the two human-scale waits in it
//! (the pause before `cra` after an `adi`, and the pause after an acknowledgement)
//! from the wave generator in [`empire_core::timing`], and records what it did so
//! the Development tab can show it.
//!
//! These are *logical human actions*. Heartbeats, login, acknowledgements and the
//! other protocol traffic are not randomised.
//!
//! Each wait takes the next value of the generator (`t = t + 1`). A wait shorter
//! than the action's protocol minimum is lengthened to it, and the global `cra`
//! spacing is applied on top afterwards by
//! [`empire_core::pacing::PacingPolicy::cra_due_at_with`] and again immediately
//! before sending: limits only ever make a wait longer, and the sample is still
//! used up rather than skipped. Because each wait is generated fresh from "now" once
//! the previous step resolves, an overdue action cannot build a backlog or fire in
//! a burst afterwards.
//!
//! Timing and geography have different lifecycles: changing kingdom resets the
//! spotlight (see `targeting`), not this generator.

use std::collections::VecDeque;

use empire_core::timing::{Projected, Sample, TimingConfig, WaveTimer};
use serde::Serialize;

/// Waits kept for the diagnostics and the Development tab's chart.
const RECENT_LEN: usize = 120;

/// The two waits in an attack, each with the protocol minimum that already applied
/// to it before the generator existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Wait {
    /// After an `adi` reply, before the `cra`.
    BeforeAttack,
    /// After a `cra` acknowledgement, before the next handshake.
    AfterAck,
}

impl Wait {
    /// The shortest this wait may be, seconds.
    fn protocol_floor(self) -> f64 {
        match self {
            Self::BeforeAttack => 2.0,
            Self::AfterAck => 0.75,
        }
    }
}

/// One wait that was actually used.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    /// Which value of the generator it was (`t`).
    pub index: u64,
    pub at_ms: i64,
    pub wait: Wait,
    /// What the generator produced.
    pub raw_s: f64,
    /// After the action's own protocol minimum.
    pub waited_s: f64,
    /// What was really waited: longer again when a global limit decided.
    pub applied_s: f64,
    pub signal: f64,
    pub noise: f64,
    pub protocol_floor_s: f64,
    /// The limit that lengthened the wait beyond `waited_s`, if one did.
    pub restriction: Option<&'static str>,
}

pub(super) struct ActionScheduler {
    timer: WaveTimer,
    recent: VecDeque<Applied>,
    pending: Option<(Sample, Wait)>,
    next_action_at_ms: Option<i64>,
    restriction: Option<&'static str>,
}

impl ActionScheduler {
    pub(super) fn new(seed: u64) -> Self {
        Self::with_config(TimingConfig::default(), seed)
    }

    pub(super) fn with_config(config: TimingConfig, seed: u64) -> Self {
        Self {
            timer: WaveTimer::new(config, seed),
            recent: VecDeque::new(),
            pending: None,
            next_action_at_ms: None,
            restriction: None,
        }
    }

    pub(super) fn timer(&self) -> &WaveTimer {
        &self.timer
    }

    /// The next `steps` waits as the generator stands now, without noise.
    pub(super) fn project(&self, steps: usize) -> Vec<Projected> {
        self.timer.project(steps)
    }

    /// Take the next value for a wait and return how long to wait for it, seconds
    /// (never below the wait's protocol minimum). The caller then applies any
    /// global limit and reports the result through [`Self::applied`].
    pub(super) fn interval(&mut self, wait: Wait) -> f64 {
        let sample = self.timer.next_interval(wait.protocol_floor());
        self.pending = Some((sample, wait));
        sample.interval_s
    }

    /// Record what a wait turned into: the real wait, and the limit that moved it
    /// later than the generator's own, if any.
    pub(super) fn applied(
        &mut self,
        wait: Wait,
        now_ms: i64,
        applied_s: f64,
        restriction: Option<&'static str>,
    ) {
        let Some((sample, pending_wait)) = self.pending.take() else { return };
        debug_assert_eq!(wait, pending_wait);
        if self.recent.len() == RECENT_LEN {
            self.recent.pop_front();
        }
        self.recent.push_back(Applied {
            index: sample.index,
            at_ms: now_ms,
            wait,
            raw_s: sample.raw_s,
            waited_s: sample.interval_s,
            applied_s: applied_s.max(sample.interval_s),
            signal: sample.signal,
            noise: sample.noise,
            protocol_floor_s: sample.floor_s,
            restriction,
        });
        self.next_action_at_ms = Some(now_ms + (applied_s * 1_000.0) as i64);
        self.restriction = restriction;
    }

    pub(super) fn clear_next(&mut self) {
        self.next_action_at_ms = None;
        self.restriction = None;
    }

    pub(super) fn next_action_at_ms(&self) -> Option<i64> {
        self.next_action_at_ms
    }

    pub(super) fn restriction(&self) -> Option<&'static str> {
        self.restriction
    }

    pub(super) fn last(&self) -> Option<&Applied> {
        self.recent.back()
    }

    /// Newest first.
    pub(super) fn recent(&self) -> impl Iterator<Item = &Applied> {
        self.recent.iter().rev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000_000;

    #[test]
    fn each_wait_respects_its_own_protocol_minimum() {
        let mut scheduler = ActionScheduler::new(1);
        for _ in 0..3_000 {
            assert!(scheduler.interval(Wait::BeforeAttack) >= 2.0);
            assert!(scheduler.interval(Wait::AfterAck) >= 0.75);
        }
    }

    #[test]
    fn most_after_ack_waits_are_short_and_a_minority_are_long() {
        let mut scheduler = ActionScheduler::new(2);
        let waits = (0..6_000).map(|_| scheduler.interval(Wait::AfterAck)).collect::<Vec<_>>();
        let short = waits.iter().filter(|wait| **wait <= 5.0).count() as f64 / waits.len() as f64;
        let long = waits.iter().filter(|wait| **wait > 10.0).count() as f64 / waits.len() as f64;
        assert!((0.6..0.9).contains(&short), "{short}");
        assert!((0.05..0.2).contains(&long), "{long}");
    }

    #[test]
    fn every_wait_uses_up_exactly_one_value_of_the_generator() {
        let mut scheduler = ActionScheduler::new(3);
        for expected in 0..50 {
            scheduler.interval(Wait::BeforeAttack);
            scheduler.applied(Wait::BeforeAttack, T0, 3.0, None);
            assert_eq!(scheduler.last().unwrap().index, expected);
        }
        assert_eq!(scheduler.timer().index(), 50);
    }

    #[test]
    fn the_projection_names_the_values_that_come_next() {
        let mut scheduler = ActionScheduler::new(4);
        scheduler.interval(Wait::AfterAck);
        let projected = scheduler.project(10);
        assert_eq!(projected.len(), 10);
        assert_eq!(projected[0].index, 1);
        assert!(projected.iter().all(|value| value.interval_s.is_finite() && value.interval_s >= 0.5));
    }

    #[test]
    fn the_applied_wait_records_which_limit_lengthened_it() {
        let mut scheduler = ActionScheduler::new(5);
        let waited = scheduler.interval(Wait::BeforeAttack);
        scheduler.applied(Wait::BeforeAttack, T0, waited + 1.5, Some("global cra spacing"));
        let last = scheduler.last().unwrap();
        assert_eq!(last.restriction, Some("global cra spacing"));
        assert!((last.applied_s - last.waited_s - 1.5).abs() < 1e-9);
        assert!(last.waited_s >= last.raw_s - 1e-12 && last.waited_s >= last.protocol_floor_s);
        assert_eq!(scheduler.next_action_at_ms(), Some(T0 + ((waited + 1.5) * 1_000.0) as i64));
        scheduler.clear_next();
        assert_eq!(scheduler.next_action_at_ms(), None);
        // An unrequested record (no value pending) changes nothing.
        scheduler.applied(Wait::AfterAck, T0, 1.0, None);
        assert_eq!(scheduler.recent().count(), 1);
    }

    #[test]
    fn the_recent_list_is_bounded_and_newest_first() {
        let mut scheduler = ActionScheduler::new(6);
        for index in 0..400 {
            let value = scheduler.interval(Wait::AfterAck);
            scheduler.applied(Wait::AfterAck, T0 + index, value, None);
        }
        assert_eq!(scheduler.recent().count(), RECENT_LEN);
        assert_eq!(scheduler.recent().next().unwrap().index, 399);
    }
}
