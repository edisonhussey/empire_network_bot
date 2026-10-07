mod automation;
mod base_scan;
mod fortress_discovery;
mod observe;
mod packets;
mod recruitment;
#[cfg(test)]
mod tests;

use automation::*;
use base_scan::*;
use fortress_discovery::*;
use observe::*;
use packets::*;
use recruitment::*;

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use empire_core::{
    account::{
        account_identity, castle_travel_options, commander_lids, fortress_targets, owned_castles,
        rbc_targets,
    },
    event::Direction,
    fortress::{
        ARM_EMPTY_LIMIT, ARM_MAX_STEPS, ARM_STEPS, BLOCK_STEP, Bounds, FortressBlock,
        SWEEP_ALIGNMENT, block_origin, bounds_covering,
    },
    hunt::{self, MapTarget},
    injection::{InjectionRequest, channel},
    pacing::{self, PacingPolicy, RecruitTempo, Rng, Waits},
    protocol::{encode_client_xt, parse_xt_packet},
    session::{KingdomScan, LoginCredentials, SessionMachine, SessionPhase, SessionSettings},
    store::{
        ActiveModeTask, ActiveRecruitment, COMMANDER_AVAILABLE, COMMANDER_OUTBOUND, CommanderState,
        HUNT_HEARTBEAT_KEY, MARCH_SENT, MarchRecord, NavigationState, RecruitCastleState,
        ReservedTarget, Store,
    },
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
    /// Which permanent kingdoms to initialise, in the order to walk them, each
    /// with its own map radius. Empty falls back to `scan_radius` everywhere.
    #[serde(default)]
    pub kingdom_scans: Vec<KingdomScan>,
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
            kingdom_scans: self.kingdom_scans,
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
    pub scan_kingdom_id: Option<i64>,
    /// Stable, user-facing phase identifier. This keeps the UI from guessing
    /// whether 1/1 means the nearby RBC pass or the fortress walk.
    pub scan_stage: Option<String>,
    pub current_kingdom_id: Option<i64>,
    pub current_castle_id: Option<i64>,
    pub map_mode: bool,
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
            scan_kingdom_id: None,
            scan_stage: None,
            current_kingdom_id: None,
            current_castle_id: None,
            map_mode: false,
            bot_state: "stopped".to_owned(),
            bot_detail: None,
            connected_at_ms: None,
        }
    }
}

/// The link to the game is gone for a reason that has nothing to do with the
/// account: the network dropped, the address changed, or the server stopped
/// talking. Anything else (a licence refusal, a login the server rejected) is
/// final, because retrying it would not help.
#[derive(Debug)]
struct ConnectionLost(String);

impl std::fmt::Display for ConnectionLost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "connection lost: {}", self.0)
    }
}

impl std::error::Error for ConnectionLost {}

fn connection_was_lost(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ConnectionLost>().is_some()
        || error.downcast_ref::<tokio_tungstenite::tungstenite::Error>().is_some()
}

/// The server sends something every few seconds while a session is alive; a
/// quiet minute and a half means the path is dead even if the socket has not
/// noticed yet. (After a Wi-Fi address change the socket took a minute and a
/// half to report a reset, and after a silent drop it can take far longer.)
const SERVER_SILENCE_LIMIT: Duration = Duration::from_secs(90);
/// Keep trying to get back for this long before declaring the session over.
const RECONNECT_GIVE_UP: Duration = Duration::from_secs(30 * 60);
/// A session shorter than this did not really come back, so it does not reset
/// the give-up clock. It also stops us fighting something that keeps kicking us.
const STABLE_SESSION: Duration = Duration::from_secs(2 * 60);

fn reconnect_delay(attempt: u32, rng: &mut Rng) -> Duration {
    let base = 5.0 * 2f64.powi(attempt.saturating_sub(1).min(4) as i32);
    Duration::from_secs_f64(base.min(60.0) + rng.uniform(0.0, 3.0))
}

