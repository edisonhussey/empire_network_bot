//! Station tests against a fake clock.
//!
//! The captures in `docs/network_packets/` are the packets: the `gam` of an attack
//! on castle 437391 of account `ventrilo` (player 596904, alliance 3026), and the
//! `dcl`, `sdi`, `cds` and `mcm` replies for the same castle. A [`Sim`] plays the
//! server: it answers each request a fixed round trip later, or not at all.

use super::station::{Gate, Phase, StationAutomation};
use super::*;
use empire_core::{account::OwnedCastle, store::StationBotKingdom};

const ACCOUNT: &str = "ventrilo";
const HEADER: &str = "EmpireEx_49";
const ATTACK: &str = include_str!("../../../../docs/network_packets/gam_156095506.txt");
const DCL: &str = include_str!("../../../../docs/network_packets/dcl.txt");
const SDI: &str = include_str!("../../../../docs/network_packets/sdi_response.txt");
const CDS: &str = include_str!("../../../../docs/network_packets/cds.txt");
const MCM: &str = include_str!("../../../../docs/network_packets/mcm_response.txt");
const T0: i64 = 1_800_000_000_000;
/// Remaining travel in the capture: `TT - PT = 185 - 4`.
const REMAINING_MS: i64 = 181_000;
const SUPPORT_MID: i64 = 156018970;

fn line<'a>(text: &'a str, prefix: &str) -> &'a str {
    text.lines().find(|line| line.starts_with(prefix)).unwrap().trim()
}

