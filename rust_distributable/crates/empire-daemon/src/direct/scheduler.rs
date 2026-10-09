//! The action scheduler: when the next logical action may happen.
//!
//! One timing layer for the existing attack sequence, not a second scheduler. The
//! sequence itself (`adi`, wait, `cra`, acknowledgement) is unchanged and still
//! driven by [`super::automation`]; this decides the two human-scale waits in it
//! (the pause before `cra` after an `adi`, and the pause after an acknowledgement)
//! from the stochastic timer in [`empire_core::timing`], and records what it did so
//! the Development tab can show it.
//!
//! Hard limits always win. The global `cra` spacing is applied afterwards by
//! [`empire_core::pacing::PacingPolicy::cra_due_at_with`] and again immediately
//! before sending; a stochastic interval can lengthen a wait but never shorten a
//! limit. Because each wait is generated fresh from "now" when the previous step
//! resolves, an overdue action can never build a backlog or fire in a burst.
//!
//! Timing and geography have different lifecycles: changing kingdom resets the
//! spotlight (see `targeting`), not this timer.

use std::{collections::VecDeque, time::Instant};

use empire_core::timing::{Sample, TimingConfig, WaveTimer};
use serde::Serialize;

/// Seconds between points kept for the waveform history.
const HISTORY_STEP_S: f64 = 2.0;
/// History points kept (about ten minutes).
const HISTORY_LEN: usize = 300;
/// Recent intervals kept for the diagnostics list.
const RECENT_LEN: usize = 24;

/// The two waits in an attack, each with the protocol minimum that already applied
/// to it. The stochastic floor is the larger of that and the timer's own 0.4 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Wait {
    /// After an `adi` reply, before the `cra`.
    BeforeAttack,
    /// After a `cra` acknowledgement, before the next handshake.
    AfterAck,
}

impl Wait {
    /// `(floor_s, baseline_s)`. The floors are the old pacing minimums; the
    /// baselines put the average where the old uniform ranges did (about 3.1 s and
    /// 1.6 s).
    fn shape(self) -> (f64, f64) {
        match self {
            Self::BeforeAttack => (2.0, 1.0),
            Self::AfterAck => (0.75, 0.8),
        }
    }
}

/// One interval that was actually used.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub at_ms: i64,
    pub wait: Wait,
    /// The stochastic interval.
    pub stochastic_s: f64,
    /// What was really waited: longer when a global limit decided.
    pub applied_s: f64,
    pub signal: f64,
    pub noise: f64,
    pub floor_s: f64,
    /// The limit that lengthened the wait, if one did.
    pub restriction: Option<&'static str>,
}

