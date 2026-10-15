//! Kingdom grouping inside the automation loop: which task is tried first, when the bot
//! waits for the kingdom it is in, and where the settings come from. The ordering rules
//! themselves are tested in `empire_core::grouping`; these run the real `next_packet`.

use super::*;
use empire_core::account::{OwnedCastle, RbcTarget};
use empire_core::grouping::{KingdomGrouping, SETTINGS_KEY};
use empire_core::planning::{Coordinate, Destination, ventrilo_sands_bundle};

const ACCOUNT: &str = "ventrilo";
const FIRE: i64 = 3;
const GREEN: i64 = 0;

fn tower(kingdom_id: i64, x: i64, y: i64, count: i64, cooldown_remaining_s: i64) -> RbcTarget {
    RbcTarget {
        kingdom_id,
        x,
        y,
        level: Some(empire_core::dungeon::level_for(count, kingdom_id)),
        victory_count: Some(count),
        cooldown_remaining_s,
    }
}

/// One task in fire (towers level 50) and one in green (towers level 5), each with four
/// commanders and one ready tower. Returns the store and the tasks as the loop sees them.
async fn two_kingdoms(green_cooldown_s: i64) -> (Store, Vec<ActiveModeTask>) {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile(ACCOUNT, "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
        .await
        .unwrap();
    let castle = |kingdom_id, castle_id, x, y| OwnedCastle {
        kingdom_id,
        castle_id,
        area_type: 12,
        x,
        y,
        name: format!("Castle {kingdom_id}"),
    };
    store
        .replace_account_bootstrap(
            ACCOUNT,
            &[castle(FIRE, 100, 600, 600), castle(GREEN, 101, 643, 637)],
            hunt::USABLE_COMMANDER_LIDS,
            now_ms(),
        )
        .await
        .unwrap();
    let mut bundle = ventrilo_sands_bundle();
    let aim = |task: &mut empire_core::planning::BundledTask, kingdom_id, x, y, min, max| {
        task.source = Coordinate { kingdom_id, x, y };
        task.destination = Destination::RbcLevelRange { kingdom_id, minimum: min, maximum: max };
        task.commander_count = 4;
    };
    aim(&mut bundle.tasks[0], FIRE, 600, 600, 45, 60);
    aim(&mut bundle.tasks[1], GREEN, 643, 637, 1, 20);
    let mode_id = store.import_mode_bundle(&bundle, now_ms()).await.unwrap();
    store.subscribe_account_mode(ACCOUNT, mode_id, true, now_ms()).await.unwrap();
    store
        .upsert_rbc_targets(
            ACCOUNT,
            &[tower(FIRE, 698, 581, 7, 0), tower(GREEN, 654, 645, 5, green_cooldown_s)],
            now_ms(),
        )
        .await
        .unwrap();
    let tasks = store.active_mode_tasks(ACCOUNT).await.unwrap();
    assert_eq!(tasks.len(), 2);
    (store, tasks)
}

async fn client_in(store: &Store, kingdom_id: i64) {
    store
        .set_navigation(
            &empire_core::store::NavigationState {
                account_id: ACCOUNT.to_owned(),
                current_kingdom_id: Some(kingdom_id),
                current_castle_id: None,
                map_mode: true,
                recruit_page: false,
                last_castle_switch_at_ms: 0,
            },
            now_ms(),
        )
        .await
        .unwrap();
}

/// The kingdom of the task the loop has just committed to inspect (or is opening a map for).
fn chosen_kingdom(automation: &Automation) -> Option<i64> {
    match &automation.phase {
        AutomationPhase::ReadyAdi { task, .. } | AutomationPhase::AwaitMap { task, .. } => {
            Some(task.kingdom_id)
        }
        _ => None,
    }
}

fn index_of(tasks: &[ActiveModeTask], kingdom_id: i64) -> usize {
    tasks.iter().position(|task| task.kingdom_id == kingdom_id).unwrap()
}

async fn step(automation: &mut Automation, store: &Store) {
    automation.next_packet(store, ACCOUNT, US1_SERVER_HEADER).await.unwrap();
}

#[tokio::test]
async fn the_bot_keeps_attacking_in_the_kingdom_the_client_is_in() {
    let (store, tasks) = two_kingdoms(0).await;
    for (here, other) in [(GREEN, FIRE), (FIRE, GREEN)] {
        client_in(&store, here).await;
        let mut automation = Automation::new();
        // The plain rotation would start with the other kingdom's task.
        automation.cursor = index_of(&tasks, other);
        step(&mut automation, &store).await;
        assert_eq!(chosen_kingdom(&automation), Some(here), "client in {here}");
    }
}

#[tokio::test]
async fn with_grouping_off_the_rotation_decides_as_it_always_did() {
    let (store, tasks) = two_kingdoms(0).await;
    client_in(&store, GREEN).await;
    let mut automation = Automation::new();
    automation.grouping.enabled = false;
    automation.cursor = index_of(&tasks, FIRE);
    step(&mut automation, &store).await;
    assert_eq!(chosen_kingdom(&automation), Some(FIRE));
}

#[tokio::test]
async fn a_short_wait_in_this_kingdom_beats_a_switch_but_a_long_one_does_not() {
    // The green tower comes off cooldown in a minute; fire is ready now.
    let (store, tasks) = two_kingdoms(60).await;
    client_in(&store, GREEN).await;
    let mut automation = Automation::new();
    automation.cursor = index_of(&tasks, FIRE);
    step(&mut automation, &store).await;
    assert!(chosen_kingdom(&automation).is_none(), "should hold, not switch");
    assert!(automation.detail.contains("staying in kingdom 0"), "{}", automation.detail);
    let AutomationPhase::Idle { due_ms } = automation.phase else { panic!("expected to wait") };
    assert!(due_ms > now_ms(), "{due_ms}");

    // Patience of zero: switch at once.
    automation.grouping.patience_ms = 0;
    automation.phase = AutomationPhase::Idle { due_ms: 0 };
    step(&mut automation, &store).await;
    assert_eq!(chosen_kingdom(&automation), Some(FIRE));

    // A tower that is an hour away is not worth waiting for, whatever the patience.
    let (store, tasks) = two_kingdoms(3_600).await;
    client_in(&store, GREEN).await;
    let mut automation = Automation::new();
    automation.cursor = index_of(&tasks, GREEN);
    step(&mut automation, &store).await;
    assert_eq!(chosen_kingdom(&automation), Some(FIRE));
}

#[tokio::test]
async fn a_long_run_in_one_kingdom_gives_the_other_a_turn() {
    let (store, tasks) = two_kingdoms(0).await;
    client_in(&store, GREEN).await;
    let mut automation = Automation::new();
    automation.grouping = KingdomGrouping { max_streak: 3, ..KingdomGrouping::default() };
    automation.cursor = index_of(&tasks, GREEN);
    for _ in 0..3 {
        automation.streak.note_attack(GREEN);
    }
    step(&mut automation, &store).await;
    assert_eq!(chosen_kingdom(&automation), Some(FIRE), "green had its streak");

    // A fresh run (the previous attacks were elsewhere) starts counting from zero.
    let mut automation = Automation::new();
    automation.grouping = KingdomGrouping { max_streak: 3, ..KingdomGrouping::default() };
    automation.cursor = index_of(&tasks, FIRE);
    for _ in 0..3 {
        automation.streak.note_attack(FIRE);
    }
    step(&mut automation, &store).await;
    assert_eq!(chosen_kingdom(&automation), Some(GREEN), "the run was in fire, client is in green");
}

#[tokio::test]
async fn an_acknowledged_attack_extends_the_run_in_its_kingdom() {
    let (store, tasks) = two_kingdoms(0).await;
    let mut automation = Automation::new();
    let task = tasks[index_of(&tasks, GREEN)].clone();
    let target = ReservedTarget { kingdom_id: GREEN, x: 654, y: 645, level: Some(5) };
    for expected in 1..=2 {
        automation.phase = AutomationPhase::AwaitCra {
            task: task.clone(),
            target: target.clone(),
            lord_id: 0,
            deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
        };
        automation
            .observe(&store, ACCOUNT, "%xt%cra%1%0%{\"AAM\":{\"M\":{\"MID\":42,\"TT\":300}}}%")
            .await;
        assert_eq!(automation.streak.length_in(Some(GREEN)), expected);
    }
    assert_eq!(automation.streak.length_in(Some(FIRE)), 0);
}

#[tokio::test]
async fn the_settings_are_read_from_app_state_once_per_session() {
    let (store, _) = two_kingdoms(0).await;
    let mut automation = Automation::new();
    assert_eq!(automation.grouping, KingdomGrouping::default());
    store
        .set_app_state(
            SETTINGS_KEY,
            &json!({"enabled": true, "max_streak": 2, "patience_s": 10}),
            now_ms(),
        )
        .await
        .unwrap();
    automation.configure(&store).await;
    assert_eq!(
        automation.grouping,
        KingdomGrouping { enabled: true, max_streak: 2, patience_ms: 10_000 }
    );
}
