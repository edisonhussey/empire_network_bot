//! Store tests.
//!
//! Kept out of the source modules so the production files stay readable. A
//! child module of `store`, so it can reach the private pool and the migration
//! definitions directly.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;

use super::schema;
use super::*;
use crate::account::{CastleTravelOptions, FortressTarget, OwnedCastle, RbcTarget};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct TempDb {
    path: PathBuf,
    url: String,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!(
            "openauto-{tag}-{}-{nanos}.sqlite3",
            std::process::id()
        ));
        Self::clean(&path);
        let url = format!("sqlite://{}?mode=rwc", path.display());
        Self { path, url }
    }

    fn clean(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let mut candidate = path.as_os_str().to_owned();
            candidate.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(candidate));
        }
    }

    async fn open(&self) -> Store {
        Store::open(&self.url).await.expect("open database")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        Self::clean(&self.path);
    }
}

const NOW: i64 = 1_800_000_000_000;

async fn seed_account(store: &Store, account_id: &str) {
    store
        .upsert_account_profile(account_id, account_id, "wss://example/", "EmpireEx_21", NOW)
        .await
        .unwrap();
    store
        .replace_account_bootstrap(
            account_id,
            &[OwnedCastle {
                kingdom_id: 1,
                castle_id: 100,
                area_type: 12,
                x: 572,
                y: 598,
                name: "main".to_owned(),
            }],
            &[11, 22],
            NOW,
        )
        .await
        .unwrap();
    store
        .upsert_rbc_targets(
            account_id,
            &[RbcTarget {
                kingdom_id: 1,
                x: 600,
                y: 610,
                level: Some(61),
            }],
            NOW,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn every_travel_mode_is_resolved_from_the_exact_source_castle() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile("Pingpoko", "Pingpoko", "wss://example/", "EmpireEx_49", NOW)
        .await
        .unwrap();
    store
        .replace_account_bootstrap(
            "PINGPOKO",
            &[
                OwnedCastle {
                    kingdom_id: 0,
                    castle_id: 70_499,
                    area_type: 1,
                    x: 10,
                    y: 20,
                    name: "Green".to_owned(),
                },
                OwnedCastle {
                    kingdom_id: 1,
                    castle_id: 341_842,
                    area_type: 12,
                    x: 722,
                    y: 533,
                    name: "Sands".to_owned(),
                },
            ],
            &[1],
            NOW,
        )
        .await
        .unwrap();
    store
        .upsert_castle_travel_options(
            "pingpoko",
            &[
                CastleTravelOptions {
                    castle_id: 70_499,
                    kingdom_id: 0,
                    unlocked_hbw: vec![1007, 1008, 1009],
                },
                CastleTravelOptions {
                    castle_id: 341_842,
                    kingdom_id: 1,
                    unlocked_hbw: vec![1004, 1005, 1006],
                },
            ],
            NOW,
        )
        .await
        .unwrap();

    assert_eq!(
        store
            .travel_for_source("PingPoko", 1, 722, 533, crate::planning::TravelMode::Coin)
            .await
            .unwrap(),
        Some(crate::account::AttackTravel { hbw: 1004, ptt: 0 })
    );
    assert_eq!(
        store
            .travel_for_source("pingpoko", 0, 10, 20, crate::planning::TravelMode::Ruby2,)
            .await
            .unwrap(),
        Some(crate::account::AttackTravel { hbw: 1009, ptt: 0 })
    );
    assert_eq!(
        store
            .travel_for_source(
                "pingpoko",
                1,
                722,
                533,
                crate::planning::TravelMode::Feather,
            )
            .await
            .unwrap(),
        Some(crate::account::AttackTravel { hbw: -1, ptt: 1 })
    );
    assert_eq!(
        store
            .travel_for_source("pingpoko", 1, 723, 533, crate::planning::TravelMode::Coin,)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn map_scan_coverage_is_durable_and_refreshes_on_a_staggered_interval() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "scan-account").await;
    store
        .record_scan_window("scan-account", 1, (572, 598, 584, 610), NOW)
        .await
        .unwrap();
    assert!(
        store
            .scan_window_is_fresh("scan-account", 1, 572, 598, NOW + 60_000)
            .await
            .unwrap()
    );
    let due = scan_refresh_after_ms(1, 572, 598);
    assert!((SCAN_REFRESH_BASE_MS - 1_800_000..=SCAN_REFRESH_BASE_MS + 1_800_000).contains(&due));
    assert!(
        !store
            .scan_window_is_fresh("scan-account", 1, 572, 598, NOW + due + 1)
            .await
            .unwrap()
    );
    let account = store.account_summaries().await.unwrap().remove(0);
    assert_eq!(account.kingdom_health[0].scan_window_count, 1);
    assert_eq!(account.kingdom_health[0].target_count, 1);
    assert_eq!(
        (account.kingdom_health[0].x, account.kingdom_health[0].y),
        (572, 598)
    );
}

#[tokio::test]
async fn account_summaries_merge_username_case_without_double_counting() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "Ventrilo").await;
    seed_account(&store, "ventrilo").await;

    let accounts = store.account_summaries().await.unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_id, "ventrilo");
    assert_eq!(accounts[0].player_name, "Ventrilo");
    assert_eq!(accounts[0].castle_count, 1);
    assert_eq!(accounts[0].commander_count, 2);
    assert_eq!(accounts[0].rbc_count, 1);
    assert_eq!(accounts[0].kingdom_health.len(), 1);
}

// ---------------------------------------------------------------------------
// message retention
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retention_is_bounded_to_recent_messages() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    for index in 0..205 {
        store
            .record_message(
                index,
                Direction::Injected,
                Some("gbl"),
                &json!({"i": index}),
            )
            .await
            .unwrap();
    }
    let messages = store.recent_messages(500).await.unwrap();
    assert_eq!(messages.len(), RECENT_MESSAGE_LIMIT as usize);
    assert_eq!(messages.first().unwrap().payload, json!({"i": 204}));
    assert_eq!(messages.last().unwrap().payload, json!({"i": 5}));
}

// ---------------------------------------------------------------------------
// migrations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fresh_database_records_the_current_schema_version() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    assert_eq!(
        schema::version(&store.pool).await.unwrap(),
        schema::SCHEMA_VERSION
    );
}

