//! Auto-station: take an attacked castle's troops out of harm's way for a few
//! seconds, then call them back. Pure rules only; the session loop that sends the
//! packets is `empire-daemon/src/direct/station.rs`.
//!
//! The sequence, from the captures in `docs/network/captures/`:
//!
//! 1. `gam` shows an attack on an owned castle ([`threat_from`]).
//! 2. About seven seconds before landing `dcl` reads every castle's stock.
//! 3. At `D = landing - 5 s` `sdi` opens the support page for the attacked castle
//!    and the destination; its reply carries the fresh home stock and castle id.
//! 4. `cds` commits the support march ([`select_support_troops`], [`cds_packet`]).
//! 5. After the landing, `mcm` recalls the support march by *its* MID.
//!
//! Timing is the delicate part. The human-paced wait generator is heavy-tailed on
//! purpose, so its raw output cannot be used to space steps inside a five second
//! window: one long draw would miss the landing. [`normalise_gaps`] keeps the
//! generator's randomness but maps each draw into a window that is known to fit,
//! and compresses the whole sequence when the time left is short.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    protocol::{PacketError, encode_client_xt},
    pvp::{AllianceIdentity, SeenMovement},
};

/// The time-critical window before landing: `sdi` opens it and `cds` has to be
/// acknowledged inside it.
pub const WINDOW_MS: i64 = 5_000;
/// `PT` and `TT` are whole seconds, so the real landing can be a second earlier
/// than the estimate.
pub const PRECISION_MS: i64 = 1_000;
/// Most distinct troop types one support march carries.
pub const MAX_SUPPORT_UNITS: usize = 10;
/// Margin kept between the last acknowledgement and the landing.
pub const ACK_MARGIN_MS: i64 = 250;
/// A one-way transport delay assumed until one has been measured.
pub const DEFAULT_LATENCY_MS: i64 = 250;
/// Where a generator draw of typical size lands in its window. About the
/// middle of the generator's 1-5 s band.
const TYPICAL_DRAW_S: f64 = 3.0;

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// An attack on one of the account's own castles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Threat {
    pub mid: i64,
    pub kingdom_id: i64,
    /// The attacked castle's area id: what `cds` and `dcl` call it.
    pub castle_id: i64,
    pub castle_x: i64,
    pub castle_y: i64,
    /// Estimated landing, from the first sight of the packet: `received_at +
    /// (TT - PT)`. Later packets for the same MID never move it.
    pub landing_ms: i64,
    pub attacker: Option<String>,
}

/// The operator's trigger: the target is one of the account's own castles, and
/// sender and receiver sit in different alliances.
///
/// Both player records and both `AID`s must be present. Equal values are skipped,
/// and since `-1` is the observed "no alliance" value, two unaffiliated players
/// are skipped too. This is an operational rule, not a verified attack flag: the
/// packets carry none. The account's own outward movements never match.
pub fn threat_from(
    movement: &SeenMovement,
    me: &AllianceIdentity,
    own_castles: &[(i64, i64, i64)],
    received_at_ms: i64,
) -> Option<Threat> {
    if movement.sender.player_id == Some(me.player_id)
        || movement.target.player_id != Some(me.player_id)
    {
        return None;
    }
    if !own_castles.contains(&(movement.kingdom_id, movement.target.x, movement.target.y)) {
        return None;
    }
    let (sender, receiver) = (movement.sender.alliance_raw?, movement.target.alliance_raw?);
    if sender == receiver {
        return None;
    }
    let remaining_s = (movement.duration_s - movement.progress_s).max(0);
    Some(Threat {
        mid: movement.mid,
        kingdom_id: movement.kingdom_id,
        castle_id: movement.target.area_id?,
        castle_x: movement.target.x,
        castle_y: movement.target.y,
        landing_ms: received_at_ms + remaining_s * 1_000,
        attacker: movement.sender.name.clone(),
    })
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// The deadlines one landing implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    /// The estimate from the packet.
    pub landing_ms: i64,
    /// The estimate pulled earlier by the packet's whole-second precision and the
    /// transport delay: every deadline is measured from here, never from the raw
    /// estimate.
    pub safe_landing_ms: i64,
    /// `D`: when the time-critical sequence starts.
    pub deadline_ms: i64,
}

impl Timeline {
    pub fn new(landing_ms: i64, latency_ms: i64) -> Self {
        let safe_landing_ms = landing_ms - PRECISION_MS - latency_ms.max(0);
        Self {
            landing_ms,
            safe_landing_ms,
            deadline_ms: safe_landing_ms - WINDOW_MS,
        }
    }

    /// The last moment `cds` may leave and still be acknowledged in time, given
    /// the round trip it needs.
    pub fn last_cds_send_ms(&self, round_trip_ms: i64) -> i64 {
        self.safe_landing_ms - round_trip_ms.max(0) - ACK_MARGIN_MS
    }
}

/// How much room one gap may use. The floor is a hard minimum (a protocol or
/// plain-humanity limit); the ceiling is the longest the gap should ever be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapSpec {
    pub floor_ms: i64,
    pub ceiling_ms: i64,
}

