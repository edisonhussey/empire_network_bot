use super::*;
use empire_core::planning::TargetAlgorithm;

#[derive(Debug)]
pub(super) enum AutomationPhase {
    Idle {
        due_ms: i64,
    },
    AwaitMap {
        task: ActiveModeTask,
        target: ReservedTarget,
        deadline_ms: i64,
    },
    AwaitFortressRefresh {
        target: ReservedTarget,
        deadline_ms: i64,
    },
    /// A tower refused an attack with status 95. Our map of it is stale, so its
    /// tile is re-read and the server's cooldown replaces our guess.
    RefreshRbc {
        target: ReservedTarget,
        due_ms: i64,
    },
    AwaitRbcRefresh {
        target: ReservedTarget,
        deadline_ms: i64,
    },
    ReadyAdi {
        task: ActiveModeTask,
        target: ReservedTarget,
        due_ms: i64,
    },
    AwaitAdi {
        task: ActiveModeTask,
        target: ReservedTarget,
        deadline_ms: i64,
    },
    ReadyCra {
        task: ActiveModeTask,
        target: ReservedTarget,
        lord_id: i64,
        due_ms: i64,
    },
    AwaitCra {
        task: ActiveModeTask,
        target: ReservedTarget,
        lord_id: i64,
        deadline_ms: i64,
    },
}

pub(super) struct Automation {
    pub(super) phase: AutomationPhase,
    pub(super) cursor: usize,
    pub(super) last_cra_ms: Option<i64>,
    pub(super) last_heartbeat_ms: i64,
    pub(super) rng: Rng,
    pub(super) detail: String,
    pub(super) operational_errors: VecDeque<i64>,
    pub(super) safety_pause_until_ms: i64,
    /// When anything last arrived from the server. A request that times out
    /// while the whole connection is silent says nothing about its target.
    pub(super) last_inbound_ms: i64,
    /// Why attacks were halted on purpose, kept so the status line still says so
    /// after the mode has been switched off.
    pub(super) stop_reason: Option<String>,
    /// Latest stock line for the main unit of the running attack, see
    /// [`format_stock_note`].
    pub(super) stock_note: Option<String>,
    /// When the next logical action may happen, and why (see `scheduler`).
    pub(super) scheduler: ActionScheduler,
    /// Unexpected missing / null responses in the last hour (see `health`).
    pub(super) health: ResponseHealth,
    pub(super) last_verdict: Option<Verdict>,
    /// One tower-selection memory per task: a spotlight follows that task's own
    /// productive ground. Nothing here is persisted.
    pub(super) selectors: HashMap<String, Selector>,
    /// The tower just claimed, not yet attacked. The spotlight moves only once the
    /// attack is acknowledged; an abandoned pick leaves the geography alone.
    pub(super) pending_pick: Option<PendingPick>,
    /// What the last selection pass saw (eligible / near the spotlight).
    pub(super) last_scan: Option<ScanStats>,
    /// The task and kingdom the map should show.
    pub(super) focus: Option<Focus>,
    /// The last tower an attack was acknowledged for.
    pub(super) selected: Option<(i64, i64)>,
    pub(super) last_completed: Option<CompletedView>,
}

#[derive(Debug, Clone)]
pub(super) struct PendingPick {
    pub(super) task_id: String,
    pub(super) tower: (i64, i64),
    pub(super) relocate: bool,
}

#[derive(Debug, Clone)]
pub(super) struct Focus {
    pub(super) task_id: String,
    pub(super) kingdom_id: i64,
    pub(super) castle: (i64, i64),
}

pub(super) fn attack_info_kind(task: &ActiveModeTask) -> hunt::AttackInfoKind {
    if task.target_kind == "fortress" {
        hunt::AttackInfoKind::Fortress
    } else {
        hunt::AttackInfoKind::Dungeon
    }
}

/// Commanders from `lids` that are home right now.
fn free_commanders(lids: &[i64], states: &[CommanderState], now: i64) -> Vec<i64> {
    lids.iter()
        .copied()
        .filter(|lid| {
            !states
                .iter()
                .any(|state| state.lord_id == *lid && state.available_after_ms > now)
        })
        .collect()
}

/// When the first of `lids` is back (`now` if one already is).
fn first_free_at(lids: &[i64], states: &[CommanderState], now: i64) -> Option<i64> {
    lids.iter()
        .map(|lid| {
            states
                .iter()
                .find(|state| state.lord_id == *lid)
                .map_or(now, |state| state.available_after_ms.max(now))
        })
        .min()
}

fn format_wait(ms: i64) -> String {
    let seconds = (ms.max(0) + 999) / 1_000;
    match seconds {
        0..=119 => format!("{seconds}s"),
        120..=7_199 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3_600, seconds % 3_600 / 60),
    }
}

/// A target that was leased for a handshake that never became an attack goes
/// straight back to the pool instead of sitting out its twelve-minute lease.
async fn release_unattacked(
    store: &Store,
    account_id: &str,
    task: &ActiveModeTask,
    target: &ReservedTarget,
) {
    let _ = if task.target_kind == "fortress" {
        store.release_fortress_target(account_id, target).await
    } else {
        store.release_rbc_target(account_id, target).await
    };
}

/// Count a routine event in the hourly summary. Telemetry must never stop an
/// attack, so a failed write is dropped.
pub(super) async fn count_event(store: &Store, account_id: &str, kind: &str, detail: &str) {
    let _ = store.bump_event(account_id, kind, detail, now_ms()).await;
}

/// Keep an important event as its own row.
pub(super) async fn log_event(store: &Store, account_id: &str, kind: &str, detail: &str) {
    let _ = store.record_event(account_id, kind, detail, now_ms()).await;
}

fn thousands(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if value < 0 { format!("-{out}") } else { out }
}

/// One line for the status bar: how much of the main unit is left and how fast
/// it is going. Shown permanently so a run heading for empty is visible hours
/// before the first refusal.
fn format_stock_note(unit: i64, forecast: &StockForecast) -> String {
    let mut note = format!(
        "unit {unit}: {} home, {} out",
        thousands(forecast.home),
        thousands(forecast.out)
    );
    if let Some(hours) = forecast.hours_left {
        note.push_str(&format!(
            ", draining {}/h, about {hours:.1}h left{}",
            thousands(forecast.drain_per_hour.round() as i64),
            if hours < 3.0 { " - LOW, recruit now" } else { "" }
        ));
    }
    note
}