async fn world(destinations: &[(i64, i64, i64)]) -> Store {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile(ACCOUNT, "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
        .await
        .unwrap();
    let lids = empire_core::hunt::USABLE_COMMANDER_LIDS[..6].to_vec();
    store
        .replace_account_bootstrap(
            ACCOUNT,
            &[OwnedCastle { kingdom_id: 0, castle_id: 437391, area_type: 1, x: 543, y: 520, name: "Castle great em".into() }],
            &lids,
            1,
        )
        .await
        .unwrap();
    store
        .set_alliance_identity(
            ACCOUNT,
            &empire_core::pvp::AllianceIdentity {
                player_id: 596904,
                player_name: Some("great emperor 9".into()),
                alliance_id: Some(3026),
                alliance_name: Some("top secret".into()),
            },
            1,
        )
        .await
        .unwrap();
    let kingdoms = destinations
        .iter()
        .map(|(kingdom_id, x, y)| StationBotKingdom { kingdom_id: *kingdom_id, x: *x, y: *y })
        .collect::<Vec<_>>();
    let bot = store.create_station_bot("test", &kingdoms, 1).await.unwrap();
    store.subscribe_account_station_bot(ACCOUNT, Some(bot), true, 1).await.unwrap();
    store
}

async fn standard_world() -> Store {
    world(&[(0, 468, 507)]).await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// Reply after one round trip.
    Yes,
    /// Never reply.
    Silent,
}

/// A fake server and clock around one [`StationAutomation`].
struct Sim {
    store: Store,
    station: StationAutomation,
    now: i64,
    round_trip_ms: i64,
    /// `(time sent, command)` of everything the station sent.
    sent: Vec<(i64, String)>,
    pending: Vec<(i64, String)>,
    answers: std::collections::HashMap<&'static str, Answer>,
    cra_in_flight: Option<(i64, i64, i64)>,
}

impl Sim {
    async fn new(store: Store) -> Self {
        let mut station = StationAutomation::new();
        station.restore(&store, ACCOUNT, T0).await;
        Self {
            store,
            station,
            now: T0,
            round_trip_ms: 120,
            sent: Vec::new(),
            pending: Vec::new(),
            answers: Default::default(),
            cra_in_flight: None,
        }
    }

    fn answer(&mut self, command: &'static str, answer: Answer) {
        self.answers.insert(command, answer);
    }

    /// What the server says to a request. The captured replies are for castle
    /// 437391 at (543, 520); for another castle the ids and coordinates in them
    /// are swapped for the ones the request named, as the server's would be.
    fn reply_for(packet: &empire_core::protocol::XtPacket) -> Option<String> {
        let command = packet.command.as_str();
        let castle = |id: i64, x: i64, y: i64, text: String| {
            text.replace("\"SCID\":437391", &format!("\"SCID\":{id}"))
                .replace("[1,543,520,437391,", &format!("[1,{x},{y},{id},"))
        };
        let spot = |id: i64| match id {
            777777 => (600, 600),
            _ => (543, 520),
        };
        Some(match command {
            "dcl" => line(DCL, "%xt%dcl%1%0%").to_owned(),
            "sdi" => {
                let (x, y) = (packet.payload["SX"].as_i64()?, packet.payload["SY"].as_i64()?);
                let id = if (x, y) == (600, 600) { 777777 } else { 437391 };
                castle(id, x, y, line(SDI, "%xt%sdi%1%0%").to_owned())
            }
            "cds" => {
                let id = packet.payload["SID"].as_i64()?;
                let (x, y) = spot(id);
                castle(id, x, y, line(CDS, "%xt%cds%1%0%").to_owned())
            }
            "mcm" => line(MCM, "%xt%mcm%1%0%").to_owned(),
            "gaa" => r#"%xt%gaa%1%0%{"KID":0,"AI":[]}%"#.to_owned(),
            _ => return None,
        })
    }

    async fn feed(&mut self, raw: &str) {
        self.station.observe(&self.store, ACCOUNT, raw, self.now).await;
    }

    /// Advance the clock to `until`, 25 ms at a time, playing the server.
    async fn run_until(&mut self, until: i64) {
        while self.now < until {
            self.now += 25;
            let due = self
                .pending
                .iter()
                .filter(|(at, _)| *at <= self.now)
                .cloned()
                .collect::<Vec<_>>();
            self.pending.retain(|(at, _)| *at > self.now);
            for (_, reply) in due {
                self.feed(&reply).await;
            }
            let packet = self
                .station
                .next_packet(&self.store, ACCOUNT, HEADER, self.now, self.cra_in_flight)
                .await
                .unwrap();
            if let Some(packet) = packet {
                let parsed = parse_xt_packet(&packet).unwrap();
                let command = parsed.command.clone();
                self.sent.push((self.now, command.clone()));
                let answer = self.answers.get(command.as_str()).copied().unwrap_or(Answer::Yes);
                if answer == Answer::Yes
                    && let Some(reply) = Self::reply_for(&parsed)
                {
                    self.pending.push((self.now + self.round_trip_ms, reply));
                }
            }
        }
    }

    /// Run until the station has sent `command` (and a step past it).
    async fn run_until_sent(&mut self, command: &str) {
        for _ in 0..40_000 {
            if self.at(command).is_some() {
                return;
            }
            self.run_until(self.now + 25).await;
        }
        panic!("{command} was never sent: {:?}", self.commands());
    }

    fn commands(&self) -> Vec<&str> {
        self.sent.iter().map(|(_, command)| command.as_str()).collect()
    }

    fn at(&self, command: &str) -> Option<i64> {
        self.sent.iter().find(|(_, sent)| sent == command).map(|(at, _)| *at)
    }

    /// The attack `gam`, as received now.
    fn attack(&self) -> String {
        line(ATTACK, "%xt%gam%").to_owned()
    }

    fn landing(&self) -> i64 {
        T0 + REMAINING_MS
    }

    async fn last_job(&self) -> empire_core::store::StationJobRow {
        self.store.recent_station_jobs(ACCOUNT, 1).await.unwrap().remove(0)
    }

    async fn lid_state(&self, lid: i64) -> Option<CommanderState> {
        self.store
            .commander_states(ACCOUNT)
            .await
            .unwrap()
            .into_iter()
            .find(|state| state.lord_id == lid)
    }
}

#[tokio::test]
async fn an_attack_on_an_owned_castle_leases_a_commander_and_plans_before_it_lands() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    assert_eq!(sim.station.jobs.len(), 1);
    let job = &sim.station.jobs[0];
    assert_eq!(job.castle_id, 437391);
    assert_eq!(job.dest, Some((468, 507)));
    assert_eq!(job.covers[0].landing_ms, sim.landing());
    // The commander is spoken for until the support comes back, and is the
    // last one no task uses.
    let lid = job.lid.unwrap();
    assert_eq!(lid, *empire_core::hunt::USABLE_COMMANDER_LIDS[..6].last().unwrap());
    let state = sim.lid_state(lid).await.unwrap();
    assert_eq!(state.status, COMMANDER_OUTBOUND);
    assert!(state.available_after_ms > sim.landing());
    assert_eq!(state.target_key.as_deref(), Some("station:437391"));
    assert_eq!(sim.last_job().await.phase, "waiting", "saved, so a restart can resume it");
    // Nothing is sent at once: the first step waits for its place in the plan.
    sim.run_until(T0 + 1_000).await;
    assert!(sim.sent.is_empty(), "{:?}", sim.commands());
}

