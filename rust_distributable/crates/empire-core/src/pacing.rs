//! Timing policy, ported from the Python bot's `pacing.py` and `ranomizer.py`.
//!
//! Two rules here are safety properties rather than tuning knobs:
//!
//! 1. No two `cra` packets may be sent within [`CRA_MIN_INTERVAL_SECONDS`]
//!    (4 s), transport-wide, and the value is persisted so it survives a restart.
//! 2. Jitter is only ever added **above** that floor. Shortening a normal delay
//!    can therefore never weaken the invariant.
//!
//! The random distributions are reproduced without a dependency: a small
//! xorshift generator plus Box-Muller for the Gaussian draws.

use serde::{Deserialize, Serialize};

/// Hard floor between two `cra` packets, in seconds. Not a tuning knob.
pub const CRA_MIN_INTERVAL_SECONDS: f64 = 4.0;

/// Small xorshift64* generator. Deterministic given a seed, which is what the
/// tests need; jitter quality only has to be "not identical every time"#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn seeded(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// Seeded from the clock. Adequate for attack jitter, not for anything
    /// security-sensitive.
    pub fn from_entropy() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        Self::seeded(nanos ^ (std::process::id() as u64).wrapping_mul(0x9E37_79B9))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    /// Normal draw, clamped at zero so a wait is never negative.
    pub fn normal(&mut self, mean: f64, standard_deviation: f64) -> f64 {
        let u1 = self.unit().max(f64::MIN_POSITIVE);
        let u2 = self.unit();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        (mean + standard_deviation * z).max(0.0)
    }
}

/// Pacing for one attack cycle, mirroring `AttackPacingPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PacingPolicy {
    pub cra_min_interval: f64,
    pub cra_jitter: (f64, f64),
    pub cra_ack_delay: (f64, f64),
    pub adi_to_cra: (f64, f64),
}

impl Default for PacingPolicy {
    fn default() -> Self {
        Self {
            cra_min_interval: CRA_MIN_INTERVAL_SECONDS,
            cra_jitter: (0.15, 0.65),
            cra_ack_delay: (0.75, 2.25),
            adi_to_cra: (5.5, 9.0),
        }
    }
}

impl PacingPolicy {
    /// Deadline for the next `cra`.
    ///
    /// Two independent constraints: the normal `adi → cra` wait, and the global
    /// `cra` floor plus jitter. The later deadline wins, so a shortened handshake
    /// can never breach the floor.
    pub fn cra_due_at(&self, now: f64, last_cra_at: Option<f64>, rng: &mut Rng) -> f64 {
        let adi_due = now + rng.uniform(self.adi_to_cra.0, self.adi_to_cra.1);
        let Some(last) = last_cra_at.filter(|value| *value > 0.0) else {
            return adi_due;
        };
        let floor_due =
            last + self.cra_min_interval + rng.uniform(self.cra_jitter.0, self.cra_jitter.1);
        adi_due.max(floor_due)
    }

    /// Earliest legal `cra` time. Used as the final guard immediately before
    /// sending; adds no jitter so it cannot move a deadline earlier.
    pub fn hard_cra_due_at(&self, last_cra_at: Option<f64>) -> f64 {
        match last_cra_at.filter(|value| *value > 0.0) {
            Some(last) => last + self.cra_min_interval,
            None => 0.0,
        }
    }

    /// Delay after a `cra` acknowledgement, before the next handshake.
    pub fn after_cra_ack(&self, rng: &mut Rng) -> f64 {
        rng.uniform(self.cra_ack_delay.0, self.cra_ack_delay.1)
    }
}

/// The Python `Randomizer` waits that the scan and handshake use, each with the
/// floor that was measured alongside it.
pub struct Waits;

impl Waits {
    /// Gap between map-scan batches. Floor 8 s; the Python form is
    /// `normal(12 + 9·U, 3.5)`.
    pub fn scan_batch(rng: &mut Rng) -> f64 {
        let mean = 12.0 + 9.0 * rng.unit();
        rng.normal(mean, 3.5).max(8.0)
    }

    /// Gap before an attack handshake. Floor 5 s; `normal(7 + 5·U, 2)`.
    pub fn attack_send(rng: &mut Rng) -> f64 {
        let mean = 7.0 + 5.0 * rng.unit();
        rng.normal(mean, 2.0).max(5.0)
    }

    /// Gap between `adi` and `cra` when not using the policy deadline. Floor 3.3 s.
    pub fn adi(rng: &mut Rng) -> f64 {
        let mean = 3.2 + (2.0 * rng.unit()).sqrt();
        rng.normal(mean, 2.0).max(3.3)
    }

