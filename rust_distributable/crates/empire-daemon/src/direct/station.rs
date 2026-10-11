//! Auto-station: when an owned castle is attacked, send its troops out as support
//! just before the landing and call them back once the danger has passed.
//!
//! The rules (what counts as a threat, how steps are spaced, which troops go, which
//! commander is lent) are pure functions in `empire_core::station`. This module is
//! the part that talks to the connection: a state machine per stationed castle,
//! driven by [`StationAutomation::next_packet`] (about every 50 ms) and
//! [`StationAutomation::observe`] (every inbound frame). Every method takes the
//! clock as an argument, so the timing is tested against a fake one.
//!
//! How it shares the connection with the rest of the bot (details in
//! `docs/architecture/station.md`):
//!
//! * **Gate.** From a margin before a job's first step until its `cds` is
//!   acknowledged, [`Gate::Owned`]: nothing else sends. Within [`QUIET_LEAD_MS`] of
//!   that, [`Gate::Quiet`]: sequences already running may finish, none may start.
//!   Held work is never abandoned mid-request; a held `cra` is released safely if the
//!   troops it needed have gone.
//! * **Castle hold.** Once support has left a castle, no attack may be sent from it
//!   until the troops are home again ([`StationAutomation::holds`]). A `cra` for troops
//!   that are away is refused by the server, and repeated refusals lock the account.
//! * **Commander.** One real LID is leased well before `D`, preferring one no running
//!   task uses, and revalidated just before `cds`.
//! * **Single flight.** One station request is in flight at a time, earliest landing
//!   first. A silent request is never repeated if repeating could duplicate it:
//!   `cds` and `mcm` timeouts are *uncertain*, recorded and not resent.

use std::collections::{BTreeMap, HashSet, VecDeque};

use empire_core::{
    protocol::XtPacket,
    pvp::{AllianceIdentity, seen_movements},
    station::{
        self, CdsAck, DclReply, PrepPlan, Threat, Timeline, cds_packet, cds_think_ms, dcl_packet,
        mcm_packet, parse_cds_ack, parse_dcl, parse_mcm_ack, parse_sdi, plan_prep,
        recall_delay_ms, sdi_packet, select_support_troops, threat_from,
    },
    store::{ActiveStation, JOB_DONE, StationJobRow},
};

use super::*;

/// How close to its first step a job takes the connection over.
const OWN_LEAD_MS: i64 = 1_500;
/// How far ahead of that new sequences are no longer started.
pub(super) const QUIET_LEAD_MS: i64 = 45_000;
/// Replies that matter inside the window are given little time: a silent request
/// there is lost, not slow.
const READ_TIMEOUT_MS: i64 = 2_500;
const SDI_TIMEOUT_MS: i64 = 3_000;
const CDS_TIMEOUT_MS: i64 = 4_000;
const MCM_TIMEOUT_MS: i64 = 5_000;
/// Less than this before the landing and there is nothing left to do.
const MIN_LEAD_MS: i64 = 1_500;
/// A support march that never answered is given this long to be found again.
const UNCERTAIN_HOLD_MS: i64 = 30 * 60_000;
/// How long the lent commander stays reserved before the support march has
/// answered: past any plausible landing and recall.
const LID_RESERVE_MS: i64 = 15 * 60_000;
/// Rest added after the troops are home before the commander is reused.
const RETURN_REST_MS: i64 = 3_000;
const CONFIG_REFRESH_MS: i64 = 2_000;
const RECENT_LEN: usize = 24;

/// What the rest of the bot may do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Gate {
    Open,
    /// A job is close: running sequences may finish, new ones do not start.
    Quiet,
    /// A job is in its window: nothing else sends.
    Owned,
}

impl Gate {
    pub(super) fn name(self) -> &'static str {
        match self {
            Gate::Open => "open",
            Gate::Quiet => "quiet",
            Gate::Owned => "owned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Step {
    Context,
    Dcl,
    Sdi,
    Cds,
    Mcm,
}

impl Step {
    pub(super) fn label(self) -> &'static str {
        match self {
            Step::Context => "opening the map",
            Step::Dcl => "reading castle stock (dcl)",
            Step::Sdi => "opening the support page (sdi)",
            Step::Cds => "sending support (cds)",
            Step::Mcm => "recalling support (mcm)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    /// The next request leaves at `due_ms`.
    Waiting { step: Step, due_ms: i64 },
    /// A request is out; `deadline_ms` is when its silence counts as a loss.
    InFlight { step: Step, sent_ms: i64, deadline_ms: i64 },
    /// The support march is confirmed and out; recalled at `recall_due_ms`.
    Away,
    /// The `cds` was sent and never answered. Not repeated: reconciled against a
    /// matching movement or left for the operator.
    Uncertain { since_ms: i64 },
    Done,
}

/// One attack a job covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Cover {
    pub(super) mid: i64,
    pub(super) landing_ms: i64,
    pub(super) attacker: Option<String>,
    pub(super) retreated: bool,
}

/// What was measured, for judging the feature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Marks {
    pub(super) context_sent: Option<i64>,
    pub(super) dcl_sent: Option<i64>,
    pub(super) dcl_reply: Option<i64>,
    pub(super) sdi_sent: Option<i64>,
    pub(super) sdi_reply: Option<i64>,
    pub(super) cds_sent: Option<i64>,
    pub(super) cds_ack: Option<i64>,
    pub(super) mcm_sent: Option<i64>,
    pub(super) mcm_ack: Option<i64>,
}

/// One stationed castle, from detection to recall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Job {
    pub(super) job_id: i64,
    pub(super) kingdom_id: i64,
    pub(super) castle_id: i64,
    pub(super) castle: (i64, i64),
    pub(super) dest: Option<(i64, i64)>,
    pub(super) lid: Option<i64>,
    pub(super) covers: Vec<Cover>,
    pub(super) detected_at_ms: i64,
    pub(super) timeline: Timeline,
    pub(super) plan: Option<PrepPlan>,
    pub(super) phase: Phase,
    /// The cross-check stock from `dcl` for this castle.
    pub(super) cross_check: Option<BTreeMap<i64, i64>>,
    pub(super) troops: Vec<(i64, i64)>,
    pub(super) support_mid: Option<i64>,
    pub(super) support_duration_s: Option<i64>,
    pub(super) recall_due_ms: Option<i64>,
    /// An early retreat arrived while the `cds` was still unanswered.
    pub(super) recall_asap: bool,
    pub(super) sdi_retried: bool,
    pub(super) marks: Marks,
    pub(super) outcome: Option<String>,
    pub(super) note: String,
    /// No attack may be sent from this castle until then: the troops are away or
    /// on their way home.
    pub(super) hold_until_ms: i64,
    pub(super) return_eta_ms: Option<i64>,
}