#[tokio::test]
async fn migrations_are_idempotent_across_reopen() {
    let db = TempDb::new("idempotent");
    let first = db.open().await;
    assert_eq!(
        schema::version(&first.pool).await.unwrap(),
        schema::SCHEMA_VERSION
    );
    drop(first);

    // Re-opening must not re-run, and must not fail on the ALTER TABLEs.
    let second = db.open().await;
    assert_eq!(
        schema::version(&second.pool).await.unwrap(),
        schema::SCHEMA_VERSION
    );
}

#[tokio::test]
async fn a_pre_migration_database_upgrades_cleanly() {
    let db = TempDb::new("legacy");
    {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&db.url)
            .await
            .unwrap();
        for statement in schema::V1 {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        // A release that predates versioning leaves no schema_version row.
        assert_eq!(schema::version(&pool).await.unwrap(), 0);
        pool.close().await;
    }

    let store = db.open().await;
    assert_eq!(
        schema::version(&store.pool).await.unwrap(),
        schema::SCHEMA_VERSION,
        "an existing database must be upgraded, not rejected"
    );
    // The v2 columns and tables must actually be usable.
    store
        .upsert_attack_profile(
            &AttackProfile {
                profile_id: "p".into(),
                name: "P".into(),
                payload: json!([]),
                notes: String::new(),
            },
            NOW,
        )
        .await
        .unwrap();
    assert_eq!(store.attack_profiles().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// configuration round-trips
// ---------------------------------------------------------------------------

#[tokio::test]
async fn attack_profile_round_trips_its_payload() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let payload = json!([{
        "L": {"T": [[614, 5], [-1, 0]], "U": [[715, 30], [-1, 0]]},
        "M": {"T": [[-1, 0], [-1, 0], [-1, 0]], "U": [[-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0]]},
        "R": {"T": [[614, 5], [-1, 0]], "U": [[715, 30], [-1, 0]]}
    }]);
    store
        .upsert_attack_profile(
            &AttackProfile {
                profile_id: "sand_dh".into(),
                name: "Deathly Horror".into(),
                payload: payload.clone(),
                notes: "4 waves".into(),
            },
            NOW,
        )
        .await
        .unwrap();

    let profiles = store.attack_profiles().await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].payload, payload);
    assert_eq!(profiles[0].name, "Deathly Horror");
}

#[tokio::test]
async fn task_and_subscription_round_trip() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let task = TaskRecord {
        task_id: "t1".into(),
        name: "Level 61 crossbow".into(),
        kind: "attack".into(),
        kingdom_id: 1,
        profile_id: Some("p1".into()),
        target_level_min: Some(61),
        target_level_max: Some(61),
        commander_count: 17,
        max_active: None,
        priority: 20,
        enabled: true,
        tags: vec!["rbc".into(), "sand".into()],
        notes: String::new(),
    };
    store.upsert_task(&task, NOW).await.unwrap();
    store
        .replace_subscriptions(
            "t1",
            &[
                SubscriptionRecord {
                    task_id: "t1".into(),
                    target_kind: "rbc".into(),
                    filter: json!({"kingdom_id": 1, "levels": [61]}),
                },
                SubscriptionRecord {
                    task_id: "t1".into(),
                    target_kind: "castle".into(),
                    filter: json!({"castle_ids": [100]}),
                },
            ],
        )
        .await
        .unwrap();

    let tasks = store.tasks().await.unwrap();
    assert_eq!(tasks, vec![task.clone()]);

    let subscriptions = store.subscriptions().await.unwrap();
    assert_eq!(subscriptions.len(), 2);
    assert_eq!(subscriptions[0].filter["levels"], json!([61]));
    assert_eq!(subscriptions[1].target_kind, "castle");

    // Replacing is wholesale, not additive.
    store.replace_subscriptions("t1", &[]).await.unwrap();
    assert!(store.subscriptions().await.unwrap().is_empty());
}

#[tokio::test]
async fn deleting_a_task_removes_its_subscriptions() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_task(
            &TaskRecord {
                task_id: "t1".into(),
                name: "T".into(),
                kind: "attack".into(),
                kingdom_id: 1,
                profile_id: None,
                target_level_min: None,
                target_level_max: None,
                commander_count: 1,
                max_active: None,
                priority: 100,
                enabled: true,
                tags: Vec::new(),
                notes: String::new(),
            },
            NOW,
        )
        .await
        .unwrap();
    store
        .replace_subscriptions(
            "t1",
            &[SubscriptionRecord {
                task_id: "t1".into(),
                target_kind: "rbc".into(),
                filter: json!({}),
            }],
        )
        .await
        .unwrap();
    store.delete_task("t1").await.unwrap();
    assert!(store.tasks().await.unwrap().is_empty());
    assert!(store.subscriptions().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// resume state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_open_march_survives_a_restart() {
    let db = TempDb::new("resume");
    {
        let store = db.open().await;
        seed_account(&store, "ventrilo").await;
        store
            .record_march(&MarchRecord {
                account_id: "ventrilo".into(),
                march_id: 9001,
                kingdom_id: 1,
                x: 600,
                y: 610,
                task_id: Some("t1".into()),
                profile_id: None,
                level: Some(61),
                lord_id: Some(11),
                commander_number: Some(1),
                troop_count: Some(50),
                duration_s: None,
                coin_loot: None,
                ruby_loot: None,
                status: MARCH_SENT.into(),
                result_flag: None,
                error_message: None,
                sent_at_ms: NOW,
                landed_at_ms: None,
                result_at_ms: None,
            })
            .await
            .unwrap();
    }

    let reopened = db.open().await;
    let open = reopened.open_marches().await.unwrap();
    assert_eq!(
        open.len(),
        1,
        "the resumed run must see the in-flight march"
    );
    assert_eq!(open[0].march_id, 9001);
    assert_eq!(open[0].lord_id, Some(11));

    reopened
        .finish_march(
            "ventrilo",
            9001,
            MARCH_RETURNING,
            Some(0),
            Some(120),
            Some(3),
            NOW + 60_000,
        )
        .await
        .unwrap();
    let after = reopened.open_marches().await.unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].status, MARCH_RETURNING);
    assert_eq!(after[0].coin_loot, Some(120));
}

