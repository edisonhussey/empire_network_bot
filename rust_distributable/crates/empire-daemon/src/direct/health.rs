//! Response health: telling a quiet, healthy exchange from one that has gone wrong.
//!
//! An empty payload is not automatically a failure, and a refusal is not a fault of
//! the connection. Only the cases where an answer should have come and did not (or
//! came back empty when it never should) count towards the tolerance:
//!
//! | Outcome | Counts |
//! | --- | --- |
//! | expected empty response (an empty map window, say) | no |
//! | explicit server rejection (a status such as 95) | no |
//! | connection loss (the link was silent) | no, the supervisor handles it |
//! | missing response / timeout on a live link | **yes** |
//! | unexpected null (status 0 with a null payload) | **yes** |
//!
//! Qualifying incidents are kept for one rolling hour. With the default tolerance
//! of two: the first is a warning, the second says the tolerance is reached, the
//! third stops new actions. Stopping is a pause, never an exit: work already on
//! the wire resolves, nothing is discarded, and nothing resumes by itself.

use std::collections::VecDeque;

use serde::Serialize;

/// The rolling window, milliseconds.
pub(super) const WINDOW_MS: i64 = 60 * 60 * 1_000;
/// Incidents kept for display; the window prunes first, this only bounds a burst.
const MAX_KEPT: usize = 64;
/// Default tolerance: this many qualifying incidents per rolling hour are allowed.
pub(super) const DEFAULT_TOLERANCE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    ExpectedEmpty,
    MissingResponse,
    Timeout,
    ConnectionLoss,
    ExplicitRejection,
    UnexpectedNull,
}

impl Outcome {
    /// Does this outcome count towards the tolerance?
    pub(super) fn qualifies(self) -> bool {
        matches!(self, Self::MissingResponse | Self::Timeout | Self::UnexpectedNull)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub at_ms: i64,
    pub outcome: Outcome,
    pub qualifies: bool,
    pub detail: String,
}

/// What a recorded incident means for the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Recorded for display only.
    Noted,
    /// Within tolerance: expose a warning.
    Warning,
    /// The last tolerated incident: the next one stops new actions.
    ToleranceReached,
    /// Over tolerance: stop starting new actions.
    Paused,
}

#[derive(Debug, Clone)]
pub(super) struct ResponseHealth {
    incidents: VecDeque<Incident>,
    tolerance: usize,
    paused_at_ms: Option<i64>,
}

impl ResponseHealth {
    pub(super) fn new(tolerance: usize) -> Self {
        Self { incidents: VecDeque::new(), tolerance: tolerance.max(1), paused_at_ms: None }
    }

    pub(super) fn tolerance(&self) -> usize {
        self.tolerance
    }

    pub(super) fn set_tolerance(&mut self, tolerance: usize) {
        self.tolerance = tolerance.max(1);
    }

    fn expire(&mut self, now_ms: i64) {
        while self.incidents.front().is_some_and(|incident| now_ms - incident.at_ms >= WINDOW_MS) {
            self.incidents.pop_front();
        }
    }

    /// Qualifying incidents inside the rolling hour.
    pub(super) fn count(&mut self, now_ms: i64) -> usize {
        self.expire(now_ms);
        self.incidents.iter().filter(|incident| incident.qualifies).count()
    }

    pub(super) fn paused(&self) -> bool {
        self.paused_at_ms.is_some()
    }

    pub(super) fn paused_at_ms(&self) -> Option<i64> {
        self.paused_at_ms
    }

    /// Only an operator action (stopping and starting the mode) clears a pause.
    pub(super) fn clear_pause(&mut self) {
        self.paused_at_ms = None;
    }

    pub(super) fn recent(&self) -> impl Iterator<Item = &Incident> {
        self.incidents.iter().rev()
    }

