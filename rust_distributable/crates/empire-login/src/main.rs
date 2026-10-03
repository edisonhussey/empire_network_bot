use std::{collections::HashMap, env, path::Path, time::Duration};

use anyhow::{Context, bail};
use empire_core::{
    protocol::{XtPacket, encode_client_xt, parse_xt_packet},
    session::{LoginCredentials, SessionMachine, SessionPhase, SessionSettings},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::header::ORIGIN},
};

struct LoginConfig {
    endpoint: String,
    credentials: LoginCredentials,
    settings: SessionSettings,
    timeout: Duration,
}

#[derive(Clone)]
struct CastleProof {
    kingdom_id: i64,
    castle_id: i64,
    x: i64,
    y: i64,
    name: String,
}

struct ProofTracker {
    server_header: String,
    expected_sands_tiles: usize,
    sands_tiles: usize,
    sands_castle: Option<CastleProof>,
    sands_castle_requested: bool,
}

enum ProofProgress {
    Continue(Vec<String>),
    Complete,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = LoginConfig::load()?;
    println!(
        "LOGIN_START player={} endpoint={} auth={} timeout={}s",
        config.credentials.player_name,
        config.endpoint,
        if config.credentials.password.is_some() {
            "password"
        } else {
            "token"
        },
        config.timeout.as_secs()
    );
    tokio::time::timeout(config.timeout, prove_login(config))
        .await
        .context("network proof timed out")??;
    Ok(())
}

async fn prove_login(config: LoginConfig) -> anyhow::Result<()> {
    let mut request = config.endpoint.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert(ORIGIN, "https://empire.goodgamestudios.com".parse()?);
    let (socket, response) = connect_async(request)
        .await
        .context("failed to connect game WebSocket")?;
    println!("SOCKET_OK http_status={}", response.status());
    let (mut sink, mut stream) = socket.split();
    let mut proof = ProofTracker::new(&config.settings);
    let mut machine = SessionMachine::new(config.credentials, config.settings);
    let mut authenticated = false;
    let mut previous_phase = machine.phase();
    for frame in machine.on_connected() {
        sink.send(Message::Text(frame.into())).await?;
    }
    report_phase(&mut previous_phase, machine.phase());

    while let Some(message) = stream.next().await {
        let message = message.context("game WebSocket receive failed")?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
                .context("game WebSocket sent non-UTF-8 binary data")?,
            _ => continue,
        };
        let packet = parse_xt_packet(&text).ok();
        let login_ack = packet
            .as_ref()
            .filter(|packet| packet.command == "lli")
            .map(|packet| packet.status.clone());
        let outbound = machine.on_server_text(&text)?;
        report_phase(&mut previous_phase, machine.phase());
        let progress = packet
            .as_ref()
            .map(|packet| proof.observe(packet))
            .transpose()?
            .unwrap_or(ProofProgress::Continue(Vec::new()));
        let mut frames = outbound;
        if let ProofProgress::Continue(additional) = &progress {
            frames.extend(additional.iter().cloned());
        }
        for frame in frames {
            sink.send(Message::Text(frame.into())).await?;
        }
        if let Some(status) = login_ack {
            if status.as_deref() != Some("0") {
                bail!(
                    "LOGIN_REJECTED command=lli status={}",
                    status.as_deref().unwrap_or("missing")
                );
            }
            println!("LOGIN_PROOF command=lli status=0 authenticated=true");
            authenticated = true;
        }
        if matches!(progress, ProofProgress::Complete) {
            println!(
                "FULL_PROOF authenticated=true main_castle=true sands_map=true sands_castle=true inventory=true"
            );
            sink.send(Message::Close(None)).await?;
            return Ok(());
        }
    }
    if authenticated {
        bail!("game WebSocket closed after login but before the full castle/map proof")
    }
    bail!("game WebSocket closed before an lli acknowledgement")
}

impl ProofTracker {
    fn new(settings: &SessionSettings) -> Self {
        Self {
            server_header: settings.server_header.clone(),
            expected_sands_tiles: usize::from(settings.map.columns)
                * usize::from(settings.map.rows),
            sands_tiles: 0,
            sands_castle: None,
            sands_castle_requested: false,
        }
    }