    /// How long a returning commander is held before it may be sent again.
    ///
    /// `U(5, 10)`. Held on top of the return trip itself, not instead of it.
    pub fn commander_return_hold(rng: &mut Rng) -> f64 {
        rng.uniform(5.0, 10.0)
    }

    /// Fallback when the result packet is missed because the socket closes:
    /// outbound plus a 20% estimated return leg, then normal reuse jitter.
    pub fn provisional_commander_hold(outbound_seconds: i64, rng: &mut Rng) -> f64 {
        outbound_seconds.max(0) as f64 * 1.2 + Self::commander_return_hold(rng)
    }

    /// How long a freshly claimed target stays out of the selection pool.
    ///
    /// The Python's `TARGET_RESERVE_SECONDS = 12 * 60`. A lease, not a cooldown
    /// tied to the march: it is written when the target is *picked* and released
    /// (with a different delay) if the attack never happens.
    pub fn target_reserve_seconds() -> f64 {
        TARGET_RESERVE_SECONDS
    }

    /// How long to stand a target down after a specific failure.
    ///
    /// The delay encodes *why* the target was dropped, which is why there are
    /// three ranges rather than one.
    pub fn target_retry(rng: &mut Rng, range: (f64, f64)) -> f64 {
        rng.uniform(range.0, range.1)
    }
}

/// Seconds a claimed target is held out of selection (`TARGET_RESERVE_SECONDS`).
pub const TARGET_RESERVE_SECONDS: f64 = 12.0 * 60.0;

/// Retry ranges in seconds, from the Python's `bot/bot.py`.
///
/// * `NO_LID` — no free commander was available for it.
/// * `ERROR` — the server rejected the attack.
/// * `BAD_LEVEL` — the observed level did not match the task.
pub mod target_retry {
    /// `TARGET_NO_LID_RETRY_RANGE` (18–44 min).
    pub const NO_LID: (f64, f64) = (18.0 * 60.0, 44.0 * 60.0);
    /// `TARGET_ERROR_RETRY_RANGE` (21–53 min).
    pub const ERROR: (f64, f64) = (21.0 * 60.0, 53.0 * 60.0);
    /// `TARGET_BAD_LEVEL_RETRY_RANGE` (2.5–4 h).
    pub const BAD_LEVEL: (f64, f64) = (150.0 * 60.0, 240.0 * 60.0);
}

// ---------------------------------------------------------------------------
// recruitment cadence
// ---------------------------------------------------------------------------

/// The observed client cadence, straight from
/// `bot/utility/recruit/config.py::CastleRecruitEvent`.
///
/// The gap between two recruitment requests is Gaussian with mean 3.1 s and
/// deviation 0.8 s, clamped to 0.5–6.0 s. Sampling each gap independently is the
/// point: a fixed beat is the thing that looks automated.
pub const RECRUIT_DELAY_MEAN: f64 = 3.1;
pub const RECRUIT_DELAY_DEVIATION: f64 = 0.8;
pub const RECRUIT_DELAY_LOW: f64 = 0.5;
pub const RECRUIT_DELAY_HIGH: f64 = 6.0;

/// How the gaps between recruitment requests are chosen.
///
/// This is a *timing* profile, not a way of choosing which castle to work on.
/// What the server sees is the rhythm of the requests, so that is what the
/// operator actually picks between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecruitTempo {
    /// As fast as the floor allows, for clearing queues in a hurry.
    Greedy,
    /// Flat random gaps with an occasional longer pause, so bursts never repeat.
    Sporadic,
    /// Bounded Gaussian, reproducing the recorded client. The default.
    #[default]
    Advanced,
}

impl RecruitTempo {
    /// Names come from the database, so an unknown or empty value must fall back
    /// to the default rather than failing a run that is already in progress.
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "greedy" => Self::Greedy,
            "sporadic" => Self::Sporadic,
            _ => Self::Advanced,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::Sporadic => "sporadic",
            Self::Advanced => "advanced",
        }
    }
}

/// One gap between two recruitment requests, in seconds.
pub fn recruit_request_delay(tempo: RecruitTempo, rng: &mut Rng) -> f64 {
    match tempo {
        // Just above the floor, with enough movement that it is not a metronome.
        RecruitTempo::Greedy => rng.uniform(RECRUIT_DELAY_LOW, RECRUIT_DELAY_LOW + 0.4),
        RecruitTempo::Sporadic => {
            if rng.unit() < 0.15 {
                rng.uniform(RECRUIT_DELAY_HIGH, RECRUIT_DELAY_HIGH + 14.0)
            } else {
                rng.uniform(RECRUIT_DELAY_LOW, RECRUIT_DELAY_HIGH)
            }
        }
        RecruitTempo::Advanced => rng
            .normal(RECRUIT_DELAY_MEAN, RECRUIT_DELAY_DEVIATION)
            .clamp(RECRUIT_DELAY_LOW, RECRUIT_DELAY_HIGH),
    }
}

