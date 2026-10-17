//! Kingdom transfers inside the session component, on a fake clock and the packets captured live
//! on 2026-10-17 (20 Traveling Knights, green to Fire Peaks, two `MS5` skips).
//!
//! `TransferAutomation` is driven exactly as the session loop does: `observe` for every inbound
//! frame, `next_packet` on a tick. Time is the `now` the test passes, so a "pause" is just the
//! next tick that is late enough.

use super::*;
use empire_core::{
    account::OwnedCastle,
    station::CastleStock,
    store::{TRANSFER_DONE, TRANSFER_FAILED, TRANSFER_QUEUED, TRANSFER_SENT, TRANSFER_TRAVELLING, TransferRow},
    transfer::KingdomState,
};
use std::collections::BTreeMap;

const ACCOUNT: &str = "ventrilo";
const SOURCE: i64 = 16_011_862;
const T0: i64 = 1_800_000_000_000;
/// Longer than any paced pause (12 s).
const STEP: i64 = 13_000;

const KUT_REPLY: &str = r#"%xt%kut%1%0%{"kpi":{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1}],"UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}}%"#;
const MSK_FIRST: &str = r#"%xt%msk%1%0%{"kpi":{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1}],"UT":[{"KID":3,"RS":3589,"I":[[655,20]]}]}}%"#;
const MSK_LAST: &str = r#"%xt%msk%1%0%{"kpi":{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1}]}}%"#;
const SCE: &str = r#"%xt%sce%1%0%[["MS5",995]]%"#;
const GCU: &str = r#"%xt%gcu%1%0%{"C1":2147480937,"C2":150163}%"#;
const KPI_EMPTY: &str = r#"%xt%kpi%1%0%{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1}]}%"#;
const KPI_FLYING: &str = r#"%xt%kpi%1%0%{"UL":[{"KID":3,"U":1}],"UT":[{"KID":3,"RS":900,"I":[[655,20]]}]}%"#;

fn packet(raw: &str) -> empire_core::protocol::XtPacket {
    parse_xt_packet(raw).unwrap()
}

