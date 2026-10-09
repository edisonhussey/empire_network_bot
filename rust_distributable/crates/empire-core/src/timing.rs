//! Stochastic timing: a few slowly drifting waves turned into positive intervals.
//!
//! ```text
//! D(t) = D_min + D0 * exp( sum_k A_k sin(theta_k(t)) + eps )
//! ```
//!
//! * `D_min` is a lower bound for the stochastic part, never a fixed delay. A caller
//!   whose action has a larger protocol minimum passes it as `floor_s`; the larger of
//!   the two applies.
//! * The waves make successive intervals drift smoothly (a calm minute, then a slower
//!   one) instead of being independent draws, which is what a person's pace does.
//! * `eps` is small independent noise, kept separate so it can be shown apart from
//!   the deterministic part.
//!
//! The signal is evaluated directly. There is no density to normalise or integrate,
//! and work per interval is constant.
//!
//! Time is passed in as seconds on a monotonic clock (`elapsed_s`), so the model
//! advances by real elapsed time, survives sleeps and reconnects (a long gap is one
//! bounded step, never numerical blow-up) and tests can drive it with a fake clock.
//! This is only the timing *signal*. Hard limits such as the 4 s `cra` floor are
//! applied by the caller afterwards and always win.

use std::f64::consts::TAU;

use crate::pacing::Rng;

/// Tunable parameters. Every field has a sensible default; a deployment normally
/// changes none of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingConfig {
    /// Number of waves. Three is enough to look irregular without being noisy.
    pub wave_count: usize,
    /// Allowed range of each wave's frequency, in Hz. Defaults give periods from
    /// 15 s to 10 minutes.
    pub min_frequency: f64,
    pub max_frequency: f64,
    /// Total variability budget: the sum of the amplitudes, in log space. 0.6 moves
    /// an interval between about 0.55x and 1.8x its baseline.
    pub variability: f64,
    /// Pull of a drifting frequency back towards its starting value, per second.
    pub drift_reversion: f64,
    /// Strength of the frequency drift, in Hz per square-root second.
    pub drift_sigma: f64,
    /// Standard deviation of the independent noise term, in log space.
    pub noise_sigma: f64,
    /// Initial stochastic lower bound, seconds.
    pub min_interval_s: f64,
    /// Baseline interval scale `D0`, seconds.
    pub baseline_s: f64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            wave_count: 3,
            min_frequency: 1.0 / 600.0,
            max_frequency: 1.0 / 15.0,
            variability: 0.6,
            drift_reversion: 0.01,
            drift_sigma: 0.0015,
            noise_sigma: 0.12,
            min_interval_s: 0.4,
            baseline_s: 1.0,
        }
    }
}

/// One wave. `frequency` drifts around `base_frequency`; `amplitude` is fixed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub base_frequency: f64,
    pub frequency: f64,
    pub amplitude: f64,
    /// Radians, kept in `[0, TAU)`.
    pub phase: f64,
}

/// One generated interval and the parts it was made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Monotonic seconds at which it was generated.
    pub at_s: f64,
    /// The deterministic wave sum at that moment.
    pub signal: f64,
    /// The independent noise term.
    pub noise: f64,
    /// The stochastic floor that applied (`max(D_min, floor_s)`).
    pub floor_s: f64,
    /// The interval to wait, seconds.
    pub interval_s: f64,
}

#[derive(Debug, Clone)]
pub struct WaveTimer {
    config: TimingConfig,
    waves: Vec<Wave>,
    rng: Rng,
    elapsed_s: f64,
}

/// The exponent is clamped so a freak noise draw cannot produce a huge interval.
const MAX_EXPONENT: f64 = 2.5;

impl WaveTimer {
    pub fn new(config: TimingConfig, seed: u64) -> Self {
        let mut rng = Rng::seeded(seed);
        let count = config.wave_count.max(1);
        let weights = (0..count).map(|_| rng.uniform(0.5, 1.5)).collect::<Vec<_>>();
        let total: f64 = weights.iter().sum();
        let low = config.min_frequency.max(1e-6);
        let high = config.max_frequency.max(low);
        let waves = weights
            .into_iter()
            .map(|weight| {
                // Spread frequencies evenly in log space so the waves are not all
                // the same period.
                let frequency = (low.ln() + rng.unit() * (high.ln() - low.ln())).exp();
                Wave {
                    base_frequency: frequency,
                    frequency,
                    amplitude: config.variability * weight / total,
                    phase: rng.uniform(0.0, TAU),
                }
            })
            .collect();
        Self { config, waves, rng, elapsed_s: 0.0 }
    }

