use super::*;

#[derive(Debug)]
pub(super) enum RecruitPhase {
    Idle {
        due_ms: i64,
    },
    AwaitCastle {
        target: ActiveRecruitment,
        deadline_ms: i64,
    },
    ReadyOrder {
        target: ActiveRecruitment,
        sent: i64,
        due_ms: i64,
    },
    AwaitOrder {
        target: ActiveRecruitment,
        sent: i64,
        deadline_ms: i64,
    },
    ReadyHelp {
        target: ActiveRecruitment,
        due_ms: i64,
    },
    AwaitHelp {
        target: ActiveRecruitment,
        deadline_ms: i64,
    },
    ReadyMap {
        target: ActiveRecruitment,
        due_ms: i64,
    },
    AwaitMap {
        target: ActiveRecruitment,
        deadline_ms: i64,
    },
}

pub(super) struct RecruitmentAutomation {
    pub(super) phase: RecruitPhase,
    pub(super) last_castle_switch_ms: i64,
    pub(super) rng: Rng,
    pub(super) detail: String,
}

impl RecruitmentAutomation {
    pub(super) fn new() -> Self {
        Self {
            phase: RecruitPhase::Idle { due_ms: 0 },
            last_castle_switch_ms: 0,
            rng: Rng::from_entropy(),
            detail: "Recruit bot is stopped".to_owned(),
        }
    }

    pub(super) fn holds_transport(&self) -> bool {
        !matches!(self.phase, RecruitPhase::Idle { .. })
    }