async fn world(coins: i64) -> Store {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store.upsert_account_profile(ACCOUNT, "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1).await.unwrap();
    let castle = |kingdom_id, castle_id, area_type, x, y| OwnedCastle {
        kingdom_id,
        castle_id,
        area_type,
        x,
        y,
        name: format!("Castle {castle_id}"),
    };
    store
        .replace_account_bootstrap(
            ACCOUNT,
            &[castle(0, SOURCE, 1, 643, 637), castle(3, 16_681_051, 12, 600, 600)],
            hunt::USABLE_COMMANDER_LIDS,
            T0,
        )
        .await
        .unwrap();
    store
        .record_castle_inventory(
            ACCOUNT,
            &[CastleStock {
                kingdom_id: 0,
                castle_id: SOURCE,
                units: BTreeMap::from([(655, 20), (277, 9_890)]),
                food: 0.0,
                mead: 0.0,
                wood: 0.0,
                stone: 0.0,
            }],
            T0,
        )
        .await
        .unwrap();
    store.set_currency(ACCOUNT, coins, 150_163, T0).await.unwrap();
    store
        .set_kingdom_state(
            ACCOUNT,
            &KingdomState { unlocked: BTreeMap::from([(0, true), (3, true)]), transfers: Vec::new() },
            T0,
        )
        .await
        .unwrap();
    store.merge_skip_stock(ACCOUNT, &BTreeMap::from([("MS5".to_owned(), 996)]), T0).await.unwrap();
    store
}

async fn queue(store: &Store, skip: Option<&str>, created_at_ms: i64) -> i64 {
    store
        .save_transfer(&TransferRow {
            job_id: 0,
            account_id: ACCOUNT.to_owned(),
            source_castle_id: SOURCE,
            source_kingdom_id: 0,
            target_kingdom_id: 3,
            target_castle_id: -1,
            units: vec![(655, 20)],
            skip_item: skip.map(str::to_owned),
            phase: TRANSFER_QUEUED.to_owned(),
            detail: "Waiting to be sent".to_owned(),
            cost_c1: Some(2_500),
            skips_sent: 0,
            created_at_ms,
            sent_at_ms: None,
            landed_at_ms: None,
            updated_at_ms: created_at_ms,
        })
        .await
        .unwrap()
}

/// What the session loop does on a tick: ask for a packet, and if one goes out, return it.
async fn tick(runner: &mut TransferAutomation, store: &Store, now: i64) -> Option<String> {
    runner.next_packet(store, ACCOUNT, US1_SERVER_HEADER, now, true, &[]).await.unwrap()
}

/// Tick until a packet goes out (each tick later than any pause), at most `limit` times.
async fn next_sent(runner: &mut TransferAutomation, store: &Store, now: &mut i64, limit: usize) -> Option<String> {
    for _ in 0..limit {
        *now += STEP;
        if let Some(packet) = tick(runner, store, *now).await {
            return Some(packet);
        }
    }
    None
}

/// Run ticks and collect every packet the runner sends; none of them may be a `kut` unless allowed.
async fn sent_in(runner: &mut TransferAutomation, store: &Store, now: &mut i64, ticks: usize) -> Vec<String> {
    let mut packets = Vec::new();
    for _ in 0..ticks {
        *now += STEP;
        packets.extend(tick(runner, store, *now).await);
    }
    packets
}

fn only_reads(packets: &[String]) -> bool {
    packets.iter().all(|packet| packet.contains("%kpi%"))
}

/// What the Overview's `dcl` reply does: a fresh reading of the source castle.
async fn castle_read_again(store: &Store, now: i64) {
    store
        .record_castle_inventory(
            ACCOUNT,
            &[CastleStock {
                kingdom_id: 0,
                castle_id: SOURCE,
                units: BTreeMap::from([(655, 20), (277, 9_890)]),
                food: 0.0,
                mead: 0.0,
                wood: 0.0,
                stone: 0.0,
            }],
            now,
        )
        .await
        .unwrap();
}

async fn reply(runner: &mut TransferAutomation, store: &Store, raw: &str, now: i64) {
    runner.observe(store, ACCOUNT, Some(&packet(raw)), now).await;
}

async fn job(store: &Store, id: i64) -> TransferRow {
    store.transfer(ACCOUNT, id).await.unwrap().unwrap()
}

#[tokio::test]
async fn twenty_knights_go_to_fire_peaks_and_two_hour_skips_land_them() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, Some("MS5"), T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;

    let kut = next_sent(&mut runner, &store, &mut now, 4).await.expect("the kut goes out after a pause");
    assert_eq!(kut, r#"%xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[655,20]]}%"#);
    let sent = job(&store, id).await;
    assert_eq!((sent.phase.as_str(), sent.cost_c1), (TRANSFER_SENT, Some(2_500)));

    // The server's answer: coins first (its own packet), then the reply with the transfer in it.
    reply(&mut runner, &store, GCU, now).await;
    reply(&mut runner, &store, KUT_REPLY, now).await;
    assert_eq!(job(&store, id).await.phase, TRANSFER_TRAVELLING);
    assert_eq!(store.currency(ACCOUNT).await.unwrap().unwrap().0, 2_147_480_937, "coins follow the server's number");
    assert!(store.kingdom_state(ACCOUNT).await.unwrap().unwrap().1.in_flight(3).is_some());

    let skip = next_sent(&mut runner, &store, &mut now, 4).await.expect("a skip follows");
    assert_eq!(skip, r#"%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%"#);
    reply(&mut runner, &store, SCE, now).await;
    reply(&mut runner, &store, MSK_FIRST, now).await;
    assert_eq!(job(&store, id).await.skips_sent, 1);
    assert_eq!(store.skip_stock(ACCOUNT).await.unwrap()["MS5"], 995);

    let second = next_sent(&mut runner, &store, &mut now, 4).await.expect("one hour is left, so a second skip");
    assert!(second.contains(r#""MST":"MS5""#));
    reply(&mut runner, &store, MSK_LAST, now).await;
    let done = job(&store, id).await;
    assert_eq!((done.phase.as_str(), done.skips_sent), (TRANSFER_DONE, 2));
    assert!(done.landed_at_ms.is_some());
    assert_eq!(next_sent(&mut runner, &store, &mut now, 6).await, None, "nothing more is sent once it landed");
}

#[tokio::test]
async fn without_a_skip_it_only_watches_and_notices_the_landing() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    next_sent(&mut runner, &store, &mut now, 4).await.unwrap();
    reply(&mut runner, &store, KUT_REPLY, now).await;
    assert_eq!(next_sent(&mut runner, &store, &mut now, 3).await, None, "no skip item, nothing to send for now");
    // The server later says nothing is travelling any more.
    reply(&mut runner, &store, KPI_EMPTY, now + 7_200_000).await;
    assert_eq!(job(&store, id).await.phase, TRANSFER_DONE);
}

#[tokio::test]
async fn while_the_bot_runs_nothing_is_sent_and_the_request_expires() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, None, T0).await;
    // A bot is running for the account.
    let mode = store.import_mode_bundle(&empire_core::planning::ventrilo_sands_bundle(), T0).await.unwrap();
    store.subscribe_account_mode(ACCOUNT, mode, true, T0).await.unwrap();
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    let packets = sent_in(&mut runner, &store, &mut now, 8).await;
    assert!(only_reads(&packets), "only harmless reads while it waits: {packets:?}");
    let waiting = job(&store, id).await;
    assert_eq!(waiting.phase, TRANSFER_QUEUED);
    assert!(waiting.detail.contains("Stop the bot"), "{}", waiting.detail);
    // Long after, still queued: it is dropped instead of firing whenever the bot stops.
    now += 6 * 60_000;
    let packets = sent_in(&mut runner, &store, &mut now, 3).await;
    assert!(only_reads(&packets), "{packets:?}");
    let dropped = job(&store, id).await;
    assert_eq!(dropped.phase, TRANSFER_FAILED);
    assert!(dropped.detail.contains("five minutes"));
}

#[tokio::test]
async fn a_failed_check_fails_the_job_without_sending_anything() {
    let store = world(2_499).await; // one coin short of the 2,500
    let id = queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    assert_eq!(next_sent(&mut runner, &store, &mut now, 5).await, None);
    let failed = job(&store, id).await;
    assert_eq!(failed.phase, TRANSFER_FAILED);
    assert!(failed.detail.contains("2500 coins") && failed.detail.contains("2499"), "{}", failed.detail);
}

#[tokio::test]
async fn a_busy_connection_holds_the_send_until_it_is_free() {
    let store = world(2_147_483_437).await;
    queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    for _ in 0..4 {
        now += STEP;
        assert!(runner.next_packet(&store, ACCOUNT, US1_SERVER_HEADER, now, false, &[]).await.unwrap().is_none());
    }
    now += STEP;
    assert!(runner.next_packet(&store, ACCOUNT, US1_SERVER_HEADER, now, true, &[]).await.unwrap().is_some());
}

#[tokio::test]
async fn a_castle_whose_troops_are_out_as_support_waits() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    let held = [(0, 643, 637)];
    for _ in 0..4 {
        now += STEP;
        let sent = runner.next_packet(&store, ACCOUNT, US1_SERVER_HEADER, now, true, &held).await.unwrap();
        assert!(sent.is_none_or(|packet| packet.contains("%kpi%")), "no kut while the troops are out");
    }
    let waiting = job(&store, id).await;
    assert_eq!(waiting.phase, TRANSFER_QUEUED);
    assert!(waiting.detail.contains("support"), "{}", waiting.detail);
}