#[tokio::test]
async fn the_sequence_runs_in_order_inside_the_window_and_is_recalled_after_the_landing() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    let timeline = sim.station.jobs[0].timeline;
    sim.run_until(sim.landing() + 12_000).await;

    let commands = sim.commands();
    let tail = &commands[commands.len() - 4..];
    assert_eq!(tail, ["dcl", "sdi", "cds", "mcm"], "{commands:?}");
    let (dcl, sdi, cds, mcm) = (sim.at("dcl").unwrap(), sim.at("sdi").unwrap(), sim.at("cds").unwrap(), sim.at("mcm").unwrap());
    assert!(sdi >= timeline.deadline_ms, "the window opens at D, not before");
    assert!(sdi - timeline.deadline_ms < 100, "and sdi goes as it opens");
    assert!((900..=3_100).contains(&(sdi - dcl)), "dcl a couple of seconds ahead: {}", sdi - dcl);
    assert!(cds > sdi + 350, "a pause to read the page: {}", cds - sdi);
    let job = sim.last_job().await;
    let acked = job.cds_acked_at_ms.unwrap();
    assert!(acked <= timeline.safe_landing_ms, "acknowledged before the landing, with margin");
    assert_eq!(job.outcome.as_deref(), Some("saved"));
    assert_eq!(job.support_mid, Some(SUPPORT_MID), "the support's MID, not the attack's");
    assert!(job.lead_ms.unwrap() > 1_000, "{:?}", job.lead_ms);
    assert!(mcm >= sim.landing() + 900 && mcm <= sim.landing() + 4_500 + 200, "recalled just after: {}", mcm - sim.landing());
    assert_eq!(job.phase, "done");
    // The recall frees the commander only when the troops are home (TT - PT).
    let state = sim.lid_state(job.lid.unwrap()).await.unwrap();
    assert!(state.available_after_ms > sim.now + 250_000, "{}", state.available_after_ms - sim.now);
}

#[tokio::test]
async fn the_support_goes_to_the_configured_coordinates_with_the_fresh_stock() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() - 500).await;
    let job = sim.store.recent_station_jobs(ACCOUNT, 1).await.unwrap().remove(0);
    assert_eq!(job.dest, Some((468, 507)));
    let state: serde_json::Value = serde_json::from_str(&job.state_json).unwrap();
    // 602 is the only troop in the stock (614, 660 and 637 are tools).
    assert_eq!(state["troops"], serde_json::json!([[602, 9]]));
}

#[tokio::test]
async fn a_repeated_gam_neither_moves_the_estimate_nor_makes_a_second_job() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.now += 30_000;
    sim.feed(&gam).await;
    assert_eq!(sim.station.jobs.len(), 1);
    assert_eq!(sim.station.jobs[0].covers.len(), 1);
    assert_eq!(sim.station.jobs[0].covers[0].landing_ms, sim.landing(), "first sight wins");
}

#[tokio::test]
async fn a_second_attack_on_the_same_castle_is_covered_by_the_same_march() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    // A second attack arrives 20 s later, landing 40 s after the first.
    let second = gam.replace("156095506", "156095999").replace("\"TT\":185", "\"TT\":245");
    sim.run_until(T0 + 20_000).await;
    sim.feed(&second).await;
    assert_eq!(sim.station.jobs.len(), 1, "one march for the castle");
    assert_eq!(sim.station.jobs[0].covers.len(), 2);
    let second_landing = T0 + 20_000 + 241_000;
    sim.run_until(second_landing + 15_000).await;
    let mcm = sim.at("mcm").unwrap();
    assert!(mcm >= second_landing + 900, "recalled after the LAST attack: {}", mcm - second_landing);
    assert_eq!(sim.commands().iter().filter(|c| **c == "cds").count(), 1);
}

#[tokio::test]
async fn an_earlier_landing_before_anything_is_sent_moves_the_whole_plan_earlier() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    let first_deadline = sim.station.jobs[0].timeline.deadline_ms;
    let sooner = gam.replace("156095506", "156095111").replace("\"TT\":185", "\"TT\":65");
    sim.run_until(T0 + 1_000).await;
    sim.feed(&sooner).await;
    assert!(sim.station.jobs[0].timeline.deadline_ms < first_deadline - 100_000);
    sim.run_until(T0 + 61_000).await;
    assert!(sim.at("sdi").is_some(), "the earlier attack is what the window is built around");
}

