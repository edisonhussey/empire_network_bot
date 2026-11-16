//! Injection mode, HTTP side: the proxy's link into a session, and the Pause button.
//!
//! `GET /v1/attach?account=<id>` (WebSocket) is opened by proxy_packet_monitor's addon when the person presses
//! **Start injection bot** there. It starts this account's session over that link instead of a socket of the
//! daemon's own (any earlier session of the account is ended). Messages, one JSON object each:
//!
//! * proxy → daemon: `{"dir":"s2c","text":…}` for a server frame, `{"dir":"c2s","text":…}` for a frame the browser sent
//!   (not the bot's own injections);
//! * daemon → proxy: `{"inject":…}`, a frame to send to the server.
//!
//! The session is the same engine as **Start bot** (`direct/mod.rs`, `direct/link.rs`); the browser's own view
//! commands make the bots step aside for a minute (`direct/pause.rs`). `docs/architecture/injection-mode.md`.

use axum::{
    Json,
    extract::{Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::StatusCode,
    response::Response,
};
use empire_core::{
    protocol::parse_xt_packet,
    session::{LoginCredentials, SessionSettings},
    store::canonical_account_id,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{ApiError, AppState, direct, now_ms};

#[derive(Deserialize)]
pub struct AttachQuery {
    account: String,
}

pub async fn attach(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<AttachQuery>,
) -> Result<Response, ApiError> {
    state.licence.require_bootstrap("game_network").await.map_err(ApiError::forbidden)?;
    let account_id = canonical_account_id(&query.account);
    let Some((player_name, endpoint, server_header)) =
        state.store.account_connection(&account_id).await.map_err(ApiError::internal)?
    else {
        return Err(ApiError::bad_request("unknown account: initialize it with Start bot once first"));
    };
    Ok(ws.on_upgrade(move |socket| run(socket, state, account_id, player_name, endpoint, server_header)))
}

async fn run(socket: WebSocket, state: AppState, account_id: String, player_name: String, endpoint: String, server_header: String) {
    let (to_session, from_game) = mpsc::channel::<String>(256);
    let (to_game, mut outbound) = mpsc::channel::<String>(64);
    let request = direct::DirectConnectRequest {
        endpoint,
        credentials: LoginCredentials {
            player_name,
            portal_account_id: String::new(),
            password: None,
            login_token: None,
            registration_token: None,
        },
        settings: SessionSettings { server_header, ..SessionSettings::default() },
        reuse_existing_map: true,
        attach: Some(direct::AttachSlot::new(direct::AttachedLink { from_game, to_game })),
    };
    let store = state.store.clone();
    let licence = state.licence.clone();
    let cells = state
        .sessions
        .replace(&account_id, |cells| {
            tokio::spawn(direct::run(request, store, cells.transport, cells.status, licence, cells.dev, cells.pace, cells.pause))
        })
        .await;
    let pause = cells.pause.clone();
    info!(%account_id, "proxy link attached");
    let (mut proxy_sink, mut proxy_stream) = socket.split();
    loop {
        tokio::select! {
            incoming = proxy_stream.next() => {
                let Some(Ok(Message::Text(text))) = incoming else { break; };
                let Ok(frame) = serde_json::from_str::<Value>(text.as_str()) else { continue; };
                let Some(raw) = frame.get("text").and_then(Value::as_str) else { continue; };
                match frame.get("dir").and_then(Value::as_str) {
                    Some("s2c") => {
                        if to_session.send(raw.to_owned()).await.is_err() {
                            break;
                        }
                    }
                    Some("c2s") => {
                        if let Ok(packet) = parse_xt_packet(raw) {
                            pause.note_human(&packet.command);
                        }
                    }
                    _ => {}
                }
            }
            frame = outbound.recv() => {
                let Some(frame) = frame else { break; };
                if proxy_sink.send(Message::Text(json!({"inject": frame}).to_string().into())).await.is_err() {
                    break;
                }
            }
        }
    }
    warn!(%account_id, "proxy link closed");
    // Dropping `to_session` ends the session's stream: injection mode does not reconnect.
}

#[derive(Deserialize)]
pub struct PauseRequest {
    account_id: String,
    paused: bool,
}

/// The Pause button: bots stop sending, the socket stays, every frame is still read. Resume continues from
/// the live state; a step that was in flight is abandoned as failed.
pub async fn pause(State(state): State<AppState>, Json(request): Json<PauseRequest>) -> Result<StatusCode, ApiError> {
    let account_id = canonical_account_id(&request.account_id);
    let cells = state.sessions.cells(&account_id).await.ok_or_else(|| ApiError::bad_request("that account has no session"))?;
    cells.pause.set_manual(request.paused);
    let _ = state
        .store
        .record_event(&account_id, if request.paused { "bot.pause_pressed" } else { "bot.resume_pressed" }, "by the operator", now_ms())
        .await;
    Ok(StatusCode::NO_CONTENT)
}