pub(super) struct ActionScheduler {
    timer: WaveTimer,
    started: Instant,
    history: VecDeque<(i64, f64)>,
    last_history_s: f64,
    recent: VecDeque<Applied>,
    pending: Option<Sample>,
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
            started: Instant::now(),
            history: VecDeque::new(),
            last_history_s: f64::NEG_INFINITY,
            recent: VecDeque::new(),
            pending: None,
            next_action_at_ms: None,
            restriction: None,
        }
    }

    pub(super) fn timer(&self) -> &WaveTimer {
        &self.timer
    }

    /// Seconds on the monotonic clock since this scheduler started.
    fn clock(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Keep the wave moving with real time even when no action happens, and add a
    /// history point every couple of seconds. Cheap enough to call on every tick.
    pub(super) fn tick(&mut self, now_ms: i64) {
        self.tick_at(self.clock(), now_ms);
    }

    fn tick_at(&mut self, elapsed_s: f64, now_ms: i64) {
        self.timer.advance(elapsed_s);
        if elapsed_s - self.last_history_s >= HISTORY_STEP_S {
            self.last_history_s = elapsed_s;
            if self.history.len() == HISTORY_LEN {
                self.history.pop_front();
            }
            self.history.push_back((now_ms, self.timer.signal()));
        }
    }

    /// Generate the stochastic interval for a wait. The caller then applies any
    /// global limit and reports the result through [`Self::applied`].
    pub(super) fn interval(&mut self, wait: Wait) -> f64 {
        self.interval_at(self.clock(), wait)
    }

    fn interval_at(&mut self, elapsed_s: f64, wait: Wait) -> f64 {
        let (floor, baseline) = wait.shape();
        let sample = self.timer.next_interval(elapsed_s, floor, Some(baseline));
        self.pending = Some(sample);
        sample.interval_s
    }

    /// Record what a wait turned into: the due time, and the limit that moved it
    /// later than the stochastic interval, if any.
    pub(super) fn applied(
        &mut self,
        wait: Wait,
        now_ms: i64,
        applied_s: f64,
        restriction: Option<&'static str>,
    ) {
        let Some(sample) = self.pending.take() else { return };
        if self.recent.len() == RECENT_LEN {
            self.recent.pop_front();
        }
        self.recent.push_back(Applied {
            at_ms: now_ms,
            wait,
            stochastic_s: sample.interval_s,
            applied_s: applied_s.max(sample.interval_s),
            signal: sample.signal,
            noise: sample.noise,
            floor_s: sample.floor_s,
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

    pub(super) fn recent(&self) -> impl Iterator<Item = &Applied> {
        self.recent.iter().rev()
    }

    pub(super) fn history(&self) -> impl Iterator<Item = &(i64, f64)> {
        self.history.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000_000;

    #[test]
    fn each_wait_respects_its_own_protocol_minimum() {
        let mut scheduler = ActionScheduler::new(1);
        for index in 1..=2_000 {
            let elapsed = index as f64 * 3.0;
            assert!(scheduler.interval_at(elapsed, Wait::BeforeAttack) >= 2.0);
            assert!(scheduler.interval_at(elapsed + 1.0, Wait::AfterAck) >= 0.75);
        }
    }

    #[test]
    fn typical_waits_sit_where_the_old_uniform_ranges_did() {
        let mut scheduler = ActionScheduler::new(2);
        let mean = |scheduler: &mut ActionScheduler, wait| {
            (1..=3_000)
                .map(|index| scheduler.interval_at(index as f64 * 5.0, wait))
                .sum::<f64>()
                / 3_000.0
        };
        let before = mean(&mut scheduler, Wait::BeforeAttack);
        let after = mean(&mut ActionScheduler::new(3), Wait::AfterAck);
        assert!((2.6..3.8).contains(&before), "before {before}");
        assert!((1.2..2.2).contains(&after), "after {after}");
    }

    #[test]
    fn the_history_is_a_bounded_ring_that_advances_with_time_not_calls() {
        let mut scheduler = ActionScheduler::new(4);
        for step in 0..100_000 {
            // Called every 250 ms like the real tick, for a long time.
            scheduler.tick_at(step as f64 * 0.25, T0 + step * 250);
        }
        assert_eq!(scheduler.history().count(), HISTORY_LEN);
        // A point only every two seconds, not one per tick.
        let times = scheduler.history().map(|(at, _)| *at).collect::<Vec<_>>();
        assert!(times.windows(2).all(|pair| pair[1] - pair[0] >= 2_000));
    }

    #[test]
    fn the_applied_wait_records_which_limit_lengthened_it() {
        let mut scheduler = ActionScheduler::new(5);
        let stochastic = scheduler.interval_at(10.0, Wait::BeforeAttack);
        scheduler.applied(Wait::BeforeAttack, T0, stochastic + 1.5, Some("cra spacing"));
        let last = scheduler.last().unwrap();
        assert_eq!(last.restriction, Some("cra spacing"));
        assert!((last.applied_s - last.stochastic_s - 1.5).abs() < 1e-9);
        assert_eq!(scheduler.next_action_at_ms(), Some(T0 + ((stochastic + 1.5) * 1_000.0) as i64));
        scheduler.clear_next();
        assert_eq!(scheduler.next_action_at_ms(), None);
        // An unrequested record (no interval pending) changes nothing.
        scheduler.applied(Wait::AfterAck, T0, 1.0, None);
        assert_eq!(scheduler.recent().count(), 1);
    }

    #[test]
    fn the_recent_list_is_bounded() {
        let mut scheduler = ActionScheduler::new(6);
        for index in 0..200 {
            let value = scheduler.interval_at(index as f64, Wait::AfterAck);
            scheduler.applied(Wait::AfterAck, T0 + index, value, None);
        }
        assert_eq!(scheduler.recent().count(), RECENT_LEN);
    }
}
