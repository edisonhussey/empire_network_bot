//! The diagnostic snapshot behind the Development tab.
//!
//! Observational only: it is built from state the scheduler already holds, copied
//! out about once a second, and read by a plain `GET`. The tab never drives the
//! bot, and nothing here is persisted. Waveform animation is the tab's own job: it
//! gets the wave parameters and evolves them locally, so the backend does not push
//! frames. No credentials, tokens or packet payloads appear anywhere in it.

use serde::Serialize;

use empire_core::timing::WaveKind;

use super::{
    automation::Automation,
    health::{Incident, Verdict},
    scheduler::{Applied, Histogram},
    station::StationAutomation,
};

/// Upcoming waits shown ahead of the present.
const PROJECTION_STEPS: usize = 30;

#[derive(Debug, Clone, Default, Serialize)]
pub struct DevSnapshot {
    /// Epoch milliseconds the snapshot was taken.
    pub at_ms: i64,
    pub timing: TimingView,
    pub scheduler: SchedulerView,
    pub spatial: Option<SpatialView>,
    pub health: HealthView,
    /// Stationed castles, recalls and their timing (see `station`).
    pub station: super::station_view::StationView,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaveView {
    /// `sin`, `cos` or `fold`.
    pub kind: &'static str,
    /// Radians per step.
    pub omega: f64,
    pub amplitude: f64,
    pub phase: f64,
    /// The shape parameter of a `fold` wave.
    pub b: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectedView {
    pub index: u64,
    /// The wave sum at that step.
    pub signal: f64,
    /// The wait that step would give if nothing drifted and there were no noise.
    pub interval_s: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TimingView {
    /// How many waits the generator has produced.
    pub index: u64,
    /// The floor `m`, seconds: no generated wait is shorter.
    pub floor_s: f64,
    /// The test pace in force: every wait, generated or projected, is this many times the
    /// design's. 1 unless changed from the Development tab.
    pub pace: f64,
    /// No generated wait is longer than this at pace 1, seconds.
    pub max_interval_s: f64,
    pub log_offset: f64,
    pub wave_sigma: f64,
    pub noise_sigma: f64,
    pub waves: Vec<WaveView>,
    /// The wave sum at the next step.
    pub signal_now: f64,
    /// The next waits as the generator stands now. A projection, not a promise.
    pub projection: Vec<ProjectedView>,
    pub last: Option<Applied>,
    /// Waits actually used, newest first.
    pub recent: Vec<Applied>,
    /// The band the design aims about 60 % of waits into, seconds.
    pub target_band_s: [f64; 2],
    /// How often each generated wait was used, over the session.
    pub histogram: HistogramView,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HistogramView {
    /// Upper edge of each bin, seconds; there is one more count than edges, the
    /// last being everything above the final edge.
    pub edges_s: Vec<f64>,
    pub counts: Vec<u64>,
    pub total: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CompletedView {
    pub march_id: i64,
    pub x: i64,
    pub y: i64,
    pub lord_id: i64,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SchedulerView {
    /// `waiting`, `pacing_attack`, `awaiting_attack_ack`, ... as the status bar.
    pub state: String,
    pub detail: String,
    /// When the next logical action may happen, if one is scheduled.
    pub next_action_at_ms: Option<i64>,
    /// The global limit that lengthened the last wait, if any.
    pub restriction: Option<&'static str>,
    pub last_cra_ms: Option<i64>,
    pub last_completed: Option<CompletedView>,
    pub kingdom_id: Option<i64>,
    /// Eligible towers seen by the last selection pass.
    pub eligible_towers: Option<u64>,
    /// Of those, how many are near the spotlight.
    pub local_towers: Option<u64>,
    pub spotlight_has_local: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpatialView {
    pub kingdom_id: i64,
    pub castle: [i64; 2],
    pub spotlight: [f64; 2],
    pub velocity: [f64; 2],
    pub selected: Option<[i64; 2]>,
    pub local_radius: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HealthView {
    pub tolerance: usize,
    pub count_last_hour: usize,
    pub paused: bool,
    pub paused_at_ms: Option<i64>,
    /// Newest first; the qualifying ones count towards the tolerance.
    pub incidents: Vec<Incident>,
    /// What the latest qualifying incident meant for the run.
    pub last_verdict: Option<Verdict>,
}

impl HistogramView {
    fn from_histogram(histogram: &Histogram) -> Self {
        let (edges_s, counts) = histogram.edges_and_counts();
        Self { edges_s, counts, total: histogram.total() }
    }
}

/// The whole snapshot: the scheduler's, plus the station's jobs and the frequency
/// of every human-paced wait either of them used.
pub(super) fn snapshot(automation: &mut Automation, station: &StationAutomation, now_ms: i64) -> DevSnapshot {
    let mut snapshot = automation.dev_snapshot(now_ms);
    snapshot.timing.histogram =
        HistogramView::from_histogram(&automation.scheduler.histogram().merged(station.histogram()));
    snapshot.station = station.view(now_ms);
    snapshot
}

impl Automation {
    /// Copy the current diagnostic state out. Cheap: it touches only in-memory
    /// structures, no database and no network.
    pub(super) fn dev_snapshot(&mut self, now_ms: i64) -> DevSnapshot {
        let timer = self.scheduler.timer();
        let (state, detail) = self.status();
        let focus = self.focus.clone();
        let selector = focus.as_ref().and_then(|focus| self.selectors.get(&focus.task_id));
        let spatial = focus.as_ref().zip(selector).map(|(focus, selector)| SpatialView {
            kingdom_id: focus.kingdom_id,
            castle: [focus.castle.0, focus.castle.1],
            spotlight: [selector.state.spotlight.0, selector.state.spotlight.1],
            velocity: [selector.state.velocity.0, selector.state.velocity.1],
            selected: self.selected.map(|(x, y)| [x, y]),
            local_radius: selector.params.local_radius,
        });
        let scan = self.last_scan;
        let last = self.scheduler.last().cloned();
        DevSnapshot {
            at_ms: now_ms,
            timing: TimingView {
                index: timer.index(),
                floor_s: timer.floor_s(),
                pace: timer.pace(),
                max_interval_s: timer.config().max_interval_s,
                log_offset: timer.config().log_offset,
                wave_sigma: timer.config().wave_sigma,
                noise_sigma: timer.config().noise_sigma,
                waves: timer
                    .waves()
                    .iter()
                    .map(|wave| WaveView {
                        kind: wave.kind.name(),
                        omega: wave.omega,
                        amplitude: wave.amplitude,
                        phase: wave.phase,
                        b: match wave.kind {
                            WaveKind::Fold { b } => Some(b),
                            _ => None,
                        },
                    })
                    .collect(),
                signal_now: timer.signal(),
                projection: self
                    .scheduler
                    .project(PROJECTION_STEPS)
                    .into_iter()
                    .map(|value| ProjectedView {
                        index: value.index,
                        signal: value.signal,
                        interval_s: value.interval_s,
                    })
                    .collect(),
                last: last.clone(),
                recent: self.scheduler.recent().cloned().collect(),
                target_band_s: [1.0, 5.0],
                histogram: HistogramView::default(),
            },
            scheduler: SchedulerView {
                state: state.to_owned(),
                detail,
                next_action_at_ms: self.scheduler.next_action_at_ms(),
                restriction: self.scheduler.restriction(),
                last_cra_ms: self.last_cra_ms,
                last_completed: self.last_completed.clone(),
                kingdom_id: focus.as_ref().map(|focus| focus.kingdom_id),
                eligible_towers: scan.map(|scan| scan.eligible),
                local_towers: scan.map(|scan| scan.local),
                spotlight_has_local: scan.map(|scan| scan.local > 0),
            },
            spatial,
            health: HealthView {
                tolerance: self.health.tolerance(),
                count_last_hour: self.health.count(now_ms),
                paused: self.health.paused(),
                paused_at_ms: self.health.paused_at_ms(),
                incidents: self.health.recent().take(20).cloned().collect(),
                last_verdict: self.last_verdict,
            },
            station: Default::default(),
        }
    }
}
