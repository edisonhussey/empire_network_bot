//! Moving troops between kingdoms (`kut`) and skipping the journey (`msk`): the rules.
//!
//! Pure: packets, parsers, the cost, and every check that can be made before anything is
//! sent. No I/O and no clock of its own (callers pass `now_ms`). The session part is
//! `empire-daemon/src/direct/transfer.rs`, the stored jobs `store/transfer.rs`.
//!
//! Packet facts, the cost formula and the live run that proved them:
//! `docs/network/kut.md`, `docs/network/msk.md`, `docs/game_infrastructure/kingdom_transfer_rules.md`.
//!
//! ```text
//! coins = round( sum(kingdomTravellingCost(unit) * count) * unitTravelTaxRate(target) / 100 )
//! ```
//!
//! The server already refuses a bad transfer; the point of [`check`] is that a refusal is a
//! mistake worth avoiding (repeated refusals are what get an account locked), so everything
//! the client's own dialog would have stopped is stopped here first.

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{PacketError, encode_client_xt};

/// `TT` in `msk` / `kst`: 1 is a troop transfer, 2 a goods transfer.
pub const UNIT_TRANSFER_TT: i64 = 1;
/// `CID` in `kut` when no particular castle in the target kingdom is chosen.
pub const ANY_CASTLE: i64 = -1;
/// A stored castle reading older than this is not trusted for a send.
pub const STOCK_MAX_AGE_MS: i64 = 10 * 60 * 1_000;
/// More skip items than this for one journey means the wrong (too short) item was picked.
pub const MAX_SKIPS: i64 = 10;
/// Refusals allowed inside [`ERROR_WINDOW_MS`] before new sends pause (the operating rule).
pub const ERROR_LIMIT: usize = 2;
pub const ERROR_WINDOW_MS: i64 = 10 * 60 * 1_000;

// ---- rates, costs, skip items -------------------------------------------------

/// `unitTravelTaxRate` of the **target** kingdom, percent.
pub fn tax_percent(kingdom_id: i64) -> Option<i64> {
    rate(kingdom_id).map(|(tax, _)| tax)
}

/// How long a troop transfer into the kingdom takes, seconds (`unitTravelTime`).
pub fn travel_seconds(kingdom_id: i64) -> Option<i64> {
    rate(kingdom_id).map(|(_, seconds)| seconds)
}