#[tokio::test]
async fn commander_lids_follow_roster_order() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    assert_eq!(
        store.commander_lids("ventrilo").await.unwrap(),
        vec![11, 22]
    );
}

#[tokio::test]
async fn commander_availability_round_trips() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .set_commander_state(
            &CommanderState {
                account_id: "ventrilo".into(),
                lord_id: 11,
                status: COMMANDER_OUTBOUND.into(),
                available_after_ms: NOW + 600_000,
                march_id: Some(9001),
                target_key: Some("1:600:610".into()),
            },
            NOW,
        )
        .await
        .unwrap();
    let states = store.commander_states("ventrilo").await.unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].status, COMMANDER_OUTBOUND);
    assert_eq!(states[0].available_after_ms, NOW + 600_000);
}

#[tokio::test]
async fn navigation_round_trips_map_and_castle_mode() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    assert!(store.navigation("ventrilo").await.unwrap().is_none());

    let mut state = NavigationState::new("ventrilo");
    state.current_kingdom_id = Some(1);
    state.current_castle_id = Some(100);
    state.map_mode = true;
    store.set_navigation(&state, NOW).await.unwrap();

    let read = store.navigation("ventrilo").await.unwrap().unwrap();
    assert!(read.map_mode);
    assert!(!read.recruit_page);
    assert_eq!(read.current_castle_id, Some(100));
}

#[tokio::test]
async fn a_return_is_attributed_by_target_and_commander_not_march_id() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let march = |march_id: i64, lord_id: i64| MarchRecord {
        account_id: "ventrilo".into(),
        march_id,
        kingdom_id: 1,
        x: 600,
        y: 610,
        task_id: None,
        profile_id: None,
        level: Some(61),
        lord_id: Some(lord_id),
        commander_number: None,
        troop_count: Some(50),
        duration_s: None,
        coin_loot: None,
        ruby_loot: None,
        status: MARCH_SENT.into(),
        result_flag: None,
        error_message: None,
        sent_at_ms: NOW,
        landed_at_ms: None,
        result_at_ms: None,
    };
    // Two marches to the same square, from different commanders.
    store.record_march(&march(1001, 0)).await.unwrap();
    store.record_march(&march(1002, 2)).await.unwrap();

    // The return arrives under an id we never acknowledged, as it does in
    // practice; only the position and commander can identify the march.
    let updated = store
        .finish_march_by_target(
            "ventrilo",
            1,
            600,
            610,
            2,
            Some(61),
            Some(39_612),
            Some(14),
            Some(0),
            NOW + 1_000,
        )
        .await
        .unwrap();
    assert_eq!(updated, 1, "exactly one in-flight march should be claimed");

    let marches = store.recent_marches("ventrilo", 10).await.unwrap();
    let by_id = |id: i64| {
        marches
            .iter()
            .find(|record| record.march_id == id)
            .unwrap_or_else(|| panic!("march {id} is missing"))
    };
    assert_eq!(by_id(1002).status, MARCH_RETURNING);
    assert_eq!(by_id(1002).coin_loot, Some(39_612));
    assert_eq!(by_id(1002).ruby_loot, Some(14));
    assert_eq!(by_id(1002).result_flag, Some(0));

    // The other commander's march to the same square must be left alone.
    assert_eq!(by_id(1001).status, MARCH_SENT);
    assert_eq!(by_id(1001).coin_loot, None);

    // Nothing in flight for that commander any more.
    assert_eq!(
        store
            .finish_march_by_target("ventrilo", 1, 600, 610, 2, None, None, None, None, NOW)
            .await
            .unwrap(),
        0
    );
    // An unknown position claims nothing.
    assert_eq!(
        store
            .finish_march_by_target("ventrilo", 1, 1, 1, 2, None, None, None, None, NOW)
            .await
            .unwrap(),
        0
    );
}

// ---------------------------------------------------------------------------
// storage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn storage_report_counts_rows_and_reports_the_schema_version() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let report = store.storage_report().await.unwrap();
    assert_eq!(report.schema_version, schema::SCHEMA_VERSION);
    let rows = |table: &str| {
        report
            .tables
            .iter()
            .find(|entry| entry.table == table)
            .map(|entry| entry.rows)
            .unwrap_or_default()
    };
    assert_eq!(rows("account_profile"), 1);
    assert_eq!(rows("owned_castle"), 1);
    assert_eq!(rows("account_commander"), 2);
    assert_eq!(rows("rbc_target"), 1);
    assert!(report.total_rows() >= 5);
}

#[tokio::test]
async fn fortress_observation_persists_the_server_cooldown_as_an_absolute_time() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .upsert_fortress_targets(
            "Ventrilo",
            &[FortressTarget {
                kingdom_id: 1,
                x: 594,
                y: 594,
                level: 45,
                cooldown_remaining_s: 38_421,
                occupier_player_id: 17_185_267,
            }],
            NOW,
        )
        .await
        .unwrap();
    let row = sqlx::query(
        "SELECT account_id, level, available_at_ms, occupier_player_id
         FROM fortress_target WHERE kingdom_id = 1 AND x = 594 AND y = 594",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("account_id"), "ventrilo");
    assert_eq!(row.get::<i64, _>("level"), 45);
    assert_eq!(row.get::<i64, _>("available_at_ms"), NOW + 38_421_000);
    assert_eq!(row.get::<i64, _>("occupier_player_id"), 17_185_267);
}