impl Job {
    pub(super) fn earliest_landing_ms(&self) -> i64 {
        self.covers.iter().map(|cover| cover.landing_ms).min().unwrap_or(self.timeline.landing_ms)
    }

    fn latest_threat_landing(&self) -> Option<i64> {
        self.covers.iter().filter(|cover| !cover.retreated).map(|cover| cover.landing_ms).max()
    }

    /// The first moment this job needs the connection.
    fn prep_start_ms(&self) -> i64 {
        match self.phase {
            Phase::Waiting { due_ms, .. } => self
                .plan
                .map_or(due_ms, |plan| {
                    [plan.context_at_ms, plan.dcl_at_ms, Some(plan.sdi_at_ms)]
                        .into_iter()
                        .flatten()
                        .min()
                        .unwrap_or(due_ms)
                })
                .min(due_ms),
            Phase::InFlight { sent_ms, .. } => sent_ms,
            _ => i64::MAX,
        }
    }

    /// Before the support march has left: a retreat can still cancel everything.
    fn not_yet_sent(&self) -> bool {
        match self.phase {
            Phase::Waiting { step, .. } => step != Step::Mcm,
            Phase::InFlight { step, .. } => step != Step::Cds && step != Step::Mcm,
            _ => false,
        }
    }

    /// Can a new attack on this castle still be covered by this march?
    fn accepts_cover(&self) -> bool {
        match self.phase {
            Phase::Waiting { step, .. } | Phase::InFlight { step, .. } => step != Step::Mcm,
            Phase::Away | Phase::Uncertain { .. } => true,
            Phase::Done => false,
        }
    }

    fn is_done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }
}

pub(super) struct StationAutomation {
    pub(super) jobs: Vec<Job>,
    /// Finished jobs, newest last: kept for the view and for castle holds.
    pub(super) recent: VecDeque<Job>,
    scheduler: ActionScheduler,
    /// Attack MIDs already handled, so a repeated `gam` never moves the first
    /// estimate or makes a second job.
    seen: HashSet<i64>,
    config: Option<ActiveStation>,
    config_checked_ms: i64,
    /// One-way transport delay, smoothed from the round trips measured here.
    latency_ms: i64,
}

impl StationAutomation {
    pub(super) fn new() -> Self {
        Self {
            jobs: Vec::new(),
            recent: VecDeque::new(),
            scheduler: ActionScheduler::new(Rng::from_entropy().next_u64()),
            seen: HashSet::new(),
            config: None,
            config_checked_ms: i64::MIN / 2,
            latency_ms: station::DEFAULT_LATENCY_MS,
        }
    }

    pub(super) fn histogram(&self) -> &scheduler::Histogram {
        self.scheduler.histogram()
    }

    pub(super) fn latency_ms(&self) -> i64 {
        self.latency_ms
    }

    pub(super) fn config_name(&self) -> Option<&str> {
        self.config.as_ref().map(|config| config.name.as_str())
    }

    /// Resume what an earlier session left open. A job that had not sent its
    /// support yet and whose landing has passed is closed as missed; one with
    /// support out carries on to its recall; an unanswered send stays uncertain.
    pub(super) async fn restore(&mut self, store: &Store, account_id: &str, now: i64) {
        let _ = store.prune_station_jobs(now).await;
        let rows = store.recent_station_jobs(account_id, 60).await.unwrap_or_default();
        for row in rows {
            let Ok(mut job) = serde_json::from_str::<Job>(&row.state_json) else { continue };
            job.job_id = row.job_id;
            for cover in &job.covers {
                self.seen.insert(cover.mid);
            }
            if job.is_done() {
                if job.hold_until_ms > now {
                    self.recent.push_front(job);
                }
                continue;
            }
            match job.phase {
                Phase::Waiting { .. } | Phase::InFlight { step: Step::Context | Step::Dcl | Step::Sdi, .. } => {
                    if now >= job.earliest_landing_ms() {
                        self.finish(store, account_id, &mut job, "missed:session_gap", true, now).await;
                    } else {
                        // Nothing irreversible was sent: plan it again from now.
                        job.timeline = Timeline::new(job.earliest_landing_ms(), self.latency_ms);
                        self.plan(store, account_id, &mut job, now).await;
                    }
                }
                Phase::InFlight { step: Step::Cds, .. } => {
                    job.phase = Phase::Uncertain { since_ms: now };
                    job.note = "the connection ended while the support was being sent".to_owned();
                }
                Phase::InFlight { step: Step::Mcm, .. } => {
                    self.finish(store, account_id, &mut job, "saved:recall_uncertain", false, now).await;
                }
                _ => {}
            }
            if job.is_done() {
                self.recent.push_front(job);
            } else {
                self.save(store, account_id, &mut job, now).await;
                self.jobs.push(job);
            }
        }
        self.recent.truncate(RECENT_LEN);
    }

    // ---- gating -------------------------------------------------------------

    pub(super) fn gate(&self, now: i64) -> Gate {
        let mut gate = Gate::Open;
        for job in &self.jobs {
            if matches!(job.phase, Phase::InFlight { step, .. } if step != Step::Mcm) {
                return Gate::Owned;
            }
            if !job.not_yet_sent() {
                continue;
            }
            let start = job.prep_start_ms();
            if now + OWN_LEAD_MS >= start {
                return Gate::Owned;
            }
            if now + QUIET_LEAD_MS >= start {
                gate = Gate::Quiet;
            }
        }
        gate
    }