fn rate(kingdom_id: i64) -> Option<(i64, i64)> {
    let rule = empire_game::kingdom_rule(kingdom_id)?;
    Some((rule.unit_travel_tax_rate?, rule.unit_travel_time?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostError {
    /// The unit has no `kingdomTravellingCost` on record: it is a tool, or not a transferable unit.
    UnknownUnit(i64),
    UnknownKingdom(i64),
}

/// Coins the game charges to send `units` into `target_kingdom` (before the client's own
/// discount function, which was 0 in the live run).
pub fn cost_c1(units: &[(i64, i64)], target_kingdom: i64) -> Result<i64, CostError> {
    let tax = tax_percent(target_kingdom).ok_or(CostError::UnknownKingdom(target_kingdom))?;
    let mut total = 0_f64;
    for (unit, count) in units {
        let each = empire_game::kingdom_travelling_cost(*unit).ok_or(CostError::UnknownUnit(*unit))?;
        total += each as f64 * (tax as f64 / 100.0) * *count as f64;
    }
    Ok(total.round() as i64)
}

/// One minute-skip item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SkipItem {
    pub key: &'static str,
    pub seconds: i64,
}

/// The skip item for a key such as `MS5`.
pub fn skip_item(key: &str) -> Option<SkipItem> {
    empire_game::minute_skips()
        .iter()
        .find(|skip| skip.mst == key)
        .map(|skip| SkipItem { key: skip.mst, seconds: skip.minutes_skip_value * 60 })
}

/// Every skip item, shortest first.
pub fn skip_items() -> Vec<SkipItem> {
    empire_game::minute_skips()
        .iter()
        .map(|skip| SkipItem { key: skip.mst, seconds: skip.minutes_skip_value * 60 })
        .collect()
}

/// Items of one kind needed to cover `remaining_s` seconds (each one overshoots when it must).
pub fn skips_needed(remaining_s: i64, item: SkipItem) -> i64 {
    if remaining_s <= 0 {
        0
    } else {
        (remaining_s + item.seconds - 1) / item.seconds
    }
}

// ---- packets ------------------------------------------------------------------

pub fn kpi_packet(server_header: &str) -> Result<String, PacketError> {
    encode_client_xt(server_header, "kpi", "1", &json!({}))
}

/// The client's own field order is kept (`serde_json` would sort the keys): the packets are
/// the ones proven live, byte for byte.
fn xt(server_header: &str, command: &str, body: String) -> Result<String, PacketError> {
    if server_header.is_empty() {
        return Err(PacketError::MissingCommand);
    }
    Ok(format!("%xt%{server_header}%{command}%1%{body}%"))
}

/// `kut`: send `units` from a castle to another kingdom. `target_castle` is [`ANY_CASTLE`] unless
/// one is chosen.
pub fn kut_packet(
    server_header: &str,
    source_castle: i64,
    source_kingdom: i64,
    target_kingdom: i64,
    target_castle: i64,
    units: &[(i64, i64)],
) -> Result<String, PacketError> {
    let army = Value::Array(units.iter().map(|(id, count)| json!([id, count])).collect());
    xt(
        server_header,
        "kut",
        format!(
            r#"{{"SCID":{source_castle},"SKID":{source_kingdom},"TKID":{target_kingdom},"CID":{target_castle},"A":{army}}}"#
        ),
    )
}

/// `msk`: spend one minute-skip item on the transfer travelling into `target_kingdom`. `KID` and
/// `TT` are strings on the wire, as the client sends them.
pub fn msk_packet(server_header: &str, item: SkipItem, target_kingdom: i64) -> Result<String, PacketError> {
    xt(
        server_header,
        "msk",
        format!(r#"{{"MST":"{}","KID":"{target_kingdom}","TT":"{UNIT_TRANSFER_TT}"}}"#, item.key),
    )
}

// ---- what the server tells us ------------------------------------------------

/// A troop transfer travelling to a kingdom (`kpi.UT[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InFlight {
    pub kingdom_id: i64,
    pub remaining_s: i64,
    pub units: BTreeMap<i64, i64>,
}

/// The parts of `kpi` transfers need: which kingdoms are open, and what is on its way.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KingdomState {
    pub unlocked: BTreeMap<i64, bool>,
    pub transfers: Vec<InFlight>,
}

impl KingdomState {
    pub fn in_flight(&self, kingdom_id: i64) -> Option<&InFlight> {
        self.transfers.iter().find(|transfer| transfer.kingdom_id == kingdom_id)
    }
}

/// The `kpi` object in a packet: the payload itself (`kpi` reply) or `payload.kpi` (inside `gbd`,
/// `kut` and `msk`).
pub fn find_kpi(payload: &Value) -> Option<&Value> {
    if payload.get("UL").is_some() {
        Some(payload)
    } else {
        payload.get("kpi").filter(|inner| inner.get("UL").is_some())
    }
}

/// Parse a `kpi` object. A snapshot: the client replaces its list on every `kpi`, so a missing
/// `UT` means nothing is travelling.
pub fn parse_kpi(kpi: &Value) -> Option<KingdomState> {
    let mut state = KingdomState::default();
    for row in kpi.get("UL")?.as_array()? {
        if let Some(kingdom) = row.get("KID").and_then(Value::as_i64) {
            state.unlocked.insert(kingdom, row.get("U").and_then(Value::as_i64) == Some(1));
        }
    }
    for row in kpi.get("UT").and_then(Value::as_array).into_iter().flatten() {
        let (Some(kingdom_id), Some(remaining_s)) =
            (row.get("KID").and_then(Value::as_i64), row.get("RS").and_then(Value::as_i64))
        else {
            continue;
        };
        state.transfers.push(InFlight { kingdom_id, remaining_s, units: pairs(row.get("I")) });
    }
    Some(state)
}

/// `gcu` object (a `gcu` packet, or `payload.gcu` in `gbd` and `kut`): `(coins, rubies)`.
pub fn find_currency(payload: &Value) -> Option<(i64, i64)> {
    let object = if payload.get("C1").is_some() { payload } else { payload.get("gcu")? };
    Some((object.get("C1")?.as_i64()?, object.get("C2")?.as_i64().unwrap_or(0)))
}

/// `sce` rows `[[key, count], ...]` into a map. Only the keys given are in it.
pub fn parse_skip_stock(sce: &Value) -> BTreeMap<String, i64> {
    sce.as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((row.first()?.as_str()?.to_owned(), row.get(1)?.as_i64()?))
        })
        .filter(|(key, _)| skip_item(key).is_some())
        .collect()
}

