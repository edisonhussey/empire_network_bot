mod dev_shared;
mod direct;
mod licence;
mod plans;
mod pvp;
mod relay;
mod sessions;
mod station;
mod troops;

use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Query, State, ws::WebSocketUpgrade},
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use empire_core::{
    injection::InjectionRequest,
    protocol::parse_xt_packet,
    store::{Store, StoredMessage},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    store: Store,
    licence: licence::LicenceGate,
    /// The live game sessions, one per account. Each has its own socket, status,
    /// injection queue and diagnostic snapshot, so accounts run side by side.
    sessions: sessions::Sessions,
    /// The queue of the browser-proxy relay (`/v1/relay`), which is not an account's
    /// own session.
    relay_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    api_version: u16,
    service_pid: u32,
    licence_active: bool,
    transport_connected: bool,
    recent_message_limit: i64,
}

#[derive(Debug, Deserialize)]
struct MessageQuery {
    limit: Option<i64>,
    /// Only this account's packets; without it, every account's.
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountQuery {
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InjectBody {
    packet: String,
    ttl_ms: Option<i64>,
    /// The account whose connection carries the packet. Without it, the only (or
    /// latest) connected account, else the relay.
    account_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct InjectAccepted {
    id: Uuid,
    status: &'static str,
}

#[derive(Debug, Deserialize)]
struct RelayQuery {
    upstream: String,
}

#[derive(Debug, Deserialize)]
struct ActivateLicenceBody {
    token: String,
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 47821),
            data_dir: empire_core::paths::data_dir(),
        }
    }
}

/// Common bind address, so the window can find the service it started.
pub const DEFAULT_BIND: &str = "127.0.0.1:47821";