    /// Castles that may not send an attack, as `(kingdom, x, y)`: the support
    /// march is out (or its troops are on the way back).
    pub(super) fn holds(&self, now: i64) -> Vec<(i64, i64, i64)> {
        self.jobs
            .iter()
            .chain(self.recent.iter())
            .filter(|job| job.hold_until_ms > now)
            .map(|job| (job.kingdom_id, job.castle.0, job.castle.1))
            .collect()
    }

    // ---- configuration -------------------------------------------------------

    async fn refresh_config(&mut self, store: &Store, account_id: &str, now: i64) {
        if now - self.config_checked_ms < CONFIG_REFRESH_MS {
            return;
        }
        self.config_checked_ms = now;
        // A failed read keeps what was known: a database hiccup must not look like
        // the operator stopping the station.
        if let Ok(config) = store.active_station(account_id).await {
            self.config = config;
        }
    }

    /// The operator ended the station: jobs whose support has not left yet are
    /// cancelled and their commanders freed. A support already out is still recalled.
    async fn cancel_unsent_if_stopped(&mut self, store: &Store, account_id: &str, now: i64) {
        if self.config.is_some() {
            return;
        }
        while let Some(index) = self
            .jobs
            .iter()
            .position(|job| job.not_yet_sent() && !matches!(job.phase, Phase::InFlight { .. }))
        {
            self.jobs[index].note = "the station bot was stopped".to_owned();
            self.finish_at(store, account_id, index, "cancelled:stopped", true, now).await;
        }
    }

    fn draw(&mut self, now: i64) -> f64 {
        let sample = self.scheduler.interval(Wait::Station);
        self.scheduler.applied(
            Wait::Station,
            now,
            sample,
            Some("normalised into the station window"),
        );
        sample
    }

    // ---- persistence ----------------------------------------------------------

    async fn save(&mut self, store: &Store, account_id: &str, job: &mut Job, now: i64) {
        let phase = if job.is_done() { JOB_DONE.to_owned() } else { phase_name(&job.phase).to_owned() };
        let row = StationJobRow {
            job_id: job.job_id,
            account_id: account_id.to_owned(),
            kingdom_id: job.kingdom_id,
            castle_id: job.castle_id,
            castle_x: job.castle.0,
            castle_y: job.castle.1,
            dest: job.dest,
            lid: job.lid,
            phase,
            outcome: job.outcome.clone(),
            support_mid: job.support_mid,
            landing_ms: job.earliest_landing_ms(),
            detected_at_ms: job.detected_at_ms,
            cds_acked_at_ms: job.marks.cds_ack,
            recalled_at_ms: job.marks.mcm_ack,
            lead_ms: job.marks.cds_ack.map(|acked| job.earliest_landing_ms() - acked),
            state_json: serde_json::to_string(job).unwrap_or_else(|_| "{}".to_owned()),
            updated_at_ms: now,
        };
        match store.save_station_job(&row).await {
            Ok(id) => job.job_id = id,
            Err(error) => warn!(%error, %account_id, "station job could not be saved"),
        }
    }

    /// Close a job. `release_lid` hands the commander back at once; when support
    /// has gone the commander is released by its own return time instead.
    async fn finish(
        &mut self,
        store: &Store,
        account_id: &str,
        job: &mut Job,
        outcome: &str,
        release_lid: bool,
        now: i64,
    ) {
        job.outcome = Some(outcome.to_owned());
        job.phase = Phase::Done;
        if release_lid && let Some(lid) = job.lid {
            release_commander(store, account_id, lid, now).await;
        }
        log_event(
            store,
            account_id,
            "station.job",
            &format!(
                "castle {} ({}:{}:{}): {outcome}{}",
                job.castle_id,
                job.kingdom_id,
                job.castle.0,
                job.castle.1,
                if job.note.is_empty() { String::new() } else { format!(" - {}", job.note) }
            ),
        )
        .await;
        self.save(store, account_id, job, now).await;
    }

    async fn finish_at(&mut self, store: &Store, account_id: &str, index: usize, outcome: &str, release_lid: bool, now: i64) {
        let mut job = self.jobs.remove(index);
        self.finish(store, account_id, &mut job, outcome, release_lid, now).await;
        self.recent.push_back(job);
        while self.recent.len() > RECENT_LEN {
            self.recent.pop_front();
        }
    }

    // ---- inbound ---------------------------------------------------------------

    pub(super) async fn observe(&mut self, store: &Store, account_id: &str, raw: &str, now: i64) {
        // Most frames are none of this module's business and some are large map
        // windows: look at the command before parsing anything.
        let Some(command) = peek_command(raw) else { return };
        let wanted = match command {
            "gam" | "mrm" | "dcl" | "sdi" | "cds" | "mcm" => true,
            // Only the map reply to a context step this module is waiting for.
            "gaa" => self.in_flight(Step::Context).is_some(),
            _ => false,
        };
        if !wanted {
            return;
        }
        let Ok(packet) = parse_xt_packet(raw) else { return };
        match packet.command.as_str() {
            "gam" => self.on_gam(store, account_id, &packet, now).await,
            "mrm" if packet.status.as_deref().is_none_or(|status| status == "0") => {
                self.on_retreat(store, account_id, &packet, now).await;
            }
            "gaa" | "dcl" | "sdi" | "cds" | "mcm" => self.on_reply(store, account_id, &packet, now).await,
            _ => {}
        }
    }

    async fn on_gam(&mut self, store: &Store, account_id: &str, packet: &XtPacket, now: i64) {
        if packet.status.as_deref().is_some_and(|status| status != "0") {
            return;
        }
        self.refresh_config(store, account_id, now).await;
        let movements = seen_movements(&packet.payload);
        if movements.is_empty() {
            return;
        }
        let Ok(Some(me)) = store.alliance_identity(account_id).await else { return };
        let castles = store.owned_castle_tuples(account_id).await.unwrap_or_default();
        for movement in &movements {
            self.adopt_support(store, account_id, &me, movement, now).await;
            if let Some(threat) = threat_from(movement, &me, &castles, now) {
                self.on_threat(store, account_id, threat, now).await;
            }
        }
    }

