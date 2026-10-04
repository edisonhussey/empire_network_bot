//! Live hunter: log in once, scan the map, and attack RBC targets.
//!
//! Design constraints that come from `docs/network_requests.md`:
//!
//! * **One login.** The account must not be re-logged within a minute, so a
//!   single socket is held for the whole run and reconnection is not attempted.
//! * **One pacing unit per batch.** The real client sends its 3×2 viewport in
//!   ~12 ms, so scan tiles go out in groups and the wait happens between groups.
//! * **The 4 s `cra` floor is persisted in memory and never breached.**
//! * **Every march is written to SQLite before the next one is considered**, so a
//!   crash or a restart still shows what was sent and what came back.
//!
//! Run: `cargo run --release -p empire-hunt -- --config <path.ini>`

use std::{collections::HashMap, env, path::Path, time::Duration};

use anyhow::{Context, bail};
use empire_core::{
    hunt::{self, MapTarget},
    pacing::{self, PacingPolicy, Rng, now_seconds},
    protocol::{XtPacket, parse_xt_packet},
    session::{LoginCredentials, SessionMachine, SessionPhase, SessionSettings},
    store::{MARCH_SENT, MarchRecord, Store},
};
use empire_game::{Attack, Side, Slot, Wave, tool_by_name, troop_by_name};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use serde_json::Value;
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::header::ORIGIN},
};

type Link = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type Sink = SplitSink<Link, Message>;
type Stream = SplitStream<Link>;

const MARCH_ID_FALLBACK_OFFSET: i64 = 1;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(40);
/// How long to wait when every commander in the chosen task is already out.
const TASK_WAIT: f64 = 15.0;
/// Assumed one-way travel when a march has no recorded duration, used only to
/// decide how long a commander is held after a restart.
const ASSUMED_TRAVEL_SECONDS: f64 = 900.0;
/// Extra time added to a restored return estimate so a march that is a little
/// late is not reused too early.
const COMMANDER_RETURN_SLACK: f64 = 60.0;

// ---------------------------------------------------------------------------
// configuration
// ---------------------------------------------------------------------------

struct Config {
    endpoint: String,
    credentials: LoginCredentials,
    settings: SessionSettings,
    /// Operator-supplied name for this run, so several modes can be told apart.
    label: String,
    /// Tiles out from the Sands castle to scan in each direction.
    radius: i64,
    /// Troops per wave in the attacking profile.
    troops: i64,
    /// Stop after this many marches. 0 means run until interrupted.
    max_attacks: u32,
    /// Where the run ledger lives.
    database_url: String,
    handshake_timeout: Duration,
}

// ---------------------------------------------------------------------------
// inbound events
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Watch {
    commanders: Vec<i64>,
    /// `(unit_id, quantity)` rows from `gui.I`.
    inventory: Vec<(i64, i64)>,
    target_level: Option<i64>,
}

impl Watch {
    fn units_of(&self, unit_id: i64) -> i64 {
        self.inventory
            .iter()
            .find(|(id, _)| *id == unit_id)
            .map(|(_, quantity)| *quantity)
            .unwrap_or_default()
    }
}

#[derive(Debug)]
enum Inbound {
    Adi(Watch),
    CraAck {
        march_id: Option<i64>,
        travel_seconds: Option<i64>,
    },
    Return {
        march_id: Option<i64>,
        loot: Option<(i64, i64)>,
        flag: Option<i64>,
    },
    Rejected(&'static str),
}

// ---------------------------------------------------------------------------
// entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    run(config).await
}

