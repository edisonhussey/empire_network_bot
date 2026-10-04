use std::{sync::Arc, time::Duration};

use empire_core::{
    account::{castle_travel_options, commander_lids, owned_castles, rbc_targets},
    event::Direction,
    hunt::{self, MapTarget},
    injection::{InjectionRequest, channel},
    pacing::{self, PacingPolicy, RecruitTempo, Rng, Waits},
    protocol::{encode_client_xt, parse_xt_packet},
    session::{LoginCredentials, SessionMachine, SessionPhase, SessionSettings},
    store::{
        ActiveModeTask, ActiveRecruitment, COMMANDER_AVAILABLE, COMMANDER_OUTBOUND, CommanderState,
        HUNT_HEARTBEAT_KEY, MARCH_SENT, MarchRecord, RecruitCastleState, ReservedTarget, Store,
    },
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::header::ORIGIN},
};
use tracing::{info, warn};

use crate::licence::LicenceGate;

const US1_ENDPOINT: &str = "wss://ep-live-us1-game.goodgamestudios.com/";
const WORLD2_ENDPOINT: &str = "wss://ep-live-world2-game.goodgamestudios.com/";
const US1_SERVER_HEADER: &str = "EmpireEx_21";
const WORLD2_SERVER_HEADER: &str = "EmpireEx_49";
const VENTRILO_PORTAL_ACCOUNT_ID: &str = "1780270034676433896";
// Confirmed by Pingpoko's successful `lli` capture. This is a portal identity,
// not the much smaller in-world owner ID, and it is account-specific.
const PINGPOKO_PORTAL_ACCOUNT_ID: &str = "1782860727866351909";

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum GameServer {
    #[serde(rename = "US1")]
    Us1,
    #[serde(rename = "WORLD2")]
    World2,
}

