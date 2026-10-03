use std::{sync::Arc, time::Duration};

use empire_core::{
    account::{commander_lids, owned_castles, rbc_targets},
    event::Direction,
    injection::{InjectionRequest, channel},
    protocol::parse_xt_packet,
    session::{LoginCredentials, SessionMachine, SessionPhase, SessionSettings},
    store::Store,
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
const US1_PORTAL_ACCOUNT_ID: &str = "1780270034676433896";

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum GameServer {
    #[serde(rename = "US1")]
    Us1,
}

#[derive(Clone, Deserialize)]
pub struct InitializeAccountRequest {
    pub server: GameServer,
    pub username: String,
    pub password: String,
}

impl InitializeAccountRequest {
    pub fn into_direct(self) -> DirectConnectRequest {
        match self.server {
            GameServer::Us1 => DirectConnectRequest {
                endpoint: US1_ENDPOINT.to_owned(),
                credentials: LoginCredentials {
                    player_name: self.username.trim().to_owned(),
                    portal_account_id: US1_PORTAL_ACCOUNT_ID.to_owned(),
                    password: Some(self.password),
                    login_token: None,
                    registration_token: None,
                },
                settings: SessionSettings::default(),
            },
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct DirectConnectRequest {
    pub endpoint: String,
    pub credentials: LoginCredentials,
    #[serde(default)]
    pub settings: SessionSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectStatus {
    pub connected: bool,
    pub phase: SessionPhase,
    pub endpoint: Option<String>,
    pub error: Option<String>,
}

impl Default for DirectStatus {
    fn default() -> Self {
        Self {
            connected: false,
            phase: SessionPhase::Disconnected,
            endpoint: None,
            error: None,
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
    status.write().await.endpoint = Some(endpoint.clone());
    let result = run_inner(request, &store, &active_transport, &status, &licence).await;
    *active_transport.write().await = None;
    let mut current = status.write().await;
    current.connected = false;
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
    let account_id = request.credentials.player_name.trim().to_owned();
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
        error: None,
    };
    info!(endpoint = %request.endpoint, "direct game session connected");

    let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
    let mut entitlement_check = tokio::time::interval(Duration::from_secs(5));
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
                    let frames = machine.on_server_text(&text)?;
                    status.write().await.phase = machine.phase();
                    for frame in frames {
                        record_safe_outbound(store, &frame).await;
                        sink.send(Message::Text(frame.into())).await?;
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
                let packet = format!(
                    "%xt%{}%pin%1%<RoundHouseKick>%",
                    request.settings.server_header
                );
                sink.send(Message::Text(packet.into())).await?;
            }
            _ = entitlement_check.tick() => {
                licence.require("game_network").await?;
            }
        }
    }
    Ok(())
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
            if castles.is_empty() {
                return;
            }
            store
                .replace_account_bootstrap(account_id, &castles, &commanders, now_ms())
                .await
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

async fn record_safe_outbound(store: &Store, raw: &str) {
    if parse_xt_packet(raw).is_ok_and(|packet| packet.command == "lli") {
        return;
    }
    record_text(store, Direction::ClientToServer, raw).await;
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
