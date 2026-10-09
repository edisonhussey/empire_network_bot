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
use crate::fortress::SWEEP_ALIGNMENT;

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
                cooldown_remaining_s: 0,
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
            .scan_window_is_fresh("scan-account", 1, (572, 598, 584, 610), NOW + 60_000,)
            .await
            .unwrap()
    );
    assert!(
        !store
            .scan_window_is_fresh("scan-account", 1, (572, 598, 672, 698), NOW + 60_000,)
            .await
            .unwrap(),
        "a cached 13x13 response must not satisfy a later 101x101 request"
    );
    let due = scan_refresh_after_ms(1, 572, 598);
    assert!((SCAN_REFRESH_BASE_MS - 1_800_000..=SCAN_REFRESH_BASE_MS + 1_800_000).contains(&due));
    assert!(
        !store
            .scan_window_is_fresh("scan-account", 1, (572, 598, 584, 610), NOW + due + 1,)
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

    // A fresh server read reporting zero is the proof that opens a new
    // one-minute claim window; an old scheduled timestamp alone is not proof.
    store
        .upsert_fortress_targets("ventrilo", &[ready], NOW + 120_000)
        .await
        .unwrap();
    let refreshed = store
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
        .expect("fresh zero-cooldown observation re-arms the minute");
    store
        .defer_fortress_target("ventrilo", &refreshed, NOW + 420_000)
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
                NOW + 120_001,
                NOW + 121_001,
            )
            .await
            .unwrap()
            .is_none(),
        "a refused claim stays quarantined instead of retrying"
    );
}

#[tokio::test]
async fn fortress_discovery_sweep_is_durable_and_never_repeats_a_block() {
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
                cooldown_remaining_s: 0,
            }],
            NOW,
        )
        .await
        .unwrap();
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fortress_scan_state WHERE account_id = 'ventrilo'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(rows, 0, "ordinary RBCs must never start a fortress sweep");

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
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fortress_scan_state WHERE account_id = 'ventrilo'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(rows, 0, "observations do not define scan completeness");

    assert!(store.start_fortress_scan("ventrilo", 1, 9).await.unwrap());
    let total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(blocks_total), 0) FROM fortress_scan_state
         WHERE account_id = 'ventrilo'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(total, 121, "one kingdom, in blocks");

    let block = store
        .next_fortress_block("ventrilo", 1, 9)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((block.x, block.y), (9, 9));
    // Reading is idempotent: the same block comes back until it is advanced,
    // so a request that never arrives is retried rather than skipped.
    let again = store
        .next_fortress_block("ventrilo", 1, 9)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((again.x, again.y), (9, 9));
    store.advance_fortress_scan("ventrilo", 1, 9).await.unwrap();
    let next = store
        .next_fortress_block("ventrilo", 1, 9)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((next.x, next.y), (126, 9));

    // Starting again must not reset a cursor that has already moved.
    assert!(!store.start_fortress_scan("ventrilo", 1, 9).await.unwrap());
    let still = store
        .next_fortress_block("ventrilo", 1, 9)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still.x, 126);
}

#[tokio::test]
async fn an_explicit_scan_rewalks_known_bounds_for_fresh_cooldowns() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    assert!(store.start_fortress_scan("ventrilo", 1, 9).await.unwrap());
    store.advance_fortress_scan("ventrilo", 1, 9).await.unwrap();
    assert_eq!(
        store
            .next_fortress_block("ventrilo", 1, 9)
            .await
            .unwrap()
            .unwrap()
            .x,
        126
    );
    store
        .reset_fortress_scans("ventrilo", &[(1, 9)])
        .await
        .unwrap();
    let restarted = store
        .next_fortress_block("ventrilo", 1, 9)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((restarted.x, restarted.y), (9, 9));
}

#[tokio::test]
async fn a_fortress_task_can_start_a_sweep_without_an_observed_fortress() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    // A fresh account knows of no fortress and has no sweep, so there is
    // nothing to scan. Starting the sweep is the bootstrap that gives the task
    // its first map request; before it existed the task idled forever.
    assert!(
        store
            .next_fortress_block("ventrilo", 1, 9)
            .await
            .unwrap()
            .is_none()
    );
    assert!(store.start_fortress_scan("ventrilo", 1, 9).await.unwrap());
    let block = store
        .next_fortress_block("ventrilo", 1, 9)
        .await
        .unwrap()
        .expect("the aligned corner is the first block");
    // The sweep starts on the lattice, not at map 0:0.
    assert_eq!((block.x, block.y), (9, 9));
    for slot in block.slots() {
        assert!(crate::fortress::is_slot(slot.0, slot.1));
    }
}

/// The sweep is a cursor over the rectangle, so it walks a row at a time and
/// wraps. It deliberately does not order by distance from the castle any more:
/// every block is visited, so the order only decides what is seen first.
#[tokio::test]
async fn the_sweep_walks_row_by_row_and_wraps() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store.start_fortress_scan("ventrilo", 1, 9).await.unwrap();
    let mut seen = Vec::new();
    for _ in 0..18 {
        let block = store
            .next_fortress_block("ventrilo", 1, 9)
            .await
            .unwrap()
            .unwrap();
        seen.push((block.x, block.y));
        store.advance_fortress_scan("ventrilo", 1, 9).await.unwrap();
    }
    assert_eq!(seen[0], (9, 9));
    assert_eq!(seen[10], (1_179, 9), "eleven 117-unit columns");
    assert_eq!(seen[11], (9, 126), "the next block starts the second row");
}

