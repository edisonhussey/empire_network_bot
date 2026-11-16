//! The resource bot's refinery job: visit the Green main castle, read the refinery and the stock,
//! start the crafts the stock covers, go back to the map, and come back when the queue is empty.
//!
//! Rules (which crafts, affordability, when next) live in `empire-core/src/refinery.rs`; the schedule
//! is stored (`refinery_state`) so it survives a restart. Flow and diagram:
//! `docs/architecture/resource-bot.md`. Packets: `docs/network/crst.md`, `crin.md`.
//!
//! Sequence on the wire, every step waiting for the server's answer:
//! `jca` (open the castle) → `crin` (refinery) → `dcl` (stock) → `crst` × 1–2 (staggered) → `gaa` (map).

use empire_core::refinery::{self, RefineRecipe, RefineryView};
use empire_core::station::parse_dcl;
use empire_core::store::RefineryState;

use super::*;

#[derive(Debug, Clone)]
pub(super) struct Visit {
    castle_id: i64,
    castle_name: String,
    x: i64,
    y: i64,
    recipes: Vec<RefineRecipe>,
    cycle_index: i64,
    view: Option<RefineryView>,
}

#[derive(Debug)]
pub(super) enum RefinePhase {
    Idle { due_ms: i64 },
    AwaitCastle { visit: Visit, deadline_ms: i64 },
    ReadyCrin { visit: Visit, due_ms: i64 },
    AwaitCrin { visit: Visit, deadline_ms: i64 },
    ReadyDcl { visit: Visit, due_ms: i64 },
    AwaitDcl { visit: Visit, deadline_ms: i64 },
    ReadyCraft { visit: Visit, crafts: Vec<RefineRecipe>, done: usize, due_ms: i64 },
    AwaitCraft { visit: Visit, crafts: Vec<RefineRecipe>, done: usize, deadline_ms: i64 },
    ReadyMap { visit: Visit, due_ms: i64 },
    AwaitMap { deadline_ms: i64 },
}

pub(super) struct RefineryAutomation {
    pub(super) phase: RefinePhase,
    pub(super) rng: Rng,
    pub(super) detail: String,
}

impl RefineryAutomation {
    pub(super) fn new() -> Self {
        Self { phase: RefinePhase::Idle { due_ms: 0 }, rng: Rng::from_entropy(), detail: "Resource bot is stopped".to_owned() }
    }

    /// While a visit is under way the connection is the refinery's: nothing else may move the view.
    pub(super) fn holds_transport(&self) -> bool {
        !matches!(self.phase, RefinePhase::Idle { .. })
    }

    pub(super) async fn next_packet(&mut self, store: &Store, account_id: &str, server_header: &str) -> anyhow::Result<Option<String>> {
        let now = now_ms();
        if let Some(deadline) = self.deadline()
            && now >= deadline
        {
            log_event(store, account_id, "refinery.timeout", &format!("no answer while: {}; retry in 30 min", self.detail)).await;
            // Keep the castle and the recipe cycle; only the next visit moves.
            let mut state = store.refinery_state(account_id).await?.unwrap_or_default();
            state.next_due_ms = now + refinery::SHORT_RETRY_MS;
            state.last_detail = format!("timed out: {}", self.detail);
            let _ = store.set_refinery_state(account_id, &state, now).await;
            self.phase = RefinePhase::Idle { due_ms: state.next_due_ms };
            return Ok(None);
        }
        let phase = std::mem::replace(&mut self.phase, RefinePhase::Idle { due_ms: now + 1_000 });
        let (packet, next) = match phase {
            RefinePhase::Idle { due_ms } if now < due_ms => (None, RefinePhase::Idle { due_ms }),
            RefinePhase::Idle { .. } => return self.start_visit(store, account_id, server_header, now).await,
            RefinePhase::ReadyCrin { visit, due_ms } if now >= due_ms => {
                self.detail = format!("Reading the refinery of {}", visit.castle_name);
                (Some(encode_client_xt(server_header, "crin", "1", &json!({}))?), RefinePhase::AwaitCrin { visit, deadline_ms: now + REQUEST_TIMEOUT_MS })
            }
            RefinePhase::ReadyDcl { visit, due_ms } if now >= due_ms => {
                self.detail = format!("Checking inventory for castle {}", visit.castle_name);
                (Some(encode_client_xt(server_header, "dcl", "1", &json!({"CD": 1}))?), RefinePhase::AwaitDcl { visit, deadline_ms: now + REQUEST_TIMEOUT_MS })
            }
            RefinePhase::ReadyCraft { visit, crafts, done, due_ms } if now >= due_ms => {
                let recipe = crafts[done];
                let Some(view) = visit.view.as_ref() else {
                    return Ok(None);
                };
                let packet = encode_client_xt(
                    server_header,
                    "crst",
                    "1",
                    &json!({"KID": view.kingdom_id, "AID": visit.castle_id, "OID": view.object_id, "PWR": 0, "CRID": recipe.crid()}),
                )?;
                self.detail = format!("Crafting {} ({} of {}) at {}", recipe.name(), done + 1, crafts.len(), visit.castle_name);
                (Some(packet), RefinePhase::AwaitCraft { visit, crafts, done, deadline_ms: now + REQUEST_TIMEOUT_MS })
            }
            RefinePhase::ReadyMap { visit, due_ms } if now >= due_ms => {
                let ax1 = visit.x.div_euclid(13) * 13;
                let ay1 = visit.y.div_euclid(13) * 13;
                let packet = encode_client_xt(server_header, "gaa", "1", &json!({"KID": 0, "AX1": ax1, "AY1": ay1, "AX2": ax1 + 12, "AY2": ay1 + 12}))?;
                self.detail = "Back to the map after the refinery".to_owned();
                (Some(packet), RefinePhase::AwaitMap { deadline_ms: now + REQUEST_TIMEOUT_MS })
            }
            other => (None, other),
        };
        self.phase = next;
        Ok(packet)
    }