/// Provisional hold written the moment a CRA leaves, before the server has said
/// anything. It closes the window in which a commander whose attack is in flight
/// still looks free; the acknowledgement replaces it with the real travel time.
const PENDING_COMMANDER_HOLD_MS: i64 = 5 * 60 * 1_000;
/// A refreshed tower the server still reports as ready is not trusted for long.
/// CRA refusals that are about the tower or the commander, not about us. Any
/// other status (101 and 313 mean the army was not there) stops the mode: the
/// server locked this account for 24 h after three of them in 40 s.
const BENIGN_CRA_STATUSES: [&str; 3] = ["95", "93", "256"];

/// The server talks every few seconds on a live session. Past this much quiet a
/// timed-out request is blamed on the link, not on the tower it was about.
const LINK_SILENCE_MS: i64 = 25_000;
pub(super) const UNEXPLAINED_REFUSAL_HOLD_MS: i64 = 30 * 60 * 1_000;

impl Automation {
    pub(super) fn new() -> Self {
        Self {
            phase: AutomationPhase::Idle { due_ms: 0 },
            cursor: 0,
            last_cra_ms: None,
            last_heartbeat_ms: 0,
            rng: Rng::from_entropy(),
            detail: "Mode is stopped".to_owned(),
            operational_errors: VecDeque::new(),
            safety_pause_until_ms: 0,
            last_inbound_ms: now_ms(),
            stop_reason: None,
            stock_note: None,
            scheduler: ActionScheduler::new(Rng::from_entropy().next_u64()),
            health: ResponseHealth::new(DEFAULT_TOLERANCE),
            last_verdict: None,
            selectors: HashMap::new(),
            pending_pick: None,
            last_scan: None,
            focus: None,
            selected: None,
            last_completed: None,
        }
    }

    /// Read optional settings once per session. The null-response tolerance can be
    /// changed with the `automation.null_tolerance` app-state value (default 2).
    pub(super) async fn configure(&mut self, store: &Store) {
        if let Ok(Some(value)) = store.app_state("automation.null_tolerance").await
            && let Some(tolerance) = value.as_u64()
        {
            self.health.set_tolerance(tolerance as usize);
        }
    }

    /// Record how an expected response went wrong. Only the cases in `health`
    /// that qualify count; the third in a rolling hour stops new actions.
    pub(super) async fn note_incident(
        &mut self,
        store: &Store,
        account_id: &str,
        outcome: Outcome,
        detail: &str,
        now: i64,
    ) {
        let verdict = self.health.record(outcome, detail, now);
        if verdict == Verdict::Noted {
            return;
        }
        self.last_verdict = Some(verdict);
        let count = self.health.count(now);
        let tolerance = self.health.tolerance();
        let message = format!("{outcome:?}: {detail} ({count} in the last hour, tolerance {tolerance})");
        log_event(store, account_id, "health.null_response", &message).await;
        if verdict == Verdict::Paused {
            self.detail = format!(
                "PAUSED: {count} unexpected missing responses in the last hour (tolerance {tolerance}). \
                 No new attacks start; stop and start the mode to resume."
            );
            log_event(store, account_id, "health.paused", &message).await;
        }
    }

    /// One recoverable operational error is tolerated in a rolling five-minute
    /// window. A second pauses all attack mutations until the oldest event has
    /// aged out; stopping/starting a mode cannot bypass the window.
    pub(super) async fn record_operational_error(&mut self, store: &Store, account_id: &str, now: i64, context: &str) {
        while self
            .operational_errors
            .front()
            .is_some_and(|at| now.saturating_sub(*at) >= ERROR_WINDOW_MS)
        {
            self.operational_errors.pop_front();
        }
        self.operational_errors.push_back(now);
        if self.operational_errors.len() > 4 {
            self.detail = format!("Fatal safety trip: {context} (5 errors in rolling window). Bot disabled.");
            let _ = store.disable_account_automation(account_id).await;
            self.safety_pause_until_ms = now + 86400_000; // sleep until mode restarted
            return;
        }
        if self.operational_errors.len() > 1 {
            self.safety_pause_until_ms = self.operational_errors[0] + ERROR_WINDOW_MS;
            self.detail = format!(
                "Safety pause after repeated errors: {context}. Resumes when the window clears."
            );
        }
    }

    pub(super) fn status(&self) -> (&'static str, String) {
        let state = match self.phase {
            AutomationPhase::Idle { .. } if self.health.paused() => "paused_on_errors",
            AutomationPhase::Idle { .. } => "waiting",
            AutomationPhase::AwaitMap { .. } => "opening_attack_map",
            AutomationPhase::AwaitFortressRefresh { .. } => "refreshing_fortress",
            AutomationPhase::RefreshRbc { .. } | AutomationPhase::AwaitRbcRefresh { .. } => {
                "refreshing_tower"
            }
            AutomationPhase::ReadyAdi { .. } => "pacing_inspection",
            AutomationPhase::AwaitAdi { .. } => "inspecting_target",
            AutomationPhase::ReadyCra { .. } => "pacing_attack",
            AutomationPhase::AwaitCra { .. } => "awaiting_attack_ack",
        };
        let detail = match &self.stock_note {
            Some(note) => format!("{} | {note}", self.detail),
            None => self.detail.clone(),
        };
        (state, detail)
    }

    pub(super) fn holds_transport(&self) -> bool {
        !matches!(self.phase, AutomationPhase::Idle { .. })
    }