/// Turn raw draws of the human-paced generator into gaps that fit a budget.
///
/// The generator is heavy-tailed: most draws are a few seconds but a few are
/// minutes. Used as they are they would drop an arbitrary step outside a
/// five-second window. Each draw is squashed monotonically into its own
/// `[floor, ceiling]` window (a larger draw is still a longer gap, a typical draw
/// lands mid-window), then, if the gaps together exceed `budget_ms`, only the part
/// above the floors is scaled down. `None` when even the floors do not fit:
/// no timing algorithm makes that feasible.
pub fn normalise_gaps(samples_s: &[f64], specs: &[GapSpec], budget_ms: i64) -> Option<Vec<i64>> {
    debug_assert_eq!(samples_s.len(), specs.len());
    let floors: i64 = specs.iter().map(|spec| spec.floor_ms).sum();
    if budget_ms < floors {
        return None;
    }
    let squashed = samples_s
        .iter()
        .zip(specs)
        .map(|(sample, spec)| {
            let sample = sample.max(0.0);
            let position = sample / (sample + TYPICAL_DRAW_S);
            let span = (spec.ceiling_ms - spec.floor_ms).max(0) as f64;
            (spec.floor_ms, (position * span).round() as i64)
        })
        .collect::<Vec<_>>();
    let extra: i64 = squashed.iter().map(|(_, extra)| extra).sum();
    let room = budget_ms - floors;
    let scale = if extra > room && extra > 0 {
        room as f64 / extra as f64
    } else {
        1.0
    };
    Some(
        squashed
            .into_iter()
            .map(|(floor, extra)| floor + (extra as f64 * scale).floor() as i64)
            .collect(),
    )
}

/// Gap between `dcl` and `sdi`: the brief puts `dcl` about two seconds before `D`.
pub const DCL_TO_SDI: GapSpec = GapSpec { floor_ms: 900, ceiling_ms: 3_000 };
/// Gap between the context step (a map tile) and `dcl`.
pub const CONTEXT_TO_DCL: GapSpec = GapSpec { floor_ms: 900, ceiling_ms: 4_000 };
/// The pause between the `sdi` reply and `cds`: reading the page.
pub const SDI_TO_CDS: GapSpec = GapSpec { floor_ms: 350, ceiling_ms: 1_800 };

/// What a plan had to give up to fit the time left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Degraded {
    /// No time for the map context step.
    NoContext,
    /// No time for the `dcl` cross-check either: `sdi` stands alone.
    NoDcl,
}

impl Degraded {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoContext => "no time for the map context step",
            Self::NoDcl => "no time for the dcl cross-check; sdi stands alone",
        }
    }
}

/// When each step before `cds` is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepPlan {
    /// The map-tile request that puts the client where a support page is
    /// reachable; `None` when it is not needed or there is no time.
    pub context_at_ms: Option<i64>,
    /// `None` when there is no time for the cross-check and `sdi` stands alone.
    pub dcl_at_ms: Option<i64>,
    pub sdi_at_ms: i64,
    /// What had to be given up to fit, if anything.
    pub degraded: Option<Degraded>,
}

/// Lay out `context -> dcl -> sdi` against the timeline.
///
/// `sdi` is anchored at `D`. The steps before it are placed backwards from it with
/// normalised gaps. When `now` is already past where a step should have gone, the
/// optional steps are dropped in order of how little they matter (the context step,
/// then the `dcl` cross-check) and `sdi` goes as soon as it can. `samples_s` are two
/// draws of the human-paced generator, for the two gaps.
pub fn plan_prep(now_ms: i64, timeline: &Timeline, needs_context: bool, samples_s: [f64; 2]) -> PrepPlan {
    let sdi_at = timeline.deadline_ms.max(now_ms);
    let budget = sdi_at - now_ms;
    let steps: &[GapSpec] = if needs_context {
        &[CONTEXT_TO_DCL, DCL_TO_SDI]
    } else {
        &[DCL_TO_SDI]
    };
    let draws = if needs_context { &samples_s[..] } else { &samples_s[1..] };
    if let Some(gaps) = normalise_gaps(draws, steps, budget) {
        let dcl_gap = *gaps.last().unwrap_or(&0);
        let dcl_at = sdi_at - dcl_gap;
        let context_at = needs_context.then(|| dcl_at - gaps[0]);
        return PrepPlan {
            context_at_ms: context_at,
            dcl_at_ms: Some(dcl_at),
            sdi_at_ms: sdi_at,
            degraded: None,
        };
    }
    // Not enough room for everything. Try without the context step.
    if needs_context
        && let Some(gaps) = normalise_gaps(&samples_s[1..], &[DCL_TO_SDI], budget)
    {
        return PrepPlan {
            context_at_ms: None,
            dcl_at_ms: Some(sdi_at - gaps[0]),
            sdi_at_ms: sdi_at,
            degraded: Some(Degraded::NoContext),
        };
    }
    PrepPlan {
        context_at_ms: None,
        dcl_at_ms: None,
        sdi_at_ms: sdi_at,
        degraded: Some(Degraded::NoDcl),
    }
}

/// How long to wait after the `sdi` reply before sending `cds`.
///
/// One draw, normalised into [`SDI_TO_CDS`] and then held to what is left: never
/// more than half the time remaining before the last safe send, and zero (send
/// now) once the floor does not fit.
pub fn cds_think_ms(sample_s: f64, now_ms: i64, last_send_ms: i64) -> i64 {
    let remaining = last_send_ms - now_ms;
    if remaining <= SDI_TO_CDS.floor_ms {
        return 0;
    }
    let gaps = normalise_gaps(&[sample_s], &[SDI_TO_CDS], remaining / 2).unwrap_or_else(|| vec![0]);
    gaps[0].min(remaining / 2).max(0)
}