    fn observe(&mut self, packet: &XtPacket) -> anyhow::Result<ProofProgress> {
        if packet.status.as_deref().is_some_and(|status| status != "0") {
            return Ok(ProofProgress::Continue(Vec::new()));
        }
        match packet.command.as_str() {
            "gbd" => {
                let castles = castles_from_gbd(&packet.payload);
                self.sands_castle = castles
                    .iter()
                    .find(|castle| castle.kingdom_id == 1)
                    .cloned();
                let main = castles.iter().find(|castle| castle.kingdom_id == 0);
                let sands = self.sands_castle.as_ref();
                println!(
                    "ACCOUNT_PROOF owned_castles={} main_castle_id={} sands_castle_id={}",
                    castles.len(),
                    main.map(|castle| castle.castle_id).unwrap_or_default(),
                    sands.map(|castle| castle.castle_id).unwrap_or_default()
                );
                Ok(ProofProgress::Continue(Vec::new()))
            }
            "jaa" => {
                let kingdom_id = packet.payload.get("KID").and_then(Value::as_i64);
                let castle = castle_from_jaa(&packet.payload);
                if kingdom_id == Some(0) {
                    if let Some(castle) = castle {
                        print_castle("MAIN_CASTLE_PROOF", &castle);
                    }
                    return Ok(ProofProgress::Continue(Vec::new()));
                }
                if kingdom_id == Some(1) && self.sands_castle_requested {
                    let castle = castle.context("Sands jaa response omitted active castle data")?;
                    print_castle("SANDS_CASTLE_PROOF", &castle);
                    let crossbowmen = inventory_count(&packet.payload, 607);
                    let inventory_entries = packet
                        .payload
                        .pointer("/gui/I")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    let food = packet
                        .payload
                        .pointer("/grc/F")
                        .and_then(Value::as_f64)
                        .unwrap_or_default();
                    println!(
                        "SANDS_STATE_PROOF crossbowmen={} food={:.0} inventory_entries={}",
                        crossbowmen, food, inventory_entries
                    );
                    return Ok(ProofProgress::Complete);
                }
                Ok(ProofProgress::Continue(Vec::new()))
            }
            "gaa" if packet.payload.get("KID").and_then(Value::as_i64) == Some(1) => {
                self.sands_tiles += 1;
                if self.sands_tiles < self.expected_sands_tiles || self.sands_castle_requested {
                    return Ok(ProofProgress::Continue(Vec::new()));
                }
                println!(
                    "SANDS_MAP_PROOF kingdom_id=1 tiles_loaded={}",
                    self.sands_tiles
                );
                let castle = self
                    .sands_castle
                    .as_ref()
                    .context("authenticated account bootstrap contained no Sands castle")?;
                self.sands_castle_requested = true;
                Ok(ProofProgress::Continue(vec![encode_client_xt(
                    &self.server_header,
                    "jca",
                    "1",
                    &json!({"CID": castle.castle_id, "KID": castle.kingdom_id}),
                )?]))
            }
            _ => Ok(ProofProgress::Continue(Vec::new())),
        }
    }
}

fn castles_from_gbd(payload: &Value) -> Vec<CastleProof> {
    let mut castles = Vec::new();
    let Some(kingdoms) = payload.pointer("/gcl/C").and_then(Value::as_array) else {
        return castles;
    };
    for kingdom in kingdoms {
        let Some(kingdom_id) = kingdom.get("KID").and_then(Value::as_i64) else {
            continue;
        };
        let Some(areas) = kingdom.get("AI").and_then(Value::as_array) else {
            continue;
        };
        for area in areas {
            let Some(row) = area.get("AI").and_then(Value::as_array) else {
                continue;
            };
            if let Some(castle) = castle_from_row(kingdom_id, row) {
                castles.push(castle);
            }
        }
    }
    castles
}

fn castle_from_jaa(payload: &Value) -> Option<CastleProof> {
    let kingdom_id = payload.get("KID")?.as_i64()?;
    let row = payload.pointer("/gca/A")?.as_array()?;
    castle_from_row(kingdom_id, row)
}

fn castle_from_row(kingdom_id: i64, row: &[Value]) -> Option<CastleProof> {
    Some(CastleProof {
        kingdom_id,
        x: row.get(1)?.as_i64()?,
        y: row.get(2)?.as_i64()?,
        castle_id: row.get(3)?.as_i64()?,
        name: row.get(10).and_then(Value::as_str).unwrap_or("").to_owned(),
    })
}