    fn deadline(&self) -> Option<i64> {
        match &self.phase {
            RefinePhase::AwaitCastle { deadline_ms, .. }
            | RefinePhase::AwaitCrin { deadline_ms, .. }
            | RefinePhase::AwaitDcl { deadline_ms, .. }
            | RefinePhase::AwaitCraft { deadline_ms, .. }
            | RefinePhase::AwaitMap { deadline_ms } => Some(*deadline_ms),
            _ => None,
        }
    }

    /// Idle and due: is a job running, is its time up, which castle; then open the castle.
    async fn start_visit(&mut self, store: &Store, account_id: &str, server_header: &str, now: i64) -> anyhow::Result<Option<String>> {
        let Some(active) = store.active_refinery(account_id).await? else {
            self.detail = "No resource bot with a refinery job is running".to_owned();
            self.phase = RefinePhase::Idle { due_ms: now + 5_000 };
            return Ok(None);
        };
        let saved = store.refinery_state(account_id).await?.unwrap_or_default();
        if saved.next_due_ms > now {
            // Checked again each minute, so a stop or a changed preset is noticed between visits.
            self.detail = format!("Refinery: next visit in {} min", (saved.next_due_ms - now + 59_999) / 60_000);
            self.phase = RefinePhase::Idle { due_ms: saved.next_due_ms.min(now + 60_000) };
            return Ok(None);
        }
        let Some((castle_id, x, y, name)) = store.green_main_castle(account_id).await? else {
            self.detail = "No Green main castle known yet; the refinery waits".to_owned();
            self.phase = RefinePhase::Idle { due_ms: now + 60_000 };
            return Ok(None);
        };
        let visit = Visit { castle_id, castle_name: format!("{name} ({castle_id})"), x, y, recipes: active.recipes, cycle_index: saved.cycle_index, view: None };
        self.detail = format!("Opening castle {} for the refinery", visit.castle_name);
        let packet = encode_client_xt(server_header, "jca", "1", &json!({"CID": castle_id, "KID": 0}))?;
        self.phase = RefinePhase::AwaitCastle { visit, deadline_ms: now + REQUEST_TIMEOUT_MS };
        Ok(Some(packet))
    }

    /// Save the schedule and go back to the map (or straight to idle when the castle was never opened).
    async fn finish_visit(&mut self, store: &Store, account_id: &str, visit: Option<Visit>, crafted: usize, next_due_ms: i64, now: i64) {
        let (castle_id, cycle_index) = visit.as_ref().map_or((0, 0), |visit| (visit.castle_id, visit.cycle_index + crafted as i64));
        let state = RefineryState { castle_id, next_due_ms, cycle_index, last_visit_ms: now, last_detail: self.detail.clone() };
        let _ = store.set_refinery_state(account_id, &state, now).await;
        self.phase = match visit {
            Some(visit) => RefinePhase::ReadyMap { visit, due_ms: now + (self.rng.uniform(1.0, 3.0) * 1_000.0) as i64 },
            None => RefinePhase::Idle { due_ms: next_due_ms },
        };
    }