#[tokio::test]
async fn fortress_reservation_has_a_strict_one_minute_dispatch_window() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let ready = FortressTarget {
        kingdom_id: 3,
        x: 692,
        y: 575,
        level: 55,
        cooldown_remaining_s: 0,
        occupier_player_id: 16_859_207,
    };
    store
        .upsert_fortress_targets("ventrilo", std::slice::from_ref(&ready), NOW)
        .await
        .unwrap();

    let first = store
        .reserve_fortress_target(
            "ventrilo",
            3,
            None,
            None,
            (700, 580),
            NOW + 60_000,
            NOW + 61_000,
        )
        .await
        .unwrap();
    assert_eq!(
        first.as_ref().map(|target| (target.x, target.y)),
        Some((692, 575))
    );
    store
        .release_fortress_target("ventrilo", first.as_ref().unwrap())
        .await
        .unwrap();
    assert!(
        store
            .reserve_fortress_target(
                "ventrilo",
                3,
                None,
                None,
                (700, 580),
                NOW + 60_001,
                NOW + 61_001,
            )
            .await
            .unwrap()
            .is_none()
    );

    // Re-reading an already-ready fortress must not restart its minute.
    store
        .upsert_fortress_targets("ventrilo", &[ready], NOW + 120_000)
        .await
        .unwrap();
    assert!(
        store
            .reserve_fortress_target(
                "ventrilo",
                3,
                None,
                None,
                (700, 580),
                NOW + 120_000,
                NOW + 121_000,
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn fortress_discovery_frontier_is_durable_and_does_not_repeat_completed_probes() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .upsert_rbc_targets(
            "ventrilo",
            &[RbcTarget {
                kingdom_id: 1,
                x: 600,
                y: 610,
                level: Some(61),
            }],
            NOW,
        )
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fortress_scan_frontier WHERE account_id = 'ventrilo'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(before, 0, "ordinary RBCs must never seed fortress scanning");
    store
        .upsert_fortress_targets(
            "ventrilo",
            &[FortressTarget {
                kingdom_id: 1,
                x: 594,
                y: 633,
                level: 45,
                cooldown_remaining_s: 1,
                occupier_player_id: 1,
            }],
            NOW,
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fortress_scan_frontier WHERE account_id = 'ventrilo'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(count, 8);

    let probe = store
        .reserve_fortress_probe("ventrilo", 1, NOW)
        .await
        .unwrap()
        .unwrap();
    store
        .complete_fortress_probe("ventrilo", &probe, NOW + 1)
        .await
        .unwrap();
    let completed: i64 = sqlx::query_scalar(
        "SELECT completed_at_ms FROM fortress_scan_frontier
         WHERE account_id = 'ventrilo' AND kingdom_id = ? AND center_x = ? AND center_y = ?",
    )
    .bind(probe.kingdom_id)
    .bind(probe.center_x)
    .bind(probe.center_y)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(completed, NOW + 1);
}

#[tokio::test]
async fn successful_fortress_loot_sets_the_internal_hundred_hour_cooldown() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .upsert_fortress_targets(
            "ventrilo",
            &[FortressTarget {
                kingdom_id: 3,
                x: 731,
                y: 575,
                level: 55,
                cooldown_remaining_s: 0,
                occupier_player_id: 1,
            }],
            NOW,
        )
        .await
        .unwrap();
    store
        .record_fortress_result("ventrilo", 3, 731, 575, Some(100), NOW + 5_000)
        .await
        .unwrap();
    let row = sqlx::query(
        "SELECT cooldown_remaining_s, available_at_ms, refresh_due_ms
         FROM fortress_target WHERE account_id = 'ventrilo' AND kingdom_id = 3
           AND x = 731 AND y = 575",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("cooldown_remaining_s"), 360_000);
    assert_eq!(
        row.get::<i64, _>("available_at_ms"),
        NOW + 5_000 + 100 * 60 * 60 * 1_000
    );
    assert_eq!(row.get::<i64, _>("refresh_due_ms"), 0);
}

#[tokio::test]
async fn prune_removes_only_history_older_than_the_cutoff() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    for (march_id, sent_at) in [(1_i64, NOW - 10 * 86_400_000), (2, NOW)] {
        store
            .record_march(&MarchRecord {
                account_id: "ventrilo".into(),
                march_id,
                kingdom_id: 1,
                x: 1,
                y: 1,
                task_id: None,
                profile_id: None,
                level: None,
                lord_id: None,
                commander_number: None,
                troop_count: None,
                duration_s: None,
                coin_loot: None,
                ruby_loot: None,
                status: "returned".into(),
                result_flag: None,
                error_message: None,
                sent_at_ms: sent_at,
                landed_at_ms: None,
                result_at_ms: None,
            })
            .await
            .unwrap();
    }

    let outcome = store.prune(7, NOW).await.unwrap();
    assert_eq!(outcome.ledger_rows, 1);
    assert!(outcome.vacuumed);

    let remaining = store.recent_marches("ventrilo", 50).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].march_id, 2);

    // Learned data is never pruned: a resumed run depends on it.
    assert_eq!(store.commander_lids("ventrilo").await.unwrap().len(), 2);
    assert_eq!(store.recent_marches("ventrilo", 50).await.unwrap().len(), 1);
    assert_eq!(store.account_summaries().await.unwrap().len(), 1);
}

#[tokio::test]
async fn reset_account_clears_one_account_and_leaves_the_other() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    seed_account(&store, "pingpoko").await;
    assert_eq!(store.account_summaries().await.unwrap().len(), 2);

    store.reset_account("ventrilo").await.unwrap();

    let summaries = store.account_summaries().await.unwrap();
    assert_eq!(
        summaries.len(),
        1,
        "the reset account must no longer look initialized"
    );
    assert_eq!(summaries[0].account_id, "pingpoko");
    assert_eq!(summaries[0].castle_count, 1);

    // The account row itself survives, so it can just be signed in again.
    assert!(store.commander_lids("ventrilo").await.unwrap().is_empty());
    assert!(store.navigation("ventrilo").await.unwrap().is_none());
}

#[tokio::test]
async fn wipe_all_returns_the_application_to_first_run() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .save_licence(
            &StoredLicence {
                token: "OA1.x.y".into(),
                license_id: "local-ventrilo".into(),
                subject: "Ventrilo".into(),
                revision: 1,
                expires_at: NOW + 1000,
                highest_seen_at: NOW,
            },
            NOW,
        )
        .await
        .unwrap();
    assert!(store.licence().await.unwrap().is_some());

    store.wipe_all().await.unwrap();

    let report = store.storage_report().await.unwrap();
    assert_eq!(report.total_rows(), 0, "every data table must be empty");
    assert!(store.licence().await.unwrap().is_none());
    assert!(store.account_summaries().await.unwrap().is_empty());
    // The schema is kept so migrations do not run again.
    assert_eq!(report.schema_version, schema::SCHEMA_VERSION);
}