/// Fortress coordinates never change, so a kingdom walked in an earlier session
/// must not be walked again. The runner asks which sweeps still have work and
/// resumes only those; with nothing left there is nothing to wait for.
#[tokio::test]
async fn only_sweeps_with_blocks_left_are_reported_as_outstanding() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let all = vec![(1_i64, SWEEP_ALIGNMENT), (2, SWEEP_ALIGNMENT)];

    // Nothing started yet: both kingdoms are still to do.
    assert_eq!(
        store
            .outstanding_fortress_sweeps("ventrilo", &all)
            .await
            .unwrap(),
        all
    );

    // Kingdom 1 finished, kingdom 2 untouched.
    let offset = SWEEP_ALIGNMENT;
    store
        .start_fortress_scan("ventrilo", 1, offset)
        .await
        .unwrap();
    let mut visited = 0;
    while store
        .next_fortress_block("ventrilo", 1, offset)
        .await
        .unwrap()
        .is_some()
    {
        store
            .advance_fortress_scan("ventrilo", 1, offset)
            .await
            .unwrap();
        visited += 1;
    }
    assert_eq!(visited, 121, "one kingdom is 121 blocks");
    assert_eq!(
        store
            .outstanding_fortress_sweeps("ventrilo", &all)
            .await
            .unwrap(),
        vec![(2, SWEEP_ALIGNMENT)],
        "a finished kingdom drops out, an untouched one stays"
    );

    // Finish the last one. An empty answer is what lets automation run.
    store
        .start_fortress_scan("ventrilo", 2, offset)
        .await
        .unwrap();
    while store
        .next_fortress_block("ventrilo", 2, offset)
        .await
        .unwrap()
        .is_some()
    {
        store
            .advance_fortress_scan("ventrilo", 2, offset)
            .await
            .unwrap();
    }
    assert!(
        store
            .outstanding_fortress_sweeps("ventrilo", &all)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Discovery is work done *for* a fortress task, so the walk waits until such a
/// task is in the bot that is actually running. A robber-baron bot, or a
/// fortress task sitting in a bot nobody started, must not cost a walk.
#[tokio::test]
async fn only_the_running_bots_fortress_kingdoms_are_walked() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    assert!(
        store
            .active_fortress_kingdoms("ventrilo")
            .await
            .unwrap()
            .is_empty(),
        "nothing is running, so there is nothing to walk for"
    );

    let task = |id: &str, kingdom_id: i64, enabled: bool| TaskRecord {
        task_id: id.into(),
        name: id.into(),
        kind: "attack".into(),
        kingdom_id,
        profile_id: None,
        target_level_min: None,
        target_level_max: None,
        commander_count: 1,
        max_active: None,
        priority: 10,
        enabled,
        tags: Vec::new(),
        notes: String::new(),
    };
    let subscribe = |id: &str, target_kind: &str, kingdom_id: i64| SubscriptionRecord {
        task_id: id.into(),
        target_kind: target_kind.into(),
        filter: json!({ "kingdom_id": kingdom_id }),
    };
    let seed = async |id: &str, kind: &str, kingdom_id: i64, enabled: bool| {
        store
            .upsert_task(&task(id, kingdom_id, enabled), NOW)
            .await
            .unwrap();
        store
            .replace_subscriptions(id, &[subscribe(id, kind, kingdom_id)])
            .await
            .unwrap();
    };
    seed("sand-fort", "fortress", 1, true).await;
    seed("fire-rbc", "rbc", 3, true).await;

    // The tasks exist, but no bot is running yet.
    assert!(
        store
            .active_fortress_kingdoms("ventrilo")
            .await
            .unwrap()
            .is_empty(),
        "a task nobody is running is not a reason to walk"
    );

    // Running the robber-baron bot: the fortress task is not in it.
    let rbc_mode = store
        .create_mode(
            "rbc only",
            &[crate::planning::ModeTaskDraft {
                task_id: "fire-rbc".into(),
                commander_count: 1,
            }],
            NOW,
        )
        .await
        .unwrap();
    store
        .subscribe_account_mode("ventrilo", rbc_mode, true, NOW)
        .await
        .unwrap();
    assert!(
        store
            .active_fortress_kingdoms("ventrilo")
            .await
            .unwrap()
            .is_empty(),
        "an rbc bot must not trigger a fortress walk"
    );

    // Running the fortress bot: that kingdom is what gets walked.
    let fortress_mode = store
        .create_mode(
            "sands fortresses",
            &[crate::planning::ModeTaskDraft {
                task_id: "sand-fort".into(),
                commander_count: 1,
            }],
            NOW,
        )
        .await
        .unwrap();
    store
        .subscribe_account_mode("ventrilo", fortress_mode, true, NOW)
        .await
        .unwrap();
    assert_eq!(
        store.active_fortress_kingdoms("ventrilo").await.unwrap(),
        vec![1]
    );

    // A disabled fortress task is not a reason to spend requests either.
    store
        .upsert_task(&task("sand-fort", 1, false), NOW)
        .await
        .unwrap();
    assert!(
        store
            .active_fortress_kingdoms("ventrilo")
            .await
            .unwrap()
            .is_empty()
    );

    // Stopping the bot puts the walk back to sleep.
    store
        .upsert_task(&task("sand-fort", 1, true), NOW)
        .await
        .unwrap();
    store.stop_account_mode("ventrilo", NOW).await.unwrap();
    assert!(
        store
            .active_fortress_kingdoms("ventrilo")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_reservation_serves_the_fortress_whose_window_closes_first() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let fortress = |x: i64, y: i64, cooldown_remaining_s: i64| FortressTarget {
        kingdom_id: 1,
        x,
        y,
        level: 45,
        cooldown_remaining_s,
        occupier_player_id: 1,
    };
    // Three fortresses came off cooldown at different moments. Whichever has
    // been available longest is closest to losing its one-minute window.
    store
        .upsert_fortress_targets(
            "ventrilo",
            &[
                fortress(600, 600, 10),
                fortress(601, 600, 5),
                fortress(700, 700, 0),
            ],
            NOW,
        )
        .await
        .unwrap();
    let now = NOW + 60_000;
    let reserve = |at: i64| {
        let store = &store;
        async move {
            store
                .reserve_fortress_target("ventrilo", 1, None, None, (600, 600), at, at + 1_000)
                .await
                .unwrap()
        }
    };
    // The oldest-ready fortress is served first, so with only a few commanders
    // the newest is the one that gets dropped.
    assert_eq!(
        reserve(now).await.map(|target| (target.x, target.y)),
        Some((700, 700))
    );
    assert_eq!(
        reserve(now).await.map(|target| (target.x, target.y)),
        Some((601, 600))
    );
    assert_eq!(
        reserve(now).await.map(|target| (target.x, target.y)),
        Some((600, 600))
    );
    // Every window is gone a minute after it opened, so nothing is eligible.
    assert!(reserve(now + 60_001).await.is_none());
}

#[tokio::test]
async fn the_sweep_does_not_depend_on_where_fortresses_were_found() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let fortress = |x: i64, y: i64| FortressTarget {
        kingdom_id: 1,
        x,
        y,
        level: 45,
        cooldown_remaining_s: 1,
        occupier_player_id: 1,
    };
    store
        .upsert_fortress_targets("ventrilo", &[fortress(594, 594)], NOW)
        .await
        .unwrap();
    // An observation on its own neither starts a sweep nor shapes one.
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fortress_scan_state WHERE account_id = 'ventrilo'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(rows, 0);

    store.start_fortress_scan("ventrilo", 1, 9).await.unwrap();
    let bounds_query = || {
        sqlx::query(
            "SELECT left_bound, top_bound, right_bound, bottom_bound FROM fortress_scan_state
             WHERE account_id = 'ventrilo' AND kingdom_id = 1 AND lattice_offset = 9",
        )
    };
    let row = bounds_query().fetch_one(&store.pool).await.unwrap();
    let recorded = (
        row.get::<i64, _>("left_bound"),
        row.get::<i64, _>("top_bound"),
        row.get::<i64, _>("right_bound"),
        row.get::<i64, _>("bottom_bound"),
    );
    assert_eq!(recorded, (9, 9, 1_285, 1_285));

    // A later observation must not move the bounds or restart the sweep.
    store
        .upsert_fortress_targets("ventrilo", &[fortress(672, 594)], NOW + 3)
        .await
        .unwrap();
    assert!(!store.start_fortress_scan("ventrilo", 1, 9).await.unwrap());
    let row = bounds_query().fetch_one(&store.pool).await.unwrap();
    assert_eq!(
        (
            row.get::<i64, _>("left_bound"),
            row.get::<i64, _>("top_bound"),
            row.get::<i64, _>("right_bound"),
            row.get::<i64, _>("bottom_bound"),
        ),
        recorded
    );
}