pub async fn run(
    request: DirectConnectRequest,
    store: Store,
    active_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: Arc<RwLock<DirectStatus>>,
    licence: LicenceGate,
) {
    let endpoint = request.endpoint.clone();
    let _account_id = request.credentials.player_name.trim().to_ascii_lowercase();
    status.write().await.endpoint = Some(endpoint.clone());
    let mut attempt_request = request;
    let mut rng = Rng::from_entropy();
    let mut failing_since: Option<tokio::time::Instant> = None;
    let mut attempt = 0_u32;
    let final_error = loop {
        let result = run_inner(attempt_request.clone(), &store, &active_transport, &status, &licence).await;
        *active_transport.write().await = None;
        // The session's own bookkeeping says how far this attempt got and how
        // long it lived; read it before it is overwritten below.
        let (reached_ready, lived) = {
            let current = status.read().await;
            (
                matches!(
                    current.phase,
                    SessionPhase::SandsReady | SessionPhase::DiscoveringFortresses
                ),
                current
                    .connected_at_ms
                    .map(|at| Duration::from_millis(now_ms().saturating_sub(at).max(0) as u64)),
            )
        };
        let error = match result {
            Err(error) if connection_was_lost(&error) => error,
            // A session that never got going and then closed cleanly, or any
            // other failure, is not something a retry would fix.
            Err(error) => break Some(error),
            Ok(()) => break None,
        };
        if reached_ready && lived.is_some_and(|lived| lived >= STABLE_SESSION) {
            failing_since = None;
            attempt = 0;
        }
        let started = *failing_since.get_or_insert_with(tokio::time::Instant::now);
        attempt += 1;
        if started.elapsed() >= RECONNECT_GIVE_UP {
            break Some(error.context("gave up reconnecting"));
        }
        let delay = reconnect_delay(attempt, &mut rng);
        warn!(%endpoint, %error, attempt, delay_s = delay.as_secs(), "game connection lost; reconnecting");
        {
            let mut current = status.write().await;
            current.connected = false;
            current.connected_at_ms = None;
            current.phase = SessionPhase::Disconnected;
            current.error = Some(error.to_string());
            current.bot_state = "reconnecting".to_owned();
            current.bot_detail = Some(format!(
                "Connection lost; reconnecting in {}s (attempt {attempt})",
                delay.as_secs()
            ));
        }
        tokio::time::sleep(delay).await;
        // Reuse the learned map on the way back: a reconnect should resume
        // attacking, not rescan the world.
        attempt_request.reuse_existing_map = true;
    };
    // We intentionally do NOT call stop_account_mode here anymore.
    // The user's configuration is preserved so a later reconnect resumes it.
    *active_transport.write().await = None;
    let mut current = status.write().await;
    current.connected = false;
    current.connected_at_ms = None;
    if let Some(error) = final_error {
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
    licence.require_bootstrap("game_network").await?;
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
        scan_kingdom_id: None,
        scan_stage: None,
        current_kingdom_id: None,
        current_castle_id: None,
        map_mode: false,
        bot_state: "stopped".to_owned(),
        bot_detail: None,
        connected_at_ms: Some(now_ms()),
    };
    info!(endpoint = %request.endpoint, "direct game session connected");

    let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
    let mut entitlement_check = tokio::time::interval(Duration::from_secs(5));
    let mut automation_tick = tokio::time::interval(Duration::from_millis(250));
    let mut silence_check = tokio::time::interval(Duration::from_secs(10));
    let mut last_frame_at = tokio::time::Instant::now();
    let mut base_scan_tick = tokio::time::interval(Duration::from_millis(100));
    let mut fortress_tick = tokio::time::interval(Duration::from_millis(250));
    let mut automation = Automation::new();
    let mut recruitment = RecruitmentAutomation::new();
    let mut fortress_rng = Rng::from_entropy();
    let mut scan_rng = Rng::from_entropy();
    let mut base_scan_queue = VecDeque::<String>::new();
    let mut active_base_scan: Option<PendingBaseScan> = None;
    let mut base_scan_due_ms = 0_i64;
    let mut base_scan_requests_sent = 0_u64;
    let mut base_scan_stats = HashMap::<i64, (u64, u64)>::new();
    // Sweeps the running bot still needs, rebuilt whenever the running task set
    // or the outstanding work changes. Empty means automation is free to run.
    let mut fortress_sweeps: Vec<(i64, i64)> = Vec::new();
    let mut fortress_plan_due_ms = 0_i64;
    let mut fortress_initialization_due_ms = 0_i64;
    let mut fortress_rescan_prepared = false;
    let mut fortress_sweep_index = 0_usize;
    let mut fortress_phase_shown = false;
    let mut active_fortress_block: Option<(i64, i64, FortressBlock)> = None;
    // Only one kingdom is measured at a time, and only until its rectangle is
    // known; the fill below then walks that rectangle from the database.
    let mut fortress_discovery: Option<FortressDiscovery> = None;
    let mut identity_verified = false;
    let mut active_scan_kingdom = None;
    let mut rbc_scan_origins =
        permanent_scan_origins(&account_id, store.owned_castles().await?.as_slice());
    heartbeat.tick().await;
    entitlement_check.tick().await;
    loop {
        tokio::select! {
            maybe = stream.next() => {
                let Some(message) = maybe else { break; };
                let message = message?;
                last_frame_at = tokio::time::Instant::now();
                let text = match message {
                    Message::Text(text) => Some(text.to_string()),
                    Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
                    _ => None,
                };
                if let Some(text) = text {
                    let parsed_packet = parse_xt_packet(&text).ok();
                    if let Some(packet) = parsed_packet.as_ref()
                        && packet.command == "gbd"
                        && packet.status.as_deref().is_none_or(|status| status == "0")
                    {
                        let identity = account_identity(&packet.payload)
                            .context("authenticated GBD did not contain account identity")?;
                        licence
                            .bind_or_validate(
                                trusted_server_name(
                                    &request.endpoint,
                                    &request.settings.server_header,
                                )
                                .context("licence activation requires a known Goodgame world endpoint")?,
                                &identity,
                            )
                            .await?;
                        identity_verified = true;
                        info!(
                            player_id = identity.player_id,
                            castle_x = identity.main_castle_x,
                            castle_y = identity.main_castle_y,
                            "licence account identity verified"
                        );
                    }
                    observe_account_packet(
                        store,
                        &account_id,
                        &text,
                        &rbc_scan_origins,
                        &request.settings,
                        active_base_scan.is_some()
                            || (!reuse_existing_map && active_fortress_block.is_some()),
                    ).await;
                    if parsed_packet.as_ref().is_some_and(|packet| packet.command == "gbd") {
                        rbc_scan_origins = permanent_scan_origins(
                            &account_id,
                            store.owned_castles().await?.as_slice(),
                        );
                        let origins = rbc_scan_origins
                            .iter()
                            .map(|(kingdom_id, (x, y))| {
                                (
                                    *kingdom_id,
                                    *x,
                                    *y,
                                    i64::from(request.settings.kingdom_radius(*kingdom_id)),
                                )
                            })
                            .collect::<Vec<_>>();
                        store
                            .prune_rbc_targets_outside_radius(&account_id, &origins)
                            .await?;
                    }
                    if let (Some(packet), Some((kingdom_id, lattice_offset, _))) =
                        (parsed_packet.as_ref(), active_fortress_block.as_ref())
                        && packet.command == "gaa"
                        && packet.status.as_deref().is_none_or(|status| status == "0")
                        && packet.payload.get("KID").and_then(Value::as_i64) == Some(*kingdom_id)
                    {
                        // A discovery arm reads the window's contents; a cell of
                        // the fill only needs to know it was answered. A refused
                        // window comes back as an empty payload and counts as
                        // empty, which ends an arm early rather than extending it
                        // over ground nothing has confirmed.
                        let holds = !fortress_targets(&packet.payload).is_empty();
                        if let Some(discovery) = fortress_discovery.as_mut()
                            && discovery.kingdom_id == *kingdom_id
                        {
                            discovery.observe(holds);
                            let mut current = status.write().await;
                            current.scan_stage = Some("fortress_boundary".to_owned());
                            current.scan_kingdom_id = Some(*kingdom_id);
                            current.scan_total = 100;
                            current.scan_sent = discovery.estimated_percent();
                            current.scan_cached = 0;
                        } else {
                            store
                                .advance_fortress_scan(&account_id, *kingdom_id, *lattice_offset)
                                .await?;
                            let (done, pending) = store
                                .fortress_probe_progress(&account_id, *kingdom_id)
                                .await?;
                            let mut current = status.write().await;
                            current.scan_stage = Some("fortress_mapping".to_owned());
                            current.scan_kingdom_id = Some(*kingdom_id);
                            current.scan_total = done + pending;
                            current.scan_sent = done;
                            current.scan_cached = 0;
                        }
                        active_fortress_block = None;
                        fortress_initialization_due_ms = now_ms()
                            + (fortress_probe_delay_seconds(&mut fortress_rng) * 1_000.0) as i64;
                    }
                    if let (Some(packet), Some(active)) =
                        (parsed_packet.as_ref(), active_base_scan.as_ref())
                        && packet.command == "gaa"
                        && packet.status.as_deref().is_none_or(|status| status == "0")
                        && packet.payload.get("KID").and_then(Value::as_i64)
                            == Some(active.kingdom_id)
                    {
                        store
                            .record_scan_window(
                                &account_id,
                                active.kingdom_id,
                                active.bounds,
                                now_ms(),
                            )
                            .await?;
                        active_base_scan = None;
                        // Keep a single base-map request in flight, then add a
                        // variable quiet period after its successful response.
                        base_scan_due_ms = now_ms()
                            + (base_scan_delay_seconds(&mut scan_rng) * 1_000.0) as i64;
                    }
                    if parsed_packet
                        .as_ref()
                        .is_some_and(|packet| matches!(packet.command.as_str(), "jaa" | "gaa"))
                        && let Some(navigation) = store.navigation(&account_id).await?
                    {
                        let mut current = status.write().await;
                        current.current_kingdom_id = navigation.current_kingdom_id;
                        current.current_castle_id = navigation.current_castle_id;
                        current.map_mode = navigation.map_mode;
                    }
                    record_text(store, Direction::ServerToClient, &text).await;
                    automation.observe(store, &account_id, &text).await;
                    recruitment.observe(store, &account_id, &text).await;
                    let frames = machine.on_server_text(&text)?;
                    // While a walk is running the phase shown is the walk's, not
                    // the machine's: the socket has been ready for a while. Once
                    // the walk ends the machine's own phase is mirrored again.
                    if machine.phase() != SessionPhase::SandsReady || !fortress_phase_shown {
                        status.write().await.phase = machine.phase();
                    } else {
                        status.write().await.phase = SessionPhase::DiscoveringFortresses;
                    }
                    let map_request_count = frames
                        .iter()
                        .filter(|frame| map_request(frame).is_some())
                        .count() as u64;
                    if map_request_count > 0 {
                        let scan_kingdom_id = frames.iter().find_map(|frame| {
                            map_request(frame).map(|(kingdom_id, _, _, _, _)| kingdom_id)
                        });
                        if let Some(kingdom_id) = scan_kingdom_id {
                            base_scan_stats.insert(kingdom_id, (map_request_count, 0));
                        }
                    }
                    let mut queued_map_requests = 0_u64;
                    let mut cached_map_requests = 0_u64;
                    for frame in frames {
                        if let Some((kingdom_id, ax1, ay1, ax2, ay2)) = map_request(&frame) {
                            // Cached tiles save discovery traffic, but the first
                            // live GAA is mandatory navigation proof. Without it
                            // a stale scan can falsely label castle mode as a
                            // ready Sands map.
                            let has_fortress_seed = !matches!(kingdom_id, 1..=3)
                                || store
                                    .account_has_fortresses(&account_id, kingdom_id)
                                    .await?;
                            let cached = reuse_existing_map
                                && has_fortress_seed
                                && store
                                    .scan_window_is_fresh(
                                        &account_id,
                                        kingdom_id,
                                        (ax1, ay1, ax2, ay2),
                                        now_ms(),
                                    )
                                    .await?;
                            if cached && queued_map_requests > 0 {
                                cached_map_requests += 1;
                                continue;
                            }
                            queued_map_requests += 1;
                        }
                        base_scan_queue.push_back(frame);
                    }
                    if cached_map_requests > 0
                        && let Some(kingdom_id) = base_scan_queue
                            .iter()
                            .rev()
                            .find_map(|frame| map_request(frame).map(|request| request.0))
                        && let Some(stats) = base_scan_stats.get_mut(&kingdom_id)
                    {
                        stats.1 = cached_map_requests;
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
            _ = silence_check.tick() => {
                let quiet = last_frame_at.elapsed();
                if quiet >= SERVER_SILENCE_LIMIT {
                    return Err(ConnectionLost(format!(
                        "the game server has been silent for {}s",
                        quiet.as_secs()
                    ))
                    .into());
                }
            }
            _ = heartbeat.tick() => {
                let packet = heartbeat_packet(&request.settings.server_header);
                sink.send(Message::Text(packet.into())).await?;
            }
            _ = entitlement_check.tick() => {
                if identity_verified {
                    licence.require("game_network").await?;
                } else {
                    licence.require_bootstrap("game_network").await?;
                }
            }
            _ = base_scan_tick.tick() => {
                let now = now_ms();
                if let Some(active) = active_base_scan.as_mut() {
                    if now.saturating_sub(active.sent_at_ms) >= 12_000 {
                        if active.retries < 2 {
                            active.retries += 1;
                            active.sent_at_ms = now;
                            warn!(
                                kingdom_id = active.kingdom_id,
                                retry = active.retries,
                                "map scan response timed out; retrying the same window"
                            );
                            record_safe_outbound(store, &account_id, &active.frame).await;
                            sink.send(Message::Text(active.frame.clone().into())).await?;
                        } else {
                            warn!(
                                kingdom_id = active.kingdom_id,
                                "map scan window abandoned after two retries"
                            );
                            active_base_scan = None;
                            base_scan_due_ms = now + 4_000;
                        }
                    }
                } else if now >= base_scan_due_ms {
                    // Control frames introducing a kingdom may go immediately,
                    // but stop after one GAA until its response is observed.
                    while let Some(frame) = base_scan_queue.pop_front() {
                        let Some((kingdom_id, ax1, ay1, ax2, ay2)) = map_request(&frame) else {
                            record_safe_outbound(store, &account_id, &frame).await;
                            sink.send(Message::Text(frame.into())).await?;
                            continue;
                        };
                        if active_scan_kingdom != Some(kingdom_id) {
                            active_scan_kingdom = Some(kingdom_id);
                            base_scan_requests_sent = 0;
                            let (total, cached) = base_scan_stats
                                .get(&kingdom_id)
                                .copied()
                                .unwrap_or((1, 0));
                            let mut current = status.write().await;
                            current.scan_total = total;
                            current.scan_sent = 0;
                            current.scan_cached = cached;
                            current.scan_kingdom_id = Some(kingdom_id);
                            current.scan_stage = Some("rbc".to_owned());
                        }
                        base_scan_requests_sent += 1;
                        status.write().await.scan_sent = base_scan_requests_sent;
                        if base_scan_requests_sent.is_multiple_of(40) {
                            let heartbeat = heartbeat_packet(&request.settings.server_header);
                            sink.send(Message::Text(heartbeat.into())).await?;
                        }
                        record_safe_outbound(store, &account_id, &frame).await;
                        sink.send(Message::Text(frame.clone().into())).await?;
                        active_base_scan = Some(PendingBaseScan {
                            frame,
                            kingdom_id,
                            bounds: (ax1, ay1, ax2, ay2),
                            sent_at_ms: now,
                            retries: 0,
                        });
                        break;
                    }
                }
            }
            _ = fortress_tick.tick() => {
                if machine.phase() == SessionPhase::SandsReady
                    && base_scan_queue.is_empty()
                    && active_base_scan.is_none()
                {
                    let now = now_ms();
                    // Which sweeps the running bot needs. Rebuilt rather than
                    // fixed at connect, because the bot is chosen *after* the
                    // socket opens: a session that never starts a fortress bot
                    // never walks, and starting one mid-session picks the walk
                    // up. Fortress coordinates never change, so a kingdom covered
                    // in an earlier session drops out here and is never paid for
                    // twice.
                    if now >= fortress_plan_due_ms {
                        fortress_plan_due_ms = now + FORTRESS_PLAN_INTERVAL_MS;
                        let kingdoms = if reuse_existing_map {
                            // A normal connection does no discovery unless the
                            // running bot actually contains a fortress task.
                            store.active_fortress_kingdoms(&account_id).await?
                        } else {
                            // Explicit initialization discovers fortress
                            // coordinates in every selected, unlocked outer
                            // kingdom. This is independent of whether a bot has
                            // been created yet.
                            request
                                .settings
                                .outer_kingdom_scans()
                                .into_iter()
                                .map(|scan| scan.kingdom_id)
                                .collect()
                        };
                        let wanted = request
                            .settings
                            .outer_kingdom_scans()
                            .into_iter()
                            .filter(|scan| {
                                kingdoms.contains(&scan.kingdom_id)
                                    && rbc_scan_origins.contains_key(&scan.kingdom_id)
                            })
                            .map(|scan| (scan.kingdom_id, SWEEP_ALIGNMENT))
                            .collect::<Vec<_>>();
                        if !reuse_existing_map && !fortress_rescan_prepared {
                            store.reset_fortress_scans(&account_id, &wanted).await?;
                            fortress_rescan_prepared = true;
                        }
                        let outstanding = store
                            .outstanding_fortress_sweeps(&account_id, &wanted)
                            .await?;
                        if outstanding != fortress_sweeps {
                            info!(
                                before = fortress_sweeps.len(),
                                after = outstanding.len(),
                                "fortress walk plan changed"
                            );
                            fortress_sweeps = outstanding;
                            fortress_sweep_index = 0;
                            fortress_initialization_due_ms = 0;
                        }
                    }
                    if active_fortress_block.is_none()
                        && fortress_sweep_index < fortress_sweeps.len()
                    {
                        if !fortress_phase_shown {
                            // Let the radius scan's last burst settle before
                            // pairing one response at a time with probes.
                            fortress_phase_shown = true;
                            fortress_initialization_due_ms = now + 5_000;
                            status.write().await.phase = SessionPhase::DiscoveringFortresses;
                        }
                        if now >= fortress_initialization_due_ms
                            && let Some(&(kingdom_id, lattice_offset)) =
                                fortress_sweeps.get(fortress_sweep_index)
                        {
                            // A kingdom with no stored bounds has never been
                            // measured, so its arms run first and the fill waits.
                            // Once the rectangle is in the database the normal
                            // row-major path below takes over, and a kingdom
                            // finished in an earlier session is still free.
                            let known = store
                                .fortress_scan_bounds(&account_id, kingdom_id, lattice_offset)
                                .await?;
                            let exact_progress = if known.is_some() {
                                Some(
                                    store
                                        .fortress_probe_progress(&account_id, kingdom_id)
                                        .await?,
                                )
                            } else {
                                None
                            };
                            {
                                let mut current = status.write().await;
                                current.phase = SessionPhase::DiscoveringFortresses;
                                current.scan_kingdom_id = Some(kingdom_id);
                                current.scan_cached = 0;
                                if known.is_none() {
                                    current.scan_stage = Some("fortress_boundary".to_owned());
                                    current.scan_total = 100;
                                    current.scan_sent = fortress_discovery
                                        .as_ref()
                                        .filter(|value| value.kingdom_id == kingdom_id)
                                        .map_or(0, FortressDiscovery::estimated_percent);
                                } else {
                                    let (done, pending) = exact_progress.unwrap_or_default();
                                    current.scan_stage = Some("fortress_mapping".to_owned());
                                    current.scan_total = done + pending;
                                    current.scan_sent = done;
                                }
                            }
                            if known.is_none() {
                                // Only one kingdom is measured at a time; a walk
                                // left half-finished by a plan change is dropped.
                                if fortress_discovery
                                    .as_ref()
                                    .is_some_and(|discovery| discovery.kingdom_id != kingdom_id)
                                {
                                    fortress_discovery = None;
                                }
                                if fortress_discovery.is_none() {
                                    // The base is the block over the kingdom's own
                                    // castle: the RBC scan origins already hold it,
                                    // and a window spanning the castle is always
                                    // full, so every arm starts from a hit.
                                    let base = rbc_scan_origins
                                        .get(&kingdom_id)
                                        .map(|(x, y)| block_origin((*x, *y)))
                                        .unwrap_or_else(|| {
                                            block_origin((SWEEP_ALIGNMENT, SWEEP_ALIGNMENT))
                                        });
                                    fortress_discovery = Some(FortressDiscovery::new(
                                        kingdom_id, base,
                                    ));
                                }
                                let measured = fortress_discovery
                                    .as_ref()
                                    .is_some_and(|discovery| discovery.done);
                                if measured {
                                    // `bounds()` is `None` when not one window
                                    // held a fortress, which for a tasked kingdom
                                    // means the walk saw nothing it could trust.
                                    // The flat rectangle keeps the old behaviour
                                    // rather than recording a one-block kingdom.
                                    let bounds = fortress_discovery
                                        .as_ref()
                                        .and_then(FortressDiscovery::bounds)
                                        .unwrap_or_else(|| {
                                            empire_core::fortress::align_bounds(
                                                Bounds::outer_kingdom(),
                                                SWEEP_ALIGNMENT,
                                            )
                                        });
                                    fortress_discovery = None;
                                    store
                                        .set_fortress_scan_bounds(
                                            &account_id,
                                            kingdom_id,
                                            lattice_offset,
                                            bounds,
                                        )
                                        .await?;
                                    info!(
                                        kingdom_id,
                                        left = bounds.left,
                                        top = bounds.top,
                                        right = bounds.right,
                                        bottom = bounds.bottom,
                                        blocks = bounds.block_count(),
                                        "fortress rectangle measured"
                                    );
                                    fortress_initialization_due_ms = now_ms()
                                        + (fortress_kingdom_switch_delay_seconds(&mut fortress_rng)
                                            * 1_000.0)
                                            as i64;
                                } else if let Some(discovery) = fortress_discovery.as_mut()
                                    && let Some(block) = discovery.next()
                                {
                                    let packet = fortress_gaa_packet(
                                        &request.settings.server_header,
                                        kingdom_id,
                                        block,
                                    )?;
                                    record_safe_outbound(store, &account_id, &packet).await;
                                    sink.send(Message::Text(packet.into())).await?;
                                    active_fortress_block =
                                        Some((kingdom_id, lattice_offset, block));
                                }
                            } else {
                                match store
                                    .next_fortress_block(&account_id, kingdom_id, lattice_offset)
                                    .await?
                                {
                                    None => {
                                        // Already walked. Pause before the next
                                        // one, because a kingdom boundary should
                                        // not be a burst of requests.
                                        fortress_sweep_index += 1;
                                        fortress_initialization_due_ms = now_ms()
                                            + (fortress_kingdom_switch_delay_seconds(
                                                &mut fortress_rng,
                                            ) * 1_000.0)
                                                as i64;
                                    }
                                    Some(block) => {
                                        let packet = fortress_gaa_packet(
                                            &request.settings.server_header,
                                            kingdom_id,
                                            block,
                                        )?;
                                        record_safe_outbound(store, &account_id, &packet).await;
                                        sink.send(Message::Text(packet.into())).await?;
                                        active_fortress_block =
                                            Some((kingdom_id, lattice_offset, block));
                                    }
                                }
                            }
                        }
                    } else if fortress_phase_shown {
                        fortress_phase_shown = false;
                        let mut current = status.write().await;
                        current.phase = SessionPhase::SandsReady;
                        current.scan_stage = Some("complete".to_owned());
                    }
                }
            }
            _ = automation_tick.tick() => {
                // Automation waits only while a walk the running bot asked for is
                // still in progress. No fortress task means nothing to wait for.
                if machine.phase() == SessionPhase::SandsReady
                    && active_fortress_block.is_none()
                    && fortress_sweep_index >= fortress_sweeps.len()
                {
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

fn trusted_server_name(endpoint: &str, server_header: &str) -> Option<&'static str> {
    match (endpoint, server_header) {
        (US1_ENDPOINT, US1_SERVER_HEADER) => Some("US1"),
        (WORLD2_ENDPOINT, WORLD2_SERVER_HEADER) => Some("WORLD2"),
        _ => None,
    }
}

const REQUEST_TIMEOUT_MS: i64 = 40_000;
const TARGET_LEASE_MS: i64 = 12 * 60 * 1_000;
// A rejected CRA never launched the commander. Keep only a short backoff so a
// bad option cannot suppress the whole configured commander pool for minutes.
const COMMANDER_REJECT_HOLD_MS: i64 = 15_000;
const ERROR_WINDOW_MS: i64 = 5 * 60 * 1_000;


fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
