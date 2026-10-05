use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    fortress::{FortressBlock, LATTICE_OFFSETS},
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

/// Delay after a confirmed fortress probe. Only one request is in flight at a
/// time, so this is the whole scan's tempo: about 1.15 s per probe turns a
/// 578-probe kingdom into the ten minutes a single-kingdom run costs.
fn fortress_probe_delay_seconds(rng: &mut Rng) -> f64 {
    rng.uniform(1.05, 1.25)
}

/// Extra pause when the walk moves to the next kingdom, so a kingdom boundary
/// is not a burst of requests.
fn fortress_kingdom_switch_delay_seconds(rng: &mut Rng) -> f64 {
    rng.uniform(3.4, 4.8)
}

#[derive(Debug, Clone)]
struct PendingBaseScan {
    frame: String,
    kingdom_id: i64,
    bounds: (i64, i64, i64, i64),
    sent_at_ms: i64,
    retries: u8,
}

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

#[cfg(test)]
mod server_tests {
    use super::*;

    /// One request has to answer four slots, so the window is the smallest
    /// rectangle containing a whole lattice block.
    #[test]
    fn a_fortress_probe_asks_for_a_whole_block() {
        let packet =
            fortress_gaa_packet("EmpireEx_21", 1, FortressBlock { x: 633, y: 633 }).unwrap();
        assert!(packet.contains("\"AX1\":632"), "{packet}");
        assert!(packet.contains("\"AX2\":673"), "{packet}");
        assert!(packet.contains("\"AY1\":632"), "{packet}");
        assert!(packet.contains("\"AY2\":673"), "{packet}");
    }

    #[test]
    fn fortress_probe_pacing_matches_the_scan_budget() {
        let mut rng = Rng::seeded(42);
        let samples = (0..1_000)
            .map(|_| fortress_probe_delay_seconds(&mut rng))
            .collect::<Vec<_>>();
        assert!(samples.iter().all(|delay| (1.05..=1.25).contains(delay)));
        // 578 probes at this tempo is the ten minutes a kingdom should cost.
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let minutes = 578.0 * mean / 60.0;
        assert!((9.5..=12.5).contains(&minutes), "{minutes} min per kingdom");
    }

