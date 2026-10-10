//! Human-paced waits from a sum of many waves.
//!
//! The wait before a logical action is one value of a deterministic-looking signal:
//!
//! ```text
//! S(t)   = sum over 10 waves  A_k * shape_k(omega_k * t + phi_k)
//! D(t)   = m + exp( offset + S(t) + eps )
//! ```
//!
//! * `t` is **a count of actions**, not seconds: each action takes the next value,
//!   `t = t + 1`. Sampled at whole steps, sines and cosines of unrelated frequencies
//!   look random (`sin(5t)` at integer `t` is not a smooth curve), and ten of them
//!   together do not repeat visibly. A single slow wave, which is what this used to
//!   be, reads as a sinusoid.
//! * The waves are of three kinds so the sum is not made of one shape: plain `sin`,
//!   plain `cos`, and a folded wave `asin(b * sin(x)) / asin(b)`, which is a rounded
//!   triangle with sharper corners than a sine.
//! * `exp(...)` makes the result positive and **low-heavy**: most waits are short
//!   and a minority are long (a log-normal-like shape). `m`, the floor, is drawn once
//!   as `0.5 + rand/2` seconds, so a wait is never below half a second and the floor
//!   differs between sessions.
//! * `offset` is the lever for the whole distribution and the one parameter that
//!   matters: it sets where the bulk sits. With the defaults about 60 % of waits fall
//!   in 1 s to 5 s, about 11 % are under a second, and a tail runs out to tens of
//!   seconds (see the tests, which pin these numbers).
//! * `eps` is small independent noise, kept separate so a chart can show it apart
//!   from the waves.
//!
//! Parameters are adjusted slightly every step (each frequency is pulled gently back
//! towards its starting value while jittering, and phases advance by the *current*
//! frequency so a change never makes a wave jump), so the pattern never settles into
//! a loop. Nothing here reads a clock. A global limit (the 4 s `cra` spacing, a
//! protocol minimum) is applied by the caller afterwards by waiting longer; it never
//! shortens a wait and never skips a sample.
//!
//! Because the signal is deterministic given its parameters, the next values can be
//! worked out in advance with [`WaveTimer::project`], which the Development tab uses.

use std::f64::consts::{PI, TAU};

use crate::pacing::Rng;

/// Tunable parameters. A deployment normally changes none of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingConfig {
    /// Number of superimposed waves.
    pub wave_count: usize,
    /// Allowed range of each wave's frequency, in radians per step. Sampled at whole
    /// steps a frequency near 0 or a whole turn (about 6.28) reads as a slow wave,
    /// so the range stays clear of both; the set actually drawn is also checked for
    /// correlation between successive waits (see `MAX_AUTOCORRELATION`).
    pub min_omega: f64,
    pub max_omega: f64,
    /// Standard deviation of the wave sum `S`, in log space. This is the spread of
    /// the distribution: larger means more very short and very long waits.
    pub wave_sigma: f64,
    /// Standard deviation of the independent noise `eps`, in log space.
    pub noise_sigma: f64,
    /// `offset` in `exp(offset + S + eps)`: where the bulk of waits sits.
    pub log_offset: f64,
    /// The floor `m` is drawn uniformly from this range of seconds, once.
    pub floor_range: (f64, f64),
    /// No wait is longer than this, seconds.
    pub max_interval_s: f64,
    /// Strength of the pull of each frequency back to its starting value, per step.
    pub omega_reversion: f64,
    /// Per-step jitter of each frequency, radians.
    pub omega_jitter: f64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            wave_count: 10,
            min_omega: 0.9,
            max_omega: 5.3,
            wave_sigma: 1.5,
            noise_sigma: 0.3,
            log_offset: 0.5,
            floor_range: (0.5, 1.0),
            max_interval_s: 120.0,
            omega_reversion: 0.02,
            omega_jitter: 0.004,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaveKind {
    Sin,
    Cos,
    /// `asin(b * sin(x)) / asin(b)` with `0 < b < 1`.
    Fold { b: f64 },
}

