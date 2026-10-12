//! The test pace of one session's human-paced waits.
//!
//! A single number shared between the HTTP handler that sets it and the session loop that
//! applies it, so neither waits on the other. It scales the attack scheduler's generated
//! waits (`WaveTimer::set_pace`); the station's waits are deadline driven and are never
//! scaled. Held in memory only: a session starts at 1, the design pace, and the value
//! lasts as long as the session's cells (it survives a reconnect, not a restart).

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use empire_core::timing::{PACE_MAX, PACE_MIN};

#[derive(Debug, Clone)]
pub struct PaceControl(Arc<AtomicU64>);

impl Default for PaceControl {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(1.0_f64.to_bits())))
    }
}

impl PaceControl {
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// Set the pace, bounded to what the generator allows; returns the value kept.
    pub fn set(&self, pace: f64) -> f64 {
        let kept = if pace.is_finite() { pace.clamp(PACE_MIN, PACE_MAX) } else { 1.0 };
        self.0.store(kept.to_bits(), Ordering::Relaxed);
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_at_the_design_pace_and_is_shared_and_bounded() {
        let pace = PaceControl::default();
        assert_eq!(pace.get(), 1.0);
        let other = pace.clone();
        assert_eq!(other.set(2.5), 2.5);
        assert_eq!(pace.get(), 2.5, "one value for every handle");
        assert_eq!(pace.set(0.0), PACE_MIN);
        assert_eq!(pace.set(99.0), PACE_MAX);
        assert_eq!(pace.set(f64::NAN), 1.0);
    }
}
