//! Manual kingdom transfers: send troops to another kingdom, optionally skip the journey.
//!
//! The window never sends game packets. It saves a transfer (`POST /v1/transfers`); this picks
//! the oldest open one up and runs it **only while the bot is off**, one step at a time:
//!
//! ```text
//! queued -> [paced pause] -> checks -> kut -> accepted -> [paced pause -> msk]* -> landed
//! ```
//!
//! Three things keep it safe and simple (design: `docs/architecture/kingdom-transfer.md`):
//!
//! * **Checks before the send.** The same [`empire_core::transfer::check`] the API ran, on what the
//!   server last told us, repeated right before the `kut`. Things that merely need a moment (stock not read
//!   yet, bot still stopping) wait; anything that would be refused fails the job without sending.
//! * **Funnels, not waiting.** What the server reports (`gcu`, `kpi`, `sce`, wherever they appear,
//!   `gbd` and the `kut`/`msk` replies included) is stored by [`TransferAutomation::observe`] as it
//!   arrives. The job reads that state; no step blocks another. A reply that never comes is
//!   reconciled by reading `kpi`, never by sending the `kut` again.
//! * **Paced like a person.** Every pause is a draw from the shared wave generator
//!   ([`Wait::Transfer`]) compressed into 1 to 12 s, and two refusals inside ten minutes stop new sends.

use super::*;
use empire_core::{
    protocol::XtPacket,
    store::{
        TRANSFER_DONE, TRANSFER_FAILED, TRANSFER_QUEUED, TRANSFER_SENT, TRANSFER_TRAVELLING,
        TransferRow,
    },
    transfer::{self, ErrorBudget, KutOutcome, ProblemCode, TransferPlan},
};

/// The application-state key the API writes a request to re-read the kingdom state into. The
/// castle readings use the Overview's own key (see `inventory`).
pub fn transfer_refresh_key(account_id: &str) -> String {
    format!("transfer.refresh_requested_ms.{}", account_id.trim().to_ascii_lowercase())
}

const CHECK_EVERY_MS: i64 = 1_500;
/// A transfer that has not been sent this long after it was asked for is dropped, so a request made
/// while the bot was on can never fire hours later when the bot is stopped.
const QUEUE_EXPIRY_MS: i64 = 5 * 60 * 1_000;
/// While troops travel with no skip being spent, the kingdom state is read this often to notice the landing.
const WATCH_EVERY_MS: i64 = 90_000;
/// No two requested kingdom reads closer than this.
const CONTEXT_MIN_GAP_MS: i64 = 15_000;
/// Reads of `kpi` to settle an unanswered send before giving up.
const RECONCILE_TRIES: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    Kut,
    Skip,
    /// Read `kpi` to learn what happened.
    Reconcile,
}

#[derive(Debug)]
enum Step {
    /// Nothing due. With a travelling job, [`TransferAutomation::watch_due_ms`] says when to read again.
    Idle,
    Pause { due_ms: i64, then: Next },
    Awaiting { what: Next, deadline_ms: i64 },
}

pub(super) struct TransferAutomation {
    job: Option<TransferRow>,
    step: Step,
    scheduler: ActionScheduler,
    errors: ErrorBudget,
    checked_ms: i64,
    watch_due_ms: i64,
    reconcile_tries: u8,
    /// The request time already acted on (kingdom read).
    handled_refresh_ms: i64,
    last_read_ms: i64,
    read_due_ms: Option<i64>,
    refresh_key: String,
    rng: Rng,
}

impl TransferAutomation {
    pub(super) fn new(account_id: &str) -> Self {
        Self {
            job: None,
            step: Step::Idle,
            scheduler: ActionScheduler::new(Rng::from_entropy().next_u64()),
            errors: ErrorBudget::default(),
            checked_ms: i64::MIN / 2,
            watch_due_ms: 0,
            reconcile_tries: 0,
            handled_refresh_ms: 0,
            last_read_ms: i64::MIN / 2,
            read_due_ms: None,
            refresh_key: transfer_refresh_key(account_id),
            rng: Rng::from_entropy(),
        }
    }

    // ---- inbound: the funnels ------------------------------------------------