#[tokio::test]
async fn a_castle_in_a_kingdom_without_a_destination_is_recorded_not_stationed() {
    let mut sim = Sim::new(world(&[(1, 600, 600)]).await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    assert!(sim.station.jobs.is_empty());
    let job = sim.last_job().await;
    assert_eq!(job.outcome.as_deref(), Some("blocked:no_destination"));
    assert!(sim.store.commander_states(ACCOUNT).await.unwrap().is_empty(), "no commander was lent");
    sim.run_until(sim.landing()).await;
    assert!(sim.sent.is_empty());
}

#[tokio::test]
async fn with_no_station_running_attacks_are_left_alone() {
    let store = standard_world().await;
    store.stop_account_station_bot(ACCOUNT, 5).await.unwrap();
    let mut sim = Sim::new(store).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    assert!(sim.station.jobs.is_empty());
    assert!(sim.store.recent_station_jobs(ACCOUNT, 5).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_attack_seen_with_no_time_left_is_marked_late_not_sent_into_the_void() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack().replace("\"TT\":185", "\"TT\":5");
    sim.feed(&gam).await;
    assert!(sim.station.jobs.is_empty());
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("late:detected"));
}

#[tokio::test]
async fn a_short_notice_drops_optional_steps_but_still_sends_the_support() {
    let mut sim = Sim::new(standard_world().await).await;
    // 7 s of notice: D is two seconds away.
    let gam = sim.attack().replace("\"TT\":185", "\"TT\":11");
    sim.feed(&gam).await;
    sim.run_until(T0 + 7_000).await;
    let commands = sim.commands();
    assert!(commands.contains(&"sdi") && commands.contains(&"cds"), "{commands:?}");
    let job = sim.store.recent_station_jobs(ACCOUNT, 1).await.unwrap().remove(0);
    assert!(job.cds_acked_at_ms.unwrap() < T0 + 7_000);
}

#[tokio::test]
async fn a_retreat_before_the_support_leaves_cancels_everything_and_frees_the_commander() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    let lid = sim.station.jobs[0].lid.unwrap();
    sim.run_until(T0 + 30_000).await;
    sim.feed(r#"%xt%mrm%1%0%{"MID":156095506}%"#).await;
    assert!(sim.station.jobs.is_empty());
    let job = sim.last_job().await;
    assert_eq!(job.outcome.as_deref(), Some("cancelled:retreat"));
    assert_eq!(sim.lid_state(lid).await.unwrap().status, COMMANDER_AVAILABLE);
    sim.run_until(sim.landing() + 10_000).await;
    assert!(sim.sent.is_empty(), "nothing was ever sent: {:?}", sim.commands());
    // A late duplicate of the same attack does not bring it back.
    sim.feed(&gam).await;
    assert!(sim.station.jobs.is_empty());
}

#[tokio::test]
async fn a_retreat_after_the_support_left_recalls_it_promptly() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() - 2_000).await;
    assert!(sim.at("cds").is_some() && sim.at("mcm").is_none());
    let retreat_at = sim.now;
    sim.feed(r#"%xt%mrm%1%0%{"MID":156095506}%"#).await;
    sim.run_until(retreat_at + 6_000).await;
    let mcm = sim.at("mcm").unwrap();
    assert!(mcm >= retreat_at + 900 && mcm <= retreat_at + 4_800, "prompt, not tied to the landing: {}", mcm - retreat_at);
}

#[tokio::test]
async fn a_retreat_while_the_support_is_unanswered_recalls_once_its_mid_is_known() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.round_trip_ms = 1_500;
    let gam = sim.attack();
    sim.feed(&gam).await;
    // Run until the cds has been sent and not yet answered.
    sim.run_until_sent("cds").await;
    assert!(sim.at("cds").is_some() && sim.at("mcm").is_none());
    sim.feed(r#"%xt%mrm%1%0%{"MID":156095506}%"#).await;
    assert_eq!(sim.station.jobs.len(), 1, "the job is kept");
    sim.run_until(sim.now + 8_000).await;
    let cds_ack = sim.last_job().await.cds_acked_at_ms.unwrap();
    let mcm = sim.at("mcm").unwrap();
    assert!(mcm > cds_ack, "never guessed ahead of the support MID");
    assert!(mcm <= cds_ack + 4_800, "and sent promptly once known: {}", mcm - cds_ack);
}