/// The HTTP contract the window relies on.
///
/// The window probes `/v1/health` and replaces any service answering with an
/// older number, so this has to be bumped whenever behaviour the window depends
/// on changes - not only when routes are added. Missing a bump means a freshly
/// installed window keeps talking to the previous service, which looks exactly
/// like the new code never shipped.
///
/// 21: a fortress task seeds its own discovery walk from its castle, so a cold
/// account starts scanning instead of idling.
/// 22: `kingdom_health` carries per-kingdom fortress counts, and the dashboard
/// fortress summary is scoped to kingdoms that actually have a fortress task.
/// 23: fortress discovery probes the real fortress lattice instead of stepping a
/// flat ±17 between slots, so it can actually find fortresses.
/// 24: a fortress mode queues a cooldown read for every fortress initialization
/// discovered, so it starts with real state instead of learning it by attacking.
/// 25: that recheck is gated on observation age, so a cooldown read can no longer
/// schedule the next one and spin forever.
/// 26: the cold-start fortress walk widens to the map instead of stopping six
/// lattice steps from the castle.
/// 27: queued fortress probes far from any known fortress are pruned, so a
/// historic widening cannot leave thousands of pointless requests queued.
/// 37: scan progress distinguishes RBC, boundary estimate, and exact fortress
/// mapping; failed fortress claims are quarantined behind a rolling error gate.
/// 38: fortress attack inspection uses ABI, with command-specific response
/// matching that preserves pending attacks across unrelated map messages.
/// 39: the attack scheduler reads each tower's server-reported cooldown, re-reads
/// a refused tower instead of parking it for an hour, sleeps until the next
/// target or commander is actually ready.
/// 40: a lost game connection (Wi-Fi drop, address change, silent server) is
/// detected within 90 s and reconnected with backoff instead of ending the run,
/// and requests that time out on a silent link no longer quarantine towers.
/// 41: commander allocation is strict again. Idle commanders are no longer lent
/// between tasks, which had sent one task's marches out under another's commanders.
/// 42: attacks are checked against the castle's home inventory before being sent,
/// an unexplained attack refusal stops the mode, and a refused login is final and
/// remembered (a status 27 lock is respected across restarts).
/// 43: durable per-account telemetry (castle stock, troops lost per attack, connection
/// and refusal events; schema V19), a live stock/burn-rate line on the status bar,
/// and a reconnect budget of two retries (about 1 and 5 minutes).
/// 44: one licence per game account. Several licences can be installed, a login picks
/// the one bound to (or issued for) that account, and an account with none is told
/// exactly which server and main castle a licence must be issued for.
/// 45: `advanced` target selection is nearest-first by straight-line distance instead
/// of least-recently-attacked first, which had left near towers idle behind untouched
/// far ones.
/// 46: spotlight target selection with directional momentum, stochastic wave-timed
/// waits under the global limits, a rolling-hour response-health pause, and the
/// read-only Development endpoints (`/v1/dev`, `/v1/dev/map`, `/v1/dev/movements`).
/// 47: waits come from ten superimposed waves stepped once per action (low-heavy,
/// about 60 % between 1 and 5 s, floor 0.5 + rand/2 s) instead of three slow clock waves.
/// 48: the PVP tab: `GET /v1/pvp` pages other players' movements recorded from `gam`
/// (incoming, outgoing, retreated), and the Development tab can draw them.
/// 49: auto-station. Support bots (`/v1/plans/station-bots`, chosen with `station_bot_id`
/// on `/v1/plans/start`), the Overview (`/v1/overview`, `/v1/overview/refresh`), the job
/// history (`/v1/station/jobs`), and `station` jobs plus a wait histogram in `/v1/dev`.
/// 50: several accounts at once. Every account has its own session (`/v1/direct?account_id=`,
/// `/v1/direct/all`), snapshot, injection queue, packet window and run heartbeat; the
/// Development views take `account_id`, and `/v1/dev/shared` lets accounts on one world see
/// each other on colour-coded maps. A bot starts only for an account whose own licence
/// permits automation.
/// 51: `GET /v1/troops` (the Troop overview); `mode_id` on `/v1/plans/start` may be absent
/// (no attack bot); attack-bot allocations may carry `commander_start` (a task placed at a
/// commander number); a server-closed session is logged with its reason and not reconnected.
/// 52: attack bots carry an optional support object (`support_bot_id` on `/v1/plans/modes`,
/// `support: true` allocations are commanders kept free for it); the Troop overview marks
/// them reserved and bounds a lost return by the outbound trip.
/// 53: `POST /v1/dev/pace` (test pace of the human-paced waits; `timing.pace` and
/// `timing.max_interval_s` in `/v1/dev`); PVP leaves NPC movements out; the spotlight
/// drifts instead of following each pick.
pub const API_VERSION: u16 = 53;

/// Is something already listening on the service port?
///
/// Reusing a running service is what keeps one game session alive across window
/// closes, instead of signing in again every time the app is opened. A connect
/// test is enough: the port is loopback-only and belongs to us.
pub async fn service_is_running(bind: &str) -> bool {
    tokio::time::timeout(
        std::time::Duration::from_millis(400),
        tokio::net::TcpStream::connect(bind),
    )
    .await
    .map(|result| result.is_ok())
    .unwrap_or(false)
}