    /// Store whatever this packet reports and advance the job it confirms. Cheap for packets that
    /// carry none of it.
    pub(super) async fn observe(&mut self, store: &Store, account_id: &str, packet: Option<&XtPacket>, now: i64) {
        let Some(packet) = packet else { return };
        let accepted = packet.status.as_deref().is_none_or(|status| status == "0");
        if accepted {
            self.funnel(store, account_id, packet, now).await;
        }
        match packet.command.as_str() {
            "kut" => self.on_kut(store, account_id, packet, now).await,
            "msk" => self.on_msk(store, account_id, packet, now).await,
            _ => {}
        }
        if accepted
            && let Some(state) = transfer::find_kpi(&packet.payload).and_then(transfer::parse_kpi)
        {
            self.on_kingdoms(store, account_id, &state, now).await;
        }
    }

    /// Coins, kingdom state and skip stock, each from its own packet or from the copy inside another.
    async fn funnel(&self, store: &Store, account_id: &str, packet: &XtPacket, now: i64) {
        let payload = &packet.payload;
        if let Some((coins, rubies)) = transfer::find_currency(payload) {
            let _ = store.set_currency(account_id, coins, rubies, now).await;
        }
        if let Some(state) = transfer::find_kpi(payload).and_then(transfer::parse_kpi) {
            let _ = store.set_kingdom_state(account_id, &state, now).await;
        }
        let sce = if packet.command == "sce" { Some(payload) } else { payload.get("sce") };
        if let Some(sce) = sce {
            let stock = transfer::parse_skip_stock(sce);
            if !stock.is_empty() {
                let _ = store.merge_skip_stock(account_id, &stock, now).await;
            }
        }
    }

    async fn on_kut(&mut self, store: &Store, account_id: &str, packet: &XtPacket, now: i64) {
        let Some(job) = self.job.as_ref() else { return };
        if !matches!(self.step, Step::Awaiting { what: Next::Kut, .. }) || job.phase != TRANSFER_SENT {
            return;
        }
        match transfer::parse_kut_reply(packet.status.as_deref(), &packet.payload, job.target_kingdom_id) {
            KutOutcome::Accepted { .. } => {
                self.accepted(store, account_id, now, "On its way").await;
            }
            KutOutcome::Refused { status } => {
                self.errors.record(now);
                log_event(store, account_id, "transfer.refused", &format!("kut refused with status {status}")).await;
                self.fail(store, account_id, now, &format!("The server refused it (status {status}). Nothing was sent or charged.")).await;
            }
        }
    }

    async fn on_msk(&mut self, store: &Store, account_id: &str, packet: &XtPacket, now: i64) {
        if !matches!(self.step, Step::Awaiting { what: Next::Skip, .. }) {
            return;
        }
        let Some(job) = self.job.as_mut() else { return };
        if packet.status.as_deref().is_some_and(|status| status != "0") {
            let status = packet.status.clone().unwrap_or_default();
            self.errors.record(now);
            log_event(store, account_id, "transfer.refused", &format!("msk refused with status {status}")).await;
            job.detail = format!("Skip refused (status {status}); the troops keep travelling");
            let _ = store.save_transfer(job).await;
            self.watch(now);
            return;
        }
        job.skips_sent += 1;
        job.detail = format!("Skipped with {} ({} used)", job.skip_item.as_deref().unwrap_or("?"), job.skips_sent);
        job.updated_at_ms = now;
        let _ = store.save_transfer(job).await;
        // The reply's own `kpi` (handled right after this) says whether it has landed; until
        // it does, the next skip is scheduled from the state it leaves.
        let pause = self.pause_ms(now);
        self.step = Step::Pause { due_ms: now + pause, then: Next::Skip };
    }

    /// A kingdom-state snapshot arrived: it confirms a send, shows a landing, or settles a doubt.
    async fn on_kingdoms(&mut self, store: &Store, account_id: &str, state: &transfer::KingdomState, now: i64) {
        let Some(job) = self.job.as_ref() else { return };
        let flying = state.in_flight(job.target_kingdom_id).is_some();
        match job.phase.as_str() {
            TRANSFER_TRAVELLING if !flying => self.landed(store, account_id, now).await,
            TRANSFER_SENT if matches!(self.step, Step::Awaiting { what: Next::Reconcile, .. }) => {
                if flying {
                    self.accepted(store, account_id, now, "Confirmed on its way").await;
                } else {
                    self.fail(store, account_id, now, "No answer came and no transfer is travelling, so it did not go out. It was not sent again.").await;
                }
            }
            TRANSFER_TRAVELLING if matches!(self.step, Step::Awaiting { what: Next::Reconcile, .. }) => {
                self.reconcile_tries = 0;
                self.watch(now);
            }
            _ => {}
        }
    }

