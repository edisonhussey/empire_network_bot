use super::*;

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
}

pub(super) fn attack_info_kind(task: &ActiveModeTask) -> hunt::AttackInfoKind {
    if task.target_kind == "fortress" {
        hunt::AttackInfoKind::Fortress
    } else {
        hunt::AttackInfoKind::Dungeon
    }
}

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
            AutomationPhase::Idle { .. } => "waiting",
            AutomationPhase::AwaitMap { .. } => "opening_attack_map",
            AutomationPhase::AwaitFortressRefresh { .. } => "refreshing_fortress",
            AutomationPhase::ReadyAdi { .. } => "pacing_inspection",
            AutomationPhase::AwaitAdi { .. } => "inspecting_target",
            AutomationPhase::ReadyCra { .. } => "pacing_attack",
            AutomationPhase::AwaitCra { .. } => "awaiting_attack_ack",
        };
        (state, self.detail.clone())
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

        match std::mem::replace(&mut self.phase, AutomationPhase::Idle { due_ms: now }) {
            AutomationPhase::AwaitMap { deadline_ms, .. } if now >= deadline_ms => {
                self.record_operational_error(store, account_id, now, "a game request timed out").await;
                self.phase = AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + 10_000),
                };
                return Ok(None);
            }
            AutomationPhase::AwaitFortressRefresh { deadline_ms, .. } if now >= deadline_ms => {
                self.record_operational_error(store, account_id, now, "a game request timed out").await;
                self.phase = AutomationPhase::Idle {
                    due_ms: self.safety_pause_until_ms.max(now + 10_000),
                };
                return Ok(None);
            }
            AutomationPhase::AwaitAdi { task, target, deadline_ms, .. } if now >= deadline_ms => {
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
                self.detail = "Map context changed before CRA; attack cancelled safely".to_owned();
                self.phase = AutomationPhase::Idle {
                    due_ms: now + 2_000,
                };
                return Ok(None);
            }
            // Stop is authoritative even in the middle of a handshake.
            if store.active_mode_tasks(account_id).await?.is_empty() {
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
            self.detail = "No running attack tasks for this account".to_owned();
            self.phase = AutomationPhase::Idle {
                due_ms: now + 2_000,
            };
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
            if task.commander_lids.iter().all(|lid| {
                states
                    .iter()
                    .any(|state| state.lord_id == *lid && state.available_after_ms > now)
            }) {
                continue;
            }
            let target = if task.target_kind == "fortress" {
                store
                    .reserve_fortress_target(
                        account_id,
                        task.kingdom_id,
                        task.level_min,
                        task.level_max,
                        (task.source_x, task.source_y),
                        now,
                        now + TARGET_LEASE_MS,
                    )
                    .await?
            } else {
                store
                    .reserve_rbc_target(
                        account_id,
                        task.kingdom_id,
                        task.level_min,
                        task.level_max,
                        (task.source_x, task.source_y),
                        &task.algorithm,
                        now,
                        now + TARGET_LEASE_MS,
                    )
                    .await?
            };
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
        for index in rotated
            .iter()
            .copied()
            .filter(|index| tasks[*index].target_kind != "fortress")
        {
            let task = &tasks[index];
            if task.commander_lids.iter().all(|lid| {
                states
                    .iter()
                    .any(|state| state.lord_id == *lid && state.available_after_ms > now)
            }) {
                continue;
            }
            if let Some(target) = store
                .reserve_rbc_target(
                    account_id,
                    task.kingdom_id,
                    task.level_min,
                    task.level_max,
                    (task.source_x, task.source_y),
                    &task.algorithm,
                    now,
                    now + TARGET_LEASE_MS,
                )
                .await?
            {
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
                let packet =
                    tile_gaa_packet(server_header, task.kingdom_id, task.source_x, task.source_y)?;
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
        self.phase = AutomationPhase::Idle {
            due_ms: now + 15_000,
        };
        self.detail = "No eligible unleased target or free task commander; retrying".to_owned();
        Ok(None)
    }

    pub(super) async fn observe(&mut self, store: &Store, account_id: &str, raw: &str) {
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
            if let Some(target) = rejected_rbc {
                if target.kingdom_id == 10 {
                    // Berimond camps don't have cooldowns. If they error, they were defeated.
                    let _ = store.delete_rbc_target(account_id, target.kingdom_id, target.x, target.y).await;
                } else {
                    // Defer for 1 hour if we hit a server error on an RBC/Tower, as it's likely a cooldown.
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
                (PacingPolicy::default().after_cra_ack(&mut self.rng) * 1_000.0) as i64
            };
            self.phase = AutomationPhase::Idle {
                due_ms: self.safety_pause_until_ms.max(now + retry_ms),
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
                    if task.target_kind == "fortress" {
                        let _ = store.release_fortress_target(account_id, &target).await;
                    }
                    self.detail =
                        "No allocated commander is currently free; target released".to_owned();
                    return;
                };
                let policy = PacingPolicy::default();
                let due_seconds = policy.cra_due_at(
                    now as f64 / 1_000.0,
                    self.last_cra_ms.map(|value| value as f64 / 1_000.0),
                    &mut self.rng,
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
                let pause = PacingPolicy::default().after_cra_ack(&mut self.rng);
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