async fn run(config: Config) -> anyhow::Result<()> {
    let store = Store::open(&config.database_url)
        .await
        .context("failed to open the ledger database")?;
    // The account id is a shared key, not a label: using the name as typed here
    // while the desktop used its lowercase form created a second copy of the
    // account, with every castle and commander listed twice.
    let account_id = empire_core::store::canonical_account_id(&config.credentials.player_name);
    store
        .set_app_state("hunt.label", &serde_json::json!(config.label), now_ms())
        .await
        .ok();
    store
        .set_app_state("hunt.started_at_ms", &serde_json::json!(now_ms()), now_ms())
        .await
        .ok();
    // A periodic sign of life, so the app can tell "in flight" from "abandoned
    // by a run that is no longer running".
    {
        let heartbeat_store = store.clone();
        tokio::spawn(async move {
            loop {
                heartbeat_store
                    .set_app_state(
                        empire_core::store::HUNT_HEARTBEAT_KEY,
                        &serde_json::json!(now_ms()),
                        now_ms(),
                    )
                    .await
                    .ok();
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    println!(
        "HUNT_START label={} account={} endpoint={} radius={} troops={} max={}",
        config.label,
        account_id,
        config.endpoint,
        config.radius,
        config.troops,
        if config.max_attacks == 0 {
            "unbounded".to_owned()
        } else {
            config.max_attacks.to_string()
        }
    );

    store
        .upsert_account_profile(
            &account_id,
            &account_id,
            &config.endpoint,
            &config.settings.server_header,
            now_ms(),
        )
        .await
        .ok();

    let mut request = config.endpoint.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert(ORIGIN, "https://empire.goodgamestudios.com".parse()?);
    let (socket, _) = connect_async(request)
        .await
        .context("failed to connect the game WebSocket")?;
    let (mut sink, mut stream) = socket.split();
    println!("SOCKET_OK");

    let handshake = handshake(&mut sink, &mut stream, &config).await?;
    let (source, roster) = handshake;
    println!(
        "READY source={}:{} roster={} commanders",
        source.0,
        source.1,
        roster.len()
    );

    store
        .set_navigation(
            &{
                let mut state = empire_core::store::NavigationState::new(&account_id);
                state.current_kingdom_id = Some(config.level_kingdom());
                state.map_mode = true;
                state
            },
            now_ms(),
        )
        .await
        .ok();

    let plan = sands_plan()?;
    println!("PLAN label={} tasks={}", config.label, plan.len());
    for task in &plan {
        println!(
            "  TASK name={} kingdom={} levels={}..={} commanders={}\n       allocated={:?}",
            task.definition.name,
            task.definition.kingdom_id,
            task.definition.level_min,
            task.definition.level_max,
            task.definition.commander_count,
            task.commander_lids
        );
    }
    // Persist the declared plan so the app can show every task the run intends to
    // send, including ones that have not been given a target yet.
    store
        .set_app_state(
            "hunt.plan",
            &serde_json::json!(
                plan.iter()
                    .map(|task| serde_json::json!({
                        "task_id": task.definition.name,
                        "kingdom_id": task.definition.kingdom_id,
                        "level_min": task.definition.level_min,
                        "level_max": task.definition.level_max,
                        "commanders": task.commander_lids,
                    }))
                    .collect::<Vec<_>>()
            ),
            now_ms(),
        )
        .await
        .ok();

    hunt_loop(
        &mut sink,
        &mut stream,
        &store,
        &config,
        &account_id,
        source,
        &plan,
    )
    .await
}

/// Destination kingdom for the run. Sands is the only one wired up.
impl Config {
    fn level_kingdom(&self) -> i64 {
        1
    }
}

// ---------------------------------------------------------------------------
// handshake
// ---------------------------------------------------------------------------

/// Drive the login state machine to the map, returning the source castle and the
/// account's commander roster.
async fn handshake(
    sink: &mut Sink,
    stream: &mut Stream,
    config: &Config,
) -> anyhow::Result<((i64, i64), Vec<i64>)> {
    let mut machine = SessionMachine::new(config.credentials.clone(), config.settings.clone());
    for frame in machine.on_connected() {
        sink.send(Message::Text(frame.into())).await?;
    }

    let mut roster: Vec<i64> = Vec::new();
    let mut source: Option<(i64, i64)> = None;
    let deadline = Instant::now() + config.handshake_timeout;
    let mut last_phase = machine.phase();

    while machine.phase() != SessionPhase::SandsReady {
        if Instant::now() >= deadline {
            bail!("handshake timed out at phase {:?}", machine.phase());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Some(text) = tokio::time::timeout(remaining, read_frame(stream))
            .await
            .unwrap_or(None)
        else {
            bail!(
                "socket closed during handshake at phase {:?}",
                machine.phase()
            );
        };

        if let Ok(packet) = parse_xt_packet(&text) {
            if packet.command == "lli" && packet.status.as_deref() != Some("0") {
                bail!(
                    "LOGIN_REJECTED status={}",
                    packet.status.as_deref().unwrap_or("missing")
                );
            }
            learn(&packet, &mut roster, &mut source);
        }

        for frame in machine.on_server_text(&text)? {
            sink.send(Message::Text(frame.into())).await?;
        }
        if machine.phase() != last_phase {
            last_phase = machine.phase();
            println!("PHASE {:?}", last_phase);
        }
    }

    let source = source.context("gbd carried no Sands castle to attack from")?;
    Ok((source, roster))
}

/// Pull the soldier roster and the Sands source castle out of `gbd`.
fn learn(packet: &XtPacket, roster: &mut Vec<i64>, source: &mut Option<(i64, i64)>) {
    if packet.command != "gbd" || packet.status.as_deref() != Some("0") {
        return;
    }
    if roster.is_empty() {
        *roster = packet
            .payload
            .pointer("/gli/C")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| row.get("ID").and_then(Value::as_i64))
            .collect();
    }
    if source.is_none() {
        *source = sands_castle(&packet.payload);
    }
}

/// The Sands castle: kingdom 1, area type 12.
fn sands_castle(payload: &Value) -> Option<(i64, i64)> {
    let kingdoms = payload.pointer("/gcl/C")?.as_array()?;
    for kingdom in kingdoms {
        if kingdom.get("KID").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        for area in kingdom.get("AI")?.as_array()? {
            let row = area.get("AI")?.as_array()?;
            if row.first().and_then(Value::as_i64) == Some(12) {
                return Some((
                    row.get(1).and_then(Value::as_i64)?,
                    row.get(2).and_then(Value::as_i64)?,
                ));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// hunt loop
// ---------------------------------------------------------------------------

/// Rebuild the commander busy map from marches that are still out.
///
/// A restart must not collide with its own marches: the server answers 256
/// (`lord_in_use`) for a lord that is already marching, and each collision wastes
/// an attack slot. The expected return is twice the one-way travel plus a margin.
/// A march whose estimate has already passed is treated as free again — if that
/// guess is wrong the server says 256 and the release path holds the lord instead.
async fn restore_busy_commanders(store: &Store) -> HashMap<i64, f64> {
    let mut busy = HashMap::new();
    let now = now_seconds();
    let open = match store.open_marches().await {
        Ok(rows) => rows,
        Err(_) => return busy,
    };
    for march in open {
        let Some(lord_id) = march.lord_id else {
            continue;
        };
        if march.status != MARCH_SENT {
            // A returning march has already resolved, so the lord is nearly free.
            continue;
        }
        let travel = march
            .duration_s
            .map_or(ASSUMED_TRAVEL_SECONDS, |seconds| seconds as f64);
        let expected_back =
            march.sent_at_ms as f64 / 1000.0 + travel * 2.0 + COMMANDER_RETURN_SLACK;
        if expected_back > now {
            busy.insert(lord_id, expected_back);
        }
    }
    busy
}

#[allow(clippy::too_many_arguments)]
async fn hunt_loop(
    sink: &mut Sink,
    stream: &mut Stream,
    store: &Store,
    config: &Config,
    account_id: &str,
    source: (i64, i64),
    plan: &[hunt::PlannedTask],
) -> anyhow::Result<()> {
    let mut task_cursor = 0usize;
    let mut rng = Rng::from_entropy();
    let policy = PacingPolicy::default();
    let mut last_cra_at: Option<f64> = None;
    let mut targets: Vec<MapTarget> = Vec::new();
    // Target key -> the time its claim expires. A lease, written when a target is
    // picked and extended with a longer, failure-specific delay if it is dropped.
    let mut target_until: HashMap<String, f64> = HashMap::new();
    let mut sent: u32 = 0;
    let mut synthetic_march_id: i64 = now_ms();
    // Lord id -> the time it is expected back. A commander on a march cannot be
    // reused: the server answers 256 (`lord_in_use`).
    let mut busy_at: HashMap<i64, f64> = restore_busy_commanders(store).await;
    if !busy_at.is_empty() {
        let mut resumed: Vec<i64> = busy_at.keys().copied().collect();
        resumed.sort_unstable();
        println!("RESUMED_COMMANDERS still_out={resumed:?}");
    }

    scan(
        sink,
        stream,
        store,
        account_id,
        &mut busy_at,
        &mut rng,
        config,
        source,
        &mut targets,
    )
    .await?;
    println!(
        "SCAN_DONE targets={} levels={:?}",
        targets.len(),
        targets
            .iter()
            .filter_map(|target| target.level)
            .collect::<Vec<_>>()
    );

    loop {
        if config.max_attacks != 0 && sent >= config.max_attacks {
            println!("HUNT_COMPLETE attacks_sent={sent}");
            return Ok(());
        }

        let lease_now = now_seconds();
        let busy: Vec<i64> = busy_at
            .iter()
            .filter(|(_, until)| **until > lease_now)
            .map(|(lord_id, _)| *lord_id)
            .collect();
        let leased: Vec<String> = target_until
            .iter()
            .filter(|(_, until)| **until > lease_now)
            .map(|(key, _)| key.clone())
            .collect();

        // Tasks are served round-robin so neither starves, and a task only ever
        // uses its own commanders -- that is what stops a kunai commander being
        // sent at a level-61 crossbow target.
        let mut chosen: Option<(usize, MapTarget)> = None;
        for offset in 0..plan.len() {
            let index = (task_cursor + offset) % plan.len();
            let task = &plan[index];
            if hunt::task_exhausted(task, &busy) {
                continue;
            }
            if let Some(target) = hunt::pick_target_for_task(&targets, task, source, &leased) {
                chosen = Some((index, target.clone()));
                task_cursor = (index + 1) % plan.len();
                break;
            }
        }

        let Some((task_index, target)) = chosen else {
            if targets.is_empty() {
                println!("NO_TARGET rescanning");
                scan(
                    sink,
                    stream,
                    store,
                    account_id,
                    &mut busy_at,
                    &mut rng,
                    config,
                    source,
                    &mut targets,
                )
                .await?;
                if targets.is_empty() {
                    println!("SCAN_EMPTY waiting before retry");
                    sleep_until(Instant::now() + Duration::from_secs(60)).await;
                }
            } else {
                // Either every commander is out or everything known is leased.
                // Wait for whichever frees first rather than hammering.
                let pool = if plan.iter().all(|task| hunt::task_exhausted(task, &busy)) {
                    busy_at.values().copied().fold(f64::INFINITY, f64::min)
                } else {
                    target_until.values().copied().fold(f64::INFINITY, f64::min)
                };
                let wait = (pool - lease_now).clamp(5.0, 60.0);
                println!(
                    "IDLE commanders_out={} leased={} wait={wait:.0}s",
                    busy.len(),
                    leased.len()
                );
                sleep_until(Instant::now() + Duration::from_secs_f64(wait)).await;
            }
            continue;
        };
        let task = &plan[task_index];
        // Claim on pick, exactly as `reserve_target_for_task` does: the lease is
        // written now, not when the attack lands.
        target_until.insert(
            target.key(),
            lease_now + pacing::Waits::target_reserve_seconds(),
        );

        // 1. Inspect the target.
        let inspect_at = now_seconds();
        sink.send(Message::Text(
            hunt::adi_packet(&config.settings.server_header, source, &target)?.into(),
        ))
        .await?;
        println!("ADI_SENT target={} level={:?}", target.key(), target.level);

        let watch = match await_inbound(
            stream,
            store,
            account_id,
            &mut busy_at,
            &mut rng,
            &mut targets,
            "adi",
        )
        .await?
        {
            Inbound::Adi(watch) => watch,
            Inbound::Rejected(reason) => {
                // The target is still standing; stand it down for the "no commander"
                // range rather than burning the next attempt on it immediately.
                let retry = pacing::Waits::target_retry(&mut rng, pacing::target_retry::NO_LID);
                target_until.insert(target.key(), now_seconds() + retry);
                println!(
                    "ADI_REJECTED target={} reason={reason} released={:.0}s",
                    target.key(),
                    retry
                );
                continue;
            }
            Inbound::Return {
                march_id,
                loot,
                flag,
            } => {
                println!("RETURN_DURING_ADI march_id={march_id:?} loot={loot:?} flag={flag:?}");
                continue;
            }
            other => {
                println!("ADI_UNEXPECTED {other:?}");
                continue;
            }
        };
        let unit_id = match task.definition.attack {
            hunt::AttackShape::SingleFlank { unit_id, .. }
            | hunt::AttackShape::FourWaveFlanks { unit_id, .. } => unit_id,
        };
        println!(
            "ADI_OK task={} target={} level={:?} units={} commanders={}",
            task.definition.name,
            target.key(),
            watch.target_level,
            watch.units_of(unit_id),
            watch.commanders.len()
        );

        // 2. Choose a commander from THIS task's allocation and nobody else's.
        let now = now_seconds();
        let busy: Vec<i64> = busy_at
            .iter()
            .filter(|(_, free_at)| **free_at > now)
            .map(|(lord_id, _)| *lord_id)
            .collect();
        let Some(lord_id) = hunt::choose_task_commander(task, &watch.commanders, &busy) else {
            println!(
                "TASK_COMMANDERS_OUT task={} waiting={TASK_WAIT:.0}s",
                task.definition.name
            );
            sleep_until(Instant::now() + Duration::from_secs_f64(TASK_WAIT)).await;
            continue;
        };

        // 3. Respect the pacing floor, then commit.
        let payload = task_payload(task.definition.attack, &watch)?;
        let due = policy.cra_due_at(now_seconds(), last_cra_at, &mut rng);
        collect_until(
            stream,
            store,
            account_id,
            &mut busy_at,
            &mut rng,
            &mut targets,
            due,
        )
        .await?;
        let now = now_seconds();
        if now < policy.hard_cra_due_at(last_cra_at) {
            bail!("refusing to send cra inside the floor window");
        }
        let _ = inspect_at;

        sink.send(Message::Text(
            hunt::attack_packet(
                &config.settings.server_header,
                source,
                &target,
                lord_id,
                &payload,
                hunt::DEFAULT_HBW,
                hunt::MAP_PTT,
            )?
            .into(),
        ))
        .await?;
        last_cra_at = Some(now_seconds());

        // 4. Record it immediately: an unrecorded march is an invisible march.
        let inbound = await_inbound(
            stream,
            store,
            account_id,
            &mut busy_at,
            &mut rng,
            &mut targets,
            "cra",
        )
        .await?;
        let (march_id, travel) = match inbound {
            Inbound::CraAck {
                march_id,
                travel_seconds,
            } => (march_id, travel_seconds),
            Inbound::Rejected(reason) => {
                // A rejection here is almost always 256: this lord is already on
                // an active march. We do not know for how long, so stand it down
                // rather than hammering the same lord again.
                busy_at.insert(lord_id, now_seconds() + 600.0);
                // The target itself may still be attackable, so release it on the
                // shorter "server rejected" range rather than the plain lease.
                let retry = pacing::Waits::target_retry(&mut rng, pacing::target_retry::ERROR);
                target_until.insert(target.key(), now_seconds() + retry);
                println!(
                    "CRA_REJECTED target={} lord={lord_id} reason={reason} released={retry:.0}s",
                    target.key()
                );
                continue;
            }
            Inbound::Return {
                march_id,
                loot,
                flag,
            } => {
                println!("RETURN_DURING_CRA march_id={march_id:?} loot={loot:?} flag={flag:?}");
                continue;
            }
            other => {
                println!("CRA_UNEXPECTED {other:?}");
                continue;
            }
        };
        let march_id = march_id.unwrap_or_else(|| {
            synthetic_march_id += MARCH_ID_FALLBACK_OFFSET;
            synthetic_march_id
        });
        sent += 1;
        println!(
            "CRA_SENT n={sent} task={} target={} lord={lord_id} march_id={march_id}",
            task.definition.name,
            target.key()
        );
        // Held for the round trip, plus a margin for the return to be processed.
        let hold = travel.map_or(600.0, |seconds| {
            pacing::Waits::provisional_commander_hold(seconds, &mut rng)
        });
        busy_at.insert(lord_id, now_seconds() + hold);
        store
            .record_march(&MarchRecord {
                account_id: account_id.to_owned(),
                march_id,
                kingdom_id: target.kingdom_id,
                x: target.x,
                y: target.y,
                task_id: Some(task.definition.name.to_owned()),
                profile_id: Some(task.definition.name.to_owned()),
                level: target.level,
                lord_id: Some(lord_id),
                commander_number: None,
                troop_count: Some(config.troops),
                duration_s: travel,
                coin_loot: None,
                ruby_loot: None,
                status: MARCH_SENT.to_owned(),
                result_flag: None,
                error_message: None,
                sent_at_ms: now_ms(),
                landed_at_ms: None,
                result_at_ms: None,
            })
            .await
            .context("failed to record the march")?;
        println!("MARCH_RECORDED march_id={march_id} travel={travel:?}");

        // 5. Let the return arrive while staying responsive.
        let settle = policy.after_cra_ack(&mut rng);
        let hold = now_seconds() + settle + pacing::Waits::attack_send(&mut rng);
        collect_until(
            stream,
            store,
            account_id,
            &mut busy_at,
            &mut rng,
            &mut targets,
            hold,
        )
        .await?;
    }
}

/// Send the scan sweep in viewport-sized batches, waiting between batches.
#[allow(clippy::too_many_arguments)]
async fn scan(
    sink: &mut Sink,
    stream: &mut Stream,
    store: &Store,
    account_id: &str,
    busy_at: &mut HashMap<i64, f64>,
    rng: &mut Rng,
    config: &Config,
    source: (i64, i64),
    targets: &mut Vec<MapTarget>,
) -> anyhow::Result<()> {
    let tiles = hunt::scan_tiles(source, config.radius);
    println!("SCAN_START tiles={}", tiles.len());
    for batch in tiles.chunks(6) {
        for origin in batch {
            sink.send(Message::Text(
                hunt::gaa_packet(
                    &config.settings.server_header,
                    config.level_kingdom(),
                    *origin,
                )?
                .into(),
            ))
            .await?;
        }
        let wait = pacing::Waits::scan_batch(rng);
        collect_until(
            stream,
            store,
            account_id,
            busy_at,
            rng,
            targets,
            now_seconds() + wait,
        )
        .await?;
    }
    Ok(())
}

/// Read frames until a specific reply arrives, remembering anything else.
async fn await_inbound(
    stream: &mut Stream,
    store: &Store,
    account_id: &str,
    busy_at: &mut HashMap<i64, f64>,
    rng: &mut Rng,
    targets: &mut Vec<MapTarget>,
    expecting: &str,
) -> anyhow::Result<Inbound> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match tokio::time::timeout(remaining, read_frame(stream)).await {
            Ok(Some(text)) => {
                if let Some(event) = handle(&text, store, account_id, busy_at, rng, targets).await?
                    && event.matches(expecting)
                {
                    return Ok(event);
                }
            }
            Ok(None) => bail!("socket closed while waiting for {expecting}"),
            Err(_) => break,
        }
    }
    Ok(Inbound::Rejected("timeout"))
}

/// Read frames until `until`, handling returns as they arrive.
#[allow(clippy::too_many_arguments)]
async fn collect_until(
    stream: &mut Stream,
    store: &Store,
    account_id: &str,
    busy_at: &mut HashMap<i64, f64>,
    rng: &mut Rng,
    targets: &mut Vec<MapTarget>,
    until: f64,
) -> anyhow::Result<()> {
    while now_seconds() < until {
        let remaining = Duration::from_secs_f64((until - now_seconds()).max(0.0));
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, read_frame(stream)).await {
            Ok(Some(text)) => {
                handle(&text, store, account_id, busy_at, rng, targets).await?;
            }
            Ok(None) => bail!("socket closed"),
            Err(_) => break,
        }
    }
    Ok(())
}

impl Inbound {
    fn matches(&self, expecting: &str) -> bool {
        matches!(
            (expecting, self),
            ("adi", Inbound::Adi(_)) | ("cra", Inbound::CraAck { .. }) | (_, Inbound::Rejected(_))
        )
    }
}

/// Interpret one server frame, persisting anything durable.
async fn handle(
    text: &str,
    store: &Store,
    account_id: &str,
    busy_at: &mut HashMap<i64, f64>,
    rng: &mut Rng,
    targets: &mut Vec<MapTarget>,
) -> anyhow::Result<Option<Inbound>> {
    let Ok(packet) = parse_xt_packet(text) else {
        return Ok(None);
    };
    if packet.status.as_deref().is_some_and(|status| status != "0") {
        println!(
            "SERVER_ERROR command={} status={}",
            packet.command,
            packet.status.as_deref().unwrap_or("?")
        );
        return Ok(Some(Inbound::Rejected("server status")));
    }
    match packet.command.as_str() {
        "gaa" => {
            for target in hunt::rbc_targets_from_map(&packet.payload) {
                if !targets.iter().any(|known| known.key() == target.key()) {
                    targets.push(target);
                }
            }
            Ok(None)
        }
        "adi" => {
            let watch = Watch {
                commanders: hunt::available_commanders(&packet.payload),
                inventory: inventory(&packet.payload),
                target_level: packet
                    .payload
                    .pointer("/AI/4")
                    .and_then(Value::as_i64)
                    .map(hunt::sands_level),
            };
            Ok(Some(Inbound::Adi(watch)))
        }
        "cra" => Ok(Some(Inbound::CraAck {
            march_id: hunt::march_id_from_ack(&packet.payload),
            travel_seconds: hunt::travel_seconds_from_ack(&packet.payload),
        })),
        "cat" => {
            let march_id = hunt::returned_march_id(&packet.payload);
            let loot = hunt::loot_from_return(&packet.payload);
            let flag = hunt::result_flag_from_return(&packet.payload);
            let lord_id = hunt::returned_lord_id(&packet.payload);
            let return_seconds = hunt::return_seconds_from_return(&packet.payload);
            let target = hunt::return_target(&packet.payload);

            // The commander is held for the return trip plus a short spread, the
            // same rule the Python applies.
            if let (Some(lord_id), Some(seconds)) = (lord_id, return_seconds) {
                let hold = seconds as f64 + pacing::Waits::commander_return_hold(rng);
                busy_at.insert(lord_id, now_seconds() + hold);
            }

            // Attribute by attacked position plus commander: the `cra`
            // acknowledgement and the return carry different march ids. Only when
            // nothing matches is a standalone row written, so loot is never lost.
            let mut attributed = 0;
            if let (Some((kingdom_id, x, y)), Some(lord_id)) = (target, lord_id) {
                attributed = store
                    .finish_march_by_target(
                        account_id,
                        kingdom_id,
                        x,
                        y,
                        lord_id,
                        return_seconds,
                        loot.map(|(coins, _)| coins),
                        loot.map(|(_, rubies)| rubies),
                        flag,
                        now_ms(),
                    )
                    .await
                    .unwrap_or(0);
                if attributed == 0
                    && let Some(march_id) = march_id
                {
                    let (coins, rubies) = loot.unwrap_or((0, 0));
                    let _ = store
                        .record_march(&MarchRecord {
                            account_id: account_id.to_owned(),
                            march_id,
                            kingdom_id,
                            x,
                            y,
                            task_id: None,
                            profile_id: None,
                            level: None,
                            lord_id: Some(lord_id),
                            commander_number: None,
                            troop_count: None,
                            duration_s: return_seconds,
                            coin_loot: Some(coins),
                            ruby_loot: Some(rubies),
                            status: "returned".to_owned(),
                            result_flag: flag,
                            error_message: None,
                            sent_at_ms: now_ms(),
                            landed_at_ms: None,
                            result_at_ms: Some(now_ms()),
                        })
                        .await;
                }
            }

            if let Some((coins, rubies)) = loot
                && coins + rubies > 0
            {
                println!(
                    "LOOT march_id={march_id:?} lord={lord_id:?} at={} coins={coins} rubies={rubies} flag={flag:?} attribution={attributed}",
                    target
                        .map(|(kingdom, x, y)| format!("{kingdom}:{x}:{y}"))
                        .unwrap_or_else(|| "unknown".to_owned())
                );
            }
            Ok(Some(Inbound::Return {
                march_id,
                loot,
                flag,
            }))
        }
        _ => Ok(None),
    }
}

/// Read one text frame, skipping protocol control frames.
async fn read_frame(stream: &mut Stream) -> Option<String> {
    loop {
        let message = stream.next().await?.ok()?;
        match message {
            Message::Text(text) => return Some(text.to_string()),
            Message::Binary(bytes) => return String::from_utf8(bytes.to_vec()).ok(),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            Message::Close(_) => return None,
        }
    }
}

fn inventory(payload: &Value) -> Vec<(i64, i64)> {
    payload
        .pointer("/gui/I")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((row.first()?.as_i64()?, row.get(1).and_then(Value::as_i64)?))
        })
        .collect()
}

/// The two Sands tasks, in the order the Python subscribes them for an account.
///
/// Order decides the commander split: the first task takes the first
/// `commander_count` usable LIDs and the second takes the rest, so the two can
/// never mix commanders.
fn sands_plan() -> anyhow::Result<Vec<hunt::PlannedTask>> {
    let crossbowman = troop_by_name("crossbowman")
        .context("crossbowman is missing from the game data")?
        .id;
    let kunai = troop_by_name("renegade_kunai_thrower")
        .context("renegade_kunai_thrower is missing from the game data")?
        .id;
    let ladder = tool_by_name("scaling_ladder")
        .context("scaling_ladder is missing from the game data")?
        .id;

    let definitions = vec![
        hunt::TaskDefinition {
            name: "sand_rbc_level_61_crossbow",
            kingdom_id: 1,
            level_min: 61,
            level_max: 61,
            commander_count: 17,
            priority: 20,
            attack: hunt::AttackShape::SingleFlank {
                unit_id: crossbowman,
                count: 50,
            },
        },
        hunt::TaskDefinition {
            name: "sand_kunai",
            kingdom_id: 1,
            level_min: 35,
            level_max: 60,
            commander_count: 18,
            priority: 10,
            attack: hunt::AttackShape::FourWaveFlanks {
                unit_id: kunai,
                count: 30,
                tool_id: ladder,
                tool_count: 5,
            },
        },
    ];
    hunt::allocate_tasks(&definitions).map_err(|error| anyhow::anyhow!(error))
}

/// Build the payload for a task's attack, clamped to the troops actually held.
///
/// The tool count is scaled with the troops because the server enforces a ratio:
/// 6 troops per tool. Five ladders with thirty troops is accepted; seven is
/// rejected with status 5.
fn task_payload(shape: hunt::AttackShape, watch: &Watch) -> anyhow::Result<Value> {
    const TROOPS_PER_TOOL: i64 = 6;
    match shape {
        hunt::AttackShape::SingleFlank { unit_id, count } => {
            let held = watch.units_of(unit_id);
            let count = if held > 0 { count.min(held) } else { count };
            if count <= 0 {
                bail!("no units held for the single-flank task");
            }
            let attack = Attack::single_wave(Wave::new(
                Some(Side::units_only(vec![Slot::new(unit_id, count)?])),
                None,
                None,
            ));
            Ok(serde_json::to_value(attack.to_payload()?)?)
        }
        hunt::AttackShape::FourWaveFlanks {
            unit_id,
            count,
            tool_id,
            tool_count,
        } => {
            let held = watch.units_of(unit_id);
            let count = if held > 0 { count.min(held) } else { count };
            if count <= 0 {
                bail!("no units held for the four-wave task");
            }
            let tools = tool_count.min(count / TROOPS_PER_TOOL);
            let flank = || -> anyhow::Result<Side> {
                let tools = if tools > 0 {
                    vec![Slot::new(tool_id, tools)?]
                } else {
                    Vec::new()
                };
                Ok(Side::new(tools, vec![Slot::new(unit_id, count)?]))
            };
            let wave =
                || -> anyhow::Result<Wave> { Ok(Wave::new(Some(flank()?), None, Some(flank()?))) };
            let attack = Attack::new(wave()?, Some(wave()?), Some(wave()?), Some(wave()?));
            Ok(serde_json::to_value(attack.to_payload()?)?)
        }
    }
}

fn now_ms() -> i64 {
    (now_seconds() * 1000.0) as i64
}

// ---------------------------------------------------------------------------
// configuration loading
// ---------------------------------------------------------------------------

impl Config {
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
        let number = |environment: &str, ini: &str, default: i64| -> anyhow::Result<i64> {
            value(environment, ini)
                .map(|raw| raw.parse().with_context(|| format!("invalid {ini}")))
                .unwrap_or(Ok(default))
        };

        let endpoint = value("GGE_SERVER_URL", "server_url")
            .unwrap_or_else(|| "wss://ep-live-us1-game.goodgamestudios.com/".to_owned());
        if !endpoint.starts_with("wss://") {
            bail!("server_url must use wss://");
        }
        // A run label lets several modes be told apart in the ledger and in any
        // summary the app shows.
        let label = args
            .windows(2)
            .find(|pair| pair[0] == "--label")
            .map(|pair| pair[1].clone())
            .or_else(|| value("HUNT_LABEL", "hunt_label"))
            .unwrap_or_else(|| "default".to_owned());
        let password = value("GGE_PASSWORD", "password");
        let login_token = value("GGE_LOGIN_TOKEN", "login_token");
        if password.is_none() && login_token.is_none() {
            bail!("set GGE_PASSWORD or GGE_LOGIN_TOKEN");
        }
        let mut settings = SessionSettings::default();
        settings.server_header = value("GGE_SERVER_HEADER", "server_header")
            .unwrap_or_else(|| settings.server_header.clone());
        settings.connection_time = number("GGE_CONM", "CONM", settings.connection_time)?;
        settings.round_trip_time = number("GGE_RTM", "RTM", settings.round_trip_time)?;

        Ok(Self {
            endpoint,
            credentials: LoginCredentials {
                player_name: value("GGE_USERNAME", "username").context("missing username")?,
                portal_account_id: value("GGE_ACCOUNT_ID", "account_id")
                    .context("missing account_id")?,
                password,
                login_token,
                registration_token: value("GGE_RCT", "RCT"),
            },
            label,
            radius: number("HUNT_RADIUS", "hunt_radius", 2)?,
            troops: number("HUNT_TROOPS", "hunt_troops", 50)?,
            max_attacks: number("HUNT_MAX_ATTACKS", "hunt_max_attacks", 0)? as u32,
            database_url: value("HUNT_DATABASE", "hunt_database")
                .unwrap_or_else(empire_core::paths::default_database_url),
            handshake_timeout: Duration::from_secs(
                number("GGE_LOGIN_TIMEOUT", "login_timeout", 45)?.clamp(10, 180) as u64,
            ),
            settings,
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
            in_gge = true;
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