    /// A movement of ours that matches a support send nobody confirmed: take its MID.
    async fn adopt_support(
        &mut self,
        store: &Store,
        account_id: &str,
        me: &AllianceIdentity,
        movement: &empire_core::pvp::SeenMovement,
        now: i64,
    ) {
        if movement.sender.player_id != Some(me.player_id) {
            return;
        }
        let Some(index) = self.jobs.iter().position(|job| {
            matches!(job.phase, Phase::Uncertain { .. })
                && movement.sender.area_id == Some(job.castle_id)
                && job.dest == Some((movement.target.x, movement.target.y))
        }) else {
            return;
        };
        let mut job = self.jobs.remove(index);
        job.support_mid = Some(movement.mid);
        job.support_duration_s = Some(movement.duration_s);
        job.marks.cds_ack = Some(now);
        job.note = "support march found among the account's movements".to_owned();
        self.enter_away(store, account_id, &mut job, now).await;
        self.jobs.push(job);
    }

    async fn on_threat(&mut self, store: &Store, account_id: &str, threat: Threat, now: i64) {
        if !self.seen.insert(threat.mid) {
            return;
        }
        let cover = Cover {
            mid: threat.mid,
            landing_ms: threat.landing_ms,
            attacker: threat.attacker.clone(),
            retreated: false,
        };
        let Some(config) = self.config.clone() else {
            // No running station: the attack is noted as seen and nothing is done.
            return;
        };
        // One support march covers every attack on the castle while it is away.
        if let Some(index) = self
            .jobs
            .iter()
            .position(|job| job.castle_id == threat.castle_id && job.accepts_cover())
        {
            self.add_cover(store, account_id, index, cover, now).await;
            return;
        }
        let latency = self.latency_ms;
        let mut job = Job {
            job_id: 0,
            kingdom_id: threat.kingdom_id,
            castle_id: threat.castle_id,
            castle: (threat.castle_x, threat.castle_y),
            dest: config.destinations.get(&threat.kingdom_id).copied(),
            lid: None,
            covers: vec![cover],
            detected_at_ms: now,
            timeline: Timeline::new(threat.landing_ms, latency),
            plan: None,
            phase: Phase::Waiting { step: Step::Dcl, due_ms: now },
            cross_check: None,
            troops: Vec::new(),
            support_mid: None,
            support_duration_s: None,
            recall_due_ms: None,
            recall_asap: false,
            sdi_retried: false,
            marks: Marks::default(),
            outcome: None,
            note: String::new(),
            hold_until_ms: 0,
            return_eta_ms: None,
        };
        if job.dest.is_none() {
            job.note = format!("the support bot has no destination for kingdom {}", threat.kingdom_id);
            self.finish(store, account_id, &mut job, "blocked:no_destination", false, now).await;
            self.push_recent(job);
            return;
        }
        if threat.landing_ms - now < MIN_LEAD_MS {
            job.note = format!("seen {} ms before the landing", threat.landing_ms - now);
            self.finish(store, account_id, &mut job, "late:detected", false, now).await;
            self.push_recent(job);
            return;
        }
        match self.lease_commander(store, account_id, &job, now).await {
            Some(lid) => job.lid = Some(lid),
            None => {
                job.note = "no commander is free or back in time".to_owned();
                self.finish(store, account_id, &mut job, "blocked:no_commander", false, now).await;
                self.push_recent(job);
                return;
            }
        }
        self.plan(store, account_id, &mut job, now).await;
        self.save(store, account_id, &mut job, now).await;
        self.jobs.push(job);
    }

    fn push_recent(&mut self, job: Job) {
        self.recent.push_back(job);
        while self.recent.len() > RECENT_LEN {
            self.recent.pop_front();
        }
    }

    async fn add_cover(&mut self, store: &Store, account_id: &str, index: usize, cover: Cover, now: i64) {
        let delay = recall_delay_ms(self.draw(now));
        let mut job = self.jobs.remove(index);
        let earlier = cover.landing_ms < job.earliest_landing_ms();
        job.covers.push(cover);
        match job.phase {
            // Nothing sent yet: an earlier landing moves the whole plan.
            _ if job.not_yet_sent() && earlier => {
                job.timeline = Timeline::new(job.earliest_landing_ms(), self.latency_ms);
                if matches!(job.phase, Phase::Waiting { .. }) {
                    self.plan(store, account_id, &mut job, now).await;
                }
            }
            Phase::Away | Phase::Uncertain { .. } => {
                let landing = job.latest_threat_landing().unwrap_or(now);
                job.recall_due_ms = Some(job.recall_due_ms.unwrap_or(0).max(landing + delay));
                job.hold_until_ms = job.hold_until_ms.max(landing + delay + 60_000);
            }
            _ => {}
        }
        self.save(store, account_id, &mut job, now).await;
        self.jobs.insert(index.min(self.jobs.len()), job);
    }

    /// Lay the preparation out against the timeline and set the first step.
    async fn plan(&mut self, store: &Store, account_id: &str, job: &mut Job, now: i64) {
        let navigation = store.navigation(account_id).await.ok().flatten();
        let needs_context = !navigation
            .as_ref()
            .is_some_and(|state| state.map_mode && state.current_kingdom_id == Some(job.kingdom_id));
        let samples = [self.draw(now), self.draw(now)];
        let plan = plan_prep(now, &job.timeline, needs_context, samples);
        job.phase = match (plan.context_at_ms, plan.dcl_at_ms) {
            (Some(due), _) => Phase::Waiting { step: Step::Context, due_ms: due },
            (None, Some(due)) => Phase::Waiting { step: Step::Dcl, due_ms: due },
            (None, None) => Phase::Waiting { step: Step::Sdi, due_ms: plan.sdi_at_ms },
        };
        if let Some(degraded) = plan.degraded {
            job.note = degraded.reason().to_owned();
        }
        job.plan = Some(plan);
    }

    // ---- commander --------------------------------------------------------------