/// The sweep covers the whole observed map, so it has to finish at the far
/// corner rather than anywhere near the castle.
#[tokio::test]
async fn the_sweep_reaches_the_observed_map_boundary() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store.start_fortress_scan("ventrilo", 1, 29).await.unwrap();
    let total: i64 = sqlx::query_scalar(
        "SELECT blocks_total FROM fortress_scan_state
         WHERE account_id = 'ventrilo' AND kingdom_id = 1 AND lattice_offset = 29",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(total, 121);

    let mut last = (0_i64, 0_i64);
    while let Some(block) = store.next_fortress_block("ventrilo", 1, 29).await.unwrap() {
        last = (block.x, block.y);
        store
            .advance_fortress_scan("ventrilo", 1, 29)
            .await
            .unwrap();
    }
    assert_eq!(last, (1_199, 1_199));
    let (done, pending) = store.fortress_probe_progress("ventrilo", 1).await.unwrap();
    assert_eq!((done, pending), (121, 0));
}

/// The sweep is the complete enumeration of a kingdom, so walking both grids
/// has to visit every block. An earlier version narrowed the queue to the
/// rectangle around the fortresses it had already found, which quietly ended a
/// kingdom's search at the handful of fortresses near its castle.
#[tokio::test]
async fn the_sweep_exhausts_the_whole_kingdom() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let fortress = |x: i64, y: i64| FortressTarget {
        kingdom_id: 1,
        x,
        y,
        level: 45,
        cooldown_remaining_s: 1,
        occupier_player_id: 1,
    };
    // A couple of hits near the castle - the shape that used to end the walk.
    store
        .upsert_fortress_targets("ventrilo", &[fortress(594, 594), fortress(633, 633)], NOW)
        .await
        .unwrap();

    let mut probed = 0_usize;
    let mut furthest = (0_i64, 0_i64);
    let offset = SWEEP_ALIGNMENT;
    store
        .start_fortress_scan("ventrilo", 1, offset)
        .await
        .unwrap();
    while let Some(block) = store
        .next_fortress_block("ventrilo", 1, offset)
        .await
        .unwrap()
    {
        probed += 1;
        assert!(probed <= 121, "the sweep has to terminate");
        if block.x + block.y > furthest.0 + furthest.1 {
            furthest = (block.x, block.y);
        }
        store
            .advance_fortress_scan("ventrilo", 1, offset)
            .await
            .unwrap();
    }
    assert_eq!(probed, 121, "every block in the kingdom has to be visited");
    assert_eq!(
        furthest,
        (1_179, 1_179),
        "the sweep has to reach the corner of the map, not stop at the castle"
    );
    let (done, pending) = store.fortress_probe_progress("ventrilo", 1).await.unwrap();
    assert_eq!((done, pending), (121, 0));
}