    pub fn config(&self) -> &TimingConfig {
        &self.config
    }

    pub fn waves(&self) -> &[Wave] {
        &self.waves
    }

    pub fn elapsed_s(&self) -> f64 {
        self.elapsed_s
    }

    /// Bring the model forward to `elapsed_s` on the monotonic clock. A time that
    /// is not later than the last one is ignored. Phases integrate the *current*
    /// frequency over the step, so a frequency change never makes the wave jump.
    pub fn advance(&mut self, elapsed_s: f64) {
        if !elapsed_s.is_finite() || elapsed_s <= self.elapsed_s {
            return;
        }
        let dt = elapsed_s - self.elapsed_s;
        self.elapsed_s = elapsed_s;
        let kappa = self.config.drift_reversion.max(1e-9);
        // Exact step of the mean-reverting process, so a long sleep is one
        // well-behaved step rather than an unstable Euler one.
        let decay = (-kappa * dt).exp();
        let spread = self.config.drift_sigma
            * ((1.0 - decay * decay) / (2.0 * kappa)).max(0.0).sqrt();
        let (low, high) = (self.config.min_frequency, self.config.max_frequency);
        for wave in &mut self.waves {
            // Phase first, with the frequency in force during the step.
            wave.phase = (wave.phase + TAU * wave.frequency * dt).rem_euclid(TAU);
            let mean = wave.base_frequency;
            let next = mean + (wave.frequency - mean) * decay + spread * self.rng.normal(0.0, 1.0);
            wave.frequency = next.clamp(low, high);
        }
    }

    /// The deterministic wave sum now.
    pub fn signal(&self) -> f64 {
        self.waves
            .iter()
            .map(|wave| wave.amplitude * wave.phase.sin())
            .sum()
    }

    /// The wave sum `ahead_s` seconds from now if frequencies stayed as they are.
    /// This is a projection for display, not a promise about future intervals.
    pub fn project(&self, ahead_s: f64) -> f64 {
        self.waves
            .iter()
            .map(|wave| wave.amplitude * (wave.phase + TAU * wave.frequency * ahead_s).sin())
            .sum()
    }

