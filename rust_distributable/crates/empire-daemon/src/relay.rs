use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::extract::ws::{Message as ClientMessage, WebSocket};
use empire_core::{
    event::Direction,
    injection::{InjectionRequest, channel},
    protocol::{MAX_PACKET_BYTES, parse_xt_packet},
    store::Store,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::{connect_async, tungstenite::Message as UpstreamMessage};
use tracing::{info, warn};

pub async fn run(
    client: WebSocket,
    upstream_url: String,
    store: Store,
    active_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
) {
    let Ok((upstream, _response)) = connect_async(&upstream_url).await else {
        warn!(%upstream_url, "upstream websocket connection failed");
        return;
    };
    let (injection_tx, mut injection_rx) = channel();
    *active_transport.write().await = Some(injection_tx);
    info!(%upstream_url, "relay connected");

    let (mut client_sink, mut client_stream) = client.split();
    let (mut upstream_sink, mut upstream_stream) = upstream.split();

    loop {
        tokio::select! {
            maybe = client_stream.next() => {
                let Some(Ok(message)) = maybe else { break; };
                if let Some(packet) = client_text(&message) {
                    record(&store, Direction::ClientToServer, packet).await;
                }
                if let Some(message) = to_upstream(message)
                    && upstream_sink.send(message).await.is_err()
                {
                    break;
                }
            }
            maybe = upstream_stream.next() => {
                let Some(Ok(message)) = maybe else { break; };
                if let Some(packet) = upstream_text(&message) {
                    record(&store, Direction::ServerToClient, packet).await;
                }
                if let Some(message) = to_client(message)
                    && client_sink.send(message).await.is_err()
                {
                    break;
                }
            }
            maybe = injection_rx.recv() => {
                let Some(request) = maybe else { break; };
                if request.expired(now_ms()) {
                    continue;
                }
                record(&store, Direction::Injected, &request.packet).await;
                if upstream_sink.send(UpstreamMessage::Text(request.packet.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    *active_transport.write().await = None;
    info!(%upstream_url, "relay disconnected");
}

async fn record(store: &Store, direction: Direction, raw: &str) {
    if raw.len() > MAX_PACKET_BYTES {
        let payload = json!({
            "omitted": true,
            "reason": "packet exceeded persistence limit",
            "bytes": raw.len(),
        });
        if let Err(error) = store
            .record_message(now_ms(), direction, None, &payload)
            .await
        {
            warn!(%error, "failed to persist oversized network event metadata");
        }
        return;
    }
    let parsed = parse_xt_packet(raw).ok();
    let command = parsed.as_ref().map(|packet| packet.command.clone());
    let payload: Value = parsed
        .as_ref()
        .map(|packet| packet.payload.clone())
        .unwrap_or_else(|| json!({"raw": raw}));
    if let Err(error) = store
        .record_message(now_ms(), direction, command.as_deref(), &payload)
        .await
    {
        warn!(%error, "failed to persist network event");
    }
}

fn client_text(message: &ClientMessage) -> Option<&str> {
    match message {
        ClientMessage::Text(text) => Some(text.as_str()),
        _ => None,
    }
}

fn upstream_text(message: &UpstreamMessage) -> Option<&str> {
    match message {
        UpstreamMessage::Text(text) => Some(text.as_str()),
        _ => None,
    }
}

fn to_upstream(message: ClientMessage) -> Option<UpstreamMessage> {
    match message {
        ClientMessage::Text(value) => Some(UpstreamMessage::Text(value.as_str().into())),
        ClientMessage::Binary(value) => Some(UpstreamMessage::Binary(value)),
        ClientMessage::Ping(value) => Some(UpstreamMessage::Ping(value)),
        ClientMessage::Pong(value) => Some(UpstreamMessage::Pong(value)),
        ClientMessage::Close(_) => Some(UpstreamMessage::Close(None)),
    }
}

fn to_client(message: UpstreamMessage) -> Option<ClientMessage> {
    match message {
        UpstreamMessage::Text(value) => Some(ClientMessage::Text(value.as_str().into())),
        UpstreamMessage::Binary(value) => Some(ClientMessage::Binary(value)),
        UpstreamMessage::Ping(value) => Some(ClientMessage::Ping(value)),
        UpstreamMessage::Pong(value) => Some(ClientMessage::Pong(value)),
        UpstreamMessage::Close(_) => Some(ClientMessage::Close(None)),
        UpstreamMessage::Frame(_) => None,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
