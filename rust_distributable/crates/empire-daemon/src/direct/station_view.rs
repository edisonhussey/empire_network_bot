//! What the Development tab shows of the station: a copy of the jobs' state,
//! taken about once a second, with no database and no network behind it. Nothing
//! secret appears: castle ids and coordinates, MIDs and timings only.

use serde::Serialize;

use super::station::{Cover, Job, Phase, StationAutomation};

#[derive(Debug, Clone, Default, Serialize)]
pub struct StationView {
    /// A support bot is chosen and running: new attacks start jobs.
    pub enabled: bool,
    pub bot: Option<String>,
    /// `open`, `quiet` (no new sequences start) or `owned` (the station is sending).
    pub gate: &'static str,
    /// The smoothed one-way delay the deadlines allow for, milliseconds.
    pub latency_ms: i64,
    /// Open jobs first, then the latest finished ones, newest first.
    pub jobs: Vec<JobView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttackView {
    pub mid: i64,
    pub attacker: Option<String>,
    pub landing_ms: i64,
    pub retreated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub job_id: i64,
    pub kingdom_id: i64,
    pub castle: [i64; 2],
    pub dest: Option<[i64; 2]>,
    /// `waiting`, `in_flight`, `away`, `uncertain` or `done`.
    pub phase: &'static str,
    /// What it is doing or waiting to do, in words.
    pub doing: String,
    /// When the next step is due, if it is waiting.
    pub next_at_ms: Option<i64>,
    pub landing_ms: i64,
    /// `D`: when the time-critical sequence starts.
    pub deadline_ms: i64,
    pub safe_landing_ms: i64,
    pub lid: Option<i64>,
    pub support_mid: Option<i64>,
    pub support_duration_s: Option<i64>,
    pub attacks: Vec<AttackView>,
    pub dcl_sent_ms: Option<i64>,
    pub sdi_sent_ms: Option<i64>,
    pub sdi_reply_ms: Option<i64>,
    pub cds_sent_ms: Option<i64>,
    pub cds_ack_ms: Option<i64>,
    pub recall_due_ms: Option<i64>,
    pub mcm_ack_ms: Option<i64>,
    pub return_eta_ms: Option<i64>,
    pub outcome: Option<String>,
    /// Milliseconds the acknowledgement came before the estimated landing.
    pub lead_ms: Option<i64>,
    pub note: String,
}

impl StationAutomation {
    pub(super) fn view(&self, now: i64) -> StationView {
        let mut jobs = self.jobs.iter().map(job_view).collect::<Vec<_>>();
        jobs.extend(self.recent.iter().rev().map(job_view));
        StationView {
            enabled: self.config_name().is_some(),
            bot: self.config_name().map(str::to_owned),
            gate: self.gate(now).name(),
            latency_ms: self.latency_ms(),
            jobs,
        }
    }
}

fn job_view(job: &Job) -> JobView {
    let (phase, doing, next_at_ms) = match job.phase {
        Phase::Waiting { step, due_ms } => ("waiting", format!("next: {}", step.label()), Some(due_ms)),
        Phase::InFlight { step, .. } => ("in_flight", step.label().to_owned(), None),
        Phase::Away => ("away", "support out; recall when the threat has passed".to_owned(), job.recall_due_ms),
        Phase::Uncertain { .. } => ("uncertain", "support send unconfirmed; not repeated".to_owned(), None),
        Phase::Done => ("done", job.outcome.clone().unwrap_or_default(), None),
    };
    JobView {
        job_id: job.job_id,
        kingdom_id: job.kingdom_id,
        castle: [job.castle.0, job.castle.1],
        dest: job.dest.map(|dest| [dest.0, dest.1]),
        phase,
        doing,
        next_at_ms,
        landing_ms: job.earliest_landing_ms(),
        deadline_ms: job.timeline.deadline_ms,
        safe_landing_ms: job.timeline.safe_landing_ms,
        lid: job.lid,
        support_mid: job.support_mid,
        support_duration_s: job.support_duration_s,
        attacks: job.covers.iter().map(attack_view).collect(),
        dcl_sent_ms: job.marks.dcl_sent,
        sdi_sent_ms: job.marks.sdi_sent,
        sdi_reply_ms: job.marks.sdi_reply,
        cds_sent_ms: job.marks.cds_sent,
        cds_ack_ms: job.marks.cds_ack,
        recall_due_ms: job.recall_due_ms,
        mcm_ack_ms: job.marks.mcm_ack,
        return_eta_ms: job.return_eta_ms,
        outcome: job.outcome.clone(),
        lead_ms: job.marks.cds_ack.map(|acked| job.earliest_landing_ms() - acked),
        note: job.note.clone(),
    }
}

fn attack_view(cover: &Cover) -> AttackView {
    AttackView {
        mid: cover.mid,
        attacker: cover.attacker.clone(),
        landing_ms: cover.landing_ms,
        retreated: cover.retreated,
    }
}
