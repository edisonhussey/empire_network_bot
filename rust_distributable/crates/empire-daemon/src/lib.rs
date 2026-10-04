mod direct;
mod licence;
mod plans;
mod relay;

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
use tokio::{
    sync::{Mutex, RwLock, mpsc},
    task::JoinHandle,
};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    store: Store,
    licence: licence::LicenceGate,
    active_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    direct_status: Arc<RwLock<direct::DirectStatus>>,
    direct_task: Arc<Mutex<Option<JoinHandle<()>>>>,
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
}

#[derive(Debug, Deserialize)]
struct InjectBody {
    packet: String,
    ttl_ms: Option<i64>,
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
    let state = AppState {
        licence: licence::LicenceGate::new(store.clone())?,
        store,
        active_transport: Arc::new(RwLock::new(None)),
        direct_status: Arc::new(RwLock::new(direct::DirectStatus::default())),
        direct_task: Arc::new(Mutex::new(None)),
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
        .route("/v1/relay", get(relay_socket))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, database = %database_url, "OpenAuto service ready");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn direct_status(State(state): State<AppState>) -> Json<direct::DirectStatus> {
    Json(state.direct_status.read().await.clone())
}

async fn direct_connect(
    State(state): State<AppState>,
    Json(request): Json<direct::DirectConnectRequest>,
) -> Result<(StatusCode, Json<direct::DirectStatus>), ApiError> {
    state
        .licence
        .require("game_network")
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
    if let Some(task) = state.direct_task.lock().await.take() {
        task.abort();
    }
    *state.active_transport.write().await = None;
    let task = tokio::spawn(direct::run(
        request,
        state.store.clone(),
        state.active_transport.clone(),
        state.direct_status.clone(),
        state.licence.clone(),
    ));
    *state.direct_task.lock().await = Some(task);
    Ok((
        StatusCode::ACCEPTED,
        Json(state.direct_status.read().await.clone()),
    ))
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

async fn direct_disconnect(State(state): State<AppState>) -> StatusCode {
    if let Some(account_id) = state.direct_status.read().await.account_id.clone() {
        let _ = state.store.stop_account_mode(&account_id, now_ms()).await;
        let _ = state
            .store
            .stop_account_recruit_bot(&account_id, now_ms())
            .await;
    }
    if let Some(task) = state.direct_task.lock().await.take() {
        task.abort();
    }
    *state.active_transport.write().await = None;
    *state.direct_status.write().await = direct::DirectStatus::default();
    StatusCode::NO_CONTENT
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let licence_active = state.licence.status().await.active;
    Json(Health {
        status: "ok",
        api_version: 16,
        service_pid: std::process::id(),
        licence_active,
        transport_connected: state.active_transport.read().await.is_some(),
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
            .recent_messages(query.limit.unwrap_or(50))
            .await?,
    ))
}

/// Aggregated view of the automation run, for the desktop overview.
async fn hunt(
    State(state): State<AppState>,
) -> Result<Json<empire_core::store::HuntSummary>, ApiError> {
    state
        .licence
        .require("account_initialize")
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(state.store.hunt_summary(20).await?))
}

async fn dashboard(
    State(state): State<AppState>,
) -> Result<Json<empire_core::store::DashboardSummary>, ApiError> {
    state
        .licence
        .require("account_initialize")
        .await
        .map_err(ApiError::forbidden)?;
    Ok(Json(state.store.dashboard_summary().await?))
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
    let sender = state
        .active_transport
        .read()
        .await
        .clone()
        .ok_or_else(|| ApiError::unavailable("no active relay transport"))?;
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
            state.active_transport,
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

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