impl WaveKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Fold { .. } => "fold",
        }
    }

    /// Unit-amplitude value at phase `x`.
    fn unit(self, x: f64) -> f64 {
        match self {
            Self::Sin => x.sin(),
            Self::Cos => x.cos(),
            Self::Fold { b } => (b * x.sin()).asin() / b.asin(),
        }
    }

    /// Variance of the unit-amplitude wave over a uniformly random phase.
    fn variance(self) -> f64 {
        match self {
            Self::Sin | Self::Cos => 0.5,
            Self::Fold { .. } => {
                const STEPS: usize = 720;
                (0..STEPS)
                    .map(|index| self.unit(TAU * index as f64 / STEPS as f64).powi(2))
                    .sum::<f64>()
                    / STEPS as f64
            }
        }
    }
}

/// One wave. `omega` drifts around `base_omega`; the amplitude is fixed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub kind: WaveKind,
    pub base_omega: f64,
    pub omega: f64,
    pub amplitude: f64,
    /// Radians, kept in `[0, TAU)`. Advances by `omega` each step.
    pub phase: f64,
}

impl Wave {
    fn value(&self) -> f64 {
        self.amplitude * self.kind.unit(self.phase)
    }

    fn value_ahead(&self, steps: f64) -> f64 {
        self.amplitude * self.kind.unit(self.phase + self.omega * steps)
    }
}

/// One generated wait and the parts it was made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// How many waits had been generated before this one (`t`).
    pub index: u64,
    /// The wave sum `S(t)`.
    pub signal: f64,
    /// The independent noise `eps`.
    pub noise: f64,
    /// The generator's own output, `m + exp(offset + S + eps)`, seconds.
    pub raw_s: f64,
    /// The protocol minimum the caller asked for.
    pub floor_s: f64,
    /// What to wait: the raw value, or the protocol minimum if that is longer.
    pub interval_s: f64,
}

/// A value worked out in advance, without noise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    pub index: u64,
    pub signal: f64,
    pub interval_s: f64,
}

#[derive(Debug, Clone)]
pub struct WaveTimer {
    config: TimingConfig,
    waves: Vec<Wave>,
    rng: Rng,
    floor_s: f64,
    index: u64,
}