    pub(super) async fn observe(&mut self, store: &Store, account_id: &str, raw: &str) {
        let Ok(packet) = parse_xt_packet(raw) else {
            return;
        };
        if !self.holds_transport() {
            return;
        }
        let now = now_ms();
        let command = packet.command.as_str();
        let refused = packet.status.as_deref().is_some_and(|status| status != "0");
        let phase = std::mem::replace(&mut self.phase, RefinePhase::Idle { due_ms: now + 1_000 });
        match (phase, command) {
            (RefinePhase::AwaitCastle { visit, .. }, "jaa") => {
                let castle = packet.payload.pointer("/gca/A/3").and_then(Value::as_i64);
                if refused || castle != Some(visit.castle_id) {
                    self.detail = format!("Castle {} did not open; retry in 30 min", visit.castle_name);
                    log_event(store, account_id, "refinery.refused", &self.detail).await;
                    self.finish_visit(store, account_id, Some(visit), 0, now + refinery::SHORT_RETRY_MS, now).await;
                    return;
                }
                let due_ms = now + (self.rng.uniform(0.8, 2.0) * 1_000.0) as i64;
                self.phase = RefinePhase::ReadyCrin { visit, due_ms };
            }
            (RefinePhase::AwaitCrin { mut visit, .. }, "crin") => {
                let Some(view) = (!refused).then(|| refinery::parse_refinery(&packet.payload, visit.castle_id)).flatten() else {
                    self.detail = format!("No refinery found in {}; retry in 30 min", visit.castle_name);
                    log_event(store, account_id, "refinery.refused", &self.detail).await;
                    self.finish_visit(store, account_id, Some(visit), 0, now + refinery::SHORT_RETRY_MS, now).await;
                    return;
                };
                visit.view = Some(view);
                let due_ms = now + (self.rng.uniform(0.5, 1.5) * 1_000.0) as i64;
                self.phase = RefinePhase::ReadyDcl { visit, due_ms };
            }
            (RefinePhase::AwaitDcl { visit, .. }, "dcl") => {
                let stock = (!refused)
                    .then(|| parse_dcl(&packet.payload))
                    .flatten()
                    .and_then(|reply| reply.castles.into_iter().find(|castle| castle.castle_id == visit.castle_id));
                let Some(stock) = stock else {
                    self.detail = format!("No stock for {} in dcl; retry in 30 min", visit.castle_name);
                    log_event(store, account_id, "refinery.refused", &self.detail).await;
                    self.finish_visit(store, account_id, Some(visit), 0, now + refinery::SHORT_RETRY_MS, now).await;
                    return;
                };
                let view = visit.view.clone().expect("crin read before dcl");
                let crafts = refinery::plan(&visit.recipes, view.free_places(), visit.cycle_index.max(0) as usize, stock.wood, stock.stone);
                let checked = format!(
                    "checking inventory for castle {} -> (wood {:.0}, stone {:.0}; refinery {} of {} free)",
                    visit.castle_name,
                    stock.wood,
                    stock.stone,
                    view.free_places(),
                    refinery::REFINERY_PLACES
                );
                if crafts.is_empty() {
                    let next = refinery::next_visit_ms(now, view.busy_for_s(), 0, &mut self.rng);
                    let reason = if view.free_places() == 0 { "refinery busy" } else { "not enough resources" };
                    self.detail = format!("{checked} -> {reason}; next visit in {} min", (next - now) / 60_000);
                    log_event(store, account_id, "refinery.check", &self.detail).await;
                    self.finish_visit(store, account_id, Some(visit), 0, next, now).await;
                    return;
                }
                let names = crafts.iter().map(|recipe| recipe.name()).collect::<Vec<_>>().join(", ");
                self.detail = format!("{checked} -> executing {names}");
                log_event(store, account_id, "refinery.check", &self.detail).await;
                let due_ms = now + refinery::stagger_ms(&mut self.rng);
                self.phase = RefinePhase::ReadyCraft { visit, crafts, done: 0, due_ms };
            }
            (RefinePhase::AwaitCraft { mut visit, crafts, done, .. }, "crst") => {
                let recipe = crafts[done];
                if refused {
                    self.detail = format!("crst {} refused (status {}) at {}; retry in 30 min", recipe.name(), packet.status.as_deref().unwrap_or("?"), visit.castle_name);
                    log_event(store, account_id, "refinery.refused", &self.detail).await;
                    self.finish_visit(store, account_id, Some(visit), done, now + refinery::SHORT_RETRY_MS, now).await;
                    return;
                }
                if let Some(view) = refinery::parse_refinery(&packet.payload, visit.castle_id) {
                    visit.view = Some(view);
                }
                log_event(store, account_id, "refinery.craft", &format!("started 100 {} at {}", recipe.name(), visit.castle_name)).await;
                let done = done + 1;
                if done < crafts.len() {
                    let due_ms = now + refinery::stagger_ms(&mut self.rng);
                    self.phase = RefinePhase::ReadyCraft { visit, crafts, done, due_ms };
                    return;
                }
                let busy = visit.view.as_ref().map_or(0, RefineryView::busy_for_s);
                let next = refinery::next_visit_ms(now, busy, done, &mut self.rng);
                self.detail = format!("Refinery full at {}; next visit in {} min", visit.castle_name, (next - now) / 60_000);
                self.finish_visit(store, account_id, Some(visit), done, next, now).await;
            }
            (RefinePhase::AwaitMap { .. }, "gaa") => {
                self.phase = RefinePhase::Idle { due_ms: now + 5_000 };
            }
            (phase, _) => self.phase = phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use empire_core::account::OwnedCastle;
    use empire_core::store::REFINERY;

    const ACCOUNT: &str = "ventrilo";
    const ZONE: &str = "EmpireEx_21";
    const CASTLE: i64 = 16011862;

    async fn store(recipes: &[RefineRecipe]) -> Store {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store.upsert_account_profile(ACCOUNT, "Ventrilo", "wss://x.invalid", ZONE, 1).await.unwrap();
        let castles = [
            OwnedCastle { kingdom_id: 0, castle_id: 16632819, area_type: 4, x: 1, y: 1, name: "outpost".into() },
            OwnedCastle { kingdom_id: 0, castle_id: CASTLE, area_type: 1, x: 468, y: 507, name: "._.".into() },
        ];
        store.replace_account_bootstrap(ACCOUNT, &castles, &[], 1).await.unwrap();
        let preset = store.create_resource_preset("p", REFINERY, recipes, 1).await.unwrap();
        let bot = store.create_resource_bot("r", Some(preset), 1).await.unwrap();
        store.subscribe_account_resource_bot(ACCOUNT, Some(bot), true, 1).await.unwrap();
        store
    }

    /// Make whatever the machine is waiting for due now.
    fn due_now(machine: &mut RefineryAutomation) {
        match &mut machine.phase {
            RefinePhase::Idle { due_ms }
            | RefinePhase::ReadyCrin { due_ms, .. }
            | RefinePhase::ReadyDcl { due_ms, .. }
            | RefinePhase::ReadyCraft { due_ms, .. }
            | RefinePhase::ReadyMap { due_ms, .. } => *due_ms = 0,
            _ => {}
        }
    }

    async fn step(machine: &mut RefineryAutomation, store: &Store) -> Option<String> {
        due_now(machine);
        machine.next_packet(store, ACCOUNT, ZONE).await.unwrap()
    }

    fn dcl(wood: f64, stone: f64) -> String {
        format!(r#"%xt%dcl%1%0%{{"PID":1,"C":[{{"KID":0,"AI":[{{"AID":{CASTLE},"W":{wood},"S":{stone},"F":0,"AC":[]}}]}}]}}%"#)
    }

    const CRIN_EMPTY: &str = r#"%xt%crin%1%0%{"CAI":[{"CBI":[{"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[],"BV":[],"RUT":[]},"PS":{"CRID":[],"BV":[],"RUT":[],"RCT":[]},"S":4,"WID":2174}],"CE":[]}]}%"#;
    const JAA: &str = r#"%xt%jaa%1%0%{"KID":0,"gca":{"A":[1,468,507,16011862,0,0,0,0,0,0]}}%"#;

    #[tokio::test]
    async fn a_visit_checks_stock_then_sends_both_crafts_staggered_and_returns_to_the_map() {
        let store = store(&[RefineRecipe::RefinedWood, RefineRecipe::RefinedStone]).await;
        let mut machine = RefineryAutomation::new();
        assert_eq!(step(&mut machine, &store).await.unwrap(), format!(r#"%xt%{ZONE}%jca%1%{{"CID":{CASTLE},"KID":0}}%"#));
        machine.observe(&store, ACCOUNT, JAA).await;
        assert_eq!(step(&mut machine, &store).await.unwrap(), format!("%xt%{ZONE}%crin%1%{{}}%"));
        machine.observe(&store, ACCOUNT, CRIN_EMPTY).await;
        assert_eq!(step(&mut machine, &store).await.unwrap(), format!(r#"%xt%{ZONE}%dcl%1%{{"CD":1}}%"#));
        machine.observe(&store, ACCOUNT, &dcl(749_990.6, 743_031.8)).await;
        // The first craft waits out a random stagger: 1.5 to 9 s.
        let RefinePhase::ReadyCraft { due_ms, .. } = &machine.phase else { panic!("{:?}", machine.phase) };
        let wait = *due_ms - now_ms();
        assert!((1_000..9_100).contains(&wait), "{wait}");
        assert_eq!(step(&mut machine, &store).await.unwrap(), format!(r#"%xt%{ZONE}%crst%1%{{"AID":{CASTLE},"CRID":1,"KID":0,"OID":393,"PWR":0}}%"#));
        machine.observe(&store, ACCOUNT, r#"%xt%crst%1%0%{"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[]},"PS":{"CRID":[1],"RCT":[3600]},"S":4}%"#).await;
        assert!(matches!(machine.phase, RefinePhase::ReadyCraft { done: 1, .. }));
        assert!(step(&mut machine, &store).await.unwrap().contains(r#""CRID":34"#));
        machine.observe(&store, ACCOUNT, r#"%xt%crst%1%0%{"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[34]},"PS":{"CRID":[1],"RCT":[3599]},"S":4}%"#).await;
        // Saved: the next visit after the queue empties (2 h) and the cycle moved on by two.
        let saved = store.refinery_state(ACCOUNT).await.unwrap().unwrap();
        assert!(saved.next_due_ms - now_ms() >= 7_199_000, "{}", saved.next_due_ms - now_ms());
        assert_eq!(saved.cycle_index, 2);
        assert!(step(&mut machine, &store).await.unwrap().contains("%gaa%"));
        machine.observe(&store, ACCOUNT, r#"%xt%gaa%1%0%{"KID":0}%"#).await;
        assert!(!machine.holds_transport());
        // A new machine (a restart) reads the stored schedule and does not visit early.
        let mut restarted = RefineryAutomation::new();
        assert!(restarted.next_packet(&store, ACCOUNT, ZONE).await.unwrap().is_none());
        assert!(!restarted.holds_transport());
        let events = store.recent_events(ACCOUNT, 10).await.unwrap();
        assert!(events.iter().any(|event| event.detail.contains("checking inventory for castle ._. (16011862) -> (wood 749991, stone 743032") && event.detail.contains("-> executing refined wood, refined stone")));
    }

    #[tokio::test]
    async fn too_little_stock_sends_nothing_and_retries_later() {
        let store = store(&[RefineRecipe::RefinedWood]).await;
        let mut machine = RefineryAutomation::new();
        step(&mut machine, &store).await.unwrap();
        machine.observe(&store, ACCOUNT, JAA).await;
        step(&mut machine, &store).await.unwrap();
        machine.observe(&store, ACCOUNT, CRIN_EMPTY).await;
        step(&mut machine, &store).await.unwrap();
        machine.observe(&store, ACCOUNT, &dcl(54_399.0, 1e6)).await;
        assert!(matches!(machine.phase, RefinePhase::ReadyMap { .. }), "{:?}", machine.phase);
        let saved = store.refinery_state(ACCOUNT).await.unwrap().unwrap();
        let wait = saved.next_due_ms - now_ms();
        assert!((refinery::SHORT_RETRY_MS - 1_000..refinery::MIN_INTERVAL_MS).contains(&wait), "{wait}");
        assert!(step(&mut machine, &store).await.unwrap().contains("%gaa%"));
    }

    #[tokio::test]
    async fn a_stopped_bot_never_opens_the_castle() {
        let store = store(&[RefineRecipe::RefinedWood]).await;
        store.stop_account_resource_bot(ACCOUNT, 2).await.unwrap();
        let mut machine = RefineryAutomation::new();
        assert!(step(&mut machine, &store).await.is_none());
        assert!(!machine.holds_transport());
    }
}