    #[test]
    fn a_kingdom_switch_pauses_longer_than_a_probe() {
        let mut rng = Rng::seeded(7);
        let switches = (0..1_000)
            .map(|_| fortress_kingdom_switch_delay_seconds(&mut rng))
            .collect::<Vec<_>>();
        assert!(switches.iter().all(|delay| (3.4..=4.8).contains(delay)));
        assert!(switches[0] > fortress_probe_delay_seconds(&mut Rng::seeded(7)));
    }

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
        assert_eq!(
            trusted_server_name(US1_ENDPOINT, US1_SERVER_HEADER),
            Some("US1")
        );
        assert_eq!(
            trusted_server_name(WORLD2_ENDPOINT, WORLD2_SERVER_HEADER),
            Some("WORLD2")
        );
        assert_eq!(
            trusted_server_name("wss://attacker.invalid/", US1_SERVER_HEADER),
            None
        );
    }

    #[tokio::test]
    async fn jaa_and_gaa_are_authoritative_castle_and_map_context() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store
            .upsert_account_profile("ventrilo", "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
            .await
            .unwrap();
        persist_navigation_packet(
            &store,
            "ventrilo",
            "jaa",
            &json!({
                "KID": 0,
                "gca": {"A": [1, 509, 405, 16011862, 16862926, 7, 7, 7, 3, 0]}
            }),
            10,
        )
        .await
        .unwrap();
        let castle = store.navigation("ventrilo").await.unwrap().unwrap();
        assert!(!castle.map_mode);
        assert_eq!(castle.current_kingdom_id, Some(0));
        assert_eq!(castle.current_castle_id, Some(16011862));

        persist_navigation_packet(&store, "ventrilo", "gaa", &json!({"KID": 1, "AI": []}), 20)
            .await
            .unwrap();
        let map = store.navigation("ventrilo").await.unwrap().unwrap();
        assert!(map.map_mode);
        assert_eq!(map.current_kingdom_id, Some(1));
        assert_eq!(map.current_castle_id, None);
        assert_eq!(map.last_castle_switch_at_ms, 10);
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
            current_kingdom_id: None,
            current_castle_id: None,
            map_mode: false,
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
    let mut fortress_initialization_started = false;
    let mut fortress_initialization_complete = false;
    let mut fortress_started = HashSet::new();
    let mut fortress_initialization_due_ms = 0_i64;
    let mut fortress_sweep_index = 0_usize;
    // One sweep per grid per kingdom, kingdom by kingdom, so a kingdom finishes
    // before the next one starts costing requests. Ice leads unless the account
    // asked for a different order.
    let fortress_sweeps = request
        .settings
        .outer_kingdom_scans()
        .into_iter()
        .flat_map(|scan| {
            LATTICE_OFFSETS
                .into_iter()
                .map(move |offset| (scan.kingdom_id, offset))
        })
        .collect::<Vec<_>>();
    let mut active_fortress_block: Option<(i64, i64, FortressBlock)> = None;
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
                        store
                            .advance_fortress_scan(&account_id, *kingdom_id, *lattice_offset)
                            .await?;
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
                            + (scan_rng.normal(2.6, 0.45).clamp(1.8, 4.0) * 1_000.0)
                                as i64;
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
                    if machine.phase() != SessionPhase::SandsReady
                        || fortress_initialization_complete
                    {
                        status.write().await.phase = machine.phase();
                    } else if fortress_initialization_started {
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
                        if let Some((kingdom_id, ax1, ay1, _, _)) = map_request(&frame) {
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
                                        ax1,
                                        ay1,
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
                    && !fortress_initialization_complete
                {
                    let now = now_ms();
                    if !fortress_initialization_started {
                        // Let the final burst of the radius scan settle before
                        // pairing one response at a time with frontier probes.
                        fortress_initialization_started = true;
                        fortress_initialization_due_ms = now + 5_000;
                        status.write().await.phase = SessionPhase::DiscoveringFortresses;
                    }
                    if active_fortress_block.is_none() && now >= fortress_initialization_due_ms {
                        loop {
                            let Some(&(kingdom_id, lattice_offset)) =
                                fortress_sweeps.get(fortress_sweep_index)
                            else {
                                fortress_initialization_complete = true;
                                let mut current = status.write().await;
                                current.phase = SessionPhase::SandsReady;
                                current.scan_kingdom_id = None;
                                break;
                            };
                            if fortress_started.insert((kingdom_id, lattice_offset)) {
                                store
                                    .start_fortress_scan(&account_id, kingdom_id, lattice_offset)
                                    .await?;
                            }
                            let (done, pending) = store
                                .fortress_probe_progress(&account_id, kingdom_id)
                                .await?;
                            {
                                let mut current = status.write().await;
                                current.phase = SessionPhase::DiscoveringFortresses;
                                current.scan_kingdom_id = Some(kingdom_id);
                                current.scan_total = done + pending;
                                current.scan_sent = done;
                                current.scan_cached = 0;
                            }
                            let block = store
                                .next_fortress_block(&account_id, kingdom_id, lattice_offset)
                                .await?;
                            let Some(block) = block else {
                                fortress_sweep_index += 1;
                                fortress_initialization_due_ms = now_ms()
                                    + (fortress_kingdom_switch_delay_seconds(&mut fortress_rng)
                                        * 1_000.0) as i64;
                                continue;
                            };
                            let packet = fortress_gaa_packet(
                                &request.settings.server_header,
                                kingdom_id,
                                block,
                            )?;
                            record_safe_outbound(store, &account_id, &packet).await;
                            sink.send(Message::Text(packet.into())).await?;
                            active_fortress_block = Some((kingdom_id, lattice_offset, block));
                            break;
                        }
                    }
                }
            }
            _ = automation_tick.tick() => {
                if machine.phase() == SessionPhase::SandsReady
                    && fortress_initialization_complete
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

#[derive(Debug)]
enum AutomationPhase {
    Idle {
        due_ms: i64,
    },
    AwaitMap {
        task: ActiveModeTask,
        target: ReservedTarget,
        requested_at_ms: i64,
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
            AutomationPhase::AwaitMap { .. } => "opening_attack_map",
            AutomationPhase::AwaitFortressRefresh { .. } => "refreshing_fortress",
            AutomationPhase::ReadyAdi { .. } => "pacing_inspection",
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
            AutomationPhase::AwaitMap { deadline_ms, .. }
            | AutomationPhase::AwaitFortressRefresh { deadline_ms, .. }
            | AutomationPhase::AwaitAdi { deadline_ms, .. }
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
                    "Fortress dispatch window expired before ADI; target dropped".to_owned();
                self.phase = AutomationPhase::Idle { due_ms: now + 250 };
                return Ok(None);
            }
            let navigation = store.navigation(account_id).await?;
            let map_ready = navigation.as_ref().is_some_and(|state| {
                state.map_mode && state.current_kingdom_id == Some(task.kingdom_id)
            });
            if !map_ready {
                self.detail = format!(
                    "Map context changed before ADI; reopening kingdom {}",
                    task.kingdom_id
                );
                self.phase = AutomationPhase::Idle { due_ms: now + 500 };
                return Ok(None);
            }
            let packet = hunt::adi_packet(
                server_header,
                (task.source_x, task.source_y),
                &as_map_target(target),
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
                if task.target_kind != "fortress"
                    && navigation.as_ref().is_some_and(|state| {
                        state.map_mode && state.current_kingdom_id == Some(task.kingdom_id)
                    })
                {
                    self.phase = AutomationPhase::ReadyAdi {
                        task: task.clone(),
                        target,
                        due_ms: now,
                    };
                    return Ok(None);
                }
                let (map_x, map_y) = if task.target_kind == "fortress" {
                    (target.x, target.y)
                } else {
                    (task.source_x, task.source_y)
                };
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
                    "Opening kingdom {} map and refreshing {}:{}",
                    task.kingdom_id, target.x, target.y
                );
                self.phase = AutomationPhase::AwaitMap {
                    task: task.clone(),
                    target,
                    requested_at_ms: now,
                    deadline_ms: now + REQUEST_TIMEOUT_MS,
                };
                return Ok(Some(packet));
            }
        }

        // A failed or unobserved fortress landing is re-read from the server
        // before ordinary farming work. The due time was randomized 1-30
        // minutes beyond the expected landing when CRA was acknowledged.
        //
        // Before draining that queue, make sure it covers everything known: a
        // fortress mode builds its cooldown state from the coordinates
        // initialization discovered rather than learning it by attacking. Rows
        // already queued are untouched, so this is safe on every pass.
        for index in rotated
            .iter()
            .copied()
            .filter(|index| tasks[*index].target_kind == "fortress")
        {
            store
                .queue_fortress_recheck(account_id, tasks[index].kingdom_id, now)
                .await?;
        }
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
                    requested_at_ms: now,
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
            let rejected_fortress = match &self.phase {
                AutomationPhase::AwaitAdi { task, target, .. }
                | AutomationPhase::AwaitCra { task, target, .. }
                    if task.target_kind == "fortress" =>
                {
                    Some(target.clone())
                }
                _ => None,
            };
            if let Some(target) = rejected_fortress {
                let _ = store.release_fortress_target(account_id, &target).await;
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
                let AutomationPhase::AwaitMap {
                    task,
                    target,
                    requested_at_ms,
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
                if packet.payload.get("KID").and_then(Value::as_i64) != Some(task.kingdom_id) {
                    self.detail = "Wrong map kingdom returned; attack remains blocked".to_owned();
                    return;
                }
                if task.target_kind == "fortress"
                    && !store
                        .fortress_is_dispatchable(account_id, &target, now, Some(requested_at_ms))
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
                self.detail = format!("Kingdom {} map confirmed; preparing ADI", task.kingdom_id);
                self.phase = AutomationPhase::ReadyAdi {
                    task,
                    target,
                    due_ms: now + 500,
                };
            }
            "adi" => {
                let AutomationPhase::AwaitAdi { task, target, .. } = std::mem::replace(
                    &mut self.phase,
                    AutomationPhase::Idle {
                        due_ms: now + 10_000,
                    },
                ) else {
                    return;
                };
                if packet.payload.is_null() {
                    self.detail =
                        "ADI returned no attack data; retrying after map refresh".to_owned();
                    self.phase = AutomationPhase::Idle {
                        due_ms: now + 10_000,
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
                }
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
                let _ = store
                    .record_fortress_result(
                        account_id,
                        kingdom_id,
                        x,
                        y,
                        loot.map(|value| value.1),
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

/// The 13×13 client tile centred on one coordinate.
///
/// This is for *opening* a kingdom's map on a point of interest — a castle to
/// inspect, or a fortress whose cooldown is being re-read — not for discovery.
/// Discovery uses [`fortress_gaa_packet`], which asks for a whole lattice block.
fn tile_gaa_packet(
    server_header: &str,
    kingdom_id: i64,
    x: i64,
    y: i64,
) -> Result<String, empire_core::protocol::PacketError> {
    let ax1 = x.saturating_sub(6);
    let ay1 = y.saturating_sub(6);
    encode_client_xt(
        server_header,
        "gaa",
        "1",
        &json!({
            "KID": kingdom_id,
            "AX1": ax1,
            "AY1": ay1,
            "AX2": ax1 + 12,
            "AY2": ay1 + 12
        }),
    )
}

fn fortress_gaa_packet(
    server_header: &str,
    kingdom_id: i64,
    block: FortressBlock,
) -> Result<String, empire_core::protocol::PacketError> {
    // One request answers a whole 2×2 block of slots, so the window is the
    // smallest rectangle that contains all four of them.
    let window = block.window();
    encode_client_xt(
        server_header,
        "gaa",
        "1",
        &json!({
            "KID": kingdom_id,
            "AX1": window.left,
            "AY1": window.top,
            "AX2": window.right,
            "AY2": window.bottom
        }),
    )
}

fn heartbeat_packet(server_header: &str) -> String {
    format!("%xt%{server_header}%pin%1%<RoundHouseKick>%")
}

async fn observe_account_packet(
    store: &Store,
    account_id: &str,
    raw: &str,
    rbc_scan_origins: &HashMap<i64, (i64, i64)>,
    settings: &SessionSettings,
) {
    let Ok(packet) = parse_xt_packet(raw) else {
        return;
    };
    if packet.status.as_deref().is_some_and(|status| status != "0") {
        return;
    }
    if let Err(error) = persist_navigation_packet(
        store,
        account_id,
        &packet.command,
        &packet.payload,
        now_ms(),
    )
    .await
    {
        warn!(%error, %account_id, command = %packet.command, "navigation persistence failed");
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
            let mut targets = rbc_targets(&packet.payload);
            // The radius is per kingdom, and a kingdom the operator did not ask
            // for is not being scanned at all, so nothing from it is kept.
            targets.retain(|target| {
                let radius = i64::from(settings.kingdom_radius(target.kingdom_id));
                radius > 0
                    && rbc_scan_origins.get(&target.kingdom_id).is_some_and(
                        |(origin_x, origin_y)| {
                            (target.x - origin_x).abs() <= radius
                                && (target.y - origin_y).abs() <= radius
                        },
                    )
            });
            let fortresses = fortress_targets(&packet.payload);
            let observed_at_ms = now_ms();
            async {
                if !targets.is_empty() {
                    store
                        .upsert_rbc_targets(account_id, &targets, observed_at_ms)
                        .await?;
                }
                if !fortresses.is_empty() {
                    store
                        .upsert_fortress_targets(account_id, &fortresses, observed_at_ms)
                        .await?;
                }
                Ok::<(), sqlx::Error>(())
            }
            .await
        }
        _ => return,
    };
    if let Err(error) = result {
        warn!(%error, %account_id, command = %packet.command, "account discovery persistence failed");
    }
}

fn permanent_scan_origins(
    account_id: &str,
    castles: &[empire_core::store::OwnedCastleRecord],
) -> HashMap<i64, (i64, i64)> {
    castles
        .iter()
        .filter(|castle| castle.account_id.eq_ignore_ascii_case(account_id))
        .filter(|castle| {
            (castle.kingdom_id == 0 && castle.area_type == 1)
                || (castle.kingdom_id != 0 && castle.area_type == 12)
        })
        .map(|castle| (castle.kingdom_id, (castle.x, castle.y)))
        .collect()
}

async fn persist_navigation_packet(
    store: &Store,
    account_id: &str,
    command: &str,
    payload: &Value,
    observed_at_ms: i64,
) -> Result<(), sqlx::Error> {
    let previous = store.navigation(account_id).await?;
    let state = match command {
        "jaa" => {
            let row = payload.pointer("/gca/A").and_then(Value::as_array);
            let Some(castle_id) = row.and_then(|values| values.get(3)).and_then(Value::as_i64)
            else {
                return Ok(());
            };
            let kingdom_id = payload
                .get("KID")
                .and_then(Value::as_i64)
                .or_else(|| row.and_then(|values| values.get(9)).and_then(Value::as_i64));
            NavigationState {
                account_id: account_id.to_owned(),
                current_kingdom_id: kingdom_id,
                current_castle_id: Some(castle_id),
                map_mode: false,
                recruit_page: false,
                last_castle_switch_at_ms: observed_at_ms,
            }
        }
        "gaa" => {
            let Some(kingdom_id) = payload.get("KID").and_then(Value::as_i64) else {
                return Ok(());
            };
            NavigationState {
                account_id: account_id.to_owned(),
                current_kingdom_id: Some(kingdom_id),
                current_castle_id: None,
                map_mode: true,
                recruit_page: false,
                last_castle_switch_at_ms: previous
                    .as_ref()
                    .map_or(0, |state| state.last_castle_switch_at_ms),
            }
        }
        _ => return Ok(()),
    };
    store.set_navigation(&state, observed_at_ms).await
}

async fn record_safe_outbound(store: &Store, _account_id: &str, raw: &str) {
    if let Ok(packet) = parse_xt_packet(raw)
        && packet.command == "lli"
    {
        return;
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