/// The pause before moving on to the next castle, using the same profile widened,
/// because a castle change is a bigger step than the next slot in a queue.
pub fn recruit_turn_delay(tempo: RecruitTempo, rng: &mut Rng) -> f64 {
    match tempo {
        RecruitTempo::Greedy => rng.uniform(RECRUIT_DELAY_LOW, RECRUIT_DELAY_LOW + 1.0),
        RecruitTempo::Sporadic => rng.uniform(RECRUIT_DELAY_LOW, RECRUIT_DELAY_HIGH + 14.0),
        RecruitTempo::Advanced => rng
            .normal(RECRUIT_DELAY_MEAN * 2.0, RECRUIT_DELAY_DEVIATION * 2.0)
            .clamp(RECRUIT_DELAY_LOW, RECRUIT_DELAY_HIGH * 2.0),
    }
}

/// Current wall-clock seconds, matching the Python `time.time()` basis.
pub fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or_default()}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_only_ever_added_above_the_cra_floor() {
        let policy = PacingPolicy::default();
        let mut rng = Rng::seeded(7);
        let last = 1_000.0;
        // Even when `now` is far in the past, the floor governs.
        for _ in 0..2_000 {
            let due = policy.cra_due_at(last, Some(last), &mut rng);
            assert!(
                due >= last + CRA_MIN_INTERVAL_SECONDS,
                "cra deadline {due} breached last_cra + floor"
            );
        }
    }

    #[test]
    fn the_later_of_the_two_constraints_wins() {
        let policy = PacingPolicy::default();
        let mut rng = Rng::seeded(11);
        // A very recent cra must dominate the normal adi-to-cra wait.
        let last = 500.0;
        let due = policy.cra_due_at(500.0, Some(last), &mut rng);
        assert!(due >= last + CRA_MIN_INTERVAL_SECONDS);
        // With no previous cra, only the normal wait applies.
        let due = policy.cra_due_at(500.0, None, &mut rng);
        assert!((500.0 + policy.adi_to_cra.0..=500.0 + policy.adi_to_cra.1).contains(&due));
        // A zero timestamp means "never", not "at the epoch".
        assert_eq!(policy.hard_cra_due_at(Some(0.0)), 0.0);
        assert_eq!(policy.hard_cra_due_at(None), 0.0);
    }

    #[test]
    fn hard_cra_due_at_adds_no_jitter() {
        let policy = PacingPolicy::default();
        assert_eq!(policy.hard_cra_due_at(Some(100.0)), 104.0);
    }

    #[test]
    fn wait_floors_hold_across_many_draws() {
        let mut rng = Rng::seeded(3);
        for _ in 0..5_000 {
            assert!(Waits::scan_batch(&mut rng) >= 8.0);
            assert!(Waits::attack_send(&mut rng) >= 5.0);
            assert!(Waits::adi(&mut rng) >= 3.3);
            assert!(Waits::commander_return_hold(&mut rng) >= 5.0);
            assert!(Waits::commander_return_hold(&mut rng) <= 10.0);
        }
    }

    #[test]
    fn the_target_lease_is_twelve_minutes() {
        assert_eq!(Waits::target_reserve_seconds(), 720.0);
    }

    #[test]
    fn missed_return_fallback_is_one_point_two_outbound_plus_small_jitter() {
        let mut rng = Rng::seeded(91);
        for _ in 0..100 {
            let hold = Waits::provisional_commander_hold(500, &mut rng);
            assert!((605.0..=610.0).contains(&hold));
        }
    }

    #[test]
    fn retry_ranges_are_ordered_by_severity() {
        let mut rng = Rng::seeded(5);
        // A server rejection is retried sooner than a level mismatch, which needs
        // re-learning. Both are longer than the plain 12 minute lease.
        assert!(target_retry::ERROR.0 >= TARGET_RESERVE_SECONDS);
        assert!(target_retry::NO_LID.0 >= TARGET_RESERVE_SECONDS);
        assert!(target_retry::BAD_LEVEL.0 > target_retry::ERROR.1);
        for _ in 0..1_000 {
            let error = Waits::target_retry(&mut rng, target_retry::ERROR);
            assert!((target_retry::ERROR.0..=target_retry::ERROR.1).contains(&error));
            let level = Waits::target_retry(&mut rng, target_retry::BAD_LEVEL);
            assert!(level >= 150.0 * 60.0);
        }
    }

    #[test]
    fn a_seeded_generator_is_reproducible() {
        let draw = |seed| {
            let mut rng = Rng::seeded(seed);
            (0..8).map(|_| rng.uniform(0.0, 1.0)).collect::<Vec<_>>()
        };
        assert_eq!(draw(42), draw(42));
        assert_ne!(draw(42), draw(43));
    }

    #[test]
    fn the_generator_stays_in_range() {
        let mut rng = Rng::from_entropy();
        for _ in 0..10_000 {
            let value = rng.uniform(-3.0, 7.0);
            assert!((-3.0..7.0).contains(&value), "{value} out of range");
        }
    }

    // -----------------------------------------------------------------------
    // recruitment cadence
    // -----------------------------------------------------------------------

    #[test]
    fn an_unset_or_unknown_tempo_falls_back_to_advanced() {
        assert_eq!(RecruitTempo::from_name(""), RecruitTempo::Advanced);
        assert_eq!(RecruitTempo::from_name("   "), RecruitTempo::Advanced);
        assert_eq!(RecruitTempo::from_name("nonsense"), RecruitTempo::Advanced);
        assert_eq!(RecruitTempo::default(), RecruitTempo::Advanced);
        assert_eq!(
            RecruitTempo::from_name("GREEDY").name(),
            "greedy",
            "names round-trip through the database in either direction"
        );
    }

    /// `advanced` is the recorded client, so its shape is the one that matters:
    /// mostly in the middle of the range, never outside it.
    #[test]
    fn the_advanced_cadence_is_a_bounded_gaussian() {
        let mut rng = Rng::seeded(7);
        let mut samples = Vec::new();
        for _ in 0..20_000 {
            let value = recruit_request_delay(RecruitTempo::Advanced, &mut rng);
            assert!(
                (RECRUIT_DELAY_LOW..=RECRUIT_DELAY_HIGH).contains(&value),
                "{value} left the observed bounds"
            );
            samples.push(value);
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!(
            (mean - RECRUIT_DELAY_MEAN).abs() < 0.1,
            "mean {mean} should sit near the observed {RECRUIT_DELAY_MEAN}"
        );
    }

    /// The point of the cadence is that it is not a metronome: a fixed beat is
    /// what looks automated.
    #[test]
    fn no_tempo_repeats_a_fixed_gap() {
        for tempo in [
            RecruitTempo::Greedy,
            RecruitTempo::Sporadic,
            RecruitTempo::Advanced,
        ] {
            let mut rng = Rng::seeded(11);
            let first = recruit_request_delay(tempo, &mut rng);
            let repeats = (0..200)
                .filter(|_| (recruit_request_delay(tempo, &mut rng) - first).abs() < f64::EPSILON)
                .count();
            assert!(
                repeats < 5,
                "{tempo:?} produced the same gap {repeats} times"
            );
        }
    }

    #[test]
    fn greedy_is_the_fastest_and_sporadic_can_stretch_furthest() {
        let mut rng = Rng::seeded(3);
        let greedy = (0..2_000)
            .map(|_| recruit_request_delay(RecruitTempo::Greedy, &mut rng))
            .fold(0.0_f64, f64::max);
        let advanced = (0..2_000)
            .map(|_| recruit_request_delay(RecruitTempo::Advanced, &mut rng))
            .fold(0.0_f64, f64::max);
        let sporadic = (0..2_000)
            .map(|_| recruit_request_delay(RecruitTempo::Sporadic, &mut rng))
            .fold(0.0_f64, f64::max);
        assert!(
            greedy <= RECRUIT_DELAY_LOW + 0.4,
            "greedy must stay near the floor, saw {greedy}"
        );
        assert!(sporadic > advanced, "sporadic must reach further than advanced");
    }

    #[test]
    fn a_castle_change_pauses_longer_than_a_slot() {
        for tempo in [
            RecruitTempo::Greedy,
            RecruitTempo::Sporadic,
            RecruitTempo::Advanced,
        ] {
            let mut rng = Rng::seeded(5);
            let turn = (0..500)
                .map(|_| recruit_turn_delay(tempo, &mut rng))
                .sum::<f64>()
                / 500.0;
            let slot = (0..500)
                .map(|_| recruit_request_delay(tempo, &mut rng))
                .sum::<f64>()
                / 500.0;
            assert!(
                turn > slot,
                "{tempo:?}: a castle change ({turn:.2}s) should outlast a slot ({slot:.2}s)"
            );
        }
    }
}