// ---------------------------------------------------------------------------
// run summary
// ---------------------------------------------------------------------------

/// A march as the hunter writes one, so the summary tests stay readable.
fn march(task: Option<&str>, march_id: i64, lord: i64) -> MarchRecord {
    MarchRecord {
        account_id: "ventrilo".into(),
        march_id,
        kingdom_id: 1,
        x: 600,
        y: 610,
        task_id: task.map(str::to_owned),
        profile_id: task.map(str::to_owned),
        level: Some(61),
        lord_id: Some(lord),
        commander_number: None,
        troop_count: Some(50),
        duration_s: Some(120),
        coin_loot: None,
        ruby_loot: None,
        status: MARCH_SENT.into(),
        result_flag: None,
        error_message: None,
        sent_at_ms: NOW,
        landed_at_ms: None,
        result_at_ms: None,
    }
}

#[tokio::test]
async fn a_declared_task_appears_before_it_has_any_marches() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;

    store
        .set_app_state(
            "hunt.plan",
            &json!([
                { "task_id": "sand_rbc_level_61_crossbow", "kingdom_id": 1, "level_min": 61, "level_max": 61, "commanders": [0, 2, 3] },
                { "task_id": "sand_kunai", "kingdom_id": 1, "level_min": 35, "level_max": 60, "commanders": [25, 26] },
            ]),
            NOW,
        )
        .await
        .unwrap();
    store
        .set_app_state("hunt.label", &json!("sand-crossbow-kunai"), NOW)
        .await
        .unwrap();
    store
        .record_march(&march(Some("sand_kunai"), 1, 25))
        .await
        .unwrap();
    store.record_march(&march(None, 2, 0)).await.unwrap();

    let summary = store.hunt_summary(5).await.unwrap();
    assert_eq!(summary.label.as_deref(), Some("sand-crossbow-kunai"));
    assert_eq!(summary.marches, 2);

    let names: Vec<&str> = summary
        .tasks
        .iter()
        .map(|task| task.task_id.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["sand_rbc_level_61_crossbow", "sand_kunai", "(earlier runs)"],
        "declared tasks come first, including one with no marches"
    );

    let crossbow = &summary.tasks[0];
    assert!(crossbow.planned);
    assert_eq!(crossbow.marches, 0);
    assert_eq!(crossbow.level_min, Some(61));
    assert_eq!(crossbow.commanders, vec![0_i64, 2, 3]);

    let kunai = &summary.tasks[1];
    assert_eq!(kunai.marches, 1);
    assert_eq!(kunai.returned, 0);
    assert_eq!(kunai.level_max, Some(60));

    let history = &summary.tasks[2];
    assert!(
        !history.planned,
        "unlabelled rows are history, not a planned task"
    );
    assert_eq!(history.marches, 1);
}

#[tokio::test]
async fn a_returned_march_is_counted_against_its_task() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .record_march(&march(Some("sand_kunai"), 77, 25))
        .await
        .unwrap();

    let attributed = store
        .finish_march_by_target(
            "ventrilo",
            1,
            600,
            610,
            25,
            Some(240),
            Some(39612),
            Some(14),
            Some(0),
            NOW + 240_000,
        )
        .await
        .unwrap();
    assert_eq!(attributed, 1);

    let summary = store.hunt_summary(5).await.unwrap();
    assert_eq!(summary.returned, 1);
    assert_eq!(summary.in_flight, 0);
    assert_eq!(summary.coins, 39612);
    assert_eq!(summary.rubies, 14);
    assert_eq!(summary.tasks[0].task_id, "sand_kunai");
    assert_eq!(summary.tasks[0].returned, 1);
    assert_eq!(summary.recent[0].status, MARCH_RETURNING);
}

#[tokio::test]
async fn dashboard_rates_series_and_scan_activity_come_from_the_database() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let real_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut recent = march(Some("sand_kunai"), 88, 25);
    recent.sent_at_ms = real_now - 30_000;
    store.record_march(&recent).await.unwrap();
    store
        .finish_march(
            "ventrilo",
            88,
            MARCH_RETURNING,
            Some(0),
            Some(500),
            Some(7),
            real_now,
        )
        .await
        .unwrap();
    store
        .record_scan_window("ventrilo", 1, (500, 500, 512, 512), real_now)
        .await
        .unwrap();

    let dashboard = store.dashboard_summary().await.unwrap();
    assert_eq!(dashboard.attacks_last_hour, 1);
    assert_eq!(dashboard.returns_last_hour, 1);
    assert_eq!(dashboard.rubies_last_hour, 7);
    assert_eq!(dashboard.coins_last_hour, 500);
    assert_eq!(dashboard.ruby_series.last().unwrap().value, 7);
    assert_eq!(dashboard.scan_activity[0].windows, 1);
}

#[tokio::test]
async fn dashboard_and_hunt_summaries_are_scoped_to_the_selected_account() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    seed_account(&store, "pingpoko").await;
    let real_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut ventrilo = march(Some("sand_kunai"), 88, 25);
    ventrilo.sent_at_ms = real_now - 30_000;
    store.record_march(&ventrilo).await.unwrap();
    store
        .finish_march(
            "ventrilo",
            88,
            MARCH_RETURNING,
            Some(0),
            Some(500),
            Some(7),
            real_now,
        )
        .await
        .unwrap();
    let mut pingpoko = march(Some("ice_kunai"), 99, 26);
    pingpoko.account_id = "pingpoko".to_owned();
    pingpoko.sent_at_ms = real_now - 20_000;
    store.record_march(&pingpoko).await.unwrap();
    store
        .finish_march(
            "pingpoko",
            99,
            MARCH_RETURNING,
            Some(0),
            Some(900),
            Some(13),
            real_now,
        )
        .await
        .unwrap();

    let dashboard = store.dashboard_summary_for(Some("Ventrilo")).await.unwrap();
    let hunt = store.hunt_summary_for(Some("Ventrilo"), 5).await.unwrap();
    assert_eq!(dashboard.rubies_last_hour, 7);
    assert_eq!(dashboard.ruby_series.last().unwrap().value, 7);
    assert_eq!(hunt.marches, 1);
    assert_eq!(hunt.coins, 500);
    assert!(
        hunt.recent
            .iter()
            .all(|march| march.account_id == "ventrilo")
    );
}