    /// Generate the next interval. `floor_s` is the action's own protocol minimum;
    /// the stochastic floor is the larger of it and `min_interval_s`. `baseline_s`
    /// overrides `D0` for actions of a different scale.
    pub fn next_interval(&mut self, elapsed_s: f64, floor_s: f64, baseline_s: Option<f64>) -> Sample {
        self.advance(elapsed_s);
        let signal = self.signal();
        let noise = self.rng.normal(0.0, self.config.noise_sigma);
        let floor = self.config.min_interval_s.max(floor_s.max(0.0));
        let baseline = baseline_s.unwrap_or(self.config.baseline_s).max(0.0);
        let interval_s = floor + baseline * (signal + noise).clamp(-MAX_EXPONENT, MAX_EXPONENT).exp();
        Sample { at_s: self.elapsed_s, signal, noise, floor_s: floor, interval_s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer(seed: u64) -> WaveTimer {
        WaveTimer::new(TimingConfig::default(), seed)
    }

    #[test]
    fn intervals_never_go_below_the_stochastic_floor_or_the_actions_own() {
        let mut timer = timer(1);
        let mut at = 0.0;
        for index in 0..20_000 {
            at += 0.7;
            let floor = if index % 2 == 0 { 0.0 } else { 2.0 };
            let sample = timer.next_interval(at, floor, None);
            assert!(sample.interval_s >= 0.4, "{sample:?}");
            assert!(sample.interval_s >= floor, "{sample:?}");
            assert!(sample.interval_s.is_finite() && sample.interval_s < 60.0, "{sample:?}");
        }
    }

    #[test]
    fn the_minimum_is_a_bound_not_a_fixed_delay() {
        let mut timer = timer(2);
        let mut at = 0.0;
        let intervals = (0..2_000)
            .map(|_| {
                at += 1.0;
                timer.next_interval(at, 0.0, None).interval_s
            })
            .collect::<Vec<_>>();
        let spread = intervals.iter().cloned().fold(f64::MIN, f64::max)
            - intervals.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread > 0.5, "intervals should vary, spread {spread}");
    }

    #[test]
    fn waves_stay_finite_and_inside_their_bounds_over_a_long_run() {
        let mut timer = timer(3);
        let config = *timer.config();
        for step in 1..=200_000 {
            timer.advance(step as f64 * 0.5);
        }
        assert_eq!(timer.waves().len(), 3);
        for wave in timer.waves() {
            assert!(wave.frequency.is_finite() && wave.phase.is_finite());
            assert!((config.min_frequency..=config.max_frequency).contains(&wave.frequency));
            assert!((0.0..TAU).contains(&wave.phase));
            assert!(wave.amplitude > 0.0);
        }
        let budget: f64 = timer.waves().iter().map(|wave| wave.amplitude).sum();
        assert!((budget - config.variability).abs() < 1e-9, "amplitudes share the budget");
        assert!(timer.signal().abs() <= config.variability + 1e-9);
    }

    /// Changing frequency must not make the wave jump: the signal moves by at most
    /// what the fastest wave allows over the step.
    #[test]
    fn frequency_drift_keeps_the_signal_continuous() {
        let mut timer = timer(4);
        let config = *timer.config();
        let dt = 0.05;
        let max_slope = TAU * config.max_frequency * config.variability;
        let mut previous = timer.signal();
        let mut at = 0.0;
        for _ in 0..50_000 {
            at += dt;
            timer.advance(at);
            let now = timer.signal();
            assert!((now - previous).abs() <= max_slope * dt + 1e-9, "jump {} at {at}", now - previous);
            previous = now;
        }
    }

    #[test]
    fn a_long_sleep_is_one_stable_step() {
        let mut timer = timer(5);
        timer.advance(10.0);
        for gap in [3_600.0, 86_400.0, 1.0e7, 1.0e9] {
            let next = timer.elapsed_s() + gap;
            let sample = timer.next_interval(next, 0.0, None);
            assert!(sample.interval_s.is_finite() && sample.interval_s >= 0.4);
            for wave in timer.waves() {
                assert!(wave.frequency.is_finite() && wave.phase.is_finite());
            }
        }
    }

    #[test]
    fn time_that_does_not_move_forward_is_ignored() {
        let mut timer = timer(6);
        timer.advance(100.0);
        let before = timer.waves().to_vec();
        timer.advance(100.0);
        timer.advance(40.0);
        timer.advance(f64::NAN);
        assert_eq!(timer.waves(), before.as_slice());
        assert_eq!(timer.elapsed_s(), 100.0);
    }

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        let run = |seed| {
            let mut timer = timer(seed);
            (1..=50).map(|index| timer.next_interval(index as f64 * 3.0, 0.0, None).interval_s).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn the_wave_count_is_configurable() {
        let config = TimingConfig { wave_count: 5, ..TimingConfig::default() };
        assert_eq!(WaveTimer::new(config, 9).waves().len(), 5);
        let config = TimingConfig { wave_count: 0, ..TimingConfig::default() };
        assert_eq!(WaveTimer::new(config, 9).waves().len(), 1, "never an empty model");
    }

    #[test]
    fn the_projection_matches_the_signal_and_is_labelled_a_projection() {
        let mut timer = timer(10);
        timer.advance(20.0);
        assert!((timer.project(0.0) - timer.signal()).abs() < 1e-12);
        // With drift switched off the projection is exactly what happens next.
        let config = TimingConfig { drift_sigma: 0.0, drift_reversion: 1e-9, ..TimingConfig::default() };
        let mut steady = WaveTimer::new(config, 10);
        steady.advance(20.0);
        let projected = steady.project(30.0);
        steady.advance(50.0);
        assert!((steady.signal() - projected).abs() < 1e-6);
    }

    #[test]
    fn the_baseline_scales_the_stochastic_part() {
        let mut small = timer(11);
        let mut large = timer(11);
        let a = small.next_interval(5.0, 0.0, Some(1.0));
        let b = large.next_interval(5.0, 0.0, Some(3.0));
        assert!((a.signal - b.signal).abs() < 1e-12);
        assert!(((b.interval_s - 0.4) / (a.interval_s - 0.4) - 3.0).abs() < 1e-9);
    }
}