    async fn lease_commander(&self, store: &Store, account_id: &str, job: &Job, now: i64) -> Option<i64> {
        let roster = store
            .commander_lids(account_id)
            .await
            .ok()?
            .into_iter()
            .filter(|lid| empire_core::hunt::USABLE_COMMANDER_LIDS.contains(lid))
            .collect::<Vec<_>>();
        let tasks = store.active_mode_tasks(account_id).await.unwrap_or_default();
        let task_sets = tasks.iter().map(|task| task.commander_lids.clone()).collect::<Vec<_>>();
        let states = store
            .commander_states(account_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|state| station::LidState { lid: state.lord_id, available_after_ms: state.available_after_ms })
            .collect::<Vec<_>>();
        // A commander busy now is acceptable only if it is back well before D.
        let ready_by = job.timeline.deadline_ms - 2_000;
        let lid = station::choose_support_lid(&roster, &task_sets, &states, now, ready_by)?;
        let state = CommanderState {
            account_id: account_id.to_owned(),
            lord_id: lid,
            status: COMMANDER_OUTBOUND.to_owned(),
            available_after_ms: now.max(ready_by) + LID_RESERVE_MS,
            march_id: None,
            target_key: Some(station_key(job.castle_id)),
        };
        store.set_commander_state(&state, now).await.ok()?;
        Some(lid)
    }

    /// Is the leased commander still ours just before `cds`? Another sequence may
    /// have written to the ledger since the lease; if so, lease again or give up.
    async fn revalidate_lid(&mut self, store: &Store, account_id: &str, job: &Job, now: i64) -> Option<i64> {
        let lid = job.lid?;
        let states = store.commander_states(account_id).await.ok()?;
        let still_ours = states
            .iter()
            .any(|state| state.lord_id == lid && state.target_key.as_deref() == Some(&station_key(job.castle_id)));
        if still_ours {
            return Some(lid);
        }
        self.lease_commander(store, account_id, job, now).await
    }

    // ---- replies -------------------------------------------------------------------

    fn in_flight(&self, step: Step) -> Option<usize> {
        self.jobs
            .iter()
            .position(|job| matches!(job.phase, Phase::InFlight { step: s, .. } if s == step))
    }

    fn measure(&mut self, sent_ms: i64, now: i64) {
        let one_way = ((now - sent_ms) / 2).clamp(20, 2_000);
        self.latency_ms = (self.latency_ms * 3 + one_way) / 4;
    }

    async fn on_reply(&mut self, store: &Store, account_id: &str, packet: &XtPacket, now: i64) {
        let refused = packet.status.as_deref().filter(|status| *status != "0").map(str::to_owned);
        match packet.command.as_str() {
            "gaa" => {
                // The context step: any map reply for the kingdom the job asked for.
                let kingdom = packet.payload.get("KID").and_then(Value::as_i64);
                if let Some(index) = self.in_flight(Step::Context)
                    && kingdom == Some(self.jobs[index].kingdom_id)
                {
                    self.after_context(store, account_id, index, now).await;
                }
            }
            "dcl" => self.on_dcl(store, account_id, packet, refused, now).await,
            "sdi" => self.on_sdi(store, account_id, packet, refused, now).await,
            "cds" => self.on_cds(store, account_id, packet, refused, now).await,
            "mcm" => self.on_mcm(store, account_id, packet, refused, now).await,
            _ => {}
        }
    }

    async fn after_context(&mut self, store: &Store, account_id: &str, index: usize, now: i64) {
        let mut job = self.jobs.remove(index);
        if let Phase::InFlight { sent_ms, .. } = job.phase {
            self.measure(sent_ms, now);
        }
        let due = job.plan.and_then(|plan| plan.dcl_at_ms).unwrap_or(now).max(now);
        job.phase = Phase::Waiting { step: Step::Dcl, due_ms: due };
        self.save(store, account_id, &mut job, now).await;
        self.jobs.insert(index, job);
    }

    async fn on_dcl(&mut self, store: &Store, account_id: &str, packet: &XtPacket, refused: Option<String>, now: i64) {
        let reply = if refused.is_none() { parse_dcl(&packet.payload) } else { None };
        // Whoever asked, the reading is current: keep it for the Overview.
        if let Some(reply) = &reply {
            let _ = store.record_castle_inventory(account_id, &reply.castles, now).await;
        }
        let Some(index) = self.in_flight(Step::Dcl) else { return };
        let mut job = self.jobs.remove(index);
        if let Phase::InFlight { sent_ms, .. } = job.phase {
            self.measure(sent_ms, now);
        }
        job.marks.dcl_reply = Some(now);
        if let Some(reply) = &reply {
            let me = store.alliance_identity(account_id).await.ok().flatten();
            if me.as_ref().is_some_and(|me| me.player_id != reply.player_id) {
                job.note = "dcl belongs to another player than the account".to_owned();
                self.finish(store, account_id, &mut job, "blocked:identity", true, now).await;
                self.push_recent(job);
                return;
            }
            job.cross_check = castle_units(reply, &job);
        } else {
            job.note = "no dcl cross-check; sdi stands alone".to_owned();
        }
        let due = job.plan.map_or(now, |plan| plan.sdi_at_ms).max(now);
        job.phase = Phase::Waiting { step: Step::Sdi, due_ms: due };
        self.save(store, account_id, &mut job, now).await;
        self.jobs.insert(index, job);
    }

    async fn on_sdi(&mut self, store: &Store, account_id: &str, packet: &XtPacket, refused: Option<String>, now: i64) {
        let Some(index) = self.in_flight(Step::Sdi) else { return };
        if let Some(status) = refused {
            self.finish_at(store, account_id, index, &format!("refused:sdi:{status}"), true, now).await;
            return;
        }
        let mut job = self.jobs.remove(index);
        if let Phase::InFlight { sent_ms, .. } = job.phase {
            self.measure(sent_ms, now);
        }
        job.marks.sdi_reply = Some(now);
        let Some(reply) = parse_sdi(&packet.payload) else {
            job.note = "the support page came back empty".to_owned();
            self.finish(store, account_id, &mut job, "blocked:empty_sdi", true, now).await;
            self.push_recent(job);
            return;
        };
        if reply.castle_id != job.castle_id {
            job.note = format!("the page is for castle {}", reply.castle_id);
            self.finish(store, account_id, &mut job, "blocked:wrong_castle", true, now).await;
            self.push_recent(job);
            return;
        }
        job.troops = select_support_troops(&reply.home, job.cross_check.as_ref());
        if job.troops.is_empty() {
            job.note = "no troops at home to send".to_owned();
            self.finish(store, account_id, &mut job, "no_troops", true, now).await;
            self.push_recent(job);
            return;
        }
        let last_send = job.timeline.last_cds_send_ms(self.latency_ms * 2);
        let sample = self.draw(now);
        let think = cds_think_ms(sample, now, last_send);
        job.phase = Phase::Waiting { step: Step::Cds, due_ms: now + think };
        self.save(store, account_id, &mut job, now).await;
        self.jobs.insert(index, job);
    }