#[tokio::test]
async fn an_unanswered_cds_is_uncertain_and_is_never_sent_twice() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.answer("cds", Answer::Silent);
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() + 30_000).await;
    assert_eq!(sim.commands().iter().filter(|c| **c == "cds").count(), 1, "{:?}", sim.commands());
    assert!(sim.at("mcm").is_none(), "there is no support MID to recall");
    assert!(matches!(sim.station.jobs[0].phase, Phase::Uncertain { .. }));
    // The commander stays reserved and the castle held while it is unknown.
    let lid = sim.station.jobs[0].lid.unwrap();
    assert_eq!(sim.lid_state(lid).await.unwrap().status, COMMANDER_OUTBOUND);
    assert!(sim.station.holds(sim.now).contains(&(0, 543, 520)));
}

#[tokio::test]
async fn an_uncertain_send_is_reconciled_by_the_matching_movement() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.answer("cds", Answer::Silent);
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() + 5_000).await;
    assert!(matches!(sim.station.jobs[0].phase, Phase::Uncertain { .. }));
    // The account's own movement shows up: from this castle to the destination.
    let ours = line(CDS, "%xt%cds%1%0%").replace("%xt%cds%1%0%", "%xt%gam%1%0%");
    let payload: serde_json::Value = serde_json::from_str(&ours[ours.find('{').unwrap()..ours.len() - 1]).unwrap();
    let movement = serde_json::json!({"M": [{"M": payload["A"]["M"], "UM": {}}], "O": payload["O"]});
    sim.feed(&format!("%xt%gam%1%0%{movement}%")).await;
    assert!(matches!(sim.station.jobs[0].phase, Phase::Away));
    assert_eq!(sim.station.jobs[0].support_mid, Some(SUPPORT_MID));
    sim.run_until(sim.now + 8_000).await;
    assert!(sim.at("mcm").is_some(), "now it can be recalled");
}

#[tokio::test]
async fn a_refused_support_frees_the_commander_and_the_castle_and_is_not_retried() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.answer("cds", Answer::Silent);
    let gam = sim.attack();
    sim.feed(&gam).await;
    let lid = sim.station.jobs[0].lid.unwrap();
    sim.run_until_sent("cds").await;
    sim.feed("%xt%cds%1%104%{}%").await;
    assert!(sim.station.jobs.is_empty());
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("refused:cds:104"));
    assert_eq!(sim.lid_state(lid).await.unwrap().status, COMMANDER_AVAILABLE);
    assert!(sim.station.holds(sim.now).is_empty(), "nothing left the castle");
    sim.run_until(sim.landing() + 20_000).await;
    assert_eq!(sim.commands().iter().filter(|c| **c == "cds").count(), 1);
}

#[tokio::test]
async fn a_silent_recall_is_recorded_as_uncertain_and_not_repeated() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.answer("mcm", Answer::Silent);
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() + 30_000).await;
    assert_eq!(sim.commands().iter().filter(|c| **c == "mcm").count(), 1);
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("saved:recall_uncertain"));
}

#[tokio::test]
async fn a_lost_sdi_is_asked_again_once_and_then_given_up_on() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.answer("sdi", Answer::Silent);
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() + 5_000).await;
    let asked = sim.commands().iter().filter(|c| **c == "sdi").count();
    assert!((1..=2).contains(&asked), "at most one retry: {asked}");
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("missed:sdi_timeout"));
    assert!(sim.at("cds").is_none());
}

#[tokio::test]
async fn a_lost_dcl_does_not_stop_the_support() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.answer("dcl", Answer::Silent);
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() + 10_000).await;
    assert!(sim.at("cds").is_some(), "{:?}", sim.commands());
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("saved"));
}

#[tokio::test]
async fn no_troops_at_home_ends_the_job_without_sending_anything() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    // Replace the sdi reply with an empty castle.
    let empty = line(SDI, "%xt%sdi%1%0%").replace("\"I\":[[602,9],[637,0],[614,5],[660,3]]", "\"I\":[[602,0],[614,5]]");
    sim.run_until_sent("sdi").await;
    sim.pending.clear();
    sim.feed(&empty).await;
    assert!(sim.station.jobs.is_empty());
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("no_troops"));
    sim.run_until(sim.landing() + 5_000).await;
    assert!(sim.at("cds").is_none());
}

#[tokio::test]
async fn an_sdi_for_another_castle_is_never_acted_on() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    let wrong = line(SDI, "%xt%sdi%1%0%").replace("\"SCID\":437391", "\"SCID\":999");
    sim.run_until_sent("sdi").await;
    sim.pending.clear();
    sim.feed(&wrong).await;
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("blocked:wrong_castle"));
}