/// When to recall after the landing: just after it, plus a random few seconds.
/// One draw normalised into a window, so the delay is human-looking and bounded.
pub fn recall_delay_ms(sample_s: f64) -> i64 {
    const WINDOW: GapSpec = GapSpec { floor_ms: 900, ceiling_ms: 4_500 };
    normalise_gaps(&[sample_s], &[WINDOW], WINDOW.ceiling_ms).map_or(WINDOW.floor_ms, |gaps| gaps[0])
}

// ---------------------------------------------------------------------------
// Commander
// ---------------------------------------------------------------------------

/// One commander's availability, as the ledger knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LidState {
    pub lid: i64,
    /// Busy until this time (`<= now` means home).
    pub available_after_ms: i64,
}

/// Pick the commander to lend to a support march.
///
/// `roster` is the account's commanders in order; `task_sets` the commanders of
/// each running attack task (each task takes its first free commander, so the
/// last ones are the least used); `reserved` the commanders the attack bot set
/// aside for support, which no task owns. Preference, to cost ordinary attacks the
/// least:
///
/// 0. a reserved commander, free now, else the reserved one back soonest (before
///    `ready_by_ms`): that is what the reserve is for;
/// 1. a free commander no task uses;
/// 2. else the last free commander of the task with the most free ones;
/// 3. else a busy one that is back by `ready_by_ms`, the earliest first.
///
/// `None` means no commander can be had in time: the job is blocked, never sent
/// with a guess.
pub fn choose_support_lid(
    roster: &[i64],
    task_sets: &[Vec<i64>],
    reserved: &[i64],
    states: &[LidState],
    now_ms: i64,
    ready_by_ms: i64,
) -> Option<i64> {
    let free_at = |lid: i64| {
        states
            .iter()
            .find(|state| state.lid == lid)
            .map_or(now_ms, |state| state.available_after_ms.max(now_ms))
    };
    let is_free = |lid: i64| free_at(lid) <= now_ms;
    if let Some(lid) = reserved.iter().copied().find(|lid| is_free(*lid)) {
        return Some(lid);
    }
    if let Some(lid) = reserved
        .iter()
        .copied()
        .filter(|lid| free_at(*lid) <= ready_by_ms)
        .min_by_key(|lid| (free_at(*lid), *lid))
    {
        return Some(lid);
    }
    let in_a_task = |lid: i64| task_sets.iter().any(|set| set.contains(&lid));
    if let Some(lid) = roster.iter().rev().copied().find(|lid| is_free(*lid) && !in_a_task(*lid)) {
        return Some(lid);
    }
    let richest = task_sets
        .iter()
        .max_by_key(|set| set.iter().filter(|lid| is_free(**lid)).count())?;
    if let Some(lid) = richest.iter().rev().copied().find(|lid| is_free(*lid)) {
        return Some(lid);
    }
    roster
        .iter()
        .copied()
        .filter(|lid| free_at(*lid) <= ready_by_ms)
        .min_by_key(|lid| (free_at(*lid), *lid))
}

// ---------------------------------------------------------------------------
// Troops
// ---------------------------------------------------------------------------

/// The support army: at most [`MAX_SUPPORT_UNITS`] distinct unit types, largest
/// stock first (ties by lower id), each at its full verified home count, so the most
/// troops are saved.
///
/// `home` is the `sdi` reply's `gui.I` (fresh, taken at the time of sending);
/// `cross_check` the matching castle's `dcl` `AC`, when there was one. With both,
/// the smaller positive count wins; with `home` alone it stands. Known tools and zero
/// counts are left out. An id the game tables do not know is kept: the tables lag the
/// game (units 215 and 227 were missing, and 35,031 + 7,281 troops stayed behind in a
/// castle for it), and every id in a castle is a troop or a tool. The captures show no
/// total capacity rule, so none is applied beyond the ten types.
pub fn select_support_troops(
    home: &BTreeMap<i64, i64>,
    cross_check: Option<&BTreeMap<i64, i64>>,
) -> Vec<(i64, i64)> {
    let mut chosen = home
        .iter()
        .filter(|(id, _)| unit_kind(**id).0 != UnitKind::Tool)
        .filter_map(|(id, count)| {
            let verified = match cross_check {
                Some(check) => (*count).min(check.get(id).copied().unwrap_or(0)),
                None => *count,
            };
            (verified > 0).then_some((*id, verified))
        })
        .collect::<Vec<_>>();
    chosen.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    chosen.truncate(MAX_SUPPORT_UNITS);
    chosen
}

// ---------------------------------------------------------------------------
// Packets
// ---------------------------------------------------------------------------

pub fn dcl_packet(server_header: &str) -> Result<String, PacketError> {
    encode_client_xt(server_header, "dcl", "1", &json!({"CD": 1}))
}