    pub(super) fn record(&mut self, outcome: Outcome, detail: &str, now_ms: i64) -> Verdict {
        self.expire(now_ms);
        let qualifies = outcome.qualifies();
        if self.incidents.len() == MAX_KEPT {
            self.incidents.pop_front();
        }
        self.incidents.push_back(Incident { at_ms: now_ms, outcome, qualifies, detail: detail.to_owned() });
        if !qualifies {
            return Verdict::Noted;
        }
        let count = self.count(now_ms);
        if count > self.tolerance {
            self.paused_at_ms.get_or_insert(now_ms);
            Verdict::Paused
        } else if count == self.tolerance {
            Verdict::ToleranceReached
        } else {
            Verdict::Warning
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000_000;

    #[test]
    fn only_missing_answers_count() {
        for (outcome, counts) in [
            (Outcome::ExpectedEmpty, false),
            (Outcome::ExplicitRejection, false),
            (Outcome::ConnectionLoss, false),
            (Outcome::MissingResponse, true),
            (Outcome::Timeout, true),
            (Outcome::UnexpectedNull, true),
        ] {
            assert_eq!(outcome.qualifies(), counts, "{outcome:?}");
        }
    }

    #[test]
    fn the_third_qualifying_incident_in_an_hour_pauses() {
        let mut health = ResponseHealth::new(DEFAULT_TOLERANCE);
        assert_eq!(health.record(Outcome::Timeout, "adi", T0), Verdict::Warning);
        assert_eq!(health.record(Outcome::UnexpectedNull, "adi", T0 + 60_000), Verdict::ToleranceReached);
        assert!(!health.paused());
        assert_eq!(health.record(Outcome::MissingResponse, "cra", T0 + 120_000), Verdict::Paused);
        assert!(health.paused());
        assert_eq!(health.count(T0 + 120_000), 3);
    }

    #[test]
    fn incidents_expire_after_one_rolling_hour() {
        let mut health = ResponseHealth::new(2);
        health.record(Outcome::Timeout, "a", T0);
        health.record(Outcome::Timeout, "b", T0 + 10 * 60_000);
        assert_eq!(health.count(T0 + WINDOW_MS - 1), 2);
        assert_eq!(health.count(T0 + WINDOW_MS), 1, "the first has aged out");
        assert_eq!(health.count(T0 + 10 * 60_000 + WINDOW_MS), 0);
        // Two hours of quiet make the next one a first warning again.
        assert_eq!(health.record(Outcome::Timeout, "c", T0 + 3 * WINDOW_MS), Verdict::Warning);
    }

    #[test]
    fn spread_out_incidents_never_pause() {
        let mut health = ResponseHealth::new(2);
        for index in 0..10 {
            let verdict = health.record(Outcome::Timeout, "slow trickle", T0 + index * 40 * 60_000);
            assert_ne!(verdict, Verdict::Paused, "incident {index}");
        }
        assert!(!health.paused());
    }

    #[test]
    fn noise_that_does_not_qualify_is_shown_but_not_counted() {
        let mut health = ResponseHealth::new(2);
        for index in 0..20 {
            assert_eq!(health.record(Outcome::ExplicitRejection, "95", T0 + index), Verdict::Noted);
            assert_eq!(health.record(Outcome::ConnectionLoss, "wifi", T0 + index), Verdict::Noted);
        }
        assert_eq!(health.count(T0 + 100), 0);
        assert!(!health.paused());
        assert!(health.recent().count() > 0, "still listed for display");
    }

    #[test]
    fn a_pause_stays_until_an_operator_clears_it_and_the_threshold_is_configurable() {
        let mut health = ResponseHealth::new(1);
        health.record(Outcome::Timeout, "a", T0);
        assert_eq!(health.record(Outcome::Timeout, "b", T0 + 1), Verdict::Paused);
        // Time passing does not resume it.
        assert_eq!(health.count(T0 + 5 * WINDOW_MS), 0);
        assert!(health.paused());
        health.clear_pause();
        assert!(!health.paused());
        health.set_tolerance(0);
        assert_eq!(health.tolerance(), 1, "never a tolerance of zero");
    }

    #[test]
    fn the_display_list_is_bounded() {
        let mut health = ResponseHealth::new(1_000);
        for index in 0..500 {
            health.record(Outcome::Timeout, "burst", T0 + index);
        }
        assert!(health.recent().count() <= MAX_KEPT);
    }
}