#[tokio::test]
async fn rechecking_a_cooldown_does_not_schedule_another_immediately() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let observed = |cooldown_remaining_s: i64| FortressTarget {
        kingdom_id: 1,
        x: 594,
        y: 594,
        level: 45,
        cooldown_remaining_s,
        occupier_player_id: 1,
    };
    // Long cooldown, just observed: the coordinates are known and the server
    // already told us when it opens, so there is nothing to re-read yet.
    store
        .upsert_fortress_targets("ventrilo", &[observed(30_000)], NOW)
        .await
        .unwrap();
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 1, NOW)
            .await
            .unwrap(),
        0,
        "a fortress whose cooldown we just read must not be read again"
    );

    // Nothing has been seen for seven hours, so a sanity read is worthwhile.
    store
        .upsert_fortress_targets("ventrilo", &[observed(29_000)], NOW - 7 * 3_600_000)
        .await
        .unwrap();
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 1, NOW)
            .await
            .unwrap(),
        1
    );

    // Reading it refreshes the observation, which must NOT queue another read -
    // that would spin forever and starve discovery and attacks.
    store
        .upsert_fortress_targets("ventrilo", &[observed(28_900)], NOW)
        .await
        .unwrap();
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 1, NOW + 1)
            .await
            .unwrap(),
        0,
        "a just-read cooldown must not be queued again"
    );

    // The window is approaching and the observation is stale enough to be worth
    // confirming: read from six minutes ago, opening in five.
    store
        .upsert_fortress_targets("ventrilo", &[observed(300)], NOW - 6 * 60_000)
        .await
        .unwrap();
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 1, NOW)
            .await
            .unwrap(),
        1,
        "a fortress opening shortly is worth confirming"
    );
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
    assert_eq!(store.licences().await.unwrap().len(), 1);

    store.wipe_all().await.unwrap();

    let report = store.storage_report().await.unwrap();
    assert_eq!(report.total_rows(), 0, "every data table must be empty");
    assert!(store.licences().await.unwrap().is_empty());
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

// ---------------------------------------------------------------------------
// durable telemetry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stock_is_sampled_at_most_once_a_minute_but_summarised_every_time() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    // 100 attacks in ten minutes would be 100 raw rows without the rate limit.
    for index in 0..100_i64 {
        store
            .record_stock_sample(
                "Ventrilo",
                16366514,
                &[(607, 22_000 - index * 20, 50)],
                NOW + index * 6_000,
            )
            .await
            .unwrap();
    }
    let raw: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM castle_stock_sample")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(raw, 10, "one raw row per minute");
    let (samples, min_home, max_home, last_home): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT samples, min_home, max_home, last_home FROM castle_stock_hourly
         WHERE account_id = 'ventrilo' AND unit_id = 607",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!((samples, min_home, max_home, last_home), (100, 20_020, 22_000, 20_020));
}

#[tokio::test]
async fn raw_stock_expires_but_the_hourly_summary_stays_and_accounts_stay_apart() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store.record_stock_sample("ventrilo", 1, &[(607, 100, 0)], NOW).await.unwrap();
    store.record_stock_sample("pingpoko", 1, &[(607, 7, 0)], NOW).await.unwrap();
    // A sample four days later prunes the old raw rows of that account only.
    store
        .record_stock_sample("ventrilo", 1, &[(607, 90, 0)], NOW + 4 * 24 * 3_600_000)
        .await
        .unwrap();
    let raw = |account: &'static str| {
        let store = store.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM castle_stock_sample WHERE account_id = ?",
            )
            .bind(account)
            .fetch_one(&store.pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(raw("ventrilo").await, 1);
    assert_eq!(raw("pingpoko").await, 1, "another account is untouched");
    let hourly: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM castle_stock_hourly")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(hourly, 3, "the summary is never pruned");
}

#[tokio::test]
async fn the_forecast_reports_the_net_drain_and_when_stock_runs_out() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    // Home plus out falls 1,200 an hour (20 a minute): 22,000 to 21,000 in 50 min.
    for minute in 0..=50_i64 {
        store
            .record_stock_sample("ventrilo", 1, &[(607, 22_000 - minute * 20 - 50, 50)], NOW + minute * 60_000)
            .await
            .unwrap();
    }
    let forecast = store
        .stock_forecast("ventrilo", 1, 607, NOW + 50 * 60_000)
        .await
        .unwrap()
        .unwrap();
    assert!((forecast.drain_per_hour - 1_200.0).abs() < 1.0, "{forecast:?}");
    let hours = forecast.hours_left.unwrap();
    assert!((hours - 17.5).abs() < 0.2, "{hours}");
    // Too little history to claim a rate.
    let fresh = store.stock_forecast("ventrilo", 1, 99, NOW).await.unwrap();
    assert!(fresh.is_none());
}

