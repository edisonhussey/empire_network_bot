use super::*;

/// Gap between outward map-scan requests.
///
/// The radius scan uses the same response-gated timing as fortress discovery:
/// a 0.7-second floor plus one random fractional second.
pub(super) fn base_scan_delay_seconds(rng: &mut Rng) -> f64 {
    0.7 + rng.uniform(0.0, 1.0)
}

#[derive(Debug, Clone)]
pub(super) struct PendingBaseScan {
    pub(super) frame: String,
    pub(super) kingdom_id: i64,
    pub(super) bounds: (i64, i64, i64, i64),
    pub(super) sent_at_ms: i64,
    pub(super) retries: u8,
}