#[tokio::test]
async fn a_cds_reply_for_another_castle_is_not_taken_as_the_pending_send() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.answer("cds", Answer::Silent);
    sim.run_until_sent("cds").await;
    let elsewhere = line(CDS, "%xt%cds%1%0%").replace("[1,543,520,437391,", "[1,543,520,111111,");
    sim.feed(&elsewhere).await;
    assert!(matches!(sim.station.jobs[0].phase, Phase::Uncertain { .. }), "not confirmed");
    assert_eq!(sim.station.jobs[0].support_mid, None);
}

#[tokio::test]
async fn an_unanswered_cra_from_the_same_castle_goes_first() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.cra_in_flight = Some((0, 543, 520));
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() - 3_000).await;
    assert!(sim.at("sdi").is_some());
    assert!(sim.at("cds").is_none(), "held while the cra is unanswered");
    sim.cra_in_flight = None;
    sim.run_until(sim.now + 500).await;
    assert!(sim.at("cds").is_some(), "and sent as soon as it is");
}

#[tokio::test]
async fn a_cra_from_another_castle_does_not_hold_the_support_up() {
    let mut sim = Sim::new(standard_world().await).await;
    sim.cra_in_flight = Some((1, 600, 600));
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing()).await;
    assert!(sim.at("cds").is_some());
}

#[tokio::test]
async fn the_gate_closes_around_the_window_and_opens_again_after_the_cds() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    let first_step = sim.station.jobs[0].plan.unwrap();
    let start = [first_step.context_at_ms, first_step.dcl_at_ms, Some(first_step.sdi_at_ms)]
        .into_iter()
        .flatten()
        .min()
        .unwrap();
    assert_eq!(sim.station.gate(T0 + 100), Gate::Open, "minutes before the window");
    assert_eq!(sim.station.gate(start - 40_000), Gate::Quiet, "no new sequence may start");
    assert_eq!(sim.station.gate(start - 1_000), Gate::Owned, "the station is about to send");
    sim.run_until(sim.landing() - 1_500).await;
    assert!(sim.at("cds").is_some());
    sim.run_until(sim.now + 400).await;
    assert_eq!(sim.station.gate(sim.now), Gate::Open, "the support is out: others may send again");
}

#[tokio::test]
async fn the_castle_is_held_from_the_send_until_the_troops_are_home() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    assert!(sim.station.holds(sim.now).is_empty(), "before the send the troops are home");
    sim.run_until(sim.landing() - 1_500).await;
    assert!(sim.at("cds").is_some());
    assert!(sim.station.holds(sim.now).contains(&(0, 543, 520)));
    sim.run_until(sim.landing() + 12_000).await;
    let home_at = sim.last_job().await;
    assert!(home_at.recalled_at_ms.is_some());
    assert!(sim.station.holds(sim.now).contains(&(0, 543, 520)), "recalled, but still walking home");
    sim.run_until(sim.now + 320_000).await;
    assert!(sim.station.holds(sim.now).is_empty(), "home: attacks may use it again");
}

#[tokio::test]
async fn overlapping_attacks_on_two_castles_are_served_earliest_landing_first_one_at_a_time() {
    let store = standard_world().await;
    store
        .replace_account_bootstrap(
            ACCOUNT,
            &[
                OwnedCastle { kingdom_id: 0, castle_id: 437391, area_type: 1, x: 543, y: 520, name: "A".into() },
                OwnedCastle { kingdom_id: 0, castle_id: 777777, area_type: 4, x: 600, y: 600, name: "B".into() },
            ],
            &empire_core::hunt::USABLE_COMMANDER_LIDS[..6],
            2,
        )
        .await
        .unwrap();
    let mut sim = Sim::new(store).await;
    let a = sim.attack();
    // The same attack on the second castle, landing 3 s later.
    let b = a
        .replace("156095506", "156095777")
        .replace("[1,543,520,437391,", "[1,600,600,777777,")
        .replace("\"TT\":185", "\"TT\":188");
    sim.feed(&a).await;
    sim.feed(&b).await;
    assert_eq!(sim.station.jobs.len(), 2);
    assert_ne!(sim.station.jobs[0].lid, sim.station.jobs[1].lid, "two commanders");
    sim.run_until(sim.landing() + 12_000).await;
    let cds = sim.sent.iter().filter(|(_, command)| command == "cds").map(|(at, _)| *at).collect::<Vec<_>>();
    assert_eq!(cds.len(), 2);
    assert!(cds[1] - cds[0] >= 120, "single flight: the second waits for the first's answer");
    assert!(cds[1] <= sim.landing() + 3_000 - 500, "and still makes its own window: {}", sim.landing() + 3_000 - cds[1]);
}