#[tokio::test]
async fn events_are_kept_per_account_and_noisy_ones_are_only_counted() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store.record_event("Ventrilo", "connection.lost", "reset", NOW).await.unwrap();
    store.record_event("pingpoko", "connection.lost", "other", NOW).await.unwrap();
    for index in 0..500 {
        store.bump_event("ventrilo", "army_short", "unit 607 need 50 have 12", NOW + index).await.unwrap();
    }
    assert_eq!(store.recent_events("ventrilo", 10).await.unwrap().len(), 1);
    let (count, last): (i64, String) = sqlx::query_as(
        "SELECT count, last_detail FROM event_hourly WHERE account_id = 'ventrilo' AND kind = 'army_short'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(count, 500, "500 occurrences are one row");
    assert!(last.contains("need 50"));
    // Ninety days on, the old event is pruned by the next write.
    store
        .record_event("ventrilo", "connection.lost", "later", NOW + EVENT_RETENTION_MS + 1)
        .await
        .unwrap();
    assert_eq!(store.recent_events("ventrilo", 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_returned_march_records_what_it_lost() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store.record_march(&march(Some("sand"), 9100, 7)).await.unwrap();
    store
        .set_march_army("ventrilo", 9100, &[(607, 50)].into_iter().collect())
        .await
        .unwrap();
    store
        .set_commander_state(
            &CommanderState {
                account_id: "ventrilo".into(),
                lord_id: 7,
                status: "outbound".into(),
                available_after_ms: NOW + 120_000,
                march_id: Some(9100),
                target_key: Some("1:600:610".into()),
            },
            NOW,
        )
        .await
        .unwrap();
    let packet = json!({"A":{"A":[[607,31]],"M":{"KID":1,"SA":[2,600,610],"TA":[12,593,613],"TT":205,"PT":0},
        "UM":{"L":{"ID":7}},"G":[["C1",22000]],"S":1}});
    assert!(store.apply_attack_return("ventrilo", &packet, NOW + 100_000, 7000).await.unwrap());
    let (sent, returned, lost): (i64, i64, i64) = sqlx::query_as(
        "SELECT troop_count, troops_returned, troops_lost FROM attack_ledger WHERE march_id = 9100",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!((sent, returned, lost), (50, 31, 19));
}

#[tokio::test]
async fn captured_packets_are_tagged_with_their_account() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .record_message_for(Some("Ventrilo"), NOW, Direction::ServerToClient, Some("adi"), &json!({}))
        .await
        .unwrap();
    store
        .record_message(NOW, Direction::ServerToClient, Some("adi"), &json!({}))
        .await
        .unwrap();
    let accounts: Vec<Option<String>> =
        sqlx::query_scalar("SELECT account_id FROM network_message ORDER BY sequence")
            .fetch_all(&store.pool)
            .await
            .unwrap();
    assert_eq!(accounts, vec![Some("ventrilo".to_owned()), None]);
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
    // The bars always have a full day of slots, and the hour just ended holds
    // the one attack, return and loot recorded above.
    let bars = &dashboard.hourly_bars;
    for series in [&bars.rubies, &bars.coins, &bars.attacks, &bars.returns] {
        assert_eq!(series.len(), 24);
    }
    assert_eq!(bars.attacks.iter().sum::<i64>(), 1);
    assert_eq!(*bars.returns.last().unwrap(), 1);
    assert_eq!(*bars.rubies.last().unwrap(), 7);
    assert_eq!(*bars.coins.last().unwrap(), 500);
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
async fn fortress_return_holds_commander_until_home_and_duplicate_cannot_release_new_trip() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let outbound = march(Some("fortress"), 9001, 7);
    store.record_march(&outbound).await.unwrap();
    let mut commander = CommanderState {
        account_id: "ventrilo".into(),
        lord_id: 7,
        status: "outbound".into(),
        available_after_ms: NOW + 120_000,
        march_id: Some(9001),
        target_key: Some("1:600:610".into()),
    };
    store.set_commander_state(&commander, NOW).await.unwrap();
    let packet = json!({"A":{"M":{"KID":1,"SA":[11,600,610],"TA":[12,593,613],"TT":205,"PT":5},
        "UM":{"L":{"ID":7}},"G":[["C2",280]],"S":1}});
    assert!(
        store
            .apply_attack_return("ventrilo", &packet, NOW + 100_000, 7000)
            .await
            .unwrap()
    );
    let states = store.commander_states("ventrilo").await.unwrap();
    assert_eq!(states[0].status, "returning");
    assert_eq!(states[0].available_after_ms, NOW + 307_000);
    let rows = store.recent_marches("ventrilo", 5).await.unwrap();
    assert_eq!(rows[0].status, "returning");
    assert_eq!(rows[0].ruby_loot, Some(280));
    commander.march_id = Some(9002);
    commander.available_after_ms = NOW + 900_000;
    store
        .set_commander_state(&commander, NOW + 400_000)
        .await
        .unwrap();
    assert!(
        !store
            .apply_attack_return("ventrilo", &packet, NOW + 100_000, 7000)
            .await
            .unwrap()
    );
    assert_eq!(
        store.commander_states("ventrilo").await.unwrap()[0].march_id,
        Some(9002)
    );
    assert_eq!(
        store.commander_states("ventrilo").await.unwrap()[0].available_after_ms,
        NOW + 900_000
    );
}

#[tokio::test]
async fn in_flight_counts_commanders_on_both_legs_even_when_disconnected() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;

    // Availability is measured against the real clock, not the fixed `NOW`, so
    // the rows are placed relative to it.
    let real_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut recent = march(Some("sand_kunai"), 88, 25);
    recent.sent_at_ms = real_now - 60_000;
    store.record_march(&recent).await.unwrap();

    // An old ledger row whose result never arrived. It must not be counted:
    // "in flight" is decided by the commander's own record, so a lost result
    // cannot leave the number climbing, and it cannot push the count past the
    // number of commanders the run actually holds.
    let mut abandoned = march(Some("sand_kunai"), 89, 26);
    abandoned.sent_at_ms = real_now - 2 * 60 * 60 * 1_000;
    store.record_march(&abandoned).await.unwrap();
    for (lid, status, available_after_ms) in [
        (25, "outbound", real_now + 60_000),
        (26, "returning", real_now + 30_000),
        (27, "returning", real_now - 1),
    ] {
        store
            .set_commander_state(
                &CommanderState {
                    account_id: "ventrilo".into(),
                    lord_id: lid,
                    status: status.into(),
                    available_after_ms,
                    march_id: None,
                    target_key: None,
                },
                real_now,
            )
            .await
            .unwrap();
    }

    // With a run checking in, only the recent march is in the air.
    store
        .set_app_state(HUNT_HEARTBEAT_KEY, &json!(real_now), real_now)
        .await
        .unwrap();
    let live = store.hunt_summary(5).await.unwrap();
    assert!(live.active);
    assert_eq!(live.marches, 2);
    assert_eq!(
        live.in_flight, 2,
        "count outbound and returning commanders, excluding those already home"
    );
    assert_eq!(live.commanders_outbound, 1);
    assert_eq!(live.commanders_returning, 1);
    assert_eq!(
        live.in_flight,
        live.commanders_outbound + live.commanders_returning
    );
    // The count can never exceed the commanders the account owns. That is the
    // property that was violated when this counted ledger rows: 15 attacks from
    // 10 commanders reported 14 in flight.
    assert!(
        live.in_flight <= store.commander_states("ventrilo").await.unwrap().len() as i64,
        "in flight must never exceed the commander count"
    );

    // Closing a connection does not bring an army home.
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
    assert_eq!(stopped.in_flight, 2);
    assert_eq!(stopped.commanders_outbound, 1);
    assert_eq!(stopped.commanders_returning, 1);
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
async fn a_tower_the_server_reports_cooling_is_not_selected_until_it_is_ready() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let cooling = RbcTarget {
        kingdom_id: 1,
        x: 600,
        y: 610,
        level: Some(61),
        cooldown_remaining_s: 600,
    };
    store.upsert_rbc_targets("ventrilo", &[cooling.clone()], NOW).await.unwrap();
    let pick = |at: i64| {
        let store = store.clone();
        async move {
            store
                .reserve_rbc_target(
                    "ventrilo", 1, Some(61), Some(61), (593, 613), "advanced", at, at + 720_000,
                )
                .await
                .unwrap()
        }
    };

    assert!(pick(NOW).await.is_none(), "the server says it is cooling");
    let ready_at = store
        .next_rbc_ready_ms("ventrilo", 1, Some(61), Some(61))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready_at, NOW + 602_000, "ten minutes plus the in-flight slack");
    assert!(pick(ready_at - 1).await.is_none());
    assert!(pick(ready_at).await.is_some());

    // A later map read corrects the cooldown, and a tower we never scanned is
    // not added to the catalogue by it.
    store.release_rbc_target("ventrilo", &ReservedTarget { kingdom_id: 1, x: 600, y: 610, level: Some(61) }).await.unwrap();
    let unknown = RbcTarget { x: 700, y: 700, ..cooling.clone() };
    store
        .refresh_rbc_cooldowns(
            "ventrilo",
            &[RbcTarget { cooldown_remaining_s: 0, ..cooling }, unknown],
            NOW + 1_000,
        )
        .await
        .unwrap();
    assert!(pick(NOW + 1_000).await.is_some(), "the server now says ready");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rbc_target WHERE x = 700")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn advanced_mode_is_biased_to_near_towers_even_when_far_ones_are_staler() {
    use crate::{planning::TargetAlgorithm, targeting::Selector};
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let tower = |x: i64, y: i64| RbcTarget {
        kingdom_id: 1,
        x,
        y,
        level: Some(61),
        cooldown_remaining_s: 0,
    };
    // Source (593, 613). Distances: 5, 7.6 (the tower `seed_account` adds at
    // 600, 610), 14.8, 29.5, 54, 88.
    let layout = [(596, 617), (600, 600), (620, 625), (640, 640), (650, 680)];
    store
        .upsert_rbc_targets("ventrilo", &layout.map(|(x, y)| tower(x, y)), NOW)
        .await
        .unwrap();
    // The far ones were attacked long ago, the near ones recently; the old order
    // would have started with the farthest.
    for (index, (x, y)) in layout.iter().enumerate() {
        store
            .mark_target_attacked(
                "ventrilo",
                &ReservedTarget { kingdom_id: 1, x: *x, y: *y, level: Some(61) },
                NOW - 100_000_000 * (index as i64 + 1),
            )
            .await
            .unwrap();
    }
    let mut selector = Selector::new(42);
    let mut counts = std::collections::HashMap::new();
    for _ in 0..300 {
        let pick = store
            .reserve_rbc_target_with(
                "ventrilo", 1, Some(61), Some(61), (593, 613), TargetAlgorithm::Advanced,
                &mut selector, NOW, NOW + 720_000,
            )
            .await
            .unwrap()
            .pick
            .expect("a tower is ready");
        *counts.entry((pick.target.x, pick.target.y)).or_insert(0_u32) += 1;
        store.release_rbc_target("ventrilo", &pick.target).await.unwrap();
    }
    let share = |at: (i64, i64)| counts.get(&at).copied().unwrap_or(0);
    let near = share((596, 617)) + share((600, 610));
    let far = share((640, 640)) + share((650, 680));
    assert!(near > far * 6, "near {near} vs far {far}: {counts:?}");
    assert!(far > 0 || near > 280, "the far ones are rare, not forbidden");
    // The selector's geography is only moved by `accept`, never by reserving.
    assert_eq!(selector.state.active_kingdom, Some(1));
    assert!(selector.state.spotlight == (593.0, 613.0));

    // `closest` is still exact nearest-first.
    let mut taken = Vec::new();
    for _ in 0..=layout.len() {
        let target = store
            .reserve_rbc_target(
                "ventrilo", 1, Some(61), Some(61), (593, 613), "closest", NOW, NOW + 720_000,
            )
            .await
            .unwrap()
            .expect("another tower is ready");
        taken.push((target.x, target.y));
    }
    let mut expected = layout.to_vec();
    expected.insert(1, (600, 610));
    assert_eq!(taken, expected, "nearest first");
}