#[tokio::test]
async fn a_march_that_never_comes_back_stops_counting_as_in_flight() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;

    // Staleness is measured against the real clock, not the fixed `NOW`, so both
    // rows are placed relative to it: one just sent, one long past the cutoff.
    let real_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut recent = march(Some("sand_kunai"), 88, 25);
    recent.sent_at_ms = real_now - 60_000;
    store.record_march(&recent).await.unwrap();

    let mut abandoned = march(Some("sand_kunai"), 89, 26);
    abandoned.sent_at_ms = real_now - super::ledger::STALE_MARCH_MILLIS - 1;
    store.record_march(&abandoned).await.unwrap();

    // With a run checking in, only the recent march is in the air.
    store
        .set_app_state(HUNT_HEARTBEAT_KEY, &json!(real_now), real_now)
        .await
        .unwrap();
    let live = store.hunt_summary(5).await.unwrap();
    assert!(live.active);
    assert_eq!(live.marches, 2);
    assert_eq!(
        live.in_flight, 1,
        "an abandoned march must not be reported as still flying"
    );

    // With no run alive, nothing can be in flight at all: the marches were left
    // behind by a session that is no longer there to receive them.
    store
        .set_app_state(
            HUNT_HEARTBEAT_KEY,
            &json!(real_now - HEARTBEAT_FRESH_MILLIS - 1),
            real_now,
        )
        .await
        .unwrap();
    let stopped = store.hunt_summary(5).await.unwrap();
    assert!(!stopped.active, "a stale heartbeat means no run is alive");
    assert_eq!(stopped.in_flight, 0);
    assert_eq!(stopped.marches, 2, "history is still reported when stopped");
}

#[tokio::test]
async fn complete_mode_import_generates_ids_and_is_capacity_checked() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let mode_id = store
        .import_mode_bundle(&crate::planning::ventrilo_sands_bundle(), NOW)
        .await
        .unwrap();

    assert_eq!(store.attack_profiles().await.unwrap().len(), 2);
    assert_eq!(store.tasks().await.unwrap().len(), 2);
    let modes = store.modes().await.unwrap();
    assert_eq!(modes[0].mode_id, mode_id);
    assert_eq!(modes[0].task_ids.len(), 2);
    assert_eq!(modes[0].allocations[0].commander_count, 17);
    assert_eq!(modes[0].allocations[1].commander_count, 18);
    assert_eq!(modes[0].commander_count, 35);

    let error = store
        .subscribe_account_mode("ventrilo", mode_id, true, NOW)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("needs 35 commanders"));

    let small_mode = store
        .create_mode(
            "Two commanders",
            &[crate::planning::ModeTaskDraft {
                task_id: modes[0].task_ids[0].clone(),
                commander_count: 2,
            }],
            NOW,
        )
        .await
        .unwrap();
    store
        .subscribe_account_mode("ventrilo", small_mode, true, NOW)
        .await
        .unwrap();
    let account_modes = store.account_modes().await.unwrap();
    assert_eq!(account_modes.len(), 1);
    assert_eq!(account_modes[0].mode_id, small_mode);
    assert!(account_modes[0].running);
}

#[tokio::test]
async fn a_running_mode_compiles_tasks_and_reserves_each_target_once() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .replace_account_bootstrap(
            "ventrilo",
            &[OwnedCastle {
                kingdom_id: 1,
                castle_id: 100,
                area_type: 12,
                x: 593,
                y: 613,
                name: "Sands".to_owned(),
            }],
            &[0, 2],
            NOW,
        )
        .await
        .unwrap();
    let imported = store
        .import_mode_bundle(&crate::planning::ventrilo_sands_bundle(), NOW)
        .await
        .unwrap();
    let imported_mode = store
        .modes()
        .await
        .unwrap()
        .into_iter()
        .find(|mode| mode.mode_id == imported)
        .unwrap();
    let portable_task_id = imported_mode.task_ids[0].clone();
    let mut portable_runtime = store
        .task_runtimes()
        .await
        .unwrap()
        .into_iter()
        .find(|runtime| runtime.task_id == portable_task_id)
        .unwrap();
    portable_runtime.source_kind = crate::planning::SourceKind::MainCastle;
    // Deliberately stale coordinates prove that execution does not use them.
    portable_runtime.source_x = 999;
    portable_runtime.source_y = 999;
    store.upsert_task_runtime(&portable_runtime).await.unwrap();
    let mode_id = store
        .create_mode(
            "Executable",
            &[crate::planning::ModeTaskDraft {
                task_id: portable_task_id,
                commander_count: 2,
            }],
            NOW,
        )
        .await
        .unwrap();
    store
        .subscribe_account_mode("VENTRILO", mode_id, true, NOW)
        .await
        .unwrap();

    assert!(store.account_mode_running("ventrilo").await.unwrap());
    let tasks = store.active_mode_tasks("ventrilo").await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].commander_lids, vec![0, 2]);
    assert_eq!(tasks[0].target_kind, "rbc");
    assert!(tasks[0].payload.is_array());
    assert_eq!((tasks[0].source_x, tasks[0].source_y), (593, 613));

    // A main-castle task is portable. Its stored coordinates came from the
    // original account, but execution must resolve the castle belonging to
    // the account that is actually running the shared mode.
    seed_account(&store, "pingpoko").await;
    store
        .replace_account_bootstrap(
            "pingpoko",
            &[OwnedCastle {
                kingdom_id: 1,
                castle_id: 200,
                area_type: 12,
                x: 722,
                y: 533,
                name: "Pingpoko Sands".to_owned(),
            }],
            &[0, 2],
            NOW,
        )
        .await
        .unwrap();
    store
        .subscribe_account_mode("pingpoko", mode_id, true, NOW)
        .await
        .unwrap();
    let pingpoko_tasks = store.active_mode_tasks("pingpoko").await.unwrap();
    assert_eq!(pingpoko_tasks.len(), 1);
    assert_eq!(
        (pingpoko_tasks[0].source_x, pingpoko_tasks[0].source_y),
        (722, 533)
    );

    // Older releases could persist a second physical row for username casing.
    // It is still one logical target and reserving both copies must succeed.
    store
        .upsert_account_profile("Ventrilo", "Ventrilo", "wss://example/", "EmpireEx_21", NOW)
        .await
        .unwrap();
    store
        .upsert_rbc_targets(
            "Ventrilo",
            &[RbcTarget {
                kingdom_id: 1,
                x: 600,
                y: 610,
                level: Some(61),
            }],
            NOW,
        )
        .await
        .unwrap();

    let first = store
        .reserve_rbc_target(
            "ventrilo",
            1,
            Some(61),
            Some(61),
            (593, 613),
            "advanced",
            NOW,
            NOW + 720_000,
        )
        .await
        .unwrap();
    assert!(first.is_some());
    let second = store
        .reserve_rbc_target(
            "ventrilo",
            1,
            Some(61),
            Some(61),
            (593, 613),
            "advanced",
            NOW,
            NOW + 720_000,
        )
        .await
        .unwrap();
    assert!(
        second.is_none(),
        "a leased target must not be selected twice"
    );
    assert!(store.account_has_targets("VENTRILO", 1).await.unwrap());
    store.stop_account_mode("Ventrilo", NOW + 1).await.unwrap();
    assert!(!store.account_mode_running("ventrilo").await.unwrap());
}