#[tokio::test]
async fn a_session_that_ended_mid_support_resumes_the_recall() {
    let store = standard_world().await;
    let mut first = Sim::new(store.clone()).await;
    let gam = first.attack();
    first.feed(&gam).await;
    first.run_until(first.landing() - 1_500).await;
    assert!(first.at("cds").is_some());
    // The connection drops; a new session starts after the landing.
    let mut second = Sim::new(store).await;
    second.now = first.landing() + 30_000;
    second.station = {
        let mut station = StationAutomation::new();
        station.restore(&second.store, ACCOUNT, second.now).await;
        station
    };
    assert_eq!(second.station.jobs.len(), 1, "the open job came back");
    assert!(matches!(second.station.jobs[0].phase, Phase::Away));
    second.run_until(second.now + 2_000).await;
    assert!(second.at("mcm").is_some(), "the recall is sent at once: it is overdue");
    // The attack re-announced on login is not a new job.
    let gam = second.attack();
    second.feed(&gam).await;
    assert!(second.station.jobs.is_empty() || second.station.jobs.iter().all(|job| job.covers.len() == 1));
}

#[tokio::test]
async fn a_job_that_never_sent_and_whose_landing_passed_is_closed_when_the_session_returns() {
    let store = standard_world().await;
    let mut first = Sim::new(store.clone()).await;
    let gam = first.attack();
    first.feed(&gam).await;
    let lid = first.station.jobs[0].lid.unwrap();
    let mut second = Sim::new(store).await;
    let late = first.landing() + 60_000;
    let mut station = StationAutomation::new();
    station.restore(&second.store, ACCOUNT, late).await;
    second.station = station;
    assert!(second.station.jobs.is_empty());
    assert_eq!(second.last_job().await.outcome.as_deref(), Some("missed:session_gap"));
    assert_eq!(second.lid_state(lid).await.unwrap().status, COMMANDER_AVAILABLE);
}

#[tokio::test]
async fn an_ally_attacked_by_the_same_alliance_is_not_a_threat() {
    let mut sim = Sim::new(standard_world().await).await;
    // Sender and receiver in the same alliance (3026).
    let gam = sim.attack().replace("\"AID\":3652", "\"AID\":3026");
    sim.feed(&gam).await;
    assert!(sim.station.jobs.is_empty());
}

#[tokio::test]
async fn the_view_names_the_phase_the_gate_and_the_timings() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.station.next_packet(&sim.store, ACCOUNT, HEADER, T0 + 500, None).await.unwrap();
    let view = sim.station.view(T0 + 500);
    assert!(view.enabled);
    assert_eq!(view.bot.as_deref(), Some("test"));
    assert_eq!(view.gate, "open");
    assert_eq!(view.jobs.len(), 1);
    let job = &view.jobs[0];
    assert_eq!(job.phase, "waiting");
    assert_eq!((job.castle, job.dest), ([543, 520], Some([468, 507])));
    assert_eq!(job.attacks[0].attacker.as_deref(), Some("WIZRAD007"));
    assert!(job.deadline_ms < job.landing_ms);
    sim.run_until(sim.landing() + 12_000).await;
    let done = &sim.station.view(sim.now).jobs[0];
    assert_eq!(done.phase, "done");
    assert!(done.lead_ms.unwrap() > 0 && done.cds_ack_ms.is_some() && done.mcm_ack_ms.is_some());
}


/// The point of the feature: whatever the generator draws, the support is
/// acknowledged before the landing. Many random seeds, short and long notice,
/// fast and slow links. These are simulated round trips, not live ones.
#[tokio::test]
async fn the_support_is_acknowledged_in_time_across_many_random_draws() {
    let mut leads = Vec::new();
    for remaining_s in [9, 12, 30, 181] {
        for round_trip_ms in [100, 400, 800] {
            for _ in 0..6 {
                let mut sim = Sim::new(standard_world().await).await;
                sim.round_trip_ms = round_trip_ms;
                let gam = sim.attack().replace("\"TT\":185", &format!("\"TT\":{}", remaining_s + 4));
                sim.feed(&gam).await;
                let landing = T0 + remaining_s * 1_000;
                sim.run_until(landing + 10_000).await;
                let job = sim.last_job().await;
                let acked = job.cds_acked_at_ms.unwrap_or_else(|| panic!("{remaining_s}s/{round_trip_ms}ms: {:?} {:?}", job.outcome, sim.commands()));
                assert!(
                    acked <= landing - 1_000,
                    "{remaining_s}s notice, {round_trip_ms} ms round trip: acknowledged {} ms before the landing",
                    landing - acked
                );
                assert_eq!(job.outcome.as_deref(), Some("saved"), "{remaining_s}s/{round_trip_ms}ms");
                leads.push(landing - acked);
            }
        }
    }
    leads.sort_unstable();
    eprintln!(
        "lead before landing over {} simulated runs: min {} ms, median {} ms, max {} ms",
        leads.len(),
        leads[0],
        leads[leads.len() / 2],
        leads[leads.len() - 1]
    );
}