#[tokio::test]
async fn the_development_map_reads_towers_castle_and_open_marches_without_writing() {
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
    store
        .upsert_rbc_targets(
            "ventrilo",
            &[RbcTarget { kingdom_id: 1, x: 640, y: 640, level: Some(50), cooldown_remaining_s: 1_000 }],
            NOW,
        )
        .await
        .unwrap();
    assert_eq!(store.main_castle("Ventrilo", 1).await.unwrap(), Some((593, 613)));
    assert_eq!(store.main_castle("ventrilo", 3).await.unwrap(), None);

    let towers = store.map_towers("ventrilo", 1).await.unwrap();
    assert_eq!(towers.len(), 2, "the seeded tower and the cooling one");
    let cooling = towers.iter().find(|tower| tower.x == 640).unwrap();
    assert_eq!(cooling.ready_at_ms, NOW + 1_002_000, "ready time includes the server cooldown");
    assert_eq!(towers.iter().find(|tower| tower.x == 600).unwrap().ready_at_ms, 0);
    assert!(store.map_towers("ventrilo", 2).await.unwrap().is_empty());

    // One outbound, one returning, one finished, and one old row that never reported.
    let mut outbound = march(Some("t"), 1, 7);
    outbound.sent_at_ms = NOW;
    outbound.duration_s = Some(300);
    let mut returning = march(Some("t"), 2, 8);
    returning.sent_at_ms = NOW - 600_000;
    returning.status = MARCH_RETURNING.into();
    returning.landed_at_ms = Some(NOW - 100_000);
    returning.duration_s = Some(280);
    let mut done = march(Some("t"), 3, 9);
    done.status = "done".into();
    let mut stale = march(Some("t"), 4, 10);
    stale.sent_at_ms = NOW - 10 * 3_600_000;
    for row in [&outbound, &returning, &done, &stale] {
        store.record_march(row).await.unwrap();
    }
    let movements = store
        .open_movements("ventrilo", 1, NOW - 4 * 3_600_000, NOW + 50_000, 50)
        .await
        .unwrap();
    assert_eq!(movements.iter().map(|m| m.march_id).collect::<Vec<_>>(), vec![1, 2], "newest first, finished and stale rows left out");
    assert_eq!(movements[0].phase, MovementPhase::Outbound);
    assert_eq!(movements[0].end_ms, Some(NOW + 300_000));
    assert_eq!(movements[1].phase, MovementPhase::Returning);
    assert_eq!((movements[1].start_ms, movements[1].end_ms), (NOW - 100_000, Some(NOW + 180_000)));
    assert_eq!(movement_progress(movements[1].start_ms, movements[1].end_ms, NOW + 40_000), Some(0.5));
}