// ---------------------------------------------------------------------------
// account identity
// ---------------------------------------------------------------------------

/// An account id is a key shared by castles, commanders, targets and the ledger.
/// Two casings of the same name used to produce two accounts, which is why every
/// castle appeared twice in the app.
#[tokio::test]
async fn the_same_account_written_with_two_casings_stays_one_account() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .upsert_account_profile(
            "Ventrilo",
            "Ventrilo",
            "wss://example/",
            "EmpireEx_21",
            NOW + 1,
        )
        .await
        .unwrap();
    store
        .replace_account_bootstrap(
            "Ventrilo",
            &[OwnedCastle {
                kingdom_id: 0,
                castle_id: 16_011_862,
                area_type: 1,
                x: 509,
                y: 405,
                name: "._.".to_owned(),
            }],
            &[7, 8],
            NOW + 1,
        )
        .await
        .unwrap();

    let summaries = store.account_summaries().await.unwrap();
    assert_eq!(summaries.len(), 1, "one account, not one per casing");
    assert_eq!(summaries[0].account_id, "ventrilo");
    assert_eq!(
        summaries[0].player_name, "Ventrilo",
        "the player's own spelling is still kept for display"
    );

    // The second bootstrap replaced the first, rather than adding a parallel set.
    let castles: Vec<i64> = store
        .owned_castles()
        .await
        .unwrap()
        .into_iter()
        .filter(|castle| castle.account_id == "ventrilo")
        .map(|castle| castle.castle_id)
        .collect();
    assert_eq!(castles, vec![16_011_862]);
    assert_eq!(store.commander_lids("ventrilo").await.unwrap(), vec![7, 8]);
}