#[tokio::test]
async fn stopping_the_station_cancels_jobs_that_have_not_sent_and_frees_their_commander() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    let lid = sim.station.jobs[0].lid.unwrap();
    sim.run_until(T0 + 5_000).await;
    sim.store.stop_account_station_bot(ACCOUNT, sim.now).await.unwrap();
    sim.run_until(T0 + 9_000).await;
    assert!(sim.station.jobs.is_empty());
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("cancelled:stopped"));
    assert_eq!(sim.lid_state(lid).await.unwrap().status, COMMANDER_AVAILABLE);
    sim.run_until(sim.landing()).await;
    assert!(sim.sent.is_empty(), "nothing was sent after the stop");
}

#[tokio::test]
async fn stopping_the_station_still_recalls_a_support_that_is_out() {
    let mut sim = Sim::new(standard_world().await).await;
    let gam = sim.attack();
    sim.feed(&gam).await;
    sim.run_until(sim.landing() - 1_500).await;
    assert!(sim.at("cds").is_some());
    sim.store.stop_account_station_bot(ACCOUNT, sim.now).await.unwrap();
    sim.run_until(sim.landing() + 12_000).await;
    assert!(sim.at("mcm").is_some(), "the troops are brought home whatever the toggle says");
}

#[test]
fn the_command_is_read_from_the_frame_without_parsing_it() {
    use super::station::peek_command_for_tests as peek;
    assert_eq!(peek("%xt%gam%1%0%{\"M\":[]}%"), Some("gam"));
    assert_eq!(peek("%xt%EmpireEx_49%dcl%1%{\"CD\":1}%"), Some("dcl"));
    assert_eq!(peek("<msg t='sys'>"), None);
    assert_eq!(peek("%xt%"), None);
}

#[tokio::test]
async fn units_the_tables_do_not_know_are_sent_and_dropped_once_if_the_server_refuses_them() {
    let mut sim = Sim::new(standard_world().await).await;
    // The castle also holds 35,031 of an id missing from the game tables.
    let sdi = line(SDI, "%xt%sdi%1%0%").replace("\"I\":[[602,9],", "\"I\":[[215,35031],[602,9],");
    let gam = sim.attack();
    sim.feed(&gam).await;
    // No dcl reading, so the sdi stock stands alone (the real one listed 215 too).
    sim.answer("dcl", Answer::Silent);
    sim.answer("sdi", Answer::Silent);
    sim.answer("cds", Answer::Silent);
    sim.run_until_sent("sdi").await;
    sim.feed(&sdi).await;
    sim.run_until_sent("cds").await;
    let sent: serde_json::Value = serde_json::from_str(&sim.store.recent_station_jobs(ACCOUNT, 1).await.unwrap()[0].state_json).unwrap();
    assert_eq!(sent["troops"], serde_json::json!([[215, 35031], [602, 9]]), "the big unknown stack goes first");
    // The server refuses: the known troops are sent once more, and only once.
    sim.feed("%xt%cds%1%104%{}%").await;
    assert!(sim.station.jobs[0].army_narrowed);
    sim.run_until(sim.now + 600).await;
    assert_eq!(sim.commands().iter().filter(|c| **c == "cds").count(), 2);
    let again: serde_json::Value = serde_json::from_str(&sim.store.recent_station_jobs(ACCOUNT, 1).await.unwrap()[0].state_json).unwrap();
    assert_eq!(again["troops"], serde_json::json!([[602, 9]]));
    // A second refusal ends it: no loop.
    sim.feed("%xt%cds%1%104%{}%").await;
    assert!(sim.station.jobs.is_empty());
    assert_eq!(sim.last_job().await.outcome.as_deref(), Some("refused:cds:104"));
}