impl GameServer {
    fn connection(self) -> (&'static str, &'static str, &'static str) {
        match self {
            GameServer::Us1 => (US1_ENDPOINT, US1_SERVER_HEADER, VENTRILO_PORTAL_ACCOUNT_ID),
            GameServer::World2 => (
                WORLD2_ENDPOINT,
                WORLD2_SERVER_HEADER,
                PINGPOKO_PORTAL_ACCOUNT_ID,
            ),
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct InitializeAccountRequest {
    pub server: GameServer,
    pub username: String,
    pub password: String,
    #[serde(default = "default_scan_radius")]
    pub scan_radius: u16,
    /// Reuse learned targets without refreshing map windows. Normal logins set
    /// this; first-time setup and the explicit "scan more" action do not.
    #[serde(default)]
    pub reuse_existing_map: bool,
}

const fn default_scan_radius() -> u16 {
    50
}

impl InitializeAccountRequest {
    pub fn into_direct(self) -> DirectConnectRequest {
        let (endpoint, server_header, portal_account_id) = self.server.connection();
        let settings = SessionSettings {
            map_scan_radius: self.scan_radius,
            server_header: server_header.to_owned(),
            ..SessionSettings::default()
        };
        DirectConnectRequest {
            endpoint: endpoint.to_owned(),
            credentials: LoginCredentials {
                player_name: self.username.trim().to_owned(),
                portal_account_id: portal_account_id.to_owned(),
                password: Some(self.password),
                login_token: None,
                registration_token: None,
            },
            settings,
            reuse_existing_map: self.reuse_existing_map,
        }
    }
}

#[cfg(test)]
mod server_tests {
    use super::*;

    #[test]
    fn world_profiles_keep_socket_and_portal_identity_together() {
        assert_eq!(
            GameServer::Us1.connection(),
            (US1_ENDPOINT, US1_SERVER_HEADER, VENTRILO_PORTAL_ACCOUNT_ID)
        );
        assert_eq!(
            GameServer::World2.connection(),
            (
                WORLD2_ENDPOINT,
                WORLD2_SERVER_HEADER,
                PINGPOKO_PORTAL_ACCOUNT_ID
            )
        );
        assert_ne!(
            GameServer::Us1.connection().2,
            GameServer::World2.connection().2
        );
        assert_ne!(
            GameServer::Us1.connection().1,
            GameServer::World2.connection().1
        );
    }
}

#[derive(Clone, Deserialize)]
pub struct DirectConnectRequest {
    pub endpoint: String,
    pub credentials: LoginCredentials,
    #[serde(default)]
    pub settings: SessionSettings,
    #[serde(default)]
    pub reuse_existing_map: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectStatus {
    pub connected: bool,
    pub phase: SessionPhase,
    pub endpoint: Option<String>,
    pub account_id: Option<String>,
    pub error: Option<String>,
    pub scan_total: u64,
    pub scan_sent: u64,
    pub scan_cached: u64,
    pub bot_state: String,
    pub bot_detail: Option<String>,
    /// When the socket was established. The UI shows how long the session has
    /// been up from this, and it survives closing the window because the
    /// service owns the session, not the window.
    pub connected_at_ms: Option<i64>,
}

impl Default for DirectStatus {
    fn default() -> Self {
        Self {
            connected: false,
            phase: SessionPhase::Disconnected,
            endpoint: None,
            account_id: None,
            error: None,
            scan_total: 0,
            scan_sent: 0,
            scan_cached: 0,
            bot_state: "stopped".to_owned(),
            bot_detail: None,
            connected_at_ms: None,
        }
    }
}

pub async fn run(
    request: DirectConnectRequest,
    store: Store,
    active_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: Arc<RwLock<DirectStatus>>,
    licence: LicenceGate,
) {
    let endpoint = request.endpoint.clone();
    let account_id = request.credentials.player_name.trim().to_ascii_lowercase();
    status.write().await.endpoint = Some(endpoint.clone());
    let result = run_inner(request, &store, &active_transport, &status, &licence).await;
    let _ = store.stop_account_mode(&account_id, now_ms()).await;
    let _ = store.stop_account_recruit_bot(&account_id, now_ms()).await;
    *active_transport.write().await = None;
    let mut current = status.write().await;
    current.connected = false;
    current.connected_at_ms = None;
    if let Err(error) = result {
        current.phase = SessionPhase::Failed;
        current.error = Some(error.to_string());
        warn!(%endpoint, %error, "direct game session stopped");
    }
}

async fn run_inner(
    request: DirectConnectRequest,
    store: &Store,
    active_transport: &Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: &Arc<RwLock<DirectStatus>>,
    licence: &LicenceGate,
) -> anyhow::Result<()> {
    licence.require("game_network").await?;
    let account_id = request.credentials.player_name.trim().to_ascii_lowercase();
    let reuse_existing_map =
        request.reuse_existing_map && store.account_has_targets(&account_id, 1).await?;
    store
        .upsert_account_profile(
            &account_id,
            &request.credentials.player_name,
            &request.endpoint,
            &request.settings.server_header,
            now_ms(),
        )
        .await?;
    let mut websocket_request = request.endpoint.as_str().into_client_request()?;
    websocket_request
        .headers_mut()
        .insert(ORIGIN, "https://empire.goodgamestudios.com".parse()?);
    let (socket, _) = connect_async(websocket_request).await?;
    let (mut sink, mut stream) = socket.split();
    let (injection_tx, mut injection_rx) = channel();
    *active_transport.write().await = Some(injection_tx);

    let mut machine = SessionMachine::new(request.credentials, request.settings.clone());
    for frame in machine.on_connected() {
        sink.send(Message::Text(frame.into())).await?;
    }
    *status.write().await = DirectStatus {
        connected: true,
        phase: machine.phase(),
        endpoint: Some(request.endpoint.clone()),
        account_id: Some(account_id.clone()),
        error: None,
        scan_total: 0,
        scan_sent: 0,
        scan_cached: 0,
        bot_state: "stopped".to_owned(),
        bot_detail: None,
        connected_at_ms: Some(now_ms()),
    };
    info!(endpoint = %request.endpoint, "direct game session connected");

    let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
    let mut entitlement_check = tokio::time::interval(Duration::from_secs(5));
    let mut automation_tick = tokio::time::interval(Duration::from_millis(250));
    let mut automation = Automation::new();
    let mut recruitment = RecruitmentAutomation::new();
    heartbeat.tick().await;
    entitlement_check.tick().await;
    loop {
        tokio::select! {
            maybe = stream.next() => {
                let Some(message) = maybe else { break; };
                let message = message?;
                let text = match message {
                    Message::Text(text) => Some(text.to_string()),
                    Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
                    _ => None,
                };
                if let Some(text) = text {
                    observe_account_packet(store, &account_id, &text).await;
                    record_text(store, Direction::ServerToClient, &text).await;
                    automation.observe(store, &account_id, &text).await;
                    recruitment.observe(store, &account_id, &text).await;
                    let frames = machine.on_server_text(&text)?;
                    status.write().await.phase = machine.phase();
                    let map_request_count = frames
                        .iter()
                        .filter(|frame| map_request(frame).is_some())
                        .count() as u64;
                    if map_request_count > 0 {
                        let mut current = status.write().await;
                        current.scan_total = map_request_count;
                        current.scan_sent = 0;
                        current.scan_cached = 0;
                    }
                    let mut saw_map_request = false;
                    let mut sent_map_requests = 0_u64;
                    for frame in frames {
                        if let Some((kingdom_id, ax1, ay1, _, _)) = map_request(&frame) {
                            saw_map_request = true;
                            if reuse_existing_map
                                || store
                                    .scan_window_is_fresh(
                                        &account_id,
                                        kingdom_id,
                                        ax1,
                                        ay1,
                                        now_ms(),
                                    )
                                    .await?
                            {
                                status.write().await.scan_cached += 1;
                                continue;
                            }
                            if sent_map_requests > 0 {
                                tokio::time::sleep(Duration::from_millis(scan_delay_ms(
                                    kingdom_id, ax1, ay1,
                                )))
                                .await;
                            }
                            sent_map_requests += 1;
                            status.write().await.scan_sent = sent_map_requests;
                            if sent_map_requests.is_multiple_of(40) {
                                sink.send(Message::Text(
                                    heartbeat_packet(&request.settings.server_header).into(),
                                ))
                                .await?;
                            }
                        }
                        record_safe_outbound(store, &account_id, &frame).await;
                        sink.send(Message::Text(frame.into())).await?;
                    }
                    if saw_map_request && sent_map_requests == 0 {
                        machine.accept_cached_map();
                        status.write().await.phase = machine.phase();
                    }
                }
            }
            maybe = injection_rx.recv() => {
                let Some(request) = maybe else { break; };
                if request.expired(now_ms()) {
                    continue;
                }
                record_text(store, Direction::Injected, &request.packet).await;
                sink.send(Message::Text(request.packet.into())).await?;
            }
            _ = heartbeat.tick() => {
                let packet = heartbeat_packet(&request.settings.server_header);
                sink.send(Message::Text(packet.into())).await?;
            }
            _ = entitlement_check.tick() => {
                licence.require("game_network").await?;
            }
            _ = automation_tick.tick() => {
                if machine.phase() == SessionPhase::SandsReady {
                    let recruit_packet = if recruitment.holds_transport() || !automation.holds_transport() {
                        recruitment.next_packet(store, &account_id, &request.settings.server_header).await?
                    } else { None };
                    if let Some(packet) = recruit_packet {
                        record_safe_outbound(store, &account_id, &packet).await;
                        sink.send(Message::Text(packet.into())).await?;
                    } else if !recruitment.holds_transport() && let Some(packet) = automation
                            .next_packet(store, &account_id, &request.settings.server_header)
                            .await?
                    {
                        record_safe_outbound(store, &account_id, &packet).await;
                        sink.send(Message::Text(packet.into())).await?;
                    }
                    let (bot_state, bot_detail) = if recruitment.holds_transport() { ("recruiting", recruitment.detail.clone()) } else { automation.status() };
                    let mut current = status.write().await;
                    current.bot_state = bot_state.to_owned();
                    current.bot_detail = Some(bot_detail);
                }
            }
        }
    }
    Ok(())
}

const REQUEST_TIMEOUT_MS: i64 = 40_000;
const TARGET_LEASE_MS: i64 = 12 * 60 * 1_000;
// A rejected CRA never launched the commander. Keep only a short backoff so a
// bad option cannot suppress the whole configured commander pool for minutes.
const COMMANDER_REJECT_HOLD_MS: i64 = 15_000;

#[derive(Debug)]
enum RecruitPhase {
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

struct RecruitmentAutomation {
    phase: RecruitPhase,
    last_castle_switch_ms: i64,
    rng: Rng,
    detail: String,
}

impl RecruitmentAutomation {
    fn new() -> Self {
        Self {
            phase: RecruitPhase::Idle { due_ms: 0 },
            last_castle_switch_ms: 0,
            rng: Rng::from_entropy(),
            detail: "Recruit bot is stopped".to_owned(),
        }
    }

    fn holds_transport(&self) -> bool {
        !matches!(self.phase, RecruitPhase::Idle { .. })
    }

    async fn next_packet(
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

    async fn observe(&mut self, store: &Store, account_id: &str, raw: &str) {
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

#[derive(Debug)]
enum AutomationPhase {
    Idle {
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

struct Automation {
    phase: AutomationPhase,
    cursor: usize,
    last_cra_ms: Option<i64>,
    last_heartbeat_ms: i64,
    rng: Rng,
    detail: String,
}

impl Automation {
    fn new() -> Self {
        Self {
            phase: AutomationPhase::Idle { due_ms: 0 },
            cursor: 0,
            last_cra_ms: None,
            last_heartbeat_ms: 0,
            rng: Rng::from_entropy(),
            detail: "Mode is stopped".to_owned(),
        }
    }

    fn status(&self) -> (&'static str, String) {
        let state = match self.phase {
            AutomationPhase::Idle { .. } => "waiting",
            AutomationPhase::AwaitAdi { .. } => "inspecting_target",
            AutomationPhase::ReadyCra { .. } => "pacing_attack",
            AutomationPhase::AwaitCra { .. } => "awaiting_attack_ack",
        };
        (state, self.detail.clone())
    }

    fn holds_transport(&self) -> bool {
        !matches!(self.phase, AutomationPhase::Idle { .. })
    }

    async fn next_packet(
        &mut self,
        store: &Store,
        account_id: &str,
        server_header: &str,
    ) -> anyhow::Result<Option<String>> {
        let now = now_ms();
        if now.saturating_sub(self.last_heartbeat_ms) >= 5_000 {
            if store.account_mode_running(account_id).await? {
                store
                    .set_app_state(HUNT_HEARTBEAT_KEY, &json!(now), now)
                    .await?;
            }
            self.last_heartbeat_ms = now;
        }

        match &self.phase {
            AutomationPhase::AwaitAdi { deadline_ms, .. }
            | AutomationPhase::AwaitCra { deadline_ms, .. }
                if now >= *deadline_ms =>
            {
                self.phase = AutomationPhase::Idle {
                    due_ms: now + 10_000,
                };
                return Ok(None);
            }
            _ => {}
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
        for offset in 0..tasks.len() {
            let index = (self.cursor + offset) % tasks.len();
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
                let packet = hunt::adi_packet(
                    server_header,
                    (task.source_x, task.source_y),
                    &as_map_target(&target),
                )?;
                self.detail = format!(
                    "Inspecting {}:{}:{} for task {}",
                    target.kingdom_id, target.x, target.y, task.name
                );
                self.phase = AutomationPhase::AwaitAdi {
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

    async fn observe(&mut self, store: &Store, account_id: &str, raw: &str) {
        let Ok(packet) = parse_xt_packet(raw) else {
            return;
        };
        let now = now_ms();
        if let Some(status) = packet.status.as_deref().filter(|status| *status != "0") {
            let expected_rejection = matches!(
                (&self.phase, packet.command.as_str()),
                (AutomationPhase::AwaitAdi { .. }, "adi")
                    | (AutomationPhase::AwaitCra { .. }, "cra")
            );
            if !expected_rejection {
                return;
            }
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
            self.phase = AutomationPhase::Idle {
                due_ms: now + COMMANDER_REJECT_HOLD_MS,
            };
            return;
        }

        match packet.command.as_str() {
            "adi" => {
                let AutomationPhase::AwaitAdi { task, target, .. } = std::mem::replace(
                    &mut self.phase,
                    AutomationPhase::Idle {
                        due_ms: now + 10_000,
                    },
                ) else {
                    return;
                };
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
                let _ = store.mark_target_attacked(account_id, &target, now).await;
                let pause = PacingPolicy::default().after_cra_ack(&mut self.rng)
                    + Waits::attack_send(&mut self.rng);
                self.phase = AutomationPhase::Idle {
                    due_ms: now + (pause * 1_000.0) as i64,
                };
                self.detail = format!("Attack {march_id} acknowledged; scheduling next target");
            }
            "cat" => {
                let (Some((kingdom_id, x, y)), Some(lord_id)) = (
                    hunt::return_target(&packet.payload),
                    hunt::returned_lord_id(&packet.payload),
                ) else {
                    return;
                };
                let loot = hunt::loot_from_return(&packet.payload);
                let return_seconds = hunt::return_seconds_from_return(&packet.payload);
                let _ = store
                    .finish_march_by_target(
                        account_id,
                        kingdom_id,
                        x,
                        y,
                        lord_id,
                        return_seconds,
                        loot.map(|value| value.0),
                        loot.map(|value| value.1),
                        hunt::result_flag_from_return(&packet.payload),
                        now,
                    )
                    .await;
                if let Some(seconds) = return_seconds {
                    let available_after_ms = now
                        + ((seconds.max(0) as f64 + Waits::commander_return_hold(&mut self.rng))
                            * 1_000.0) as i64;
                    let state = CommanderState {
                        account_id: account_id.to_owned(),
                        lord_id,
                        status: COMMANDER_OUTBOUND.to_owned(),
                        available_after_ms,
                        march_id: None,
                        target_key: Some(format!("{kingdom_id}:{x}:{y}")),
                    };
                    let _ = store.set_commander_state(&state, now).await;
                    self.detail =
                        format!("Commander {lord_id} returning for about {seconds}s before reuse");
                }
            }
            _ => {}
        }
    }
}

fn as_map_target(target: &ReservedTarget) -> MapTarget {
    MapTarget {
        kingdom_id: target.kingdom_id,
        x: target.x,
        y: target.y,
        level: target.level,
    }
}

fn heartbeat_packet(server_header: &str) -> String {
    format!("%xt%{server_header}%pin%1%<RoundHouseKick>%")
}

async fn observe_account_packet(store: &Store, account_id: &str, raw: &str) {
    let Ok(packet) = parse_xt_packet(raw) else {
        return;
    };
    if packet.status.as_deref().is_some_and(|status| status != "0") {
        return;
    }
    let result = match packet.command.as_str() {
        "gbd" => {
            let castles = owned_castles(&packet.payload);
            let commanders = commander_lids(&packet.payload);
            let travel_options = castle_travel_options(&packet.payload);
            if castles.is_empty() {
                return;
            }
            if let Err(error) = store
                .replace_account_bootstrap(account_id, &castles, &commanders, now_ms())
                .await
            {
                Err(error)
            } else {
                store
                    .upsert_castle_travel_options(account_id, &travel_options, now_ms())
                    .await
            }
        }
        "gaa" => {
            let targets = rbc_targets(&packet.payload);
            if targets.is_empty() {
                return;
            }
            store
                .upsert_rbc_targets(account_id, &targets, now_ms())
                .await
        }
        _ => return,
    };
    if let Err(error) = result {
        warn!(%error, %account_id, command = %packet.command, "account discovery persistence failed");
    }
}

async fn record_safe_outbound(store: &Store, account_id: &str, raw: &str) {
    if let Ok(packet) = parse_xt_packet(raw) {
        if packet.command == "lli" {
            return;
        }
        if packet.command == "gaa" {
            let values = ["AX1", "AY1", "AX2", "AY2"]
                .map(|key| packet.payload.get(key).and_then(serde_json::Value::as_i64));
            if let (Some(kingdom_id), [Some(ax1), Some(ay1), Some(ax2), Some(ay2)]) = (
                packet
                    .payload
                    .get("KID")
                    .and_then(serde_json::Value::as_i64),
                values,
            ) && let Err(error) = store
                .record_scan_window(account_id, kingdom_id, (ax1, ay1, ax2, ay2), now_ms())
                .await
            {
                warn!(%error, %account_id, "map scan coverage persistence failed");
            }
        }
    }
    record_text(store, Direction::ClientToServer, raw).await;
}

fn map_request(raw: &str) -> Option<(i64, i64, i64, i64, i64)> {
    let packet = parse_xt_packet(raw).ok()?;
    if packet.command != "gaa" {
        return None;
    }
    Some((
        packet.payload.get("KID")?.as_i64()?,
        packet.payload.get("AX1")?.as_i64()?,
        packet.payload.get("AY1")?.as_i64()?,
        packet.payload.get("AX2")?.as_i64()?,
        packet.payload.get("AY2")?.as_i64()?,
    ))
}

fn scan_delay_ms(kingdom_id: i64, ax1: i64, ay1: i64) -> u64 {
    let mixed = kingdom_id
        .wrapping_mul(31)
        .wrapping_add(ax1.wrapping_mul(17))
        .wrapping_add(ay1.wrapping_mul(13));
    700 + u64::try_from(mixed.rem_euclid(601)).unwrap_or(0)
}

async fn record_text(store: &Store, direction: Direction, raw: &str) {
    let parsed = parse_xt_packet(raw).ok();
    let command = parsed.as_ref().map(|packet| packet.command.clone());
    let payload = parsed
        .map(|packet| packet.payload)
        .unwrap_or_else(|| json!({"system_frame": true}));
    if let Err(error) = store
        .record_message(now_ms(), direction, command.as_deref(), &payload)
        .await
    {
        warn!(%error, "failed to persist direct-session event");
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