pub async fn serve_from_env() -> anyhow::Result<()> {
    let bind: SocketAddr = env::var("EMPIRE_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_owned())
        .parse()
        .context("EMPIRE_BIND must be an IP socket address")?;
    let data_dir = empire_core::paths::data_dir();
    serve(DaemonConfig { bind, data_dir }).await
}

pub async fn serve(config: DaemonConfig) -> anyhow::Result<()> {
    let bind = config.bind;
    let data_dir = config.data_dir;
    tokio::fs::create_dir_all(&data_dir).await?;
    let database_url = format!(
        "sqlite://{}?mode=rwc",
        data_dir.join("empire.sqlite3").display()
    );
    let store = Store::open(&database_url).await?;
    store.reconcile_stored_attack_returns().await?;
    let state = AppState {
        licence: licence::LicenceGate::new(store.clone())?,
        store,
        sessions: sessions::Sessions::default(),
        relay_transport: Arc::new(RwLock::new(None)),
    };

    let allowed_origins = [
        "http://localhost:1420".parse::<HeaderValue>().unwrap(),
        "tauri://localhost".parse::<HeaderValue>().unwrap(),
        "https://tauri.localhost".parse::<HeaderValue>().unwrap(),
    ];
    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    let app = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/licence", get(licence_status).post(activate_licence))
        .route("/v1/messages", get(messages))
        .route("/v1/accounts", get(accounts).post(initialize_account))
        .route("/v1/hunt", get(hunt))
        .route("/v1/dashboard", get(dashboard))
        .route("/v1/injections", post(inject))
        .route("/v1/plans", get(plans::library))
        .route("/v1/plans/example", get(plans::example))
        .route("/v1/plans/attacks", post(plans::create_attack))
        .route("/v1/plans/attacks/{id}", delete(plans::delete_attack))
        .route("/v1/plans/tasks", post(plans::create_task))
        .route("/v1/plans/tasks/{id}", delete(plans::delete_task))
        .route("/v1/plans/modes", post(plans::create_mode))
        .route("/v1/plans/modes/{id}", delete(plans::delete_mode))
        .route("/v1/plans/recruitments", post(plans::create_recruitment))
        .route(
            "/v1/plans/recruitments/{id}",
            delete(plans::delete_recruitment),
        )
        .route("/v1/plans/recruit-bots", post(plans::create_recruit_bot))
        .route(
            "/v1/plans/recruit-bots/{id}",
            delete(plans::delete_recruit_bot),
        )
        .route("/v1/plans/start", post(plans::start_bots))
        .route("/v1/plans/import", post(plans::import_mode))
        .route("/v1/plans/subscribe", post(plans::subscribe_mode))
        .route(
            "/v1/direct",
            get(direct_status)
                .post(direct_connect)
                .delete(direct_disconnect),
        )
        .route("/v1/direct/all", get(direct_all))
        .route("/v1/dev", get(dev_snapshot))
        .route("/v1/dev/shared", get(dev_shared))
        .route("/v1/dev/pace", post(dev_pace))
        .route("/v1/dev/map", get(dev_map))
        .route("/v1/dev/movements", get(dev_movements))
        .route("/v1/pvp", get(pvp::pvp))
        .route("/v1/plans/station-bots", post(station::create_station_bot))
        .route("/v1/plans/station-bots/{id}", delete(station::delete_station_bot))
        .route("/v1/overview", get(station::overview))
        .route("/v1/troops", get(troops::troops))
        .route("/v1/overview/refresh", post(station::refresh_overview))
        .route("/v1/station/jobs", get(station::jobs))
        .route("/v1/relay", get(relay_socket))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, database = %database_url, "OpenAuto service ready");
    axum::serve(listener, app).await?;
    Ok(())
}