#[tokio::test]
async fn a_refusal_fails_it_and_two_refusals_pause_new_sends() {
    let store = world(2_147_483_437).await;
    let first = queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    assert!(next_sent(&mut runner, &store, &mut now, 4).await.unwrap().contains("%kut%"));
    reply(&mut runner, &store, "%xt%kut%1%27%{}%", now).await;
    let refused = job(&store, first).await;
    assert_eq!(refused.phase, TRANSFER_FAILED);
    assert!(refused.detail.contains("status 27") && refused.detail.contains("Nothing was sent"));

    // A second request, with the castle read again (the send made the old reading stale).
    now += STEP;
    castle_read_again(&store, now).await;
    let second = queue(&store, None, now).await;
    assert!(next_sent(&mut runner, &store, &mut now, 4).await.unwrap().contains("%kut%"));
    reply(&mut runner, &store, "%xt%kut%1%27%{}%", now).await;
    assert_eq!(job(&store, second).await.phase, TRANSFER_FAILED);

    // Two refusals inside ten minutes: the third is held back, however fresh everything is.
    now += STEP;
    castle_read_again(&store, now).await;
    let third = queue(&store, None, now).await;
    let packets = sent_in(&mut runner, &store, &mut now, 8).await;
    assert!(packets.iter().all(|packet| !packet.contains("%kut%")), "paused: {packets:?}");
    let waiting = job(&store, third).await;
    assert_eq!(waiting.phase, TRANSFER_QUEUED);
    assert!(waiting.detail.contains("Paused"), "{}", waiting.detail);
}

#[tokio::test]
async fn an_unanswered_kut_is_read_back_never_sent_again() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    let kut = next_sent(&mut runner, &store, &mut now, 4).await.unwrap();
    assert!(kut.contains("%kut%"));
    // No reply. After the timeout the next packet is a kpi read, not another kut.
    now += REQUEST_TIMEOUT_MS + 1_000;
    assert!(tick(&mut runner, &store, now).await.is_none());
    let read = next_sent(&mut runner, &store, &mut now, 4).await.unwrap();
    assert_eq!(read, "%xt%EmpireEx_21%kpi%1%{}%");
    // The state shows the troops travelling: it did go out.
    reply(&mut runner, &store, KPI_FLYING, now).await;
    assert_eq!(job(&store, id).await.phase, TRANSFER_TRAVELLING);
}