    // ---- job lifecycle -------------------------------------------------------

    fn pause_ms(&mut self, now: i64) -> i64 {
        let seconds = transfer::paced(self.scheduler.interval(Wait::Transfer));
        self.scheduler.applied(Wait::Transfer, now, seconds, None);
        (seconds * 1_000.0) as i64
    }

    /// Nothing to send for now: read `kpi` again in a while to notice the landing.
    fn watch(&mut self, now: i64) {
        self.step = Step::Idle;
        self.watch_due_ms = now + WATCH_EVERY_MS;
    }

    async fn save(&self, store: &Store) {
        if let Some(job) = &self.job {
            let _ = store.save_transfer(job).await;
        }
    }

    async fn accepted(&mut self, store: &Store, account_id: &str, now: i64, detail: &str) {
        let Some(job) = self.job.as_mut() else { return };
        job.phase = TRANSFER_TRAVELLING.to_owned();
        job.detail = detail.to_owned();
        job.updated_at_ms = now;
        let wants_skip = job.skip_item.is_some();
        log_event(store, account_id, "transfer.sent", &format!("{} troop types to kingdom {}", job.units.len(), job.target_kingdom_id)).await;
        self.save(store).await;
        self.reconcile_tries = 0;
        // The castle's stock just changed: have it read again before anything else is sent from it.
        let _ = store.set_app_state(&overview_refresh_key(account_id), &json!(now), now).await;
        if wants_skip {
            let pause = self.pause_ms(now);
            self.step = Step::Pause { due_ms: now + pause, then: Next::Skip };
        } else {
            self.watch(now);
        }
    }

    async fn landed(&mut self, store: &Store, account_id: &str, now: i64) {
        let Some(job) = self.job.as_mut() else { return };
        job.phase = TRANSFER_DONE.to_owned();
        job.detail = "Landed".to_owned();
        job.landed_at_ms = Some(now);
        job.updated_at_ms = now;
        log_event(store, account_id, "transfer.landed", &format!("to kingdom {}", job.target_kingdom_id)).await;
        self.save(store).await;
        self.finish(store, account_id, now).await;
    }

    async fn fail(&mut self, store: &Store, account_id: &str, now: i64, detail: &str) {
        let Some(job) = self.job.as_mut() else { return };
        job.phase = TRANSFER_FAILED.to_owned();
        job.detail = detail.to_owned();
        job.updated_at_ms = now;
        log_event(store, account_id, "transfer.failed", detail).await;
        self.save(store).await;
        self.finish(store, account_id, now).await;
    }

    /// The job is over: forget it, and have both castles read again.
    async fn finish(&mut self, store: &Store, account_id: &str, now: i64) {
        self.job = None;
        self.step = Step::Idle;
        self.reconcile_tries = 0;
        let _ = store.set_app_state(&overview_refresh_key(account_id), &json!(now), now).await;
    }

    /// Take the oldest open job, expire one that waited too long, and plan its first step.
    async fn adopt(&mut self, store: &Store, account_id: &str, now: i64) -> anyhow::Result<()> {
        let Some(job) = store.open_transfers(account_id).await?.into_iter().next() else {
            return Ok(());
        };
        self.job = Some(job);
        let job = self.job.as_ref().expect("just set");
        if job.phase == TRANSFER_QUEUED && now - job.created_at_ms > QUEUE_EXPIRY_MS {
            self.fail(store, account_id, now, "Dropped: it waited more than five minutes to be sent. Ask again.").await;
            return Ok(());
        }
        let phase = job.phase.clone();
        let skip_due = job.skip_item.is_some();
        let pause = self.pause_ms(now);
        self.step = match phase.as_str() {
            TRANSFER_QUEUED => Step::Pause { due_ms: now + pause, then: Next::Kut },
            // A send that was in doubt when the window or service stopped: read before anything else.
            TRANSFER_SENT => Step::Pause { due_ms: now + pause, then: Next::Reconcile },
            _ if skip_due => Step::Pause { due_ms: now + pause, then: Next::Skip },
            _ => Step::Pause { due_ms: now + pause, then: Next::Reconcile },
        };
        Ok(())
    }

    // ---- outbound -----------------------------------------------------------