/// How a `kut` reply ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KutOutcome {
    /// Status 0. `transfer` is the entry for the target kingdom in the reply's `kpi`, if it had one.
    Accepted { transfer: Option<InFlight> },
    /// Any other status: nothing was sent, nothing was charged.
    Refused { status: String },
}

pub fn parse_kut_reply(status: Option<&str>, payload: &Value, target_kingdom: i64) -> KutOutcome {
    match status {
        Some("0") | None => KutOutcome::Accepted {
            transfer: find_kpi(payload)
                .and_then(parse_kpi)
                .and_then(|state| state.in_flight(target_kingdom).cloned()),
        },
        Some(other) => KutOutcome::Refused { status: other.to_owned() },
    }
}

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

// ---- the plan and the checks before sending -----------------------------------

/// What the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferPlan {
    pub source_castle_id: i64,
    pub source_kingdom_id: i64,
    pub target_kingdom_id: i64,
    /// [`ANY_CASTLE`] unless a castle was chosen.
    pub target_castle_id: i64,
    /// `[unit id, count]`, each unit once.
    pub units: Vec<(i64, i64)>,
    /// A skip item key such as `MS5`: spend it until the troops land. `None` waits the journey out.
    pub skip: Option<String>,
}

/// What is known right now. Every field is `None`/default when it has not been read yet, and an
/// unknown is itself a problem, never a pass.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The source castle's stock and when it was read.
    pub stock: Option<(i64, BTreeMap<i64, i64>)>,
    /// When a transfer from the source castle was last sent (its stock reading must be newer).
    pub last_sent_from_castle_ms: Option<i64>,
    pub coins: Option<i64>,
    pub kingdoms: Option<KingdomState>,
    pub skip_stock: BTreeMap<String, i64>,
    /// An attack bot, recruitment or support is running.
    pub bot_running: bool,
    /// The castle's troops are out as support, or a support march is being sent.
    pub castle_held: bool,
    /// Whether the stored castle list says the account owns the source castle in that kingdom.
    pub owns_source: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemCode {
    NoUnits,
    SameKingdom,
    UnknownUnit,
    NotOwned,
    KingdomLocked,
    BotRunning,
    CastleBusy,
    StockUnknown,
    StockStale,
    NotEnoughTroops,
    CoinsUnknown,
    NotEnoughCoins,
    KingdomsUnknown,
    AlreadyTravelling,
    SkipUnknown,
    SkipTooMany,
    NotEnoughSkips,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub code: ProblemCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// Coins the transfer will cost, when it can be computed.
    pub cost_c1: Option<i64>,
    pub travel_s: Option<i64>,
    /// Skip items one journey needs, when a skip is chosen and known.
    pub skips_needed: Option<i64>,
    /// Empty means nothing known stops it. It is still the server's call.
    pub problems: Vec<Problem>,
}

