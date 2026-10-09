//! The diagnostic snapshot behind the Development tab.
//!
//! Observational only: it is built from state the scheduler already holds, copied
//! out about once a second, and read by a plain `GET`. The tab never drives the
//! bot, and nothing here is persisted. Waveform animation is the tab's own job: it
//! gets the wave parameters and evolves them locally, so the backend does not push
//! frames. No credentials, tokens or packet payloads appear anywhere in it.

use serde::Serialize;

use super::{
    automation::Automation,
    health::{Incident, Verdict},
    scheduler::Applied,
};

#[derive(Debug, Clone, Default, Serialize)]
pub struct DevSnapshot {
    /// Epoch milliseconds the snapshot was taken.
    pub at_ms: i64,
    pub timing: TimingView,
    pub scheduler: SchedulerView,
    pub spatial: Option<SpatialView>,
    pub health: HealthView,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WaveView {
    pub frequency_hz: f64,
    pub amplitude: f64,
    /// Phase at `DevSnapshot::at_ms`, radians.
    pub phase: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TimingView {
    pub waves: Vec<WaveView>,
    pub noise_sigma: f64,
    /// The stochastic lower bound `D_min`, seconds.
    pub min_interval_s: f64,
    /// The baseline scale `D0` used for the shown interval, seconds.
    pub baseline_s: f64,
    /// The deterministic wave sum now.
    pub signal: f64,
    /// Recent signal values as `[epoch_ms, value]`, oldest first.
    pub history: Vec<(i64, f64)>,
    pub last: Option<Applied>,
    pub recent: Vec<Applied>,
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

impl Automation {
    /// Copy the current diagnostic state out. Cheap: it touches only in-memory
    /// structures, no database and no network.
    pub(super) fn dev_snapshot(&mut self, now_ms: i64) -> DevSnapshot {
        self.scheduler.tick(now_ms);
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
        let baseline = timer.config().baseline_s;
        let last = self.scheduler.last().cloned();
        DevSnapshot {
            at_ms: now_ms,
            timing: TimingView {
                waves: timer
                    .waves()
                    .iter()
                    .map(|wave| WaveView {
                        frequency_hz: wave.frequency,
                        amplitude: wave.amplitude,
                        phase: wave.phase,
                    })
                    .collect(),
                noise_sigma: timer.config().noise_sigma,
                min_interval_s: timer.config().min_interval_s,
                baseline_s: baseline,
                signal: timer.signal(),
                history: self.scheduler.history().copied().collect(),
                last: last.clone(),
                recent: self.scheduler.recent().cloned().collect(),
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
        }
    }
}