/// Frequencies near a simple fraction of a turn repeat after a few steps
/// (`pi` alternates every step, `pi/2` every four). Keep clear of them.
fn is_repetitive(omega: f64) -> bool {
    if (omega - PI).abs() < 0.2 {
        return true;
    }
    (1..=6_u32).any(|q| {
        (1..=q).any(|p| {
            gcd(p, q) == 1 && (omega - TAU * f64::from(p) / f64::from(q)).abs() < 0.12
        })
    })
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The frequencies are tuned until successive waits are close to uncorrelated. For
/// a lone wave the correlation between waits `lag` steps apart is `cos(lag * omega)`,
/// so a frequency near `pi` alternates (lag 2 is near 1) and one near a turn looks
/// slow (lag 1 is near 1). Weighting each wave by its share of the variance gives
/// the correlation of the sum, and that is what is kept small, at every lag up to
/// [`CHECKED_LAGS`]. A random draw of ten waves typically has a worst lag around
/// 0.5 (a single wave: 0.9 and more); moving one frequency at a time and keeping
/// the moves that help gets it to about 0.3. Ten angles cannot satisfy twelve lags
/// much better than that, so 0.3 is the target rather than something near zero.
const TARGET_AUTOCORRELATION: f64 = 0.3;
/// How many lags the check covers.
const CHECKED_LAGS: usize = 12;
/// Single-frequency moves tried per start, and how many starts at most.
const TUNING_MOVES: usize = 800;
const RESTARTS: usize = 6;

fn worst_autocorrelation(waves: &[Wave]) -> f64 {
    let weights = waves
        .iter()
        .map(|wave| wave.kind.variance() * wave.amplitude * wave.amplitude)
        .collect::<Vec<_>>();
    let total: f64 = weights.iter().sum();
    (1..=CHECKED_LAGS)
        .map(|lag| {
            let correlation: f64 = waves
                .iter()
                .zip(&weights)
                .map(|(wave, weight)| weight * (lag as f64 * wave.omega).cos())
                .sum::<f64>()
                / total;
            correlation.abs()
        })
        .fold(0.0, f64::max)
}

impl WaveTimer {
    fn draw_waves(config: &TimingConfig, rng: &mut Rng) -> Vec<Wave> {
        let count = config.wave_count.max(1);
        let (low, high) = (config.min_omega, config.max_omega.max(config.min_omega));
        (0..count)
            .map(|index| {
                let kind = match index % 3 {
                    0 => WaveKind::Sin,
                    1 => WaveKind::Cos,
                    _ => WaveKind::Fold { b: rng.uniform(0.55, 0.92) },
                };
                let mut omega = rng.uniform(low, high);
                for _ in 0..100 {
                    if !is_repetitive(omega) {
                        break;
                    }
                    omega = rng.uniform(low, high);
                }
                Wave {
                    kind,
                    base_omega: omega,
                    omega,
                    // A relative weight for now; scaled to the budget by the caller.
                    amplitude: rng.uniform(0.5, 1.5),
                    phase: rng.uniform(0.0, TAU),
                }
            })
            .collect()
    }

    /// Draw waves and tune their frequencies for low correlation between waits,
    /// restarting from a fresh draw a few times if a start gets stuck high.
    fn tuned_waves(config: &TimingConfig, rng: &mut Rng) -> Vec<Wave> {
        let (low, high) = (config.min_omega, config.max_omega.max(config.min_omega));
        let mut best: Option<(f64, Vec<Wave>)> = None;
        for _ in 0..RESTARTS {
            let mut waves = Self::draw_waves(config, rng);
            let mut worst = worst_autocorrelation(&waves);
            for _ in 0..TUNING_MOVES {
                if worst < TARGET_AUTOCORRELATION {
                    break;
                }
                let which = (rng.next_u64() % waves.len() as u64) as usize;
                let before = waves[which].omega;
                let proposal = rng.uniform(low, high);
                if is_repetitive(proposal) {
                    continue;
                }
                waves[which].omega = proposal;
                let candidate = worst_autocorrelation(&waves);
                if candidate < worst {
                    worst = candidate;
                    waves[which].base_omega = proposal;
                } else {
                    waves[which].omega = before;
                }
            }
            if best.as_ref().is_none_or(|(known, _)| worst < *known) {
                best = Some((worst, waves));
            }
            if best.as_ref().is_some_and(|(known, _)| *known < TARGET_AUTOCORRELATION) {
                break;
            }
        }
        best.expect("at least one start").1
    }

    pub fn new(config: TimingConfig, seed: u64) -> Self {
        let mut rng = Rng::seeded(seed);
        let floor_s = rng.uniform(config.floor_range.0, config.floor_range.1);
        let mut waves = Self::tuned_waves(&config, &mut rng);
        // One shared variability budget: scale every amplitude by the same factor
        // so the wave sum has the configured spread, whatever the mix of shapes.
        let variance: f64 = waves
            .iter()
            .map(|wave| wave.kind.variance() * wave.amplitude * wave.amplitude)
            .sum();
        let scale = config.wave_sigma / variance.sqrt();
        for wave in &mut waves {
            wave.amplitude *= scale;
        }
        Self { config, waves, rng, floor_s, index: 0 }
    }

    pub fn config(&self) -> &TimingConfig {
        &self.config
    }

    pub fn waves(&self) -> &[Wave] {
        &self.waves
    }

    /// The floor `m`, seconds: no raw wait is shorter.
    pub fn floor_s(&self) -> f64 {
        self.floor_s
    }

    /// How many waits have been generated (`t`).
    pub fn index(&self) -> u64 {
        self.index
    }

    /// The wave sum at the current step.
    pub fn signal(&self) -> f64 {
        self.waves.iter().map(Wave::value).sum()
    }

    fn to_seconds(&self, exponent: f64) -> f64 {
        let ceiling = (self.config.max_interval_s - self.floor_s).max(1.0).ln();
        self.floor_s + exponent.clamp(-6.0, ceiling).exp()
    }

    /// The next `steps` values if parameters stayed as they are and there were no
    /// noise. A projection to look at, not a promise: parameters drift each step.
    pub fn project(&self, steps: usize) -> Vec<Projected> {
        (0..steps)
            .map(|ahead| {
                let signal: f64 = self.waves.iter().map(|wave| wave.value_ahead(ahead as f64)).sum();
                Projected {
                    index: self.index + ahead as u64,
                    signal,
                    interval_s: self.to_seconds(self.config.log_offset + signal),
                }
            })
            .collect()
    }

    /// Take the next value and move on one step (`t = t + 1`). `floor_s` is the
    /// action's protocol minimum: if the raw value is shorter the caller waits that
    /// long instead; the sample is still used up, never skipped.
    pub fn next_interval(&mut self, floor_s: f64) -> Sample {
        let signal = self.signal();
        let noise = self.rng.gaussian(0.0, self.config.noise_sigma);
        let raw_s = self.to_seconds(self.config.log_offset + signal + noise);
        let sample = Sample {
            index: self.index,
            signal,
            noise,
            raw_s,
            floor_s: floor_s.max(0.0),
            interval_s: raw_s.max(floor_s.max(0.0)),
        };
        self.step();
        sample
    }

    /// `t = t + 1`: phases advance by the current frequency, then each frequency is
    /// pulled gently back to where it started and nudged. Phases integrate the
    /// frequency actually in force, so a change never makes a wave jump.
    fn step(&mut self) {
        self.index += 1;
        let (low, high) = (self.config.min_omega, self.config.max_omega);
        for wave in &mut self.waves {
            wave.phase = (wave.phase + wave.omega).rem_euclid(TAU);
            let pull = self.config.omega_reversion * (wave.base_omega - wave.omega);
            let nudge = self.config.omega_jitter * self.rng.gaussian(0.0, 1.0);
            wave.omega = (wave.omega + pull + nudge).clamp(low, high);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer(seed: u64) -> WaveTimer {
        WaveTimer::new(TimingConfig::default(), seed)
    }

    fn raw_values(seed: u64, count: usize) -> Vec<f64> {
        let mut timer = timer(seed);
        (0..count).map(|_| timer.next_interval(0.0).raw_s).collect()
    }

    #[test]
    fn there_are_ten_waves_of_three_kinds_with_the_configured_spread() {
        let timer = timer(1);
        assert_eq!(timer.waves().len(), 10);
        let kinds = timer.waves().iter().map(|wave| wave.kind.name()).collect::<std::collections::HashSet<_>>();
        assert_eq!(kinds.len(), 3, "sin, cos and the folded wave all present");
        let config = *timer.config();
        for wave in timer.waves() {
            assert!((config.min_omega..=config.max_omega).contains(&wave.omega));
            assert!(!is_repetitive(wave.omega), "{}", wave.omega);
            assert!(wave.amplitude > 0.0 && wave.amplitude.is_finite());
        }
        // The shared budget gives the wave sum its configured spread.
        let variance: f64 = timer.waves().iter().map(|wave| wave.kind.variance() * wave.amplitude.powi(2)).sum();
        assert!((variance.sqrt() - config.wave_sigma).abs() < 1e-9);
    }

    #[test]
    fn the_wave_count_is_configurable_and_never_empty() {
        let config = TimingConfig { wave_count: 4, ..TimingConfig::default() };
        assert_eq!(WaveTimer::new(config, 2).waves().len(), 4);
        let config = TimingConfig { wave_count: 0, ..TimingConfig::default() };
        assert_eq!(WaveTimer::new(config, 2).waves().len(), 1);
    }

    #[test]
    fn the_floor_is_half_a_second_plus_half_a_random_second_and_no_wait_goes_below_it() {
        let mut floors = Vec::new();
        for seed in 0..60 {
            let mut timer = timer(seed);
            let floor = timer.floor_s();
            assert!((0.5..1.0).contains(&floor), "{floor}");
            floors.push(floor);
            for _ in 0..500 {
                let sample = timer.next_interval(0.0);
                assert!(sample.raw_s >= floor && sample.raw_s.is_finite(), "{sample:?}");
                assert!(sample.raw_s <= 120.0 + 1e-9, "{sample:?}");
                assert!(sample.interval_s > 0.0, "never a negative or zero wait");
            }
        }
        let spread = floors.iter().cloned().fold(f64::MIN, f64::max) - floors.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread > 0.3, "the floor differs between sessions");
    }

    /// The point of the whole design: short waits are the likely ones.
    #[test]
    fn about_sixty_percent_of_waits_are_between_one_and_five_seconds_with_a_long_tail() {
        let values = (0..40).flat_map(|seed| raw_values(seed, 4_000)).collect::<Vec<_>>();
        let share = |test: &dyn Fn(f64) -> bool| values.iter().filter(|value| test(**value)).count() as f64 / values.len() as f64;
        let bulk = share(&|value| (1.0..=5.0).contains(&value));
        assert!((0.55..=0.67).contains(&bulk), "1 to 5 s: {bulk:.3}");
        assert!(share(&|value| value < 1.0) < 0.2, "under a second is the minority");
        assert!((0.07..=0.2).contains(&share(&|value| value > 10.0)), "a real tail");
        assert!(share(&|value| value > 30.0) > 0.005, "and it reaches tens of seconds");
        let mut sorted = values.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        assert!((1.8..=3.0).contains(&median), "median {median:.2}");
        assert!(mean > median * 1.5, "low-heavy: the mean {mean:.2} sits well above the median {median:.2}");
    }

    #[test]
    fn successive_waits_do_not_look_like_a_single_wave() {
        // A lone slow sinusoid has autocorrelation near 1 at short lags. Ten waves
        // sampled at whole steps are close to uncorrelated.
        for seed in 0..12 {
            let values = raw_values(seed, 20_000).iter().map(|value| (value - 0.0).ln()).collect::<Vec<_>>();
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance = values.iter().map(|value| (value - mean).powi(2)).sum::<f64>();
            for lag in 1..=12 {
                let covariance = values.iter().zip(&values[lag..]).map(|(a, b)| (a - mean) * (b - mean)).sum::<f64>();
                let correlation = covariance / variance;
                assert!(correlation.abs() < 0.36, "seed {seed} lag {lag}: {correlation:.3}");
            }
        }
    }

    #[test]
    fn the_sequence_does_not_settle_into_a_loop() {
        let values = raw_values(11, 3_000);
        for period in 2..=40 {
            let matches = values.iter().zip(&values[period..]).filter(|(a, b)| (**a - **b).abs() < 1e-6).count();
            assert!(matches < 5, "period {period} repeats {matches} times");
        }
        // (Compared at full precision: low-heavy waits crowd just above the floor,
        // so rounding to a millisecond would make different waits look equal.)
        let distinct = values.iter().map(|value| value.to_bits()).collect::<std::collections::HashSet<_>>();
        assert!(distinct.len() > 2_950, "{} distinct of 3000 (a few sit exactly on the floor)", distinct.len());
    }

    #[test]
    fn the_chosen_frequencies_never_alias_to_a_slow_or_alternating_wave() {
        for seed in 0..60 {
            let timer = timer(seed);
            assert!(worst_autocorrelation(timer.waves()) < 0.42, "seed {seed}");
            for wave in timer.waves() {
                assert!((0.9..=5.3).contains(&wave.omega), "{}", wave.omega);
            }
        }
    }

    #[test]
    fn noise_is_symmetric_and_separate_from_the_waves() {
        let mut timer = timer(3);
        let noise = (0..20_000).map(|_| timer.next_interval(0.0).noise).collect::<Vec<_>>();
        let mean = noise.iter().sum::<f64>() / noise.len() as f64;
        let std = (noise.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / noise.len() as f64).sqrt();
        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((std - 0.3).abs() < 0.01, "std {std}");
        assert!(noise.iter().any(|value| *value < -0.3) && noise.iter().any(|value| *value > 0.3));
    }

    #[test]
    fn a_protocol_minimum_lengthens_a_wait_and_never_skips_a_sample() {
        let mut free = timer(4);
        let mut bound = timer(4);
        for index in 0..2_000 {
            let a = free.next_interval(0.0);
            let b = bound.next_interval(2.0);
            assert_eq!(a.raw_s, b.raw_s, "same sequence either way");
            assert_eq!(b.index, index);
            assert!(b.interval_s >= 2.0 && b.interval_s >= b.raw_s);
            assert_eq!(b.interval_s, b.raw_s.max(2.0));
        }
        assert_eq!(bound.index(), 2_000, "one step per wait");
    }

    #[test]
    fn parameters_drift_a_little_each_step_but_stay_in_bounds_and_phases_stay_continuous() {
        let mut timer = timer(5);
        let config = *timer.config();
        let start = timer.waves().to_vec();
        let mut previous = timer.waves().to_vec();
        for _ in 0..200_000 {
            timer.next_interval(0.0);
            for (now, before) in timer.waves().iter().zip(&previous) {
                assert!(now.omega.is_finite() && now.phase.is_finite());
                assert!((config.min_omega..=config.max_omega).contains(&now.omega));
                assert!((0.0..TAU).contains(&now.phase));
                // The phase moved by the frequency in force, to within rounding.
                let moved = (now.phase - before.phase - before.omega).rem_euclid(TAU);
                assert!(moved < 1e-6 || TAU - moved < 1e-6, "phase jumped by {moved}");
                assert!((now.omega - before.omega).abs() < 0.05, "frequency moved too fast");
            }
            previous = timer.waves().to_vec();
        }
        let drifted = timer.waves().iter().zip(&start).filter(|(a, b)| (a.omega - b.omega).abs() > 1e-4).count();
        assert!(drifted >= 8, "parameters really adjust over time");
        let budget: f64 = timer.waves().iter().map(|wave| wave.amplitude).sum();
        assert!(timer.signal().abs() <= budget + 1e-9);
    }

    #[test]
    fn the_projection_is_exactly_what_comes_next_when_nothing_drifts() {
        let config = TimingConfig { omega_jitter: 0.0, omega_reversion: 0.0, noise_sigma: 0.0, ..TimingConfig::default() };
        let mut timer = WaveTimer::new(config, 6);
        timer.next_interval(0.0);
        let projected = timer.project(25);
        assert_eq!(projected[0].index, 1);
        for projection in projected {
            let actual = timer.next_interval(0.0);
            assert_eq!(actual.index, projection.index);
            assert!((actual.signal - projection.signal).abs() < 1e-9);
            assert!((actual.raw_s - projection.interval_s).abs() < 1e-9);
        }
    }

    #[test]
    fn with_drift_the_projection_stays_close_for_the_next_few_steps_only() {
        let mut timer = timer(8);
        let projected = timer.project(30);
        let actual = (0..30).map(|_| timer.next_interval(0.0)).collect::<Vec<_>>();
        let first = (projected[0].signal - actual[0].signal).abs();
        assert!(first < 1e-9, "step 0 is exact: {first}");
        let near = (projected[3].signal - actual[3].signal).abs();
        assert!(near < 0.5, "a few steps ahead is still close: {near}");
    }

    #[test]
    fn the_same_seed_gives_the_same_sequence_and_different_seeds_differ() {
        assert_eq!(raw_values(9, 200), raw_values(9, 200));
        assert_ne!(raw_values(9, 200), raw_values(10, 200));
    }

    #[test]
    fn extreme_settings_still_give_finite_positive_waits() {
        for config in [
            TimingConfig { wave_sigma: 6.0, noise_sigma: 2.0, log_offset: 4.0, ..TimingConfig::default() },
            TimingConfig { wave_sigma: 0.0, noise_sigma: 0.0, log_offset: -10.0, ..TimingConfig::default() },
            TimingConfig { max_interval_s: 0.2, ..TimingConfig::default() },
        ] {
            let mut timer = WaveTimer::new(config, 12);
            for _ in 0..5_000 {
                let sample = timer.next_interval(0.0);
                assert!(sample.interval_s.is_finite() && sample.interval_s > 0.0, "{sample:?} {config:?}");
            }
        }
    }

    #[test]
    fn the_folded_wave_is_normalised_and_bounded() {
        let kind = WaveKind::Fold { b: 0.8 };
        for step in 0..1_000 {
            let value = kind.unit(step as f64 * 0.0137);
            assert!(value.abs() <= 1.0 + 1e-12, "{value}");
        }
        assert!((kind.unit(PI / 2.0) - 1.0).abs() < 1e-12, "peak is exactly 1");
        // Between a triangle (1/3) and a sine (1/2) for the same peak.
        assert!(kind.variance() > 1.0 / 3.0 && kind.variance() < 0.5, "{}", kind.variance());
    }
}