    async fn on_cds(&mut self, store: &Store, account_id: &str, packet: &XtPacket, refused: Option<String>, now: i64) {
        let Some(index) = self.in_flight(Step::Cds) else { return };
        if let Some(status) = refused {
            let mut job = self.jobs.remove(index);
            job.note = format!("the server refused the support march (status {status})");
            // Nothing left the castle, so nothing needs holding.
            job.hold_until_ms = 0;
            self.finish(store, account_id, &mut job, &format!("refused:cds:{status}"), true, now).await;
            count_event(store, account_id, &format!("cds_refused_{status}"), "support refused").await;
            self.push_recent(job);
            return;
        }
        let ack = parse_cds_ack(&packet.payload);
        let mut job = self.jobs.remove(index);
        if let Phase::InFlight { sent_ms, .. } = job.phase {
            self.measure(sent_ms, now);
        }
        let Some(CdsAck { mid, duration_s, source_area_id, destination, .. }) = ack else {
            job.phase = Phase::Uncertain { since_ms: now };
            job.note = "the reply to the support send could not be read".to_owned();
            self.save(store, account_id, &mut job, now).await;
            self.jobs.push(job);
            return;
        };
        if source_area_id != job.castle_id || job.dest != Some(destination) {
            job.phase = Phase::Uncertain { since_ms: now };
            job.note = "a cds reply that does not match the pending send".to_owned();
            self.save(store, account_id, &mut job, now).await;
            self.jobs.push(job);
            return;
        }
        job.support_mid = Some(mid);
        job.support_duration_s = Some(duration_s);
        job.marks.cds_ack = Some(now);
        self.enter_away(store, account_id, &mut job, now).await;
        self.jobs.push(job);
    }

    /// The support march is confirmed: schedule its recall and judge the timing.
    async fn enter_away(&mut self, store: &Store, account_id: &str, job: &mut Job, now: i64) {
        let acked = job.marks.cds_ack.unwrap_or(now);
        let landing = job.earliest_landing_ms();
        let in_time = acked <= job.timeline.safe_landing_ms;
        job.outcome = Some(if in_time { "saved" } else { "late" }.to_owned());
        let delay = recall_delay_ms(self.draw(now));
        let threat_over = job.latest_threat_landing().map_or(now, |end| end + delay);
        job.recall_due_ms = Some(if job.recall_asap { now + delay } else { threat_over.max(now) });
        job.hold_until_ms = job.recall_due_ms.unwrap_or(now) + 60_000;
        job.phase = Phase::Away;
        if let Some(lid) = job.lid {
            // Out until recalled; the recall's answer sets the real return.
            let state = CommanderState {
                account_id: account_id.to_owned(),
                lord_id: lid,
                status: COMMANDER_OUTBOUND.to_owned(),
                available_after_ms: job.hold_until_ms + LID_RESERVE_MS,
                march_id: job.support_mid,
                target_key: Some(station_key(job.castle_id)),
            };
            let _ = store.set_commander_state(&state, now).await;
        }
        let lead = landing - acked;
        log_event(
            store,
            account_id,
            "station.support_sent",
            &format!(
                "castle {} -> {:?}: support {} acknowledged {} ms before the estimated landing ({}), {} troop types",
                job.castle_id,
                job.dest,
                job.support_mid.unwrap_or(0),
                lead,
                job.outcome.as_deref().unwrap_or("?"),
                job.troops.len()
            ),
        )
        .await;
        self.save(store, account_id, job, now).await;
    }

    async fn on_mcm(&mut self, store: &Store, account_id: &str, packet: &XtPacket, refused: Option<String>, now: i64) {
        let Some(index) = self.in_flight(Step::Mcm) else { return };
        if let Some(status) = refused {
            let mut job = self.jobs.remove(index);
            job.note = format!("the server refused the recall (status {status}); check the march in game");
            let outcome = format!("{}:recall_refused:{status}", job.outcome.clone().unwrap_or_default());
            self.finish(store, account_id, &mut job, &outcome, false, now).await;
            self.push_recent(job);
            return;
        }
        let mut job = self.jobs.remove(index);
        if let Phase::InFlight { sent_ms, .. } = job.phase {
            self.measure(sent_ms, now);
        }
        job.marks.mcm_ack = Some(now);
        match parse_mcm_ack(&packet.payload).filter(|ack| Some(ack.mid) == job.support_mid) {
            Some(ack) => {
                let home_at = now + ack.return_s() * 1_000;
                job.return_eta_ms = Some(home_at);
                job.hold_until_ms = home_at + RETURN_REST_MS;
                if let Some(lid) = job.lid {
                    // The recall does not free the commander: the troops are still
                    // walking home.
                    let state = CommanderState {
                        account_id: account_id.to_owned(),
                        lord_id: lid,
                        status: COMMANDER_OUTBOUND.to_owned(),
                        available_after_ms: home_at + RETURN_REST_MS,
                        march_id: job.support_mid,
                        target_key: Some(station_key(job.castle_id)),
                    };
                    let _ = store.set_commander_state(&state, now).await;
                }
                job.note = format!("recalled; home in about {} s", ack.return_s());
            }
            None => {
                job.note = "the recall reply did not name the support march".to_owned();
                let outcome = format!("{}:recall_unverified", job.outcome.clone().unwrap_or_default());
                job.outcome = Some(outcome);
            }
        }
        let outcome = job.outcome.clone().unwrap_or_else(|| "saved".to_owned());
        self.finish(store, account_id, &mut job, &outcome, false, now).await;
        self.push_recent(job);
    }

