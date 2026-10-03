use std::{sync::Arc, time::Duration};

use empire_core::{
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
) {
    let endpoint = request.endpoint.clone();
    status.write().await.endpoint = Some(endpoint.clone());
    let result = run_inner(request, &store, &active_transport, &status).await;
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
) -> anyhow::Result<()> {
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
    heartbeat.tick().await;
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
        }
    }
    Ok(())
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