#[tokio::test]
async fn an_unanswered_kut_that_never_went_out_fails_and_is_not_repeated() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, None, T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    next_sent(&mut runner, &store, &mut now, 4).await.unwrap();
    now += REQUEST_TIMEOUT_MS + 1_000;
    tick(&mut runner, &store, now).await;
    next_sent(&mut runner, &store, &mut now, 4).await.unwrap();
    reply(&mut runner, &store, KPI_EMPTY, now).await;
    let failed = job(&store, id).await;
    assert_eq!(failed.phase, TRANSFER_FAILED);
    assert!(failed.detail.contains("did not go out"), "{}", failed.detail);
}

#[tokio::test]
async fn after_a_restart_a_send_in_doubt_is_read_before_anything_else() {
    let store = world(2_147_483_437).await;
    let id = queue(&store, Some("MS5"), T0).await;
    let mut row = job(&store, id).await;
    row.phase = TRANSFER_SENT.to_owned();
    row.sent_at_ms = Some(T0);
    store.save_transfer(&row).await.unwrap();
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    let first = next_sent(&mut runner, &store, &mut now, 4).await.unwrap();
    assert_eq!(first, "%xt%EmpireEx_21%kpi%1%{}%", "a fresh runner reads; it never resends the kut");
}

#[tokio::test]
async fn a_skip_stops_when_the_item_runs_out_and_the_troops_keep_travelling() {
    let store = world(2_147_483_437).await;
    store.merge_skip_stock(ACCOUNT, &BTreeMap::from([("MS5".to_owned(), 0)]), T0).await.unwrap();
    // Zero stock is caught by the check before anything is sent.
    let id = queue(&store, Some("MS5"), T0).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let mut now = T0;
    assert_eq!(next_sent(&mut runner, &store, &mut now, 5).await, None);
    assert_eq!(job(&store, id).await.phase, TRANSFER_FAILED);
}

#[tokio::test]
async fn the_funnels_store_coins_kingdoms_and_skip_stock_from_a_login_packet_and_never_wait() {
    let store = world(1).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let gbd = r#"%xt%gbd%1%0%{"gcu":{"C1":95081601,"C2":18793},"kpi":{"UL":[{"KID":0,"U":1},{"KID":2,"U":1},{"KID":4,"U":0}]},"sce":[["MS1",3786],["MS5",194],["KT",9]],"gpi":{}}%"#;
    reply(&mut runner, &store, gbd, T0 + 5).await;
    assert_eq!(store.currency(ACCOUNT).await.unwrap(), Some((95_081_601, 18_793, T0 + 5)));
    let (_, kingdoms) = store.kingdom_state(ACCOUNT).await.unwrap().unwrap();
    assert_eq!(kingdoms.unlocked, BTreeMap::from([(0, true), (2, true), (4, false)]));
    let stock = store.skip_stock(ACCOUNT).await.unwrap();
    assert_eq!((stock["MS1"], stock["MS5"], stock.get("KT")), (3_786, 194, None), "only skip items are kept");
    // A refused or unrelated packet changes nothing.
    reply(&mut runner, &store, r#"%xt%gcu%1%999%{"C1":1,"C2":1}%"#, T0 + 9).await;
    assert_eq!(store.currency(ACCOUNT).await.unwrap().unwrap().0, 95_081_601);
}

#[tokio::test]
async fn a_kingdom_read_is_sent_when_the_window_asks_and_not_more_often_than_every_fifteen_seconds() {
    let store = world(2_147_483_437).await;
    let mut runner = TransferAutomation::new(ACCOUNT);
    let key = transfer_refresh_key(ACCOUNT);
    assert!(tick(&mut runner, &store, T0).await.is_none(), "nothing asked");
    store.set_app_state(&key, &json!(T0), T0).await.unwrap();
    let mut now = T0;
    let mut got = None;
    for _ in 0..4 {
        now += 1_000;
        if let Some(packet) = tick(&mut runner, &store, now).await {
            got = Some(packet);
            break;
        }
    }
    assert_eq!(got.as_deref(), Some("%xt%EmpireEx_21%kpi%1%{}%"));
    store.set_app_state(&key, &json!(now + 1), now).await.unwrap();
    let mut early = None;
    for step in 1..=10 {
        if let Some(packet) = tick(&mut runner, &store, now + step * 1_000).await {
            early = Some(packet);
        }
    }
    assert!(early.is_none(), "a second ask inside fifteen seconds waits");
}