#[tokio::test]
async fn an_abandoned_lease_is_released_back_to_the_pool() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    let reserve = || async {
        store
            .reserve_rbc_target(
                "ventrilo", 1, Some(61), Some(61), (593, 613), "advanced", NOW, NOW + 720_000,
            )
            .await
            .unwrap()
    };
    let target = reserve().await.expect("seeded tower is free");
    assert!(reserve().await.is_none(), "leased");
    store.release_rbc_target("ventrilo", &target).await.unwrap();
    assert!(reserve().await.is_some(), "released");
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
                cooldown_remaining_s: 0,
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

/// Give an account a fortress task for one kingdom.
///
/// The runner only walks and attacks kingdoms it has a fortress task for, so
/// anything that expects fortress work has to set this up first. Tasks are
/// global objects here; a mode is what binds one to an account.
async fn seed_fortress_task(store: &Store, kingdom_id: i64) {
    store
        .upsert_task(
            &TaskRecord {
                task_id: "fortress-task".into(),
                name: "Fortress".into(),
                kind: "attack".into(),
                kingdom_id,
                profile_id: None,
                target_level_min: None,
                target_level_max: None,
                commander_count: 5,
                max_active: None,
                priority: 10,
                enabled: true,
                tags: vec!["fortress".into()],
                notes: String::new(),
            },
            NOW,
        )
        .await
        .unwrap();
    store
        .replace_subscriptions(
            "fortress-task",
            &[SubscriptionRecord {
                task_id: "fortress-task".into(),
                target_kind: "fortress".into(),
                filter: json!({"kingdom_id": kingdom_id}),
            }],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn the_dashboard_counts_down_to_the_next_fortress_window() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    seed_fortress_task(&store, 1).await;
    // Two fortresses: one free in five minutes, one in three hours.
    store
        .upsert_fortress_targets(
            "ventrilo",
            &[
                FortressTarget {
                    kingdom_id: 1,
                    x: 594,
                    y: 594,
                    level: 45,
                    cooldown_remaining_s: 300,
                    occupier_player_id: 0,
                },
                FortressTarget {
                    kingdom_id: 1,
                    x: 600,
                    y: 600,
                    level: 45,
                    cooldown_remaining_s: 10_800,
                    occupier_player_id: 0,
                },
            ],
            NOW,
        )
        .await
        .unwrap();

    let summary = store.dashboard_summary_for(Some("ventrilo")).await.unwrap();
    assert_eq!(summary.fortress_count, 2);
    assert_eq!(
        summary.fortress_next_available_at_ms,
        Some(NOW + 300_000),
        "the soonest window is the one the dashboard counts down to"
    );

    // Cooldowns belong to the account that observed them.
    let other = store.dashboard_summary_for(Some("pingpoko")).await.unwrap();
    assert_eq!(other.fortress_count, 0);
    assert_eq!(other.fortress_next_available_at_ms, None);
}

#[tokio::test]
async fn the_dashboard_ignores_fortresses_in_untasked_kingdoms() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    seed_fortress_task(&store, 1).await;
    // A fortress observed while the map happened to be open in Ice. The runner
    // has no Ice task, so it is not something the dashboard may advertise.
    store
        .upsert_fortress_targets(
            "ventrilo",
            &[FortressTarget {
                kingdom_id: 2,
                x: 692,
                y: 575,
                level: 55,
                cooldown_remaining_s: 0,
                occupier_player_id: 1,
            }],
            NOW,
        )
        .await
        .unwrap();
    let summary = store.dashboard_summary_for(Some("ventrilo")).await.unwrap();
    assert_eq!(
        summary.fortress_count, 0,
        "an Ice fortress is not a Sands target"
    );
    assert_eq!(summary.fortress_next_available_at_ms, None);
}