impl Check {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Every reason the transfer should not be sent yet. `now_ms` is only used to age the stock.
pub fn check(plan: &TransferPlan, snapshot: &Snapshot, now_ms: i64) -> Check {
    let mut problems = Vec::new();
    let mut add = |code, message: String| problems.push(Problem { code, message });

    if plan.units.is_empty() || plan.units.iter().any(|(_, count)| *count <= 0) {
        add(ProblemCode::NoUnits, "Choose at least one unit and a count above zero.".to_owned());
    }
    if plan.source_kingdom_id == plan.target_kingdom_id {
        add(ProblemCode::SameKingdom, "The target is the kingdom the troops are already in.".to_owned());
    }
    if snapshot.bot_running {
        add(ProblemCode::BotRunning, "Stop the bot first: a transfer only runs while it is off.".to_owned());
    }
    if !snapshot.owns_source {
        add(ProblemCode::NotOwned, "That castle is not one of this account's castles.".to_owned());
    }
    if snapshot.castle_held {
        add(ProblemCode::CastleBusy, "This castle's troops are out as support right now.".to_owned());
    }

    let cost = match cost_c1(&plan.units, plan.target_kingdom_id) {
        Ok(cost) => Some(cost),
        Err(CostError::UnknownUnit(id)) => {
            add(ProblemCode::UnknownUnit, format!("Unit {id} cannot be sent between kingdoms from here."));
            None
        }
        Err(CostError::UnknownKingdom(id)) => {
            add(ProblemCode::KingdomLocked, format!("Kingdom {id} is not a transfer target."));
            None
        }
    };

    match &snapshot.kingdoms {
        None => add(ProblemCode::KingdomsUnknown, "Kingdom state has not been read yet; refresh and retry.".to_owned()),
        Some(state) => {
            if state.unlocked.get(&plan.target_kingdom_id) != Some(&true) {
                add(ProblemCode::KingdomLocked, format!("Kingdom {} is not unlocked.", plan.target_kingdom_id));
            }
            if state.in_flight(plan.target_kingdom_id).is_some() {
                add(
                    ProblemCode::AlreadyTravelling,
                    "Troops are already travelling to that kingdom; wait until they land.".to_owned(),
                );
            }
        }
    }

    match &snapshot.stock {
        None => add(ProblemCode::StockUnknown, "This castle's troops have not been read yet; refresh and retry.".to_owned()),
        Some((read_at_ms, stock)) => {
            if now_ms.saturating_sub(*read_at_ms) > STOCK_MAX_AGE_MS
                || snapshot.last_sent_from_castle_ms.is_some_and(|sent| *read_at_ms <= sent)
            {
                add(ProblemCode::StockStale, "The castle reading is out of date; refresh and retry.".to_owned());
            }
            for (unit, need) in &plan.units {
                let have = stock.get(unit).copied().unwrap_or(0);
                if have < *need {
                    add(
                        ProblemCode::NotEnoughTroops,
                        format!("Unit {unit}: needs {need}, the castle has {have}."),
                    );
                }
            }
        }
    }

    match (snapshot.coins, cost) {
        (None, _) => add(ProblemCode::CoinsUnknown, "Coins are not known yet; refresh and retry.".to_owned()),
        (Some(have), Some(need)) if have < need => {
            add(ProblemCode::NotEnoughCoins, format!("Needs {need} coins, you have {have}."));
        }
        _ => {}
    }

    let travel_s = travel_seconds(plan.target_kingdom_id);
    let mut skips = None;
    if let Some(key) = &plan.skip {
        match (skip_item(key), travel_s) {
            (None, _) => add(ProblemCode::SkipUnknown, format!("{key} is not a skip item.")),
            (Some(item), Some(travel)) => {
                let needed = skips_needed(travel, item);
                skips = Some(needed);
                if needed > MAX_SKIPS {
                    add(ProblemCode::SkipTooMany, format!("{key} would take {needed} items; pick a longer one."));
                }
                if let Some(have) = snapshot.skip_stock.get(key)
                    && *have < needed
                {
                    add(ProblemCode::NotEnoughSkips, format!("Needs {needed} of {key}, you have {have}."));
                }
            }
            (Some(_), None) => {}
        }
    }

    Check { cost_c1: cost, travel_s, skips_needed: skips, problems }
}

// ---- pacing and the error budget ---------------------------------------------

/// The shortest and longest a manual step waits, seconds.
pub const PACE_FLOOR_S: f64 = 1.0;
pub const PACE_CAP_S: f64 = 12.0;

/// Map a draw from the shared wait generator (heavy-tailed: mostly seconds, sometimes minutes)
/// into a window that suits a step the operator is watching. Monotone, so the generator's
/// randomness is kept; only the tail is compressed.
pub fn paced(raw_s: f64) -> f64 {
    let raw = if raw_s.is_finite() { raw_s.max(0.0) } else { PACE_FLOOR_S };
    PACE_FLOOR_S + (PACE_CAP_S - PACE_FLOOR_S) * (1.0 - (-(raw - PACE_FLOOR_S).max(0.0) / 5.0).exp())
}

/// Refusals in a rolling window. When [`ERROR_LIMIT`] have happened inside
/// [`ERROR_WINDOW_MS`] nothing new is sent until the oldest ages out.
#[derive(Debug, Default, Clone)]
pub struct ErrorBudget {
    at_ms: VecDeque<i64>,
}

impl ErrorBudget {
    pub fn record(&mut self, now_ms: i64) {
        self.at_ms.push_back(now_ms);
        self.prune(now_ms);
    }

    fn prune(&mut self, now_ms: i64) {
        while self.at_ms.front().is_some_and(|at| now_ms - at >= ERROR_WINDOW_MS) {
            self.at_ms.pop_front();
        }
    }