/// Open the support page: `source` is the attacked castle, `destination` the
/// configured station.
pub fn sdi_packet(
    server_header: &str,
    source: (i64, i64),
    destination: (i64, i64),
) -> Result<String, PacketError> {
    encode_client_xt(
        server_header,
        "sdi",
        "1",
        &json!({"TX": destination.0, "TY": destination.1, "SX": source.0, "SY": source.1}),
    )
}

/// Commit the support march. `castle_id` is the source castle's area id (`SID` in
/// the capture, equal to the `sdi` reply's `SCID`). `HBW -1` is no horse, `WT 1`
/// the observed support type; `BPC`, `PTT` and `SD` are zero as captured.
pub fn cds_packet(
    server_header: &str,
    castle_id: i64,
    destination: (i64, i64),
    lid: i64,
    troops: &[(i64, i64)],
) -> Result<String, PacketError> {
    encode_client_xt(
        server_header,
        "cds",
        "1",
        &json!({
            "SID": castle_id, "TX": destination.0, "TY": destination.1, "LID": lid,
            "WT": 1, "HBW": -1, "BPC": 0, "PTT": 0, "SD": 0,
            "A": troops.iter().map(|(id, count)| json!([id, count])).collect::<Vec<_>>(),
        }),
    )
}

/// Recall a support march by its own MID (not the attack's).
pub fn mcm_packet(server_header: &str, support_mid: i64) -> Result<String, PacketError> {
    encode_client_xt(server_header, "mcm", "1", &json!({"MID": support_mid}))
}

// ---------------------------------------------------------------------------
// Replies
// ---------------------------------------------------------------------------

fn pairs(value: Option<&Value>) -> BTreeMap<i64, i64> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((row.first()?.as_i64()?, row.get(1)?.as_i64()?))
        })
        .collect()
}

/// One castle's stock from a `dcl` reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CastleStock {
    pub kingdom_id: i64,
    pub castle_id: i64,
    /// Every unit and tool in `AC`, zero counts included; the catalogue tells
    /// them apart (see [`UnitKind`]).
    pub units: BTreeMap<i64, i64>,
    pub food: f64,
    pub mead: f64,
    pub wood: f64,
    pub stone: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DclReply {
    pub player_id: i64,
    pub castles: Vec<CastleStock>,
}

/// `dcl`: `PID` and, per kingdom, per castle, its resources and `AC` stock. The
/// layout was captured with a single castle; several are read the same way, by
/// kingdom and area id, and one castle's `AC` is never applied to another.
pub fn parse_dcl(payload: &Value) -> Option<DclReply> {
    let player_id = payload.get("PID").and_then(Value::as_i64)?;
    let mut castles = Vec::new();
    for kingdom in payload.get("C").and_then(Value::as_array)? {
        let Some(kingdom_id) = kingdom.get("KID").and_then(Value::as_i64) else {
            continue;
        };
        for area in kingdom.get("AI").and_then(Value::as_array).into_iter().flatten() {
            let Some(castle_id) = area.get("AID").and_then(Value::as_i64) else {
                continue;
            };
            let number = |key: &str| area.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            castles.push(CastleStock {
                kingdom_id,
                castle_id,
                units: pairs(area.get("AC")),
                food: number("F"),
                mead: number("MEAD"),
                wood: number("W"),
                stone: number("S"),
            });
        }
    }
    Some(DclReply { player_id, castles })
}

/// What an id in `AC` is, by the game tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Troop,
    Tool,
    Unknown,
}

pub fn unit_kind(id: i64) -> (UnitKind, String) {
    if let Some(troop) = empire_game::troop_by_id(id) {
        (UnitKind::Troop, troop.name.to_owned())
    } else if let Some(tool) = empire_game::tool_by_id(id) {
        (UnitKind::Tool, tool.name.to_owned())
    } else {
        (UnitKind::Unknown, format!("unit {id}"))
    }
}

/// What an id is and what to call it **on screen**, from the game's own complete unit table
/// (`empire_game::units`): the game's `role` says troop or tool, and the name is the one the player reads.
///
/// This is for showing stock, not for deciding what is sent. [`unit_kind`] and [`known_troops_only`] still use the older,
/// smaller tables on purpose, so support armies and attacks are built from exactly the units they always were
/// (`docs/game_data/tables.md`, "What still uses the older tables").
pub fn unit_label(id: i64) -> (UnitKind, String) {
    match empire_game::unit_by_id(id) {
        Some(unit) => (if unit.is_troop() { UnitKind::Troop } else { UnitKind::Tool }, unit.label().to_owned()),
        None => (UnitKind::Unknown, format!("unit {id}")),
    }
}

/// The same army without the ids the game tables do not know: the fallback if the
/// server refuses an army that included one.
pub fn known_troops_only(army: &[(i64, i64)]) -> Vec<(i64, i64)> {
    army.iter()
        .copied()
        .filter(|(id, _)| unit_kind(*id).0 == UnitKind::Troop)
        .collect()
}

/// `sdi` reply: the source castle and what is at home there right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdiReply {
    /// `SCID`: must equal the castle the job is about.
    pub castle_id: i64,
    /// `gui.I`: `[id, count]` at home.
    pub home: BTreeMap<i64, i64>,
    /// `gui.TU`: units out on marches. Not available troops.
    pub out: BTreeMap<i64, i64>,
}