    pub(super) async fn next_packet(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
    ) -> anyhow::Result<Option<String>> {
        let now = now_ms();
        let deadline = match &self.phase {
            RecruitPhase::AwaitCastle { deadline_ms, .. }
            | RecruitPhase::AwaitOrder { deadline_ms, .. }
            | RecruitPhase::AwaitHelp { deadline_ms, .. }
            | RecruitPhase::AwaitMap { deadline_ms, .. } => Some(*deadline_ms),
            _ => None,
        };
        if deadline.is_some_and(|value| now >= value) {
            self.detail = "Recruitment response timed out; retrying later".to_owned();
            self.phase = RecruitPhase::Idle {
                due_ms: now + 30_000,
            };
            return Ok(None);
        }
        match &self.phase {
            RecruitPhase::ReadyOrder {
                target,
                sent,
                due_ms,
            } if now >= *due_ms => {
                let navigation = store.navigation(account_id).await?;
                let castle_ready = navigation.as_ref().is_some_and(|state| {
                    !state.map_mode
                        && state.current_castle_id == Some(target.castle_id)
                        && state.current_kingdom_id == Some(target.kingdom_id)
                });
                if !castle_ready {
                    self.detail = format!(
                        "Castle context changed before recruitment at {}",
                        target.castle_id
                    );
                    self.phase = RecruitPhase::Idle {
                        due_ms: now + 2_000,
                    };
                    return Ok(None);
                }
                let packet = encode_client_xt(
                    server_header,
                    "bup",
                    "1",
                    &json!({
                        "LID": target.lane_id, "WID": target.troop_id, "AMT": target.quantity,
                        "PO": -1, "PWR": 0, "SK": target.skill_id,
                        "SID": target.kingdom_id, "AID": target.castle_id
                    }),
                )?;
                self.detail = format!(
                    "Recruiting slot {} of {} at castle {}",
                    sent + 1,
                    target.slot_count,
                    target.castle_id
                );
                self.phase = RecruitPhase::AwaitOrder {
                    target: target.clone(),
                    sent: *sent,
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
            RecruitPhase::ReadyHelp { target, due_ms } if now >= *due_ms => {
                let packet = encode_client_xt(
                    server_header,
                    "ahr",
                    "1",
                    &json!({"ID": target.lane_id, "T": 6}),
                )?;
                self.detail = format!("Requesting alliance help for castle {}", target.castle_id);
                self.phase = RecruitPhase::AwaitHelp {
                    target: target.clone(),
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
            RecruitPhase::ReadyMap { target, due_ms } if now >= *due_ms => {
                let ax1 = target.castle_x.div_euclid(13) * 13;
                let ay1 = target.castle_y.div_euclid(13) * 13;
                let packet = encode_client_xt(
                    server_header,
                    "gaa",
                    "1",
                    &json!({"KID": target.kingdom_id, "AX1": ax1, "AY1": ay1, "AX2": ax1 + 12, "AY2": ay1 + 12}),
                )?;
                self.detail = format!(
                    "Restoring {} map context before attacks resume",
                    target.kingdom_id
                );
                self.phase = RecruitPhase::AwaitMap {
                    target: target.clone(),
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
            RecruitPhase::ReadyOrder { .. }
            | RecruitPhase::ReadyHelp { .. }
            | RecruitPhase::ReadyMap { .. }
            | RecruitPhase::AwaitCastle { .. }
            | RecruitPhase::AwaitOrder { .. }
            | RecruitPhase::AwaitHelp { .. }
            | RecruitPhase::AwaitMap { .. } => return Ok(None),
            RecruitPhase::Idle { due_ms } if now < *due_ms => return Ok(None),
            RecruitPhase::Idle { .. } => {}
        }

        let targets = store.active_recruitments(account_id).await?;
        if targets.is_empty() {
            self.detail = "No running recruit bot for this account".to_owned();
            self.phase = RecruitPhase::Idle {
                due_ms: now + 2_000,
            };
            return Ok(None);
        }
        // Which castle to work on is a separate question from how fast to send,
        // so it does not depend on the chosen cadence: take whichever queue is
        // free and clears soonest, so no castle sits idle waiting its turn.
        let Some(target) = targets
            .iter()
            .filter(|target| target.queue_clear_at_ms <= now)
            .min_by_key(|target| (target.queue_clear_at_ms, target.castle_id))
            .cloned()
        else {
            let next = targets
                .iter()
                .map(|target| target.queue_clear_at_ms)
                .min()
                .unwrap_or(now + 30_000);
            self.detail = format!(
                "Recruit queues busy; next estimate {}s",
                next.saturating_sub(now) / 1_000
            );
            self.phase = RecruitPhase::Idle {
                due_ms: next.max(now + 1_000),
            };
            return Ok(None);
        };
        let switch_due = (self.last_castle_switch_ms + 3_000).max(now);
        if switch_due > now {
            self.phase = RecruitPhase::Idle { due_ms: switch_due };
            return Ok(None);
        }
        let packet = encode_client_xt(
            server_header,
            "jca",
            "1",
            &json!({"CID": target.castle_id, "KID": target.kingdom_id}),
        )?;
        self.last_castle_switch_ms = now;
        self.detail = format!("Opening castle {} for recruitment", target.castle_id);
        self.phase = RecruitPhase::AwaitCastle {
            target,
            deadline_ms: now + REQUEST_TIMEOUT_MS,
        };
        Ok(Some(packet))
    }

    pub(super) async fn observe(&mut self, store: &Store, account_id: &str, raw: &str) {
        let Ok(packet) = parse_xt_packet(raw) else {
            return;
        };
        let now = now_ms();
        if packet.status.as_deref().is_some_and(|status| status != "0") {
            if self.holds_transport() {
                self.detail = format!("Recruitment command {} rejected; retrying", packet.command);
                self.phase = RecruitPhase::Idle {
                    due_ms: now + 30_000,
                };
            }
            return;
        }
        match packet.command.as_str() {
            "jaa" => {
                let RecruitPhase::AwaitCastle { target, .. } = std::mem::replace(
                    &mut self.phase,
                    RecruitPhase::Idle {
                        due_ms: now + 30_000,
                    },
                ) else {
                    return;
                };
                let returned_castle = packet.payload.pointer("/gca/A/3").and_then(Value::as_i64);
                let returned_kingdom = packet
                    .payload
                    .get("KID")
                    .and_then(Value::as_i64)
                    .or_else(|| packet.payload.pointer("/gca/A/9").and_then(Value::as_i64));
                if returned_castle != Some(target.castle_id)
                    || returned_kingdom != Some(target.kingdom_id)
                {
                    self.detail = format!(
                        "Castle confirmation did not match {}; recruitment blocked",
                        target.castle_id
                    );
                    return;
                }
                self.phase = RecruitPhase::ReadyOrder {
                    target,
                    sent: 0,
                    due_ms: now + 1_500,
                };
            }
            "bup" => {
                let RecruitPhase::AwaitOrder { target, sent, .. } = std::mem::replace(
                    &mut self.phase,
                    RecruitPhase::Idle {
                        due_ms: now + 30_000,
                    },
                ) else {
                    return;
                };
                let spl = packet.payload.get("spl").unwrap_or(&packet.payload);
                let total = spl
                    .get("TCT")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                let active = spl
                    .get("PS")
                    .and_then(|v| v.get("TUA"))
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let queued = spl
                    .get("QS")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|v| {
                                v.get("P")
                                    .and_then(|p| p.get("TUA"))
                                    .and_then(serde_json::Value::as_i64)
                            })
                            .sum()
                    })
                    .unwrap_or(0);
                let next_sent = sent + 1;
                let queue_clear_at_ms = now + (total + 10) * 1_000;
                let state = RecruitCastleState {
                    account_id: account_id.to_owned(),
                    castle_id: target.castle_id,
                    task_id: Some(target.recruitment_id.clone()),
                    queue_clear_at_ms,
                    last_duration_s: total,
                    last_request_at_ms: now,
                    active_quantity: active,
                    queued_quantity: queued,
                    help_active: false,
                    last_status: if next_sent >= target.slot_count {
                        "queued"
                    } else {
                        "filling"
                    }
                    .to_owned(),
                };
                let _ = store.set_recruit_state(&state, now).await;
                if next_sent < target.slot_count {
                    // The gap between two slots is the operator's chosen cadence;
                    // `advanced` reproduces the recorded client (bounded Gaussian).
                    let tempo = RecruitTempo::from_name(&target.algorithm);
                    let delay = pacing::recruit_request_delay(tempo, &mut self.rng);
                    self.phase = RecruitPhase::ReadyOrder {
                        target,
                        sent: next_sent,
                        due_ms: now + (delay * 1_000.0) as i64,
                    };
                } else if target.ask_alliance_help {
                    self.phase = RecruitPhase::ReadyHelp {
                        target,
                        due_ms: now + 1_000,
                    };
                } else {
                    self.phase = RecruitPhase::ReadyMap {
                        target,
                        due_ms: now + 1_000,
                    };
                }
            }
            "ahr" => {
                let RecruitPhase::AwaitHelp { target, .. } = std::mem::replace(
                    &mut self.phase,
                    RecruitPhase::Idle {
                        due_ms: now + 30_000,
                    },
                ) else {
                    return;
                };
                if let Ok(mut states) = store.recruit_states(account_id).await
                    && let Some(state) = states
                        .iter_mut()
                        .find(|state| state.castle_id == target.castle_id)
                {
                    state.help_active = true;
                    state.last_status = "help_requested".to_owned();
                    let _ = store.set_recruit_state(state, now).await;
                }
                self.phase = RecruitPhase::ReadyMap {
                    target,
                    due_ms: now + 1_000,
                };
            }
            "gaa" => {
                let RecruitPhase::AwaitMap { target, .. } = std::mem::replace(
                    &mut self.phase,
                    RecruitPhase::Idle {
                        due_ms: now + 30_000,
                    },
                ) else {
                    return;
                };
                // Same cadence, widened: a castle change is a bigger step than the
                // next slot in the same queue.
                let extra = pacing::recruit_turn_delay(
                    RecruitTempo::from_name(&target.algorithm),
                    &mut self.rng,
                );
                self.detail = format!("Castle {} queued; attacks may resume", target.castle_id);
                self.phase = RecruitPhase::Idle {
                    due_ms: now + (extra * 1_000.0) as i64,
                };
            }
            _ => {}
        }
    }
}