    /// When sending may resume, if it is paused now.
    pub fn paused_until(&mut self, now_ms: i64) -> Option<i64> {
        self.prune(now_ms);
        (self.at_ms.len() >= ERROR_LIMIT)
            .then(|| self.at_ms[self.at_ms.len() - ERROR_LIMIT] + ERROR_WINDOW_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "EmpireEx_21";
    const NOW: i64 = 1_800_000_000_000;

    fn plan() -> TransferPlan {
        TransferPlan {
            source_castle_id: 16_011_862,
            source_kingdom_id: 0,
            target_kingdom_id: 3,
            target_castle_id: ANY_CASTLE,
            units: vec![(655, 20)],
            skip: Some("MS5".to_owned()),
        }
    }

    fn ready() -> Snapshot {
        Snapshot {
            stock: Some((NOW - 1_000, BTreeMap::from([(655, 20), (277, 9_890)]))),
            last_sent_from_castle_ms: None,
            coins: Some(2_147_483_437),
            kingdoms: Some(KingdomState {
                unlocked: BTreeMap::from([(0, true), (3, true), (4, false)]),
                transfers: Vec::new(),
            }),
            skip_stock: BTreeMap::from([("MS5".to_owned(), 996)]),
            bot_running: false,
            castle_held: false,
            owns_source: true,
        }
    }

    fn codes(check: &Check) -> Vec<ProblemCode> {
        check.problems.iter().map(|problem| problem.code).collect()
    }

    // The live run of 2026-10-17 (docs/game_infrastructure/kingdom_transfer_rules.md).
    #[test]
    fn the_cost_formula_reproduces_both_live_transfers() {
        assert_eq!(cost_c1(&[(277, 1)], 0), Ok(210), "one unit 277 into green: 300 x 70 %");
        assert_eq!(cost_c1(&[(655, 20)], 3), Ok(2_500), "20 knights into Fire Peaks: 250 x 50 % x 20");
        assert_eq!(cost_c1(&[(277, 20)], 3), Ok(3_000));
        assert_eq!(cost_c1(&[(655, 3), (277, 1)], 0), Ok((250.0 * 0.7 * 3.0 + 300.0 * 0.7_f64).round() as i64));
    }

    #[test]
    fn unknown_units_and_kingdoms_have_no_cost() {
        assert_eq!(cost_c1(&[(999_999, 1)], 3), Err(CostError::UnknownUnit(999_999)));
        assert_eq!(cost_c1(&[(655, 1)], 77), Err(CostError::UnknownKingdom(77)));
        assert_eq!(cost_c1(&[], 3), Ok(0));
    }

    #[test]
    fn rates_come_from_the_items_file() {
        assert_eq!([0, 1, 2, 3, 4].map(|k| tax_percent(k).unwrap()), [70, 40, 30, 50, 40]);
        assert!([0, 1, 2, 3, 4].iter().all(|k| travel_seconds(*k) == Some(7_200)));
        assert_eq!(tax_percent(99), None);
    }

    #[test]
    fn skip_items_are_the_seven_minute_skips_and_ms5_is_an_hour() {
        let seconds = skip_items().iter().map(|item| (item.key, item.seconds)).collect::<Vec<_>>();
        assert_eq!(
            seconds,
            vec![("MS1", 60), ("MS2", 300), ("MS3", 600), ("MS4", 1_800), ("MS5", 3_600), ("MS6", 18_000), ("MS7", 86_400)]
        );
        assert_eq!(skip_item("MS5").unwrap().seconds, 3_600);
        assert!(skip_item("MS0").is_none() && skip_item("kut").is_none());
    }

    #[test]
    fn a_two_hour_journey_needs_two_hour_skips() {
        let hour = skip_item("MS5").unwrap();
        assert_eq!(skips_needed(7_200, hour), 2);
        assert_eq!(skips_needed(3_589, hour), 1);
        assert_eq!(skips_needed(7_201, hour), 3);
        assert_eq!(skips_needed(0, hour), 0);
        assert_eq!(skips_needed(7_200, skip_item("MS6").unwrap()), 1);
    }

    #[test]
    fn packets_match_the_ones_proven_live() {
        assert_eq!(
            kut_packet(HEADER, 16_011_862, 0, 3, ANY_CASTLE, &[(655, 20)]).unwrap(),
            r#"%xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[655,20]]}%"#
        );
        assert_eq!(
            msk_packet(HEADER, skip_item("MS5").unwrap(), 3).unwrap(),
            r#"%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%"#
        );
        assert_eq!(kpi_packet(HEADER).unwrap(), "%xt%EmpireEx_21%kpi%1%{}%");
    }

    #[test]
    fn the_live_kut_reply_is_read_for_the_target_kingdom() {
        let reply = json!({"kpi":{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1}],
            "UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}});
        let KutOutcome::Accepted { transfer: Some(flight) } = parse_kut_reply(Some("0"), &reply, 3) else {
            panic!("expected an accepted transfer");
        };
        assert_eq!((flight.kingdom_id, flight.remaining_s, flight.units.get(&655)), (3, 7_200, Some(&20)));
        // Accepted but no entry for that kingdom (it already landed, or another kingdom's entry).
        assert_eq!(parse_kut_reply(Some("0"), &reply, 0), KutOutcome::Accepted { transfer: None });
        assert_eq!(
            parse_kut_reply(Some("27"), &json!({}), 3),
            KutOutcome::Refused { status: "27".to_owned() }
        );
    }

    #[test]
    fn kpi_is_a_snapshot_and_no_ut_means_nothing_travels() {
        let landed = json!({"UL":[{"KID":3,"U":1},{"KID":4,"U":0}]});
        let state = parse_kpi(find_kpi(&landed).unwrap()).unwrap();
        assert!(state.transfers.is_empty() && state.unlocked[&3] && !state.unlocked[&4]);
        // Inside gbd / kut / msk the same object is under `kpi`.
        let wrapped = json!({"kpi": landed, "other": 1});
        assert!(find_kpi(&wrapped).is_some());
        assert!(find_kpi(&json!({"x": 1})).is_none());
        assert!(parse_kpi(&json!({"nothing": true})).is_none());
    }

    #[test]
    fn coins_and_skip_stock_are_read_from_their_packets_and_from_gbd() {
        assert_eq!(find_currency(&json!({"C1": 2_147_480_937_i64, "C2": 150_163})), Some((2_147_480_937, 150_163)));
        assert_eq!(find_currency(&json!({"gcu": {"C1": 5, "C2": 6}, "gpi": {}})), Some((5, 6)));
        assert_eq!(find_currency(&json!({"gpi": {}})), None);
        let stock = parse_skip_stock(&json!([["KT", 9], ["MS5", 996], ["MS1", 3786], ["bogus"]]));
        assert_eq!(stock, BTreeMap::from([("MS1".to_owned(), 3786), ("MS5".to_owned(), 996)]));
    }

    #[test]
    fn a_ready_transfer_has_no_problems_and_costs_what_the_live_run_charged() {
        let result = check(&plan(), &ready(), NOW);
        assert!(result.ok(), "{:?}", result.problems);
        assert_eq!((result.cost_c1, result.travel_s, result.skips_needed), (Some(2_500), Some(7_200), Some(2)));
    }

    #[test]
    fn each_failure_the_server_would_refuse_is_caught_first() {
        let mut snapshot = ready();
        snapshot.coins = Some(2_499);
        snapshot.stock = Some((NOW - 1_000, BTreeMap::from([(655, 19)])));
        snapshot.kingdoms.as_mut().unwrap().unlocked.insert(3, false);
        snapshot.bot_running = true;
        snapshot.castle_held = true;
        snapshot.owns_source = false;
        snapshot.skip_stock.insert("MS5".to_owned(), 1);
        let mut bad = plan();
        bad.target_kingdom_id = 3;
        assert_eq!(
            codes(&check(&bad, &snapshot, NOW)),
            vec![
                ProblemCode::BotRunning,
                ProblemCode::NotOwned,
                ProblemCode::CastleBusy,
                ProblemCode::KingdomLocked,
                ProblemCode::NotEnoughTroops,
                ProblemCode::NotEnoughCoins,
                ProblemCode::NotEnoughSkips,
            ]
        );
    }

    #[test]
    fn nothing_known_is_never_a_pass() {
        let result = check(&plan(), &Snapshot { owns_source: true, ..Snapshot::default() }, NOW);
        assert_eq!(
            codes(&result),
            vec![ProblemCode::KingdomsUnknown, ProblemCode::StockUnknown, ProblemCode::CoinsUnknown]
        );
    }

    #[test]
    fn stale_stock_and_stock_read_before_the_last_send_are_refused() {
        let mut snapshot = ready();
        snapshot.stock.as_mut().unwrap().0 = NOW - STOCK_MAX_AGE_MS - 1;
        assert_eq!(codes(&check(&plan(), &snapshot, NOW)), vec![ProblemCode::StockStale]);
        let mut snapshot = ready();
        snapshot.last_sent_from_castle_ms = Some(NOW - 500);
        assert_eq!(codes(&check(&plan(), &snapshot, NOW)), vec![ProblemCode::StockStale]);
        snapshot.last_sent_from_castle_ms = Some(NOW - 5_000);
        assert!(check(&plan(), &snapshot, NOW).ok(), "read after the send is fine");
    }

    #[test]
    fn a_second_transfer_to_a_kingdom_still_being_travelled_to_is_refused() {
        let mut snapshot = ready();
        snapshot.kingdoms.as_mut().unwrap().transfers.push(InFlight {
            kingdom_id: 3,
            remaining_s: 3_000,
            units: BTreeMap::from([(655, 20)]),
        });
        assert_eq!(codes(&check(&plan(), &snapshot, NOW)), vec![ProblemCode::AlreadyTravelling]);
        let mut elsewhere = plan();
        elsewhere.target_kingdom_id = 2;
        snapshot.kingdoms.as_mut().unwrap().unlocked.insert(2, true);
        assert!(check(&elsewhere, &snapshot, NOW).ok(), "another target kingdom is independent");
    }

    #[test]
    fn plan_shape_mistakes_are_caught() {
        let mut empty = plan();
        empty.units.clear();
        assert!(codes(&check(&empty, &ready(), NOW)).contains(&ProblemCode::NoUnits));
        let mut zero = plan();
        zero.units = vec![(655, 0)];
        assert!(codes(&check(&zero, &ready(), NOW)).contains(&ProblemCode::NoUnits));
        let mut same = plan();
        same.target_kingdom_id = 0;
        assert!(codes(&check(&same, &ready(), NOW)).contains(&ProblemCode::SameKingdom));
        let mut tool = plan();
        tool.units = vec![(999_999, 1)];
        assert!(codes(&check(&tool, &ready(), NOW)).contains(&ProblemCode::UnknownUnit));
        let mut bad_skip = plan();
        bad_skip.skip = Some("MS9".to_owned());
        assert!(codes(&check(&bad_skip, &ready(), NOW)).contains(&ProblemCode::SkipUnknown));
        let mut tiny = plan();
        tiny.skip = Some("MS1".to_owned());
        assert!(codes(&check(&tiny, &ready(), NOW)).contains(&ProblemCode::SkipTooMany), "120 one-minute skips");
    }

    #[test]
    fn a_transfer_without_a_skip_needs_no_skip_stock() {
        let mut waiting = plan();
        waiting.skip = None;
        let mut snapshot = ready();
        snapshot.skip_stock.clear();
        let result = check(&waiting, &snapshot, NOW);
        assert!(result.ok());
        assert_eq!(result.skips_needed, None);
    }

    #[test]
    fn paced_waits_stay_in_the_window_and_keep_their_order() {
        let draws = [0.0, 0.5, 1.0, 2.0, 5.0, 20.0, 240.0, 5_000.0];
        let waits = draws.map(paced);
        assert!(waits.iter().all(|w| (PACE_FLOOR_S..=PACE_CAP_S).contains(w)), "{waits:?}");
        assert!(waits.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(paced(f64::NAN), PACE_FLOOR_S);
        assert!(paced(1e9) < PACE_CAP_S + 1e-9);
    }

    #[test]
    fn two_refusals_in_ten_minutes_pause_sending_until_the_older_one_ages_out() {
        let mut budget = ErrorBudget::default();
        assert_eq!(budget.paused_until(NOW), None);
        budget.record(NOW);
        assert_eq!(budget.paused_until(NOW + 1_000), None, "one is allowed");
        budget.record(NOW + 60_000);
        assert_eq!(budget.paused_until(NOW + 61_000), Some(NOW + ERROR_WINDOW_MS), "the second pauses");
        assert_eq!(budget.paused_until(NOW + ERROR_WINDOW_MS + 1), None, "the first aged out");
        budget.record(NOW + ERROR_WINDOW_MS + 5);
        assert_eq!(budget.paused_until(NOW + ERROR_WINDOW_MS + 6), Some(NOW + 60_000 + ERROR_WINDOW_MS));
    }
}