    pub(super) async fn next_packet(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
    ) -> anyhow::Result<Option<String>> {
        let now = now_ms();
        self.scheduler.tick(now);
        if now < self.safety_pause_until_ms {
            // Allow the operator to clear the safety trip by turning the mode off.
            if store.active_mode_tasks(account_id).await?.is_empty() {
                self.operational_errors.clear();
                self.safety_pause_until_ms = 0;
            } else {
                return Ok(None);
            }
        }
        if now.saturating_sub(self.last_heartbeat_ms) >= 5_000 {
            if store.account_mode_running(account_id).await? {
                store
                    .set_app_state(HUNT_HEARTBEAT_KEY, &json!(now), now)
                    .await?;
            }
            self.last_heartbeat_ms = now;
        }

        // A request that times out while nothing at all is arriving was not
        // refused by its target; the connection is down (the supervisor will
        // reconnect). Don't quarantine the tower, don't count an error, and
        // keep a possibly-launched attack's commander reserved.
        let link_silent = now.saturating_sub(self.last_inbound_ms) >= LINK_SILENCE_MS;
        if link_silent {
            let expired = match &self.phase {
                AutomationPhase::AwaitMap { deadline_ms, .. }
                | AutomationPhase::AwaitFortressRefresh { deadline_ms, .. }
                | AutomationPhase::AwaitRbcRefresh { deadline_ms, .. }
                | AutomationPhase::AwaitAdi { deadline_ms, .. }
                | AutomationPhase::AwaitCra { deadline_ms, .. } => now >= *deadline_ms,
                _ => false,
            };
            if expired {
                match std::mem::replace(&mut self.phase, AutomationPhase::Idle { due_ms: now + 10_000 }) {
                    AutomationPhase::AwaitMap { task, target, .. }
                    | AutomationPhase::AwaitAdi { task, target, .. } => {
                        release_unattacked(store, account_id, &task, &target).await;
                    }
                    AutomationPhase::AwaitFortressRefresh { target, .. } => {
                        let _ = store.release_fortress_target(account_id, &target).await;
                    }
                    AutomationPhase::AwaitRbcRefresh { target, .. } => {
                        let _ = store.release_rbc_target(account_id, &target).await;
                    }
                    // The CRA may have left before the link died. Its target
                    // stays leased and its commander stays reserved.
                    _ => {}
                }
                self.scheduler.clear_next();
                count_event(store, account_id, "request_timeout_link_silent", "request timed out while the link was silent").await;
                self.note_incident(store, account_id, Outcome::ConnectionLoss, "request timed out while the link was silent", now).await;
                self.detail = "Connection to the game is silent; waiting for it to come back".to_owned();
                return Ok(None);
            }
        }

        match std::mem::replace(&mut self.phase, AutomationPhase::Idle { due_ms: now }) {
            AutomationPhase::AwaitMap { deadline_ms, .. } if now >= deadline_ms => {
                self.note_incident(store, account_id, Outcome::Timeout, "map window never answered", now).await;
                self.record_operational_error(store, account_id, now, "a game request timed out").await;
                self.phase = AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + 10_000),
                };
                return Ok(None);
            }
            AutomationPhase::AwaitFortressRefresh { deadline_ms, .. } if now >= deadline_ms => {
                self.note_incident(store, account_id, Outcome::Timeout, "fortress refresh never answered", now).await;
                self.record_operational_error(store, account_id, now, "a game request timed out").await;
                self.phase = AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + 10_000),
                };
                return Ok(None);
            }
            AutomationPhase::AwaitRbcRefresh { target, deadline_ms } if now >= deadline_ms => {
                // A missing tile read says nothing about the tower; it is not an
                // operational fault. Park the tower and carry on. It is still a
                // missing answer, so it is counted for connection health.
                self.note_incident(store, account_id, Outcome::MissingResponse, "tower tile read never answered", now).await;
                let _ = store
                    .defer_rbc_target(account_id, &target, now + UNEXPLAINED_REFUSAL_HOLD_MS)
                    .await;
                self.phase = AutomationPhase::Idle { due_ms: now + 2_000 };
                return Ok(None);
            }
            AutomationPhase::AwaitAdi { task, target, deadline_ms, .. } if now >= deadline_ms => {
                count_event(store, account_id, "timeout_adi", "attack details never answered").await;
                self.note_incident(store, account_id, Outcome::Timeout, "attack details never answered", now).await;
                // If ADI times out, it often means the target is on cooldown (e.g. towers).
                // Quarantine the target for an hour so we don't keep hitting it.
                if task.target_kind == "fortress" {
                    let _ = store.defer_fortress_target(account_id, &target, now + 3600_000).await;
                } else if target.kingdom_id == 10 {
                    let _ = store.delete_rbc_target(account_id, target.kingdom_id, target.x, target.y).await;
                } else {
                    let _ = store.defer_rbc_target(account_id, &target, now + 3600_000).await;
                    self.record_operational_error(store, account_id, now, "a game request timed out (target quarantined)").await;
                }
                self.phase = AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + 10_000),
                };
                return Ok(None);
            }
            AutomationPhase::AwaitCra { task, target, deadline_ms, .. } if now >= deadline_ms => {
                count_event(store, account_id, "timeout_cra", "attack never acknowledged").await;
                self.note_incident(store, account_id, Outcome::Timeout, "attack never acknowledged", now).await;
                if task.target_kind == "fortress" {
                    let _ = store.defer_fortress_target(account_id, &target, now + 3600_000).await;
                } else if target.kingdom_id == 10 {
                    let _ = store.delete_rbc_target(account_id, target.kingdom_id, target.x, target.y).await;
                } else {
                    let _ = store.defer_rbc_target(account_id, &target, now + 3600_000).await;
                    self.record_operational_error(store, account_id, now, "a game request timed out (target quarantined)").await;
                }
                self.phase = AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + 10_000),
                };
                return Ok(None);
            }
            other => {
                self.phase = other;
            }
        }

        if let AutomationPhase::RefreshRbc { target, due_ms } = &self.phase {
            if now < *due_ms {
                return Ok(None);
            }
            let packet = tile_gaa_packet(server_header, target.kingdom_id, target.x, target.y)?;
            self.detail = format!(
                "Tower {}:{}:{} refused the attack; re-reading its cooldown",
                target.kingdom_id, target.x, target.y
            );
            self.phase = AutomationPhase::AwaitRbcRefresh {
                target: target.clone(),
                deadline_ms: now + REQUEST_TIMEOUT_MS,
            };
            return Ok(Some(packet));
        }

        if let AutomationPhase::ReadyAdi {
            task,
            target,
            due_ms,
        } = &self.phase
        {
            if now < *due_ms {
                return Ok(None);
            }
            if task.target_kind == "fortress"
                && !store
                    .fortress_is_dispatchable(account_id, target, now, None)
                    .await?
            {
                store.release_fortress_target(account_id, target).await?;
                self.detail =
                    "Fortress dispatch window expired before inspection; target dropped".to_owned();
                self.phase = AutomationPhase::Idle { due_ms: now + 250 };
                return Ok(None);
            }
            let navigation = store.navigation(account_id).await?;
            let map_ready = navigation.as_ref().is_some_and(|state| {
                state.map_mode && state.current_kingdom_id == Some(task.kingdom_id)
            });
            if !map_ready {
                release_unattacked(store, account_id, task, target).await;
                self.detail = format!(
                    "Map context changed before inspection; reopening kingdom {}",
                    task.kingdom_id
                );
                self.phase = AutomationPhase::Idle { due_ms: now + 500 };
                return Ok(None);
            }
            let packet = hunt::attack_info_packet(
                server_header,
                (task.source_x, task.source_y),
                &as_map_target(target),
                attack_info_kind(task),
            )?;
            self.detail = format!(
                "Inspecting {}:{}:{} for task {}",
                target.kingdom_id, target.x, target.y, task.name
            );
            self.phase = AutomationPhase::AwaitAdi {
                task: task.clone(),
                target: target.clone(),
                deadline_ms: now + REQUEST_TIMEOUT_MS,
            };
            return Ok(Some(packet));
        }

        if let AutomationPhase::ReadyCra {
            task,
            target,
            lord_id,
            due_ms,
        } = &self.phase
        {
            if now < *due_ms {
                return Ok(None);
            }
            if task.target_kind == "fortress"
                && !store
                    .fortress_is_dispatchable(account_id, target, now, None)
                    .await?
            {
                store.release_fortress_target(account_id, target).await?;
                self.detail =
                    "Fortress dispatch window expired before CRA; target dropped".to_owned();
                self.phase = AutomationPhase::Idle { due_ms: now + 250 };
                return Ok(None);
            }
            let navigation = store.navigation(account_id).await?;
            if !navigation.as_ref().is_some_and(|state| {
                state.map_mode && state.current_kingdom_id == Some(task.kingdom_id)
            }) {
                release_unattacked(store, account_id, task, target).await;
                self.detail = "Map context changed before CRA; attack cancelled safely".to_owned();
                self.phase = AutomationPhase::Idle {
                    due_ms: now + 2_000,
                };
                return Ok(None);
            }
            // Stop is authoritative even in the middle of a handshake.
            if store.active_mode_tasks(account_id).await?.is_empty() {
                release_unattacked(store, account_id, task, target).await;
                self.detail = "Mode stopped before attack commit".to_owned();
                self.phase = AutomationPhase::Idle {
                    due_ms: now + 2_000,
                };
                return Ok(None);
            }
            let hard_due = self.last_cra_ms.map_or(0, |last| last + 4_000);
            if now < hard_due {
                return Ok(None);
            }
            let Some(travel) = store
                .travel_for_source(
                    account_id,
                    task.kingdom_id,
                    task.source_x,
                    task.source_y,
                    task.travel_mode,
                )
                .await?
            else {
                self.detail = format!(
                    "Travel options are not initialized for source {}:{}:{}; reconnect the account",
                    task.kingdom_id, task.source_x, task.source_y
                );
                self.phase = AutomationPhase::Idle {
                    due_ms: now + 30_000,
                };
                return Ok(None);
            };
            let packet = hunt::attack_packet(
                server_header,
                (task.source_x, task.source_y),
                &as_map_target(target),
                *lord_id,
                &task.payload,
                travel.hbw,
                travel.ptt,
            )?;
            self.last_cra_ms = Some(now);
            // Pessimistic reservation: the commander is spoken for from the
            // moment the CRA leaves, not from the moment it is acknowledged.
            let pending = CommanderState {
                account_id: account_id.to_owned(),
                lord_id: *lord_id,
                status: COMMANDER_OUTBOUND.to_owned(),
                available_after_ms: now + PENDING_COMMANDER_HOLD_MS,
                march_id: None,
                target_key: Some(format!("{}:{}:{}", target.kingdom_id, target.x, target.y)),
            };
            let _ = store.set_commander_state(&pending, now).await;
            self.detail = format!(
                "Attack sent with commander {lord_id} to {}:{}:{} using {} (HBW {}, PTT {})",
                target.kingdom_id,
                target.x,
                target.y,
                task.travel_mode.as_str(),
                travel.hbw,
                travel.ptt,
            );
            self.phase = AutomationPhase::AwaitCra {
                task: task.clone(),
                target: target.clone(),
                lord_id: *lord_id,
                deadline_ms: now + REQUEST_TIMEOUT_MS,
            };
            return Ok(Some(packet));
        }

        let AutomationPhase::Idle { due_ms } = self.phase else {
            return Ok(None);
        };
        if now < due_ms {
            return Ok(None);
        }
        let tasks = store.active_mode_tasks(account_id).await?;
        if tasks.is_empty() {
            self.detail = self
                .stop_reason
                .clone()
                .unwrap_or_else(|| "No running attack tasks for this account".to_owned());
            // Stopping the mode is the operator action that clears a health pause.
            self.health.clear_pause();
            self.scheduler.clear_next();
            self.phase = AutomationPhase::Idle {
                due_ms: now + 2_000,
            };
            return Ok(None);
        }
        self.stop_reason = None;
        if self.health.paused() {
            // Over the tolerance for unexpected missing responses: nothing new
            // starts. Work already on the wire has resolved by now (this is only
            // reached from Idle). It does not resume by itself.
            self.phase = AutomationPhase::Idle { due_ms: now + 2_000 };
            return Ok(None);
        }
        let states = store.commander_states(account_id).await?;
        let rotated = (0..tasks.len())
            .map(|offset| (self.cursor + offset) % tasks.len())
            .collect::<Vec<_>>();
        let indexes = rotated
            .iter()
            .copied()
            .filter(|index| tasks[*index].target_kind == "fortress")
            .collect::<Vec<_>>();
        for index in indexes {
            let task = &tasks[index];
            if free_commanders(&task.commander_lids, &states, now).is_empty() {
                continue;
            }
            let target = store
                .reserve_fortress_target(
                    account_id,
                    task.kingdom_id,
                    task.level_min,
                    task.level_max,
                    (task.source_x, task.source_y),
                    now,
                    now + TARGET_LEASE_MS,
                )
                .await?;
            if let Some(target) = target {
                self.cursor = (index + 1) % tasks.len();
                let navigation = store.navigation(account_id).await?;
                if navigation.as_ref().is_some_and(|state| {
                    state.map_mode && state.current_kingdom_id == Some(task.kingdom_id)
                }) {
                    self.phase = AutomationPhase::ReadyAdi {
                        task: task.clone(),
                        target,
                        due_ms: now,
                    };
                    return Ok(None);
                }
                // Only establish the kingdom's map context here. Fortress
                // cooldowns came from the broad scan and are counted down
                // locally; re-reading each target would be redundant load.
                let (map_x, map_y) = (task.source_x, task.source_y);
                let ax1 = map_x.div_euclid(13) * 13;
                let ay1 = map_y.div_euclid(13) * 13;
                let packet = encode_client_xt(
                    server_header,
                    "gaa",
                    "1",
                    &json!({
                        "KID": task.kingdom_id,
                        "AX1": ax1,
                        "AY1": ay1,
                        "AX2": ax1 + 12,
                        "AY2": ay1 + 12
                    }),
                )?;
                self.detail = format!(
                    "Opening kingdom {} map before inspecting {}:{}",
                    task.kingdom_id, target.x, target.y
                );
                self.phase = AutomationPhase::AwaitMap {
                    task: task.clone(),
                    target,
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
        }

        // Failed or unobserved landings already carry an explicit refresh_due
        // timestamp. Do not turn every known fortress into an individual poll;
        // routine availability comes from the broad scan.
        for index in rotated
            .iter()
            .copied()
            .filter(|index| tasks[*index].target_kind == "fortress")
        {
            let task = &tasks[index];
            if let Some(target) = store
                .reserve_due_fortress_refresh(account_id, task.kingdom_id, now)
                .await?
            {
                let packet = tile_gaa_packet(server_header, target.kingdom_id, target.x, target.y)?;
                self.detail = format!(
                    "Refreshing fortress result at {}:{}:{}",
                    target.kingdom_id, target.x, target.y
                );
                self.phase = AutomationPhase::AwaitFortressRefresh {
                    target,
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
        }

        // Ordinary targets run only after every currently actionable fortress
        // and due fortress maintenance request has been considered.
        //
        // A task's commanders are strictly its own: each belongs to one task
        // (its section of the roster) and no other task may borrow it, whatever
        // is idle. Lending was tried and sent crossbow marches out under
        // commanders reserved for the other section.
        for index in rotated
            .iter()
            .copied()
            .filter(|index| tasks[*index].target_kind != "fortress")
        {
            let task = &tasks[index];
            if free_commanders(&task.commander_lids, &states, now).is_empty() {
                continue;
            }
            // The task's own selector remembers where its spotlight is. Choosing
            // reads only the database: no request leaves for a pick.
            let selector = self
                .selectors
                .entry(task.task_id.clone())
                .or_insert_with(Selector::from_entropy);
            let castle = (task.source_x, task.source_y);
            let scan = store
                .reserve_rbc_target_with(
                    account_id,
                    task.kingdom_id,
                    task.level_min,
                    task.level_max,
                    castle,
                    TargetAlgorithm::from_name(&task.algorithm),
                    selector,
                    now,
                    now + TARGET_LEASE_MS,
                )
                .await?;
            self.last_scan = Some(scan.stats);
            self.focus = Some(Focus {
                task_id: task.task_id.clone(),
                kingdom_id: task.kingdom_id,
                castle,
            });
            if let Some(pick) = scan.pick {
                let target = pick.target;
                self.pending_pick = Some(PendingPick {
                    task_id: task.task_id.clone(),
                    tower: (target.x, target.y),
                    relocate: pick.relocate,
                });
                self.cursor = (index + 1) % tasks.len();
                let task = task.clone();
                let navigation = store.navigation(account_id).await?;
                if navigation.as_ref().is_some_and(|state| {
                    state.map_mode && state.current_kingdom_id == Some(task.kingdom_id)
                }) {
                    self.phase = AutomationPhase::ReadyAdi {
                        task,
                        target,
                        due_ms: now,
                    };
                    return Ok(None);
                }
                let packet =
                    tile_gaa_packet(server_header, task.kingdom_id, task.source_x, task.source_y)?;
                self.detail = format!(
                    "Opening kingdom {} map before inspecting {}:{}",
                    task.kingdom_id, target.x, target.y
                );
                self.phase = AutomationPhase::AwaitMap {
                    task,
                    target,
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
        }

        // Nothing can be sent. Instead of polling on a fixed beat, work out when
        // the first thing that is blocking us clears (a tower coming off
        // cooldown, or a commander getting home) and sleep until then, saying
        // which of the two is the limit.
        let mut wake: Option<i64> = None;
        let mut notes = Vec::new();
        for task in tasks.iter().filter(|task| task.target_kind != "fortress") {
            let free = free_commanders(&task.commander_lids, &states, now).len();
            let target_ready = store
                .next_rbc_ready_ms(account_id, task.kingdom_id, task.level_min, task.level_max)
                .await?;
            let Some(target_ready) = target_ready else {
                notes.push(format!("{}: no towers in range", task.name));
                continue;
            };
            let commander_ready = first_free_at(&task.commander_lids, &states, now).unwrap_or(now);
            let ready = target_ready.max(commander_ready);
            wake = Some(wake.map_or(ready, |earliest| earliest.min(ready)));
            notes.push(if target_ready > now {
                format!(
                    "{}: {free}/{} commanders home, next tower in {}",
                    task.name,
                    task.commander_lids.len(),
                    format_wait(target_ready - now)
                )
            } else {
                format!(
                    "{}: tower ready, next commander in {}",
                    task.name,
                    format_wait(commander_ready - now)
                )
            });
        }
        let has_fortress = tasks.iter().any(|task| task.target_kind == "fortress");
        let ceiling = if has_fortress { 15_000 } else { 30_000 };
        let sleep = wake
            .map_or(ceiling, |at| at.saturating_sub(now))
            .clamp(1_500, ceiling)
            + self.rng.uniform(0.0, 1_200.0) as i64;
        self.phase = AutomationPhase::Idle { due_ms: now + sleep };
        self.detail = if notes.is_empty() {
            "No eligible unleased target or free task commander; retrying".to_owned()
        } else {
            format!("Waiting - {}", notes.join("; "))
        };
        Ok(None)
    }

    pub(super) async fn observe(&mut self, store: &Store, account_id: &str, raw: &str) {
        self.last_inbound_ms = now_ms();
        let Ok(packet) = parse_xt_packet(raw) else {
            return;
        };
        let now = now_ms();
        if let Some(status) = packet.status.as_deref().filter(|status| *status != "0") {
            let expected_rejection = match &self.phase {
                AutomationPhase::AwaitAdi { task, .. } => {
                    packet.command == attack_info_kind(task).command()
                }
                AutomationPhase::AwaitCra { .. } => packet.command == "cra",
                _ => false,
            };
            if !expected_rejection {
                return;
            }
            let (rejected_fortress, rejected_rbc) = match &self.phase {
                AutomationPhase::AwaitAdi { task, target, .. }
                | AutomationPhase::AwaitCra { task, target, .. } => {
                    if task.target_kind == "fortress" {
                        (Some(target.clone()), None)
                    } else {
                        (None, Some(target.clone()))
                    }
                }
                _ => (None, None),
            };
            if let Some(target) = rejected_fortress {
                let _ = store
                    .defer_fortress_target(account_id, &target, now + ERROR_WINDOW_MS)
                    .await;
            }
            let mut refresh_target = None;
            if let Some(target) = rejected_rbc {
                if target.kingdom_id == 10 {
                    // Berimond camps don't have cooldowns. If they error, they were defeated.
                    let _ = store.delete_rbc_target(account_id, target.kingdom_id, target.x, target.y).await;
                } else if status == "95" {
                    // Our map of this tower is stale: someone else hit it, or it
                    // regenerated. Parking it for an hour would waste every
                    // minute it is actually ready, so hold it only briefly and
                    // re-read its tile: the server reports the exact cooldown.
                    let _ = store
                        .defer_rbc_target(account_id, &target, now + 60_000)
                        .await;
                    refresh_target = Some(target);
                } else {
                    // Any other refusal is unexplained; stand the tower down.
                    let _ = store
                        .defer_rbc_target(account_id, &target, now + 3600_000)
                        .await;
                }
            }
            let rejected_cra = matches!(self.phase, AutomationPhase::AwaitCra { .. });
            if let AutomationPhase::AwaitCra { lord_id, .. } = &self.phase {
                let state = CommanderState {
                    account_id: account_id.to_owned(),
                    lord_id: *lord_id,
                    status: COMMANDER_AVAILABLE.to_owned(),
                    available_after_ms: now + COMMANDER_REJECT_HOLD_MS,
                    march_id: None,
                    target_key: None,
                };
                let _ = store.set_commander_state(&state, now).await;
            }
            if rejected_cra && !BENIGN_CRA_STATUSES.contains(&status) {
                if let AutomationPhase::AwaitCra { task, target, .. } = &self.phase {
                    release_unattacked(store, account_id, task, target).await;
                }
                let _ = store.disable_account_automation(account_id).await;
                let reason = format!(
                    "STOPPED: the server refused an attack with status {status}. Attacks are off \
                     until you start the mode again; check the castle's troops first."
                );
                warn!(%account_id, %status, "attack refused for an unexplained reason; mode stopped");
                self.health.record(Outcome::ExplicitRejection, &format!("attack refused with status {status}"), now);
                log_event(store, account_id, "attack.stopped", &reason).await;
                self.stop_reason = Some(reason.clone());
                self.detail = reason;
                self.phase = AutomationPhase::Idle { due_ms: now + 2_000 };
                return;
            }
            count_event(
                store,
                account_id,
                &format!("{}_refused_{status}", packet.command),
                "routine refusal",
            )
            .await;
            self.detail = format!(
                "{} rejected with status {status}; retrying after backoff",
                packet.command
            );
            warn!(%account_id, command = %packet.command, %status, "automation request rejected");
            // Do not record an operational error here! We already gracefully deferred the target
            // or deleted it, so the bot won't spin. Game errors like 95 or 93 are expected.
            // An ADI rejection did not claim a commander, so it can move to the
            // next task after the normal short acknowledgement delay. Only a
            // rejected CRA needs the commander-specific backoff.
            let retry_ms = if rejected_cra {
                COMMANDER_REJECT_HOLD_MS
            } else {
                let pause = self.scheduler.interval(Wait::AfterAck);
                self.scheduler.applied(Wait::AfterAck, now, pause, None);
                (pause * 1_000.0) as i64
            };
            self.phase = match refresh_target {
                Some(target) => AutomationPhase::RefreshRbc {
                    target,
                    due_ms: self.safety_pause_until_ms.max(now + retry_ms),
                },
                None => AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + retry_ms),
                },
            };
            return;
        }

        match packet.command.as_str() {
            "gaa" => {
                if let AutomationPhase::AwaitFortressRefresh { target, .. } = &self.phase {
                    if packet.payload.get("KID").and_then(Value::as_i64) == Some(target.kingdom_id)
                    {
                        self.detail = format!(
                            "Fortress server truth refreshed at {}:{}:{}",
                            target.kingdom_id, target.x, target.y
                        );
                        self.phase = AutomationPhase::Idle { due_ms: now + 250 };
                    }
                    return;
                }
                if let AutomationPhase::AwaitRbcRefresh { target, .. } = &self.phase {
                    if packet.payload.get("KID").and_then(Value::as_i64) == Some(target.kingdom_id)
                    {
                        // An empty tile is a valid answer (the tower is gone), not
                        // a missing one. Shown for context; never counted.
                        if packet.payload.get("AI").and_then(Value::as_array).is_some_and(Vec::is_empty) {
                            self.health.record(Outcome::ExpectedEmpty, "tower tile came back empty", now);
                        }
                        // The cooldown itself was stored when the response was
                        // first observed; this only decides what to do next.
                        let free_at = store
                            .rbc_server_free_at(account_id, target)
                            .await
                            .ok()
                            .flatten()
                            .unwrap_or(0);
                        if free_at > now {
                            self.detail = format!(
                                "Tower {}:{}:{} is on cooldown for {} (server); moving on",
                                target.kingdom_id,
                                target.x,
                                target.y,
                                format_wait(free_at - now)
                            );
                        } else {
                            // The map says it is ready yet the attack was
                            // refused. Don't keep hammering it.
                            let _ = store
                                .defer_rbc_target(
                                    account_id,
                                    target,
                                    now + UNEXPLAINED_REFUSAL_HOLD_MS,
                                )
                                .await;
                            self.detail = format!(
                                "Tower {}:{}:{} refused but shows no cooldown; parked",
                                target.kingdom_id, target.x, target.y
                            );
                        }
                        self.phase = AutomationPhase::Idle { due_ms: now + 250 };
                    }
                    return;
                }
                // Unsolicited map updates must not consume an attack reply
                // waiter. Only the pending navigation request owns this GAA.
                if !matches!(&self.phase, AutomationPhase::AwaitMap { task, .. }
                    if packet.payload.get("KID").and_then(Value::as_i64) == Some(task.kingdom_id))
                {
                    return;
                }
                let AutomationPhase::AwaitMap { task, target, .. } = std::mem::replace(
                    &mut self.phase,
                    AutomationPhase::Idle {
                        due_ms: now + 10_000,
                    },
                ) else {
                    return;
                };
                if packet.payload.get("KID").and_then(Value::as_i64) != Some(task.kingdom_id) {
                    self.detail = "Wrong map kingdom returned; attack remains blocked".to_owned();
                    return;
                }
                if task.target_kind == "fortress"
                    && !store
                        .fortress_is_dispatchable(account_id, &target, now, None)
                        .await
                        .unwrap_or(false)
                {
                    let _ = store.release_fortress_target(account_id, &target).await;
                    self.detail =
                        "Fortress was unavailable or its one-minute window expired; dropped"
                            .to_owned();
                    self.phase = AutomationPhase::Idle { due_ms: now + 250 };
                    return;
                }
                self.detail = format!(
                    "Kingdom {} map confirmed; preparing attack details",
                    task.kingdom_id
                );
                self.phase = AutomationPhase::ReadyAdi {
                    task,
                    target,
                    due_ms: now + 500,
                };
            }
            "adi" | "abi" => {
                if !matches!(&self.phase, AutomationPhase::AwaitAdi { task, .. }
                    if packet.command == attack_info_kind(task).command())
                {
                    return;
                }
                let AutomationPhase::AwaitAdi { task, target, .. } = std::mem::replace(
                    &mut self.phase,
                    AutomationPhase::Idle {
                        due_ms: now + 10_000,
                    },
                ) else {
                    return;
                };
                if packet.payload.is_null() {
                    // Status 0 yet no payload: an answer that should have had
                    // content did not. (A refusal carries a status and is handled
                    // above, so this is not one.)
                    self.note_incident(store, account_id, Outcome::UnexpectedNull, "attack details came back empty", now).await;
                    if task.target_kind == "fortress" {
                        let _ = store
                            .defer_fortress_target(account_id, &target, now + ERROR_WINDOW_MS)
                            .await;
                    } else if target.kingdom_id == 10 {
                        let _ = store.delete_rbc_target(account_id, target.kingdom_id, target.x, target.y).await;
                    } else {
                        // Defer regular RBC target for 1 hour to prevent looping
                        let _ = store.defer_rbc_target(account_id, &target, now + 3600_000).await;
                    }
                    
                    self.detail = "Attack details were unavailable; target deferred".to_owned();
                    self.phase = AutomationPhase::Idle {
                        due_ms: self.safety_pause_until_ms.max(now + 10_000),
                    };
                    return;
                }
                // Never send an army the castle cannot supply. The reply says
                // exactly what is at home; the server answers a short army with
                // 101/313 and repeated ones get the account locked.
                let stock = hunt::home_inventory(&packet.payload);
                if !stock.is_empty() {
                    // Keep the figure that explains a run: what this attack uses,
                    // at home and out, per castle. See docs/architecture/database.md.
                    let castle_id = packet.payload.get("SCID").and_then(Value::as_i64).unwrap_or(0);
                    let out = hunt::units_out(&packet.payload);
                    let used = hunt::army_troops(&task.payload);
                    let rows = used
                        .keys()
                        .map(|unit| {
                            (
                                *unit,
                                stock.get(unit).copied().unwrap_or(0),
                                out.get(unit).copied().unwrap_or(0),
                            )
                        })
                        .collect::<Vec<_>>();
                    let _ = store.record_stock_sample(account_id, castle_id, &rows, now).await;
                    if let Some((unit, _)) = used.iter().max_by_key(|(_, count)| **count) {
                        self.stock_note = store
                            .stock_forecast(account_id, castle_id, *unit, now)
                            .await
                            .ok()
                            .flatten()
                            .map(|forecast| format_stock_note(*unit, &forecast));
                    }
                    let short = hunt::army_shortfall(&task.payload, &stock);
                    if !short.is_empty() {
                        release_unattacked(store, account_id, &task, &target).await;
                        let detail = short
                            .iter()
                            .map(|(unit, need, have)| format!("unit {unit} need {need} have {have}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        count_event(store, account_id, "army_short", &detail).await;
                        self.detail = format!("Waiting for troops to return: {detail}");
                        let wait_ms = self.rng.uniform(30_000.0, 60_000.0) as i64;
                        self.phase = AutomationPhase::Idle { due_ms: now + wait_ms };
                        return;
                    }
                }
                let offered = hunt::available_commanders(&packet.payload);
                let states = store.commander_states(account_id).await.unwrap_or_default();
                let busy = states
                    .iter()
                    .filter(|state| state.available_after_ms > now)
                    .map(|state| state.lord_id)
                    .collect::<Vec<_>>();
                let Some(lord_id) = task
                    .commander_lids
                    .iter()
                    .copied()
                    .filter(|lid| offered.is_empty() || offered.contains(lid))
                    .find(|lid| !busy.contains(lid))
                else {
                    release_unattacked(store, account_id, &task, &target).await;
                    self.detail =
                        "No allocated commander is currently free; target released".to_owned();
                    self.phase = AutomationPhase::Idle { due_ms: now + 1_000 };
                    return;
                };
                // The wait before the attack comes from the stochastic timer; the
                // global cra spacing is applied on top and always wins.
                let policy = PacingPolicy::default();
                let wait = self.scheduler.interval(Wait::BeforeAttack);
                let (due_seconds, limited) = policy.cra_due_at_with(
                    now as f64 / 1_000.0,
                    self.last_cra_ms.map(|value| value as f64 / 1_000.0),
                    wait,
                    &mut self.rng,
                );
                self.scheduler.applied(
                    Wait::BeforeAttack,
                    now,
                    due_seconds - now as f64 / 1_000.0,
                    limited.then_some("global cra spacing"),
                );
                self.phase = AutomationPhase::ReadyCra {
                    task,
                    target,
                    lord_id,
                    due_ms: (due_seconds * 1_000.0) as i64,
                };
                self.detail = format!("Commander {lord_id} selected; waiting above CRA floor");
            }
            "cra" => {
                if !matches!(self.phase, AutomationPhase::AwaitCra { .. }) {
                    return;
                }
                let AutomationPhase::AwaitCra {
                    task,
                    target,
                    lord_id,
                    ..
                } = std::mem::replace(
                    &mut self.phase,
                    AutomationPhase::Idle {
                        due_ms: now + 10_000,
                    },
                )
                else {
                    return;
                };
                let march_id = hunt::march_id_from_ack(&packet.payload).unwrap_or(now);
                let travel = hunt::travel_seconds_from_ack(&packet.payload);
                let available_after_ms = now
                    + (Waits::provisional_commander_hold(travel.unwrap_or(300), &mut self.rng)
                        * 1_000.0) as i64;
                let march = MarchRecord {
                    account_id: account_id.to_owned(),
                    march_id,
                    kingdom_id: target.kingdom_id,
                    x: target.x,
                    y: target.y,
                    task_id: Some(task.task_id.clone()),
                    profile_id: Some(task.profile_id.clone()),
                    level: target.level,
                    lord_id: Some(lord_id),
                    commander_number: None,
                    troop_count: None,
                    duration_s: travel,
                    coin_loot: None,
                    ruby_loot: None,
                    status: MARCH_SENT.to_owned(),
                    result_flag: None,
                    error_message: None,
                    sent_at_ms: now,
                    landed_at_ms: None,
                    result_at_ms: None,
                };
                let state = CommanderState {
                    account_id: account_id.to_owned(),
                    lord_id,
                    status: COMMANDER_OUTBOUND.to_owned(),
                    available_after_ms,
                    march_id: Some(march_id),
                    target_key: Some(format!("{}:{}:{}", target.kingdom_id, target.x, target.y)),
                };
                let _ = store.record_march(&march).await;
                let _ = store
                    .set_march_army(account_id, march_id, &hunt::army_troops(&task.payload))
                    .await;
                let _ = store.set_commander_state(&state, now).await;
                if task.target_kind == "fortress" {
                    let outbound_s = travel.unwrap_or(300).max(0);
                    let refresh_delay_ms = (1 + march_id.unsigned_abs() % 30) as i64 * 60 * 1_000;
                    let _ = store
                        .mark_fortress_attack_sent(
                            account_id,
                            &target,
                            now,
                            now.saturating_add(outbound_s.saturating_mul(1_000)),
                            refresh_delay_ms,
                        )
                        .await;
                } else {
                    let _ = store.mark_target_attacked(account_id, &target, now).await;
                    if target.kingdom_id != 10 {
                        let duration_ms = travel.unwrap_or(300).max(0) as i64 * 1_000;
                        let exact_cooldown = now + duration_ms + 3 * 3600 * 1_000;
                        let upper_bound = now + 4 * 3600 * 1_000;
                        let buffer_ms = self.rng.uniform(240.0, 360.0) as i64 * 1_000;
                        let cooldown = exact_cooldown.min(upper_bound) + buffer_ms;
                        let _ = store
                            .defer_rbc_target(account_id, &target, cooldown)
                            .await;
                    }
                }
                // Match the Python proxy's successful path: the next ADI opens
                // after the short CRA acknowledgement delay. ADI -> CRA has its
                // own independent pacing below, so adding `attack_send` here
                // serialised the commander pool for no additional safety.
                // The attack is really going ahead, so the geography may move now.
                // (A pick that was released, refused or timed out never gets here.)
                if let Some(pick) = self.pending_pick.take()
                    && pick.task_id == task.task_id
                    && pick.tower == (target.x, target.y)
                    && let Some(selector) = self.selectors.get_mut(&pick.task_id)
                {
                    selector.accept(pick.tower, pick.relocate);
                }
                self.selected = Some((target.x, target.y));
                self.last_completed = Some(CompletedView {
                    march_id,
                    x: target.x,
                    y: target.y,
                    lord_id,
                    at_ms: now,
                });
                let pause = self.scheduler.interval(Wait::AfterAck);
                self.scheduler.applied(Wait::AfterAck, now, pause, None);
                self.phase = AutomationPhase::Idle {
                    due_ms: now + (pause * 1_000.0) as i64,
                };
                self.detail = format!("Attack {march_id} acknowledged; scheduling next target");
            }
            "cat" => {
                let (Some(_), Some(lord_id)) = (
                    hunt::return_target(&packet.payload),
                    hunt::returned_lord_id(&packet.payload),
                ) else {
                    return;
                };
                let return_seconds = hunt::return_seconds_from_return(&packet.payload);
                let rest_ms = (Waits::commander_return_hold(&mut self.rng) * 1_000.0) as i64;
                let applied = store
                    .apply_attack_return(account_id, &packet.payload, now, rest_ms)
                    .await;
                if let Err(error) = &applied {
                    warn!(%error, %account_id, "attack return persistence failed");
                }
                if matches!(applied, Ok(true))
                    && let Some((kid, x, y)) = hunt::return_target(&packet.payload)
                {
                    let buffer_ms = self.rng.uniform(240.0, 360.0) as i64 * 1_000;
                    let _ = store
                        .refine_rbc_cooldown(account_id, kid, x, y, now, buffer_ms)
                        .await;
                }
                if matches!(applied, Ok(true))
                    && let Some(seconds) = return_seconds
                {
                    self.detail =
                        format!("Commander {lord_id} returning for about {seconds}s before reuse");
                }
            }
            _ => {}
        }
    }
}