pub fn parse_sdi(payload: &Value) -> Option<SdiReply> {
    Some(SdiReply {
        castle_id: payload.get("SCID").and_then(Value::as_i64)?,
        home: pairs(payload.pointer("/gui/I")),
        out: pairs(payload.pointer("/gui/TU")),
    })
}

/// A confirmed support march.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdsAck {
    /// The support movement's own MID: the one `mcm` must name.
    pub mid: i64,
    pub progress_s: i64,
    pub duration_s: i64,
    pub kingdom_id: i64,
    pub source_area_id: i64,
    pub destination: (i64, i64),
    pub troops: Vec<(i64, i64)>,
}

pub fn parse_cds_ack(payload: &Value) -> Option<CdsAck> {
    let movement = payload.pointer("/A/M")?;
    let target = movement.get("TA")?.as_array()?;
    let source = movement.get("SA")?.as_array()?;
    Some(CdsAck {
        mid: movement.get("MID").and_then(Value::as_i64)?,
        progress_s: movement.get("PT").and_then(Value::as_i64).unwrap_or(0),
        duration_s: movement.get("TT").and_then(Value::as_i64)?,
        kingdom_id: movement.get("KID").and_then(Value::as_i64).unwrap_or(0),
        source_area_id: source.get(3).and_then(Value::as_i64)?,
        destination: (target.get(1)?.as_i64()?, target.get(2)?.as_i64()?),
        troops: pairs(payload.pointer("/A/A")).into_iter().collect(),
    })
}

/// A confirmed recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McmAck {
    pub mid: i64,
    pub progress_s: i64,
    pub duration_s: i64,
}

impl McmAck {
    /// Estimated seconds until the troops are home: `TT - PT`.
    pub fn return_s(&self) -> i64 {
        (self.duration_s - self.progress_s).max(0)
    }
}