/// The migration has to repair a database that already has both spellings,
/// without losing ledger history.
#[tokio::test]
async fn the_identity_migration_folds_existing_duplicate_rows() {
    let db = TempDb::new("identity-migration");
    let store = db.open().await;
    // The lowercase account is the one the session maintains, and it is set up.
    seed_account(&store, "legacy").await;
    let path = db.path.clone();
    drop(store);

    let pool = SqlitePoolOptions::new().connect(&db.url).await.unwrap();
    // Now add what the old hunter wrote: a second profile, a second copy of the
    // same castle, and a ledger row that has no lowercase twin and must survive.
    for sql in [
        "INSERT INTO account_profile (account_id, player_name, endpoint, server_header, updated_at_ms, initialized_at_ms) VALUES ('Legacy', 'Legacy', 'wss://example/', 'EmpireEx_21', 1, 1)",
        "INSERT INTO owned_castle (account_id, castle_id, kingdom_id, area_type, x, y, name, observed_at_ms) VALUES ('Legacy', 100, 1, 1, 572, 598, 'main', 1)",
        "INSERT INTO attack_ledger (account_id, march_id, kingdom_id, x, y, status, sent_at_ms) VALUES ('Legacy', 5, 1, 2, 2, 'sent', 1)",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    // Rewind the recorded version so only the identity migration runs again, the
    // way it will for a database that predates it.
    sqlx::query("UPDATE schema_version SET version = 6 WHERE singleton = 1")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let repaired = Store::open(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap();
    let summaries = repaired.account_summaries().await.unwrap();
    assert_eq!(summaries.len(), 1, "the duplicate account is folded away");
    assert_eq!(summaries[0].account_id, "legacy");
    let castles: Vec<i64> = repaired
        .owned_castles()
        .await
        .unwrap()
        .into_iter()
        .map(|castle| castle.castle_id)
        .collect();
    assert_eq!(
        castles,
        vec![100],
        "the castle is listed once, not once per casing"
    );
    let marches = repaired.recent_marches_all(10).await.unwrap();
    assert_eq!(
        marches.len(),
        1,
        "history written under the other casing survives"
    );
    assert_eq!(marches[0].account_id, "legacy");
}

// ---------------------------------------------------------------------------
// deleting something another row depends on
// ---------------------------------------------------------------------------

async fn seed_recruitment(store: &Store, id: &str, name: &str) {
    store
        .upsert_recruitment(
            &RecruitmentTemplate {
                recruitment_id: id.to_owned(),
                name: name.to_owned(),
                troop_id: 606,
                quantity: 50,
                slot_count: 2,
                ask_alliance_help: false,
                lane_id: 0,
                skill_id: 73,
            },
            NOW,
        )
        .await
        .unwrap();
}

fn simple_task(id: &str, name: &str) -> TaskRecord {
    TaskRecord {
        task_id: id.to_owned(),
        name: name.to_owned(),
        kind: "attack".to_owned(),
        kingdom_id: 1,
        profile_id: None,
        target_level_min: Some(40),
        target_level_max: Some(60),
        commander_count: 1,
        max_active: None,
        priority: 100,
        enabled: true,
        tags: Vec::new(),
        notes: String::new(),
    }
}

#[tokio::test]
async fn a_recruitment_a_recruit_bot_still_uses_is_not_deleted() {
    let db = TempDb::new("recruit-in-use");
    let store = db.open().await;
    seed_account(&store, "ventrilo").await;
    seed_recruitment(&store, "recruit-1", "Spears").await;
    let bot = store
        .create_recruit_bot(
            "Spear Keeper",
            "advanced",
            &[RecruitBotCastle {
                account_id: "ventrilo".to_owned(),
                castle_id: 100,
                recruitment_id: "recruit-1".to_owned(),
                position: 0,
            }],
            NOW,
        )
        .await
        .unwrap();

    // Refused, and the bot is named. The schema's own RESTRICT also refuses, but
    // it can only say "constraint failed", which does not tell the user that a
    // bot they built is holding the recruitment open or which one it is.
    let blocking = store.delete_recruitment("recruit-1").await.unwrap();
    assert_eq!(blocking, vec!["Spear Keeper".to_owned()]);
    assert_eq!(
        store.recruitments().await.unwrap().len(),
        1,
        "a refused delete leaves the recruitment alone"
    );

    store.delete_recruit_bot(bot).await.unwrap();
    assert!(
        store
            .delete_recruitment("recruit-1")
            .await
            .unwrap()
            .is_empty(),
        "with no bot left the recruitment deletes cleanly"
    );
    assert!(store.recruitments().await.unwrap().is_empty());
}

#[tokio::test]
async fn deleting_a_recruit_bot_takes_its_castles_and_its_subscription() {
    let db = TempDb::new("recruit-bot-delete");
    let store = db.open().await;
    seed_account(&store, "ventrilo").await;
    seed_recruitment(&store, "recruit-1", "Spears").await;
    let bot = store
        .create_recruit_bot(
            "Spear Keeper",
            "advanced",
            &[RecruitBotCastle {
                account_id: "ventrilo".to_owned(),
                castle_id: 100,
                recruitment_id: "recruit-1".to_owned(),
                position: 0,
            }],
            NOW,
        )
        .await
        .unwrap();
    store
        .subscribe_account_recruit_bot("ventrilo", Some(bot), true, NOW)
        .await
        .unwrap();
    assert_eq!(
        store.active_recruitments("ventrilo").await.unwrap().len(),
        1,
        "the bot is running before the delete"
    );

    store.delete_recruit_bot(bot).await.unwrap();

    assert!(store.recruit_bots().await.unwrap().is_empty());
    assert!(
        store.account_recruit_bots().await.unwrap().is_empty(),
        "the account is no longer subscribed to a bot that is gone"
    );
    let orphans: i64 = sqlx::query("SELECT COUNT(*) AS n FROM recruit_bot_castle")
        .fetch_one(&store.pool)
        .await
        .unwrap()
        .get("n");
    assert_eq!(orphans, 0, "no castle row outlives the bot it belonged to");
    assert!(
        store
            .active_recruitments("ventrilo")
            .await
            .unwrap()
            .is_empty(),
        "the runner now finds nothing to do, rather than a broken bot"
    );
}

#[tokio::test]
async fn deleting_a_task_takes_it_out_of_the_modes_that_use_it() {
    let db = TempDb::new("task-in-mode");
    let store = db.open().await;
    store
        .upsert_task(&simple_task("task-1", "Farm forts"), NOW)
        .await
        .unwrap();
    store
        .upsert_task(&simple_task("task-2", "Farm barons"), NOW)
        .await
        .unwrap();
    let mode_id = store
        .create_mode(
            "Sands",
            &[
                crate::planning::ModeTaskDraft {
                    task_id: "task-1".to_owned(),
                    commander_count: 2,
                },
                crate::planning::ModeTaskDraft {
                    task_id: "task-2".to_owned(),
                    commander_count: 1,
                },
            ],
            NOW,
        )
        .await
        .unwrap();

    store.delete_task("task-1").await.unwrap();

    let modes = store.modes().await.unwrap();
    let mode = modes.iter().find(|value| value.mode_id == mode_id).unwrap();
    assert_eq!(
        mode.task_ids,
        vec!["task-2".to_owned()],
        "the mode survives, minus the task that was deleted"
    );
    assert_eq!(
        mode.commander_count, 1,
        "the mode's commander total is recomputed from what is left"
    );
}

#[tokio::test]
async fn upgrading_sweeps_children_whose_parent_is_gone() {
    let db = TempDb::new("orphan-sweep");
    let bot = {
        let store = db.open().await;
        seed_account(&store, "ventrilo").await;
        seed_recruitment(&store, "recruit-1", "Spears").await;
        store
            .create_recruit_bot(
                "Spear Keeper",
                "advanced",
                &[RecruitBotCastle {
                    account_id: "ventrilo".to_owned(),
                    castle_id: 100,
                    recruitment_id: "recruit-1".to_owned(),
                    position: 0,
                }],
                NOW,
            )
            .await
            .unwrap()
    };

    // A connection that never had foreign keys enforced turns a cascade into a
    // no-op. That is how the debris this migration clears was created, so it is
    // recreated the same way rather than by hand-written SQL that proves nothing.
    {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&db.url)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .unwrap();
        for (bot_id, castle_id, recruitment_id) in [
            (bot, 999, "recruit-gone"),   // the template went, the row stayed
            (bot + 99, 998, "recruit-1"), // the bot went, the row stayed
        ] {
            sqlx::query(
                "INSERT INTO recruit_bot_castle
                    (recruit_bot_id, account_id, castle_id, recruitment_id, position)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(bot_id)
            .bind("ventrilo")
            .bind(castle_id)
            .bind(recruitment_id)
            .bind(0)
            .execute(&pool)
            .await
            .unwrap();
        }
        // Wind the version back so the next open runs the sweep.
        sqlx::query("UPDATE schema_version SET version = 7")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    let repaired = db.open().await;
    assert_eq!(
        schema::version(&repaired.pool).await.unwrap(),
        schema::SCHEMA_VERSION
    );
    let castles: Vec<i64> = repaired
        .recruit_bots()
        .await
        .unwrap()
        .into_iter()
        .flat_map(|bot| bot.castles)
        .map(|castle| castle.castle_id)
        .collect();
    assert_eq!(
        castles,
        vec![100],
        "the reachable castle survives and both orphans are swept"
    );
}