    // ---- retreat ----------------------------------------------------------------------

    /// An inbound `mrm`: the attack is called off, for good. It applies to any MID
    /// whoever owns it; only the jobs that cover it care.
    async fn on_retreat(&mut self, store: &Store, account_id: &str, packet: &XtPacket, now: i64) {
        let Some(mid) = empire_core::pvp::retreated_mid(&packet.payload) else { return };
        let delay = recall_delay_ms(self.draw(now));
        let mut index = 0;
        while index < self.jobs.len() {
            let Some(cover) = self.jobs[index].covers.iter_mut().find(|cover| cover.mid == mid) else {
                index += 1;
                continue;
            };
            cover.retreated = true;
            let all_called_off = self.jobs[index].covers.iter().all(|cover| cover.retreated);
            if all_called_off && self.jobs[index].not_yet_sent() {
                // Before the support left: nothing to save the troops from.
                self.jobs[index].note = "the attack was retreated before the support left".to_owned();
                self.finish_at(store, account_id, index, "cancelled:retreat", true, now).await;
                continue;
            }
            let job = &mut self.jobs[index];
            match job.phase {
                Phase::InFlight { step: Step::Cds, .. } | Phase::Uncertain { .. } => {
                    // The support march's MID is not known yet; recall once it is.
                    job.recall_asap = all_called_off;
                }
                Phase::Away => {
                    let due = if all_called_off {
                        now + delay
                    } else {
                        job.latest_threat_landing().map_or(now, |end| end + delay).max(now)
                    };
                    job.recall_due_ms = Some(due);
                    job.hold_until_ms = job.hold_until_ms.max(due + 60_000);
                }
                _ => {}
            }
            let mut job = self.jobs.remove(index);
            self.save(store, account_id, &mut job, now).await;
            self.jobs.insert(index, job);
            index += 1;
        }
    }

    // ---- outbound -----------------------------------------------------------------------

    /// The packet to send now, if any. `cra_in_flight` is the source castle of an
    /// attack whose `cra` has left and not been answered: support from that castle
    /// waits for the answer, since both draw on the same troops.
    pub(super) async fn next_packet(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
        now: i64,
        cra_in_flight: Option<(i64, i64, i64)>,
    ) -> anyhow::Result<Option<String>> {
        self.refresh_config(store, account_id, now).await;
        self.cancel_unsent_if_stopped(store, account_id, now).await;
        self.expire(store, account_id, now).await;
        // Single flight: one station request at a time.
        if self.jobs.iter().any(|job| matches!(job.phase, Phase::InFlight { .. })) {
            return Ok(None);
        }
        let Some(index) = self.due_job(now) else { return Ok(None) };
        let step = match self.jobs[index].phase {
            Phase::Waiting { step, .. } => step,
            Phase::Away => Step::Mcm,
            _ => return Ok(None),
        };
        let mut job = self.jobs.remove(index);
        let packet = self.build(store, account_id, server_header, &mut job, step, now, cra_in_flight).await;
        match packet {
            Ok(Some(packet)) => {
                self.save(store, account_id, &mut job, now).await;
                self.jobs.insert(index, job);
                Ok(Some(packet))
            }
            Ok(None) => {
                if job.is_done() {
                    self.push_recent(job);
                } else {
                    self.jobs.insert(index, job);
                }
                Ok(None)
            }
            Err(error) => {
                self.jobs.insert(index, job);
                Err(error)
            }
        }
    }

    /// The job whose next step is due, earliest landing first.
    fn due_job(&self, now: i64) -> Option<usize> {
        self.jobs
            .iter()
            .enumerate()
            .filter(|(_, job)| match job.phase {
                Phase::Waiting { due_ms, .. } => now >= due_ms,
                Phase::Away => job.recall_due_ms.is_some_and(|due| now >= due) && job.support_mid.is_some(),
                _ => false,
            })
            .min_by_key(|(_, job)| (job.earliest_landing_ms(), job.job_id))
            .map(|(index, _)| index)
    }

    #[allow(clippy::too_many_arguments)]
    async fn build(
        &mut self,
        store: &Store,
        account_id: &str,
        header: &str,
        job: &mut Job,
        step: Step,
        now: i64,
        cra_in_flight: Option<(i64, i64, i64)>,
    ) -> anyhow::Result<Option<String>> {
        let last_send = job.timeline.last_cds_send_ms(self.latency_ms * 2);
        let flight = |timeout: i64, cap: i64| Phase::InFlight {
            step,
            sent_ms: now,
            deadline_ms: (now + timeout).min(cap.max(now + 600)),
        };
        // Past the landing there is nothing to do for a step that is only worth
        // doing before it.
        if matches!(step, Step::Context | Step::Dcl | Step::Sdi | Step::Cds)
            && now + self.latency_ms >= job.timeline.landing_ms
        {
            job.note = "the window passed before the support could be sent".to_owned();
            self.finish(store, account_id, job, "missed:window", true, now).await;
            return Ok(None);
        }
        match step {
            Step::Context => {
                job.marks.context_sent = Some(now);
                job.phase = flight(READ_TIMEOUT_MS, last_send);
                Ok(Some(super::packets::tile_gaa_packet(header, job.kingdom_id, job.castle.0, job.castle.1)?))
            }
            Step::Dcl => {
                job.marks.dcl_sent = Some(now);
                job.phase = flight(READ_TIMEOUT_MS, last_send);
                Ok(Some(dcl_packet(header)?))
            }
            Step::Sdi => {
                let dest = job.dest.context("a job without a destination reached sdi")?;
                job.marks.sdi_sent = Some(now);
                job.phase = flight(SDI_TIMEOUT_MS, last_send);
                Ok(Some(sdi_packet(header, job.castle, dest)?))
            }
            Step::Cds => {
                // Both draw on the same troops: let an unanswered cra land first,
                // unless the window is nearly gone.
                if cra_in_flight == Some((job.kingdom_id, job.castle.0, job.castle.1)) && now + 400 < last_send {
                    return Ok(None);
                }
                let dest = job.dest.context("a job without a destination reached cds")?;
                let Some(lid) = self.revalidate_lid(store, account_id, job, now).await else {
                    job.note = "the leased commander is gone and none can replace it".to_owned();
                    self.finish(store, account_id, job, "blocked:no_commander", true, now).await;
                    return Ok(None);
                };
                job.lid = Some(lid);
                job.marks.cds_sent = Some(now);
                // From here the troops are spoken for; no attack may use them.
                job.hold_until_ms = job.timeline.landing_ms + 90_000;
                job.phase = flight(CDS_TIMEOUT_MS, job.timeline.landing_ms + 1_500);
                Ok(Some(cds_packet(header, job.castle_id, dest, lid, &job.troops)?))
            }
            Step::Mcm => {
                let mid = job.support_mid.context("a recall without a support MID")?;
                job.marks.mcm_sent = Some(now);
                job.phase = flight(MCM_TIMEOUT_MS, i64::MAX);
                Ok(Some(mcm_packet(header, mid)?))
            }
        }
    }