/// One account's session. With no account named, the only (or latest) connected one.
/// An account with no session reads as disconnected, still carrying its id.
async fn direct_status(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Json<direct::DirectStatus> {
    let account = match query.account_id.filter(|account| !account.trim().is_empty()) {
        Some(account) => Some(account),
        None => state.sessions.default_account().await,
    };
    let status = match &account {
        Some(account) => state.sessions.status(account).await,
        None => None,
    };
    Json(status.unwrap_or_else(|| direct::DirectStatus {
        account_id: account.map(|account| empire_core::store::canonical_account_id(&account)),
        ..direct::DirectStatus::default()
    }))
}

/// Every account's session, for the account switcher.
async fn direct_all(State(state): State<AppState>) -> Json<Vec<direct::DirectStatus>> {
    Json(state.sessions.statuses().await)
}

async fn direct_connect(
    State(state): State<AppState>,
    Json(request): Json<direct::DirectConnectRequest>,
) -> Result<(StatusCode, Json<direct::DirectStatus>), ApiError> {
    state
        .licence
        .require_bootstrap("game_network")
        .await
        .map_err(ApiError::forbidden)?;
    if !request.endpoint.starts_with("wss://") {
        return Err(ApiError::bad_request("direct endpoint must use wss://"));
    }
    if request.settings.map_scan_radius > 500 {
        return Err(ApiError::bad_request(
            "scan radius must be between 0 and 500",
        ));
    }
    if request.credentials.player_name.is_empty()
        || request.credentials.portal_account_id.is_empty()
        || (request
            .credentials
            .password
            .as_deref()
            .is_none_or(str::is_empty)
            && request
                .credentials
                .login_token
                .as_deref()
                .is_none_or(str::is_empty))
    {
        return Err(ApiError::bad_request(
            "player, account ID, and either password or login token are required",
        ));
    }
    // Only this account's own session is replaced; others keep running.
    let account_id = empire_core::store::canonical_account_id(&request.credentials.player_name);
    let store = state.store.clone();
    let licence = state.licence.clone();
    let cells = state
        .sessions
        .replace(&account_id, |cells| {
            tokio::spawn(direct::run(
                request,
                store,
                cells.transport,
                cells.status,
                licence,
                cells.dev,
                cells.pace,
            ))
        })
        .await;
    let status = cells.status.read().await.clone();
    Ok((StatusCode::ACCEPTED, Json(status)))
}

async fn initialize_account(
    State(state): State<AppState>,
    Json(request): Json<direct::InitializeAccountRequest>,
) -> Result<(StatusCode, Json<direct::DirectStatus>), ApiError> {
    if request.username.trim().is_empty() || request.password.is_empty() {
        return Err(ApiError::bad_request("username and password are required"));
    }
    if request.scan_radius > 500 {
        return Err(ApiError::bad_request(
            "scan radius must be between 0 and 500",
        ));
    }
    direct_connect(State(state), Json(request.into_direct())).await
}

/// End one account's session. The account is named with `?account_id=`; without it the
/// request is only unambiguous when exactly one session exists.
async fn direct_disconnect(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Result<StatusCode, ApiError> {
    let account_id = match query.account_id.filter(|account| !account.trim().is_empty()) {
        Some(account) => account,
        None => {
            let all = state.sessions.all().await;
            match all.as_slice() {
                [(only, _)] => only.clone(),
                [] => return Ok(StatusCode::NO_CONTENT),
                _ => return Err(ApiError::bad_request("several accounts are signed in; say which one")),
            }
        }
    };
    // Say in the event log that this was the operator's doing: a session ended here
    // is never reconnected (its task is aborted, supervisor included), unlike a
    // dropped link, which the supervisor retries.
    let _ = state
        .store
        .record_event(&account_id, "connection.logout", "ended by the operator; not reconnecting", now_ms())
        .await;
    // Ending a session ends its automation too: nothing can run without the socket,
    // and a restart should not silently resume what was running.
    let _ = state.store.stop_account_mode(&account_id, now_ms()).await;
    let _ = state.store.stop_account_recruit_bot(&account_id, now_ms()).await;
    let _ = state.store.stop_account_station_bot(&account_id, now_ms()).await;
    state.sessions.remove(&account_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let licence_active = state.licence.status().await.active;
    Json(Health {
        status: "ok",
        api_version: API_VERSION,
        service_pid: std::process::id(),
        licence_active,
        transport_connected: !state.sessions.connected_accounts().await.is_empty()
            || state.relay_transport.read().await.is_some(),
        recent_message_limit: empire_core::RECENT_MESSAGE_LIMIT,
    })
}

async fn messages(
    State(state): State<AppState>,
    Query(query): Query<MessageQuery>,
) -> Result<Json<Vec<StoredMessage>>, ApiError> {
    state
        .licence
        .require("game_network")
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(
        state
            .store
            .recent_messages_for(
                query.account_id.as_deref().filter(|account| !account.trim().is_empty()),
                query.limit.unwrap_or(50),
            )
            .await?,
    ))
}

/// Scheduler, timing, spatial and health state for one account's Development view.
async fn dev_snapshot(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Result<Json<direct::DevSnapshot>, ApiError> {
    let account = focus_account(&state, query.account_id).await?;
    Ok(Json(state.sessions.dev(&account).await.unwrap_or_default()))
}

#[derive(Debug, Deserialize)]
struct PaceRequest {
    account_id: Option<String>,
    pace: f64,
}

/// Set the test pace of one account's human-paced waits (1 is the design; the service
/// keeps it within `PACE_MIN..=PACE_MAX` and says what it kept). Affects the next waits and
/// the projection, never a wait already running, the station's, or the protocol minimums.
async fn dev_pace(
    State(state): State<AppState>,
    Json(request): Json<PaceRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let account = focus_account(&state, request.account_id).await?;
    let cells = state
        .sessions
        .cells(&account)
        .await
        .ok_or_else(|| ApiError::bad_request("that account has no session"))?;
    let kept = cells.pace.set(request.pace);
    Ok(Json(serde_json::json!({ "account_id": account, "pace": kept })))
}

#[derive(Debug, Deserialize)]
struct DevMapQuery {
    kingdom: Option<i64>,
    account_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct DevMap {
    account_id: String,
    kingdom_id: i64,
    now_ms: i64,
    /// The main castle: where marches start and end.
    castle: Option<[i64; 2]>,
    towers: Vec<empire_core::store::MapTower>,
}

#[derive(Debug, Serialize)]
struct DevMovements {
    kingdom_id: i64,
    now_ms: i64,
    movements: Vec<empire_core::store::Movement>,
}

/// The account and kingdom a Development view is about: the account asked for (else
/// the connected one), and the kingdom of its active task unless one is asked for.
async fn dev_focus(
    state: &AppState,
    account: Option<String>,
    requested: Option<i64>,
) -> Result<(String, i64), ApiError> {
    let account = focus_account(state, account).await?;
    let kingdom = match requested {
        Some(kingdom) => kingdom,
        None => state
            .sessions
            .dev(&account)
            .await
            .and_then(|snapshot| snapshot.spatial.map(|spatial| spatial.kingdom_id))
            .unwrap_or(1),
    };
    Ok((account, kingdom))
}

/// Towers of one kingdom, read from the same table the bot selects from. The tab
/// asks for this when it opens or the kingdom changes, not every frame.
async fn dev_map(
    State(state): State<AppState>,
    Query(query): Query<DevMapQuery>,
) -> Result<Json<DevMap>, ApiError> {
    let (account, kingdom_id) = dev_focus(&state, query.account_id, query.kingdom).await?;
    Ok(Json(DevMap {
        kingdom_id,
        now_ms: now_ms(),
        castle: state
            .store
            .main_castle(&account, kingdom_id)
            .await?
            .map(|(x, y)| [x, y]),
        towers: state.store.map_towers(&account, kingdom_id).await?,
        account_id: account,
    }))
}

/// Armies out or coming back, with the timestamps the tab animates from.
async fn dev_movements(
    State(state): State<AppState>,
    Query(query): Query<DevMapQuery>,
) -> Result<Json<DevMovements>, ApiError> {
    let (account, kingdom_id) = dev_focus(&state, query.account_id, query.kingdom).await?;
    let now = now_ms();
    Ok(Json(DevMovements {
        kingdom_id,
        now_ms: now,
        movements: state
            .store
            .open_movements(&account, kingdom_id, now - 4 * 3_600_000, now, 200)
            .await?,
    }))
}

/// What every signed-in account on the viewed account's world is doing in one kingdom,
/// for the colour-coded maps (see `dev_shared.rs`).
async fn dev_shared(
    State(state): State<AppState>,
    Query(query): Query<DevMapQuery>,
) -> Result<Json<dev_shared::SharedView>, ApiError> {
    let (account, kingdom_id) = dev_focus(&state, query.account_id, query.kingdom).await?;
    Ok(Json(dev_shared::build(&state, &account, kingdom_id).await?))
}

/// Aggregated view of the automation run, for the desktop overview.
async fn hunt(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Result<Json<empire_core::store::HuntSummary>, ApiError> {
    state
        .licence
        .require("account_initialize")
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(
        state
            .store
            .hunt_summary_for(query.account_id.as_deref(), 20)
            .await?,
    ))
}

async fn dashboard(
    State(state): State<AppState>,
    Query(query): Query<AccountQuery>,
) -> Result<Json<empire_core::store::DashboardSummary>, ApiError> {
    state
        .licence
        .require("account_initialize")
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(
        state
            .store
            .dashboard_summary_for(query.account_id.as_deref())
            .await?,
    ))
}

async fn accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<empire_core::store::AccountSummary>>, ApiError> {
    state
        .licence
        .require("account_initialize")
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(state.store.account_summaries().await?))
}

async fn inject(
    State(state): State<AppState>,
    Json(body): Json<InjectBody>,
) -> Result<(StatusCode, Json<InjectAccepted>), ApiError> {
    state
        .licence
        .require("game_network")
        .await
        .map_err(ApiError::forbidden)?;
    parse_xt_packet(&body.packet).map_err(|error| ApiError::bad_request(error.to_string()))?;
    let now = now_ms();
    let request = InjectionRequest::new(
        body.packet,
        now,
        body.ttl_ms.unwrap_or(15_000).clamp(1_000, 60_000),
    );
    let id = request.id;
    let account = match body.account_id.filter(|account| !account.trim().is_empty()) {
        Some(account) => Some(account),
        None => state.sessions.default_account().await,
    };
    let sender = match account {
        Some(account) => state.sessions.transport(&account).await,
        None => state.relay_transport.read().await.clone(),
    }
    .ok_or_else(|| ApiError::unavailable("no active transport for that account"))?;
    sender.try_send(request).map_err(|error| {
        warn!(%error, "injection rejected");
        ApiError::unavailable("injection queue is full or disconnected")
    })?;
    Ok((
        StatusCode::ACCEPTED,
        Json(InjectAccepted {
            id,
            status: "queued",
        }),
    ))
}

async fn relay_socket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<RelayQuery>,
) -> Result<Response, ApiError> {
    state
        .licence
        .require("game_network")
        .await
        .map_err(ApiError::forbidden)?;
    if !query.upstream.starts_with("wss://") && !query.upstream.starts_with("ws://") {
        return Err(ApiError::bad_request("upstream must use ws:// or wss://"));
    }
    Ok(ws.on_upgrade(move |socket| {
        relay::run(
            socket,
            query.upstream,
            state.store,
            state.relay_transport,
            state.licence,
        )
    }))
}

async fn licence_status(State(state): State<AppState>) -> Json<licence::LicenceStatus> {
    Json(state.licence.status().await)
}

async fn activate_licence(
    State(state): State<AppState>,
    Json(body): Json<ActivateLicenceBody>,
) -> Result<Json<licence::LicenceStatus>, ApiError> {
    if body.token.len() > 16 * 1024 {
        return Err(ApiError::bad_request("application token is too large"));
    }
    let status = state
        .licence
        .activate(&body.token)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(status))
}

/// The account a read-only view is about: the one asked for, else the connected one
/// (the only one, or the latest), else the one most recently seen, so history can be
/// read while offline.
async fn focus_account(state: &AppState, requested: Option<String>) -> Result<String, ApiError> {
    if let Some(account) = requested.filter(|account| !account.trim().is_empty()) {
        return Ok(empire_core::store::canonical_account_id(&account));
    }
    if let Some(account) = state.sessions.default_account().await {
        return Ok(account);
    }
    state
        .store
        .latest_alliance_account()
        .await?
        .ok_or_else(|| ApiError::bad_request("no account has been seen yet"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    fn forbidden(message: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.to_string(),
        }
    }

    fn internal(message: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.to_string(),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({"error": self.message})),
        )
            .into_response()
    }
}