#[tokio::test]
async fn a_fortress_mode_queues_a_cooldown_read_for_every_known_fortress() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store
        .upsert_fortress_targets(
            "ventrilo",
            &[
                FortressTarget {
                    kingdom_id: 1,
                    x: 594,
                    y: 594,
                    level: 45,
                    cooldown_remaining_s: 30_000,
                    occupier_player_id: 1,
                },
                FortressTarget {
                    kingdom_id: 1,
                    x: 614,
                    y: 614,
                    level: 45,
                    cooldown_remaining_s: 0,
                    occupier_player_id: 1,
                },
                FortressTarget {
                    kingdom_id: 3,
                    x: 692,
                    y: 575,
                    level: 55,
                    cooldown_remaining_s: 30_000,
                    occupier_player_id: 1,
                },
            ],
            NOW - 7 * 3_600_000,
        )
        .await
        .unwrap();
    // Fortresses nobody has looked at for hours are queued, so a mode starts
    // from real state rather than from a timer another player may have reset.
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 1, NOW + 1_000)
            .await
            .unwrap(),
        2
    );
    // A second pass changes nothing, because the queue is already populated.
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 1, NOW + 2_000)
            .await
            .unwrap(),
        0
    );
    // The kingdom with no fortress task was left alone.
    assert_eq!(
        store
            .queue_fortress_recheck("ventrilo", 3, NOW + 2_000)
            .await
            .unwrap(),
        1
    );
    // The runner can now drain what was queued instead of guessing.
    assert!(
        store
            .reserve_due_fortress_refresh("ventrilo", 1, NOW + 3_000)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn kingdom_health_counts_fortresses_per_kingdom() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile("ventrilo", "Ventrilo", "wss://example/", "EmpireEx_21", NOW)
        .await
        .unwrap();
    store
        .replace_account_bootstrap(
            "ventrilo",
            &[
                OwnedCastle {
                    kingdom_id: 0,
                    castle_id: 1,
                    area_type: 1,
                    x: 509,
                    y: 405,
                    name: "Green".into(),
                },
                OwnedCastle {
                    kingdom_id: 1,
                    castle_id: 2,
                    area_type: 12,
                    x: 593,
                    y: 613,
                    name: "Sands".into(),
                },
                OwnedCastle {
                    kingdom_id: 3,
                    castle_id: 3,
                    area_type: 12,
                    x: 688,
                    y: 582,
                    name: "Fire".into(),
                },
            ],
            &[11, 22],
            NOW,
        )
        .await
        .unwrap();
    seed_fortress_task(&store, 3).await;
    for (kingdom_id, level) in [(1, 45), (3, 55)] {
        store
            .upsert_fortress_targets(
                "ventrilo",
                &[FortressTarget {
                    kingdom_id,
                    x: 594,
                    y: 594,
                    level,
                    cooldown_remaining_s: 100,
                    occupier_player_id: 1,
                }],
                NOW,
            )
            .await
            .unwrap();
        store
            .start_fortress_scan("ventrilo", kingdom_id, 9)
            .await
            .unwrap();
    }

    let accounts = store.account_summaries().await.unwrap();
    let health = &accounts[0].kingdom_health;
    let row = |kingdom_id: i64| {
        health
            .iter()
            .find(|entry| entry.kingdom_id == kingdom_id)
            .expect("every owned permanent kingdom has a row")
    };
    assert_eq!(row(1).fortress_count, 1);
    assert_eq!(row(3).fortress_count, 1);
    // Green has no fortresses at all, so this zero is a fact about the kingdom
    // rather than a scan that never ran.
    assert_eq!(row(0).fortress_count, 0);
    // Initialization walks every owned outer kingdom, independently of which
    // kingdom later receives a fortress attack task.
    assert!(row(3).fortress_task);
    assert!(!row(1).fortress_task);
    assert!(row(1).fortress_probes_pending > 0);
    assert_eq!(row(0).fortress_probes_pending, 0);
}