    /// Requests that went unanswered, and sends that stayed unconfirmed too long.
    async fn expire(&mut self, store: &Store, account_id: &str, now: i64) {
        // Each case below changes the job so it no longer matches, so this ends.
        while let Some((index, step)) = self.jobs.iter().enumerate().find_map(|(index, job)| match job.phase {
            Phase::InFlight { step, deadline_ms, .. } if now >= deadline_ms => Some((index, Some(step))),
            Phase::Uncertain { since_ms } if now - since_ms >= UNCERTAIN_HOLD_MS => Some((index, None)),
            _ => None,
        }) {
            match step {
                Some(step) => self.timed_out(store, account_id, index, step, now).await,
                None => {
                    self.jobs[index].note = "the support send was never confirmed".to_owned();
                    self.finish_at(store, account_id, index, "uncertain:cds_unconfirmed", false, now).await;
                }
            }
        }
    }

    async fn timed_out(&mut self, store: &Store, account_id: &str, index: usize, step: Step, now: i64) {
        match step {
            Step::Context | Step::Dcl => {
                // Optional reads: carry on without them.
                let mut job = self.jobs.remove(index);
                let (next, due) = if step == Step::Context {
                    (Step::Dcl, job.plan.and_then(|plan| plan.dcl_at_ms).unwrap_or(now).max(now))
                } else {
                    job.note = "no dcl reply; sdi stands alone".to_owned();
                    (Step::Sdi, job.plan.map_or(now, |plan| plan.sdi_at_ms).max(now))
                };
                job.phase = Phase::Waiting { step: next, due_ms: due };
                self.save(store, account_id, &mut job, now).await;
                self.jobs.insert(index, job);
            }
            Step::Sdi => {
                let job = &self.jobs[index];
                let last_send = job.timeline.last_cds_send_ms(self.latency_ms * 2);
                if !job.sdi_retried && now + 1_200 < last_send {
                    // A read: asking again cannot do harm.
                    let mut job = self.jobs.remove(index);
                    job.sdi_retried = true;
                    job.phase = Phase::Waiting { step: Step::Sdi, due_ms: now + 300 };
                    self.save(store, account_id, &mut job, now).await;
                    self.jobs.insert(index, job);
                } else {
                    self.jobs[index].note = "the support page never answered".to_owned();
                    self.finish_at(store, account_id, index, "missed:sdi_timeout", true, now).await;
                }
            }
            Step::Cds => {
                // The send may have worked. Never repeat it.
                let mut job = self.jobs.remove(index);
                job.phase = Phase::Uncertain { since_ms: now };
                job.note = "no answer to the support send; not repeated".to_owned();
                log_event(
                    store,
                    account_id,
                    "station.uncertain",
                    &format!("castle {}: no answer to cds; waiting to see the march", job.castle_id),
                )
                .await;
                self.save(store, account_id, &mut job, now).await;
                self.jobs.insert(index, job);
            }
            Step::Mcm => {
                let outcome = format!(
                    "{}:recall_uncertain",
                    self.jobs[index].outcome.clone().unwrap_or_else(|| "saved".to_owned())
                );
                self.jobs[index].note = "no answer to the recall; not repeated - check the march in game".to_owned();
                log_event(
                    store,
                    account_id,
                    "station.recall_uncertain",
                    &format!("castle {}: no answer to mcm for support {:?}", self.jobs[index].castle_id, self.jobs[index].support_mid),
                )
                .await;
                self.finish_at(store, account_id, index, &outcome, false, now).await;
            }
        }
    }
}

/// The command of an XT frame, read without parsing its payload.
fn peek_command(raw: &str) -> Option<&str> {
    let mut fields = raw.trim_start().strip_prefix("%xt%")?.splitn(3, '%');
    let first = fields.next()?;
    let command = if first.starts_with("EmpireEx_") { fields.next() } else { Some(first) };
    command.filter(|command| !command.is_empty())
}

/// The ledger tag of a lease, so a lease can be told from an attack's reservation.
fn station_key(castle_id: i64) -> String {
    format!("station:{castle_id}")
}

async fn release_commander(store: &Store, account_id: &str, lid: i64, now: i64) {
    let state = CommanderState {
        account_id: account_id.to_owned(),
        lord_id: lid,
        status: COMMANDER_AVAILABLE.to_owned(),
        available_after_ms: now,
        march_id: None,
        target_key: None,
    };
    let _ = store.set_commander_state(&state, now).await;
}

/// The `dcl` stock of this job's castle, matched by kingdom and area id.
fn castle_units(reply: &DclReply, job: &Job) -> Option<BTreeMap<i64, i64>> {
    reply
        .castles
        .iter()
        .find(|castle| castle.kingdom_id == job.kingdom_id && castle.castle_id == job.castle_id)
        .map(|castle| castle.units.clone())
}

fn phase_name(phase: &Phase) -> &'static str {
    match phase {
        Phase::Waiting { .. } => "waiting",
        Phase::InFlight { .. } => "in_flight",
        Phase::Away => "away",
        Phase::Uncertain { .. } => "uncertain",
        Phase::Done => JOB_DONE,
    }
}

#[cfg(test)]
pub(super) fn peek_command_for_tests(raw: &str) -> Option<&str> {
    peek_command(raw)
}