pub fn parse_mcm_ack(payload: &Value) -> Option<McmAck> {
    let movement = payload.pointer("/A/M")?;
    Some(McmAck {
        mid: movement.get("MID").and_then(Value::as_i64)?,
        progress_s: movement.get("PT").and_then(Value::as_i64).unwrap_or(0),
        duration_s: movement.get("TT").and_then(Value::as_i64)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_xt_packet;
    use crate::pvp::seen_movements;

    const ATTACK: &str = include_str!("../../../docs/network/captures/gam_attack_on_owned_castle.txt");
    const DCL: &str = include_str!("../../../docs/network/captures/dcl_exchange.txt");
    const SDI_REPLY: &str = include_str!("../../../docs/network/captures/sdi_response.txt");
    const CDS: &str = include_str!("../../../docs/network/captures/cds_response.txt");
    const MCM: &str = include_str!("../../../docs/network/captures/mcm_response.txt");

    fn reply(text: &str, prefix: &str) -> Value {
        let line = text
            .lines()
            .find(|line| line.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix} line"));
        parse_xt_packet(line.trim()).unwrap().payload
    }

    /// The account the capture targets: player 596904 in alliance 3026, whose
    /// castle (0, 543, 520) is attacked by 432945 of alliance 3652.
    fn me() -> AllianceIdentity {
        AllianceIdentity {
            player_id: 596904,
            player_name: Some("great emperor 9".into()),
            alliance_id: Some(3026),
            alliance_name: Some("top secret".into()),
        }
    }

    fn attack() -> SeenMovement {
        let payload = parse_xt_packet(ATTACK.trim()).unwrap().payload;
        seen_movements(&payload).remove(0)
    }

    // ---- detection ----

    #[test]
    fn an_attack_on_an_owned_castle_by_another_alliance_is_a_threat() {
        let threat = threat_from(&attack(), &me(), &[(0, 543, 520)], 1_000_000).unwrap();
        assert_eq!(threat.mid, 156095506);
        assert_eq!((threat.castle_id, threat.castle_x, threat.castle_y), (437391, 543, 520));
        assert_eq!(threat.landing_ms, 1_000_000 + 181_000, "TT - PT = 185 - 4");
        assert_eq!(threat.attacker.as_deref(), Some("WIZRAD007"));
    }

    #[test]
    fn only_a_tuple_of_the_account_matches() {
        assert!(threat_from(&attack(), &me(), &[(0, 546, 520)], 0).is_none(), "the sender's coordinates");
        assert!(threat_from(&attack(), &me(), &[(1, 543, 520)], 0).is_none(), "another kingdom");
        assert!(threat_from(&attack(), &me(), &[], 0).is_none());
    }

    #[test]
    fn the_same_alliance_or_no_alliance_at_all_is_skipped() {
        let mut same = attack();
        same.sender.alliance_raw = same.target.alliance_raw;
        assert!(threat_from(&same, &me(), &[(0, 543, 520)], 0).is_none());
        let mut neither = attack();
        neither.sender.alliance_raw = Some(-1);
        neither.target.alliance_raw = Some(-1);
        assert!(threat_from(&neither, &me(), &[(0, 543, 520)], 0).is_none(), "-1 against -1");
        let mut one_sided = attack();
        one_sided.sender.alliance_raw = Some(-1);
        assert!(threat_from(&one_sided, &me(), &[(0, 543, 520)], 0).is_some(), "-1 against a real alliance");
        let mut missing = attack();
        missing.sender.alliance_raw = None;
        assert!(threat_from(&missing, &me(), &[(0, 543, 520)], 0).is_none(), "no player record");
    }

    #[test]
    fn our_own_outward_march_and_other_peoples_castles_are_ignored() {
        let mut own = attack();
        own.sender.player_id = Some(me().player_id);
        assert!(threat_from(&own, &me(), &[(0, 543, 520)], 0).is_none());
        let mut someone_elses = attack();
        someone_elses.target.player_id = Some(1);
        assert!(threat_from(&someone_elses, &me(), &[(0, 543, 520)], 0).is_none());
    }

    // ---- time ----

    #[test]
    fn every_deadline_is_measured_from_a_landing_pulled_earlier() {
        let timeline = Timeline::new(100_000, 300);
        assert_eq!(timeline.safe_landing_ms, 100_000 - 1_000 - 300);
        assert_eq!(timeline.deadline_ms, timeline.safe_landing_ms - 5_000);
        assert_eq!(timeline.last_cds_send_ms(400), timeline.safe_landing_ms - 400 - 250);
    }

    const SPECS: [GapSpec; 3] = [CONTEXT_TO_DCL, DCL_TO_SDI, SDI_TO_CDS];

    #[test]
    fn normalised_gaps_stay_inside_their_windows_whatever_the_draw() {
        for sample in [0.0, 0.4, 1.0, 3.0, 9.0, 60.0, 4_000.0, f64::MAX] {
            let gaps = normalise_gaps(&[sample; 3], &SPECS, i64::MAX / 4).unwrap();
            for (gap, spec) in gaps.iter().zip(SPECS) {
                assert!(*gap >= spec.floor_ms && *gap <= spec.ceiling_ms, "{sample}: {gap}");
            }
        }
    }

    #[test]
    fn a_larger_draw_is_never_a_shorter_gap() {
        let mut last = [0_i64; 3];
        for sample in [0.0, 0.5, 1.0, 2.0, 3.0, 6.0, 20.0, 600.0] {
            let gaps = normalise_gaps(&[sample; 3], &SPECS, i64::MAX / 4).unwrap();
            for index in 0..3 {
                assert!(gaps[index] >= last[index]);
                last[index] = gaps[index];
            }
        }
    }

    #[test]
    fn a_typical_draw_lands_mid_window_and_keeps_randomness() {
        let gaps = normalise_gaps(&[3.0], &[DCL_TO_SDI], 10_000).unwrap();
        assert_eq!(gaps[0], 900 + (2_100.0 * 0.5) as i64);
        let a = normalise_gaps(&[1.2], &[DCL_TO_SDI], 10_000).unwrap();
        let b = normalise_gaps(&[4.8], &[DCL_TO_SDI], 10_000).unwrap();
        assert_ne!(a, b, "different draws give different gaps");
    }

    #[test]
    fn a_short_budget_scales_the_slack_down_but_never_below_a_floor() {
        let floors: i64 = SPECS.iter().map(|spec| spec.floor_ms).sum();
        let gaps = normalise_gaps(&[50.0; 3], &SPECS, floors + 500).unwrap();
        assert!(gaps.iter().sum::<i64>() <= floors + 500);
        for (gap, spec) in gaps.iter().zip(SPECS) {
            assert!(*gap >= spec.floor_ms);
        }
        assert_eq!(normalise_gaps(&[1.0; 3], &SPECS, floors - 1), None, "floors alone do not fit");
    }

    #[test]
    fn the_prep_is_laid_out_backwards_from_d() {
        let timeline = Timeline::new(200_000, 250);
        let plan = plan_prep(0, &timeline, true, [3.0, 3.0]);
        assert_eq!(plan.sdi_at_ms, timeline.deadline_ms, "sdi starts the window");
        let dcl = plan.dcl_at_ms.unwrap();
        let context = plan.context_at_ms.unwrap();
        assert!(context < dcl && dcl < plan.sdi_at_ms);
        assert!(plan.sdi_at_ms - dcl >= 900 && plan.sdi_at_ms - dcl <= 3_000);
        assert_eq!(plan.degraded, None);
        let without = plan_prep(0, &timeline, false, [3.0, 3.0]);
        assert_eq!(without.context_at_ms, None);
    }

    #[test]
    fn a_late_detection_drops_optional_steps_in_order_instead_of_missing() {
        let timeline = Timeline::new(10_000, 250);
        // D = 10_000 - 1_000 - 250 - 5_000 = 3_750, now = 3_000: 750 ms to D.
        let plan = plan_prep(3_000, &timeline, true, [3.0, 3.0]);
        assert_eq!(plan.context_at_ms, None);
        assert_eq!(plan.dcl_at_ms, None, "900 ms floor does not fit in 750");
        assert_eq!(plan.sdi_at_ms, 3_750);
        assert!(plan.degraded.is_some());
        // 1.25 s to D: the dcl fits but the context step does not.
        let plan = plan_prep(2_500, &timeline, true, [3.0, 3.0]);
        assert_eq!(plan.context_at_ms, None);
        assert!(plan.dcl_at_ms.is_some());
        assert_eq!(plan.degraded, Some(Degraded::NoContext));
        // Past D: sdi goes now.
        assert_eq!(plan_prep(9_000, &timeline, true, [3.0, 3.0]).sdi_at_ms, 9_000);
    }

    #[test]
    fn the_think_pause_never_eats_the_window() {
        assert_eq!(cds_think_ms(500.0, 0, 100), 0, "no room: send at once");
        let long = cds_think_ms(500.0, 0, 2_000);
        assert!(long <= 1_000, "{long}: at most half what is left");
        let typical = cds_think_ms(3.0, 0, 10_000);
        assert!((350..=1_800).contains(&typical));
    }

    #[test]
    fn the_recall_comes_a_little_after_the_landing() {
        for sample in [0.1, 1.0, 3.0, 40.0, 4_000.0] {
            let delay = recall_delay_ms(sample);
            assert!((900..=4_500).contains(&delay), "{delay}");
        }
    }

    // ---- commander ----

    fn free(lid: i64) -> LidState {
        LidState { lid, available_after_ms: 0 }
    }

    #[test]
    fn a_commander_no_task_uses_is_lent_first() {
        let roster = [1, 2, 3, 4, 5];
        let tasks = vec![vec![1, 2], vec![3, 4]];
        let states: Vec<_> = roster.iter().map(|lid| free(*lid)).collect();
        assert_eq!(choose_support_lid(&roster, &tasks, &[], &states, 100, 1_000), Some(5));
    }

    #[test]
    fn otherwise_the_last_free_commander_of_the_richest_task_goes() {
        let roster = [1, 2, 3, 4];
        let tasks = vec![vec![1, 2], vec![3, 4]];
        let states = vec![
            free(1),
            free(2),
            LidState { lid: 3, available_after_ms: 99_999 },
            free(4),
        ];
        assert_eq!(choose_support_lid(&roster, &tasks, &[], &states, 100, 1_000), Some(2), "task 1 has two free");
    }

    #[test]
    fn a_reserved_commander_is_lent_before_anyone_else_and_never_one_a_task_owns_if_it_can_be_helped() {
        let roster = [1, 2, 3, 4, 5];
        let tasks = vec![vec![1, 2, 3]];
        let states: Vec<_> = roster.iter().map(|lid| free(*lid)).collect();
        // Commanders 4 and 5 belong to no task; 4 is the reserve.
        assert_eq!(choose_support_lid(&roster, &tasks, &[4], &states, 100, 1_000), Some(4));
        // The reserve is out, but back before it is needed: wait for it rather than take a task's.
        let busy = vec![free(1), free(2), free(3), LidState { lid: 4, available_after_ms: 600 }, free(5)];
        assert_eq!(choose_support_lid(&roster, &tasks, &[4], &busy, 100, 1_000), Some(4));
        // Back too late: the old order applies (a free commander no task uses).
        assert_eq!(choose_support_lid(&roster, &tasks, &[4], &busy, 100, 300), Some(5));
        // Two reserved: the free one first.
        let two = vec![LidState { lid: 4, available_after_ms: 900 }, free(5), free(1), free(2), free(3)];
        assert_eq!(choose_support_lid(&roster, &tasks, &[4, 5], &two, 100, 1_000), Some(5));
    }

    #[test]
    fn with_everyone_out_the_first_back_in_time_is_reserved_or_none() {
        let roster = [1, 2];
        let tasks = vec![vec![1, 2]];
        let states = vec![
            LidState { lid: 1, available_after_ms: 900 },
            LidState { lid: 2, available_after_ms: 500 },
        ];
        assert_eq!(choose_support_lid(&roster, &tasks, &[], &states, 100, 1_000), Some(2));
        assert_eq!(choose_support_lid(&roster, &tasks, &[], &states, 100, 400), None, "nobody is back in time");
    }

    // ---- troops ----

    fn map(rows: &[(i64, i64)]) -> BTreeMap<i64, i64> {
        rows.iter().copied().collect()
    }

    #[test]
    fn support_takes_troops_only_largest_first_with_ties_by_id() {
        // 602 is a troop; 614 (ladder), 660 (banner) and 637 are tools.
        let home = map(&[(602, 9), (637, 0), (614, 5), (660, 3)]);
        assert_eq!(select_support_troops(&home, None), vec![(602, 9)]);
        let many = map(&[(602, 5), (10, 5), (11, 7)]);
        assert_eq!(select_support_troops(&many, None), vec![(11, 7), (10, 5), (602, 5)]);
    }

    #[test]
    fn a_unit_the_tables_do_not_know_is_sent_when_it_is_among_the_largest_stacks() {
        // The pingpoko castle: 215 and 227 are not in the tables, 614/660/476 are tools.
        let home = map(&[(216, 44_129), (215, 35_031), (476, 9_571), (227, 7_281), (238, 7_258), (9, 4_000), (12, 1_762), (217, 528), (34, 176), (35, 130), (754, 10), (760, 10), (614, 2_929)]);
        let picked = select_support_troops(&home, None);
        let ids: Vec<i64> = picked.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![216, 215, 227, 238, 9, 12, 217, 34, 35, 754], "ten types, largest first, the smallest (760) dropped");
        assert!(!ids.contains(&476) && !ids.contains(&614), "tools stay home");
        assert_eq!(picked[1], (215, 35_031));
        assert_eq!(known_troops_only(&picked).len(), 8, "the fallback leaves out 215 and 227");
    }

    #[test]
    fn at_most_ten_types_and_the_smaller_of_two_readings_wins() {
        let ids: Vec<i64> = empire_game::TROOPS.iter().map(|troop| troop.id).take(14).collect();
        let home: BTreeMap<_, _> = ids.iter().enumerate().map(|(index, id)| (*id, 100 + index as i64)).collect();
        let picked = select_support_troops(&home, None);
        assert_eq!(picked.len(), MAX_SUPPORT_UNITS);
        assert!(picked.windows(2).all(|pair| pair[0].1 >= pair[1].1));
        let check = map(&[(602, 4)]);
        assert_eq!(select_support_troops(&map(&[(602, 9)]), Some(&check)), vec![(602, 4)]);
        assert!(select_support_troops(&map(&[(602, 9)]), Some(&map(&[]))).is_empty(), "dcl does not list it");
    }

    // ---- packets ----

    #[test]
    fn requests_match_the_captured_wire_format() {
        assert_eq!(dcl_packet("EmpireEx_49").unwrap(), r#"%xt%EmpireEx_49%dcl%1%{"CD":1}%"#);
        assert_eq!(
            sdi_packet("EmpireEx_49", (543, 520), (468, 507)).unwrap(),
            r#"%xt%EmpireEx_49%sdi%1%{"SX":543,"SY":520,"TX":468,"TY":507}%"#
        );
        assert_eq!(mcm_packet("EmpireEx_49", 156018970).unwrap(), r#"%xt%EmpireEx_49%mcm%1%{"MID":156018970}%"#);
        let cds = cds_packet("EmpireEx_49", 437391, (468, 507), 0, &[(602, 5)]).unwrap();
        let captured: Value = serde_json::from_str(
            r#"{"SID":437391,"TX":468,"TY":507,"LID":0,"WT":1,"HBW":-1,"BPC":0,"PTT":0,"SD":0,"A":[[602,5]]}"#,
        )
        .unwrap();
        assert_eq!(parse_xt_packet(&cds).unwrap().payload, captured, "same fields as the capture");
    }

    // ---- replies ----

    #[test]
    fn dcl_reads_the_castle_stock_and_food() {
        let dcl = parse_dcl(&reply(DCL, "%xt%dcl%1%0%")).unwrap();
        assert_eq!(dcl.player_id, 596904);
        let castle = &dcl.castles[0];
        assert_eq!((castle.kingdom_id, castle.castle_id), (0, 437391));
        assert_eq!(castle.units.get(&602), Some(&9));
        assert_eq!(castle.units.get(&637), Some(&0), "zero rows are kept, not available");
        assert!((castle.food - 637.14).abs() < 1e-6);
        assert_eq!(castle.mead, 0.0);
    }

    #[test]
    fn units_are_told_apart_by_the_game_tables() {
        assert_eq!(unit_kind(602).0, UnitKind::Troop);
        assert_eq!(unit_kind(614), (UnitKind::Tool, "scaling_ladder".to_owned()));
        assert_eq!(unit_kind(999_999).0, UnitKind::Unknown);
        // The display labels come from the game's own table and know every id the older tables miss.
        assert_eq!(unit_label(214), (UnitKind::Troop, "Valkyrie ranger".to_owned()));
        assert_eq!(unit_label(215), (UnitKind::Troop, "Shield-maiden".to_owned()));
        assert_eq!(unit_label(614), (UnitKind::Tool, "Scaling ladder".to_owned()));
        assert_eq!(unit_label(999_999), (UnitKind::Unknown, "unit 999999".to_owned()));
        // ...but what is sent is still decided by the older tables: 215 is a troop on screen, not a known troop to send.
        assert_eq!(unit_kind(215).0, UnitKind::Unknown);
        assert_eq!(known_troops_only(&[(215, 10), (607, 5)]), vec![(607, 5)]);
    }

    #[test]
    fn sdi_reads_the_castle_and_what_is_home() {
        let sdi = parse_sdi(&reply(SDI_REPLY, "%xt%sdi%1%0%")).unwrap();
        assert_eq!(sdi.castle_id, 437391);
        assert_eq!(sdi.home.get(&602), Some(&9));
        assert!(sdi.out.is_empty());
    }

    #[test]
    fn the_cds_ack_names_the_support_march_not_the_attack() {
        let ack = parse_cds_ack(&reply(CDS, "%xt%cds%1%0%")).unwrap();
        assert_eq!(ack.mid, 156018970);
        assert_eq!((ack.progress_s, ack.duration_s), (0, 4509));
        assert_eq!(ack.source_area_id, 437391);
        assert_eq!(ack.destination, (468, 507));
        assert_eq!(ack.troops, vec![(602, 5)]);
    }

    #[test]
    fn the_mcm_ack_gives_the_return_time() {
        let ack = parse_mcm_ack(&reply(MCM, "%xt%mcm%1%0%")).unwrap();
        assert_eq!(ack.mid, 156018970);
        assert_eq!(ack.return_s(), 1349 - 1044);
    }

    #[test]
    fn replies_missing_their_key_fields_are_not_read_as_something_else() {
        assert!(parse_sdi(&json!({"gui": {"I": [[602, 9]]}})).is_none());
        assert!(parse_cds_ack(&json!({"A": {"M": {"MID": 1}}})).is_none());
        assert!(parse_mcm_ack(&json!({})).is_none());
        assert!(parse_dcl(&json!({"C": []})).is_none());
    }
}
