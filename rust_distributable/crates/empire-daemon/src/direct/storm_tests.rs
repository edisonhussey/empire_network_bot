//! Storm Islands in the attack bot: scan on demand, pick a fresh fort, tolerate forts that vanish.
use super::*;

const STORM_CASTLE: (i64, i64) = (557, 531);

async fn storm_store() -> Store {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store.upsert_account_profile("ventrilo", "Ventrilo", "wss://x.invalid", "EmpireEx_21", 1).await.unwrap();
    store
        .replace_account_bootstrap(
            "ventrilo",
            &[empire_core::account::OwnedCastle { kingdom_id: 4, castle_id: 4960, area_type: 12, x: STORM_CASTLE.0, y: STORM_CASTLE.1, name: "Castle Ventrilo".into() }],
            hunt::USABLE_COMMANDER_LIDS,
            now_ms(),
        )
        .await
        .unwrap();
    let mut bundle = empire_core::planning::ventrilo_sands_bundle();
    bundle.name = "Storm".into();
    bundle.tasks.truncate(1);
    let task = &mut bundle.tasks[0];
    task.name = "Storm 60-80".into();
    task.source = empire_core::planning::Coordinate { kingdom_id: 4, x: STORM_CASTLE.0, y: STORM_CASTLE.1 };
    task.destination = empire_core::planning::Destination::StormLevelRange { kingdom_id: 4, minimum: 60, maximum: 80 };
    let mode_id = store.import_mode_bundle(&bundle, now_ms()).await.unwrap();
    store.subscribe_account_mode("ventrilo", mode_id, true, now_ms()).await.unwrap();
    store
}

fn gaa_reply(entries: &str) -> String {
    format!(r#"%xt%gaa%1%0%{{"KID":4,"AI":[{entries}]}}%"#)
}

#[tokio::test]
async fn with_no_fresh_fort_the_bot_reads_the_map_around_its_storm_castle_then_attacks_what_it_found() {
    let store = storm_store().await;
    let tasks = store.active_mode_tasks("ventrilo").await.unwrap();
    assert_eq!(tasks[0].target_kind, "storm");
    let mut automation = Automation::new();
    let scan = automation.next_packet(&store, "ventrilo", "EmpireEx_21").await.unwrap().expect("a scan");
    assert_eq!(scan, r#"%xt%EmpireEx_21%gaa%1%{"AX1":546,"AX2":558,"AY1":520,"AY2":532,"KID":4}%"#);
    assert!(matches!(automation.phase, AutomationPhase::AwaitStormScan { .. }));
    // The captured level-60 fort, one on cooldown and one too low.
    automation
        .observe(&store, "ventrilo", &gaa_reply("[25,551,542,4,-1,12,0,8,0],[25,546,527,4,-1,13,34300,0,34300],[25,549,532,4,-1,11,0,1,0]"))
        .await;
    assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
    let wait = automation.storm_scan.due_ms - now_ms();
    assert!((5_000..12_600).contains(&wait), "the next scan waits 5.5-12.5 s: {wait}");
    automation.phase = AutomationPhase::Idle { due_ms: 0 };
    let open_map = automation.next_packet(&store, "ventrilo", "EmpireEx_21").await.unwrap().expect("map for the pick");
    assert!(open_map.contains("%gaa%") && open_map.contains(r#""KID":4"#), "{open_map}");
    let AutomationPhase::AwaitMap { target, .. } = &automation.phase else { panic!("{:?}", automation.phase) };
    assert_eq!((target.x, target.y, target.level), (551, 542, Some(60)));
}

#[tokio::test]
async fn a_fort_taken_before_the_inspection_answers_is_skipped_without_an_incident() {
    let store = storm_store().await;
    let task = store.active_mode_tasks("ventrilo").await.unwrap().remove(0);
    store
        .record_storm_forts("ventrilo", None, &[empire_core::storm::StormFort { x: 551, y: 542, level: Some(60), free: true }], now_ms())
        .await
        .unwrap();
    let target = store.reserve_storm_target("ventrilo", Some(60), Some(80), STORM_CASTLE, now_ms(), now_ms() + 60_000).await.unwrap().unwrap();
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitAdi { task, target, deadline_ms: now_ms() + 40_000 };
    // The reply's own map entry now shows a cooldown: someone else took it.
    automation
        .observe(&store, "ventrilo", r#"%xt%adi%1%0%{"KID":4,"gaa":{"AI":[25,551,542,4,-1,12,1800,8,1800]},"gui":{"I":[[35,500]]}}%"#)
        .await;
    assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
    assert_eq!(automation.health.recent().count(), 0, "a vanished storm fort is routine");
    assert!(store.reserve_storm_target("ventrilo", None, None, STORM_CASTLE, now_ms(), now_ms() + 60_000).await.unwrap().is_none());
}

#[tokio::test]
async fn a_refused_storm_attack_is_tolerated_but_a_short_army_still_stops_the_bot() {
    let store = storm_store().await;
    let task = store.active_mode_tasks("ventrilo").await.unwrap().remove(0);
    let target = ReservedTarget { kingdom_id: 4, x: 551, y: 542, level: Some(60) };
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitCra { task: task.clone(), target: target.clone(), lord_id: 3, deadline_ms: now_ms() + 40_000 };
    automation.observe(&store, "ventrilo", "%xt%cra%1%7%{}%").await;
    assert!(automation.stop_reason.is_none(), "an unexplained storm refusal is the fort vanishing");
    assert!(!store.active_mode_tasks("ventrilo").await.unwrap().is_empty());
    automation.phase = AutomationPhase::AwaitCra { task, target, lord_id: 3, deadline_ms: now_ms() + 40_000 };
    automation.observe(&store, "ventrilo", "%xt%cra%1%313%{}%").await;
    assert!(automation.stop_reason.is_some(), "the castle could not supply the army: stop as for any attack");
}