fn inventory_count(payload: &Value, wanted_id: i64) -> i64 {
    payload
        .pointer("/gui/I")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find_map(|row| {
                let row = row.as_array()?;
                (row.first()?.as_i64()? == wanted_id)
                    .then(|| row.get(1).and_then(Value::as_i64).unwrap_or_default())
            })
        })
        .unwrap_or_default()
}

fn print_castle(label: &str, castle: &CastleProof) {
    let name = serde_json::to_string(&castle.name).unwrap_or_else(|_| "\"\"".to_owned());
    println!(
        "{label} kingdom_id={} castle_id={} name={} coordinates={}:{}",
        castle.kingdom_id, castle.castle_id, name, castle.x, castle.y
    );
}

fn report_phase(previous: &mut SessionPhase, current: SessionPhase) {
    if *previous != current {
        println!("PHASE {:?}", current);
        *previous = current;
    }
}

impl LoginConfig {
    fn load() -> anyhow::Result<Self> {
        let args: Vec<String> = env::args().collect();
        let config_path = args
            .windows(2)
            .find(|pair| pair[0] == "--config")
            .map(|pair| pair[1].as_str());
        let file = config_path.map(read_ini).transpose()?.unwrap_or_default();
        let value = |environment: &str, ini: &str| {
            env::var(environment)
                .ok()
                .filter(|value| !value.is_empty())
                .or_else(|| file.get(&ini.to_ascii_lowercase()).cloned())
        };
        let endpoint = value("GGE_SERVER_URL", "server_url")
            .unwrap_or_else(|| "wss://ep-live-us1-game.goodgamestudios.com/".to_owned());
        if !endpoint.starts_with("wss://") {
            bail!("server_url must use wss://");
        }
        let player_name = value("GGE_USERNAME", "username").context("missing username")?;
        let portal_account_id =
            value("GGE_ACCOUNT_ID", "account_id").context("missing account_id")?;
        let password = value("GGE_PASSWORD", "password");
        let login_token = value("GGE_LOGIN_TOKEN", "login_token");
        if password.is_none() && login_token.is_none() {
            bail!("set GGE_PASSWORD or GGE_LOGIN_TOKEN");
        }
        let number = |environment: &str, ini: &str, default: i64| -> anyhow::Result<i64> {
            value(environment, ini)
                .map(|raw| raw.parse().with_context(|| format!("invalid {ini}")))
                .unwrap_or(Ok(default))
        };
        let mut settings = SessionSettings::default();
        settings.server_header = value("GGE_SERVER_HEADER", "server_header")
            .unwrap_or_else(|| settings.server_header.clone());
        settings.client_version = value("GGE_CLIENT_VERSION", "client_version")
            .unwrap_or_else(|| settings.client_version.clone());
        settings.connection_time = number("GGE_CONM", "CONM", settings.connection_time)?;
        settings.round_trip_time = number("GGE_RTM", "RTM", settings.round_trip_time)?;
        let timeout_seconds = number("GGE_LOGIN_TIMEOUT", "login_timeout", 30)?;
        Ok(Self {
            endpoint,
            credentials: LoginCredentials {
                player_name,
                portal_account_id,
                password,
                login_token,
                registration_token: value("GGE_RCT", "RCT"),
            },
            settings,
            timeout: Duration::from_secs(timeout_seconds.clamp(5, 120) as u64),
        })
    }
}

fn read_ini(path: &str) -> anyhow::Result<HashMap<String, String>> {
    let contents = std::fs::read_to_string(Path::new(path))
        .with_context(|| format!("failed to read config {path}"))?;
    let mut values = HashMap::new();
    let mut in_gge = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_gge = line.eq_ignore_ascii_case("[gge]");
            continue;
        }
        if !in_gge || line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some((key, raw_value)) = line.split_once('=') {
            let value = raw_value
                .split(['#', ';'])
                .next()
                .unwrap_or_default()
                .trim();
            if !value.is_empty() {
                values.insert(key.trim().to_ascii_lowercase(), value.to_owned());
            }
        }
    }
    Ok(values)
}