    /// The packet to send now, if any. `allowed` is the connection being free (as for the Overview's
    /// `dcl`); `held` is the castles whose troops are out as support.
    pub(super) async fn next_packet(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
        now: i64,
        allowed: bool,
        held: &[(i64, i64, i64)],
    ) -> anyhow::Result<Option<String>> {
        if let Some(packet) = self.kingdom_read(store, server_header, now, allowed).await? {
            return Ok(Some(packet));
        }
        if self.job.is_none() {
            if now - self.checked_ms < CHECK_EVERY_MS {
                return Ok(None);
            }
            self.checked_ms = now;
            self.adopt(store, account_id, now).await?;
            if self.job.is_none() {
                return Ok(None);
            }
        }
        match self.step {
            Step::Pause { due_ms, then } if now >= due_ms && allowed => {
                self.run(store, account_id, server_header, now, then, held).await
            }
            Step::Awaiting { what, deadline_ms } if now >= deadline_ms => {
                self.timed_out(store, account_id, now, what).await;
                Ok(None)
            }
            Step::Idle if now >= self.watch_due_ms && self.job.as_ref().is_some_and(|job| job.phase == TRANSFER_TRAVELLING) => {
                let pause = self.pause_ms(now);
                self.step = Step::Pause { due_ms: now + pause, then: Next::Reconcile };
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    async fn run(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
        now: i64,
        then: Next,
        held: &[(i64, i64, i64)],
    ) -> anyhow::Result<Option<String>> {
        match then {
            Next::Kut => self.run_kut(store, account_id, server_header, now, held).await,
            Next::Skip => self.run_skip(store, account_id, server_header, now).await,
            Next::Reconcile => {
                self.step = Step::Awaiting { what: Next::Reconcile, deadline_ms: now + REQUEST_TIMEOUT_MS };
                Ok(Some(transfer::kpi_packet(server_header)?))
            }
        }
    }

    async fn run_kut(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
        now: i64,
        held: &[(i64, i64, i64)],
    ) -> anyhow::Result<Option<String>> {
        let Some(job) = self.job.as_ref() else { return Ok(None) };
        if job.phase != TRANSFER_QUEUED {
            return Ok(None);
        }
        if now - job.created_at_ms > QUEUE_EXPIRY_MS {
            self.fail(store, account_id, now, "Dropped: it waited more than five minutes to be sent. Ask again.").await;
            return Ok(None);
        }
        let plan = plan_of(job);
        let snapshot = store.transfer_snapshot(account_id, &plan, held).await?;
        let check = transfer::check(&plan, &snapshot, now);
        if !check.ok() {
            // Wait for what a moment (or a fresh reading) fixes; fail on anything the server would refuse.
            let waitable = check.problems.iter().all(|problem| {
                matches!(
                    problem.code,
                    ProblemCode::BotRunning
                        | ProblemCode::CastleBusy
                        | ProblemCode::StockUnknown
                        | ProblemCode::StockStale
                        | ProblemCode::KingdomsUnknown
                        | ProblemCode::CoinsUnknown
                )
            });
            if waitable {
                let _ = store.set_app_state(&overview_refresh_key(account_id), &json!(now), now).await;
                let _ = store.set_app_state(&self.refresh_key, &json!(now), now).await;
                let pause = self.pause_ms(now).max(3_000);
                self.step = Step::Pause { due_ms: now + pause, then: Next::Kut };
                if let Some(job) = self.job.as_mut() {
                    job.detail = format!("Waiting: {}", check.problems[0].message);
                    let _ = store.save_transfer(job).await;
                }
            } else {
                let text = check.problems.iter().map(|problem| problem.message.as_str()).collect::<Vec<_>>().join(" ");
                self.fail(store, account_id, now, &format!("Not sent. {text}")).await;
            }
            return Ok(None);
        }
        if let Some(until) = self.errors.paused_until(now) {
            self.step = Step::Pause { due_ms: until, then: Next::Kut };
            if let Some(job) = self.job.as_mut() {
                job.detail = "Paused: two refusals in ten minutes. It will go when the pause ends, if still wanted.".to_owned();
                let _ = store.save_transfer(job).await;
            }
            return Ok(None);
        }
        let packet = transfer::kut_packet(
            server_header,
            plan.source_castle_id,
            plan.source_kingdom_id,
            plan.target_kingdom_id,
            plan.target_castle_id,
            &plan.units,
        )?;
        if let Some(job) = self.job.as_mut() {
            job.phase = TRANSFER_SENT.to_owned();
            job.sent_at_ms = Some(now);
            job.cost_c1 = check.cost_c1;
            job.detail = "Sent; waiting for the server's answer".to_owned();
            job.updated_at_ms = now;
            let _ = store.save_transfer(job).await;
        }
        self.step = Step::Awaiting { what: Next::Kut, deadline_ms: now + REQUEST_TIMEOUT_MS };
        Ok(Some(packet))
    }

    async fn run_skip(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
        now: i64,
    ) -> anyhow::Result<Option<String>> {
        let Some(job) = self.job.as_ref() else { return Ok(None) };
        let target = job.target_kingdom_id;
        let Some(item) = job.skip_item.as_deref().and_then(transfer::skip_item) else {
            self.watch(now);
            return Ok(None);
        };
        // Skip only a transfer that is really travelling: read the state, not our own memory.
        let flying = store
            .kingdom_state(account_id)
            .await?
            .and_then(|(_, state)| state.in_flight(target).cloned());
        let Some(flight) = flying else {
            // Not in flight any more: it landed (the next snapshot, or this one, says so).
            self.landed(store, account_id, now).await;
            return Ok(None);
        };
        let _ = flight;
        let spent = job.skips_sent;
        let cap = transfer::skips_needed(transfer::travel_seconds(target).unwrap_or(7_200), item) + 1;
        let stock = store.skip_stock(account_id).await?.get(item.key).copied();
        if spent >= cap || stock == Some(0) {
            if let Some(job) = self.job.as_mut() {
                job.detail = if stock == Some(0) {
                    format!("Out of {}; the troops keep travelling", item.key)
                } else {
                    "Stopped skipping; the troops keep travelling".to_owned()
                };
                let _ = store.save_transfer(job).await;
            }
            self.watch(now);
            return Ok(None);
        }
        self.step = Step::Awaiting { what: Next::Skip, deadline_ms: now + REQUEST_TIMEOUT_MS };
        Ok(Some(transfer::msk_packet(server_header, item, target)?))
    }

    async fn timed_out(&mut self, store: &Store, account_id: &str, now: i64, what: Next) {
        self.reconcile_tries += 1;
        match what {
            // The `kut` may have gone out: never send it again, read the state instead.
            Next::Kut => {
                if let Some(job) = self.job.as_mut() {
                    job.detail = "No answer yet; checking whether it went out (it will not be sent again)".to_owned();
                    let _ = store.save_transfer(job).await;
                }
                let pause = self.pause_ms(now);
                self.step = Step::Pause { due_ms: now + pause, then: Next::Reconcile };
            }
            Next::Skip => self.watch(now),
            Next::Reconcile => {
                if self.reconcile_tries >= RECONCILE_TRIES {
                    let sent = self.job.as_ref().is_some_and(|job| job.phase == TRANSFER_SENT);
                    if sent {
                        self.fail(store, account_id, now, "The server did not answer. Check the game before trying again; it was not sent twice.").await;
                    } else {
                        self.reconcile_tries = 0;
                        self.watch(now);
                    }
                } else {
                    let pause = self.pause_ms(now);
                    self.step = Step::Pause { due_ms: now + pause, then: Next::Reconcile };
                }
            }
        }
    }

    /// Read `kpi` when the window asks (its Send button, or its first look), never more than every
    /// [`CONTEXT_MIN_GAP_MS`] and only when the connection is free.
    async fn kingdom_read(
        &mut self,
        store: &Store,
        server_header: &str,
        now: i64,
        allowed: bool,
    ) -> anyhow::Result<Option<String>> {
        if self.read_due_ms.is_none() {
            let requested = store.app_state(&self.refresh_key).await?.and_then(|value| value.as_i64()).unwrap_or(0);
            if requested > self.handled_refresh_ms {
                self.handled_refresh_ms = requested;
                let pause = self.rng.uniform(800.0, 2_500.0) as i64;
                self.read_due_ms = Some((now + pause).max(self.last_read_ms + CONTEXT_MIN_GAP_MS));
            }
        }
        match self.read_due_ms {
            Some(due) if allowed && now >= due => {
                self.read_due_ms = None;
                self.last_read_ms = now;
                Ok(Some(transfer::kpi_packet(server_header)?))
            }
            _ => Ok(None),
        }
    }
}

fn plan_of(job: &TransferRow) -> TransferPlan {
    TransferPlan {
        source_castle_id: job.source_castle_id,
        source_kingdom_id: job.source_kingdom_id,
        target_kingdom_id: job.target_kingdom_id,
        target_castle_id: job.target_castle_id,
        units: job.units.clone(),
        skip: job.skip_item.clone(),
    }
}
