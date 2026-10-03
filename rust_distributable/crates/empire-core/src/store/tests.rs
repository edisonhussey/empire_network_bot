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
use crate::account::{OwnedCastle, RbcTarget};

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
        path.push(format!("openauto-{tag}-{}-{nanos}.sqlite3", std::process::id()));
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
                area_type: 1,
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

// ---------------------------------------------------------------------------
// message retention
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retention_is_bounded_to_recent_messages() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    for index in 0..205 {
        store
            .record_message(index, Direction::Injected, Some("gbl"), &json!({"i": index}))
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
    assert_eq!(open.len(), 1, "the resumed run must see the in-flight march");
    assert_eq!(open[0].march_id, 9001);
    assert_eq!(open[0].lord_id, Some(11));

    reopened
        .finish_march("ventrilo", 9001, MARCH_RETURNING, Some(0), Some(120), Some(3), NOW + 60_000)
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
    assert_eq!(store.commander_lids("ventrilo").await.unwrap(), vec![11, 22]);
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
    store.set_app_state("hunt.label", &json!("sand-crossbow-kunai"), NOW).await.unwrap();
    store.record_march(&march(Some("sand_kunai"), 1, 25)).await.unwrap();
    store.record_march(&march(None, 2, 0)).await.unwrap();

    let summary = store.hunt_summary(5).await.unwrap();
    assert_eq!(summary.label.as_deref(), Some("sand-crossbow-kunai"));
    assert_eq!(summary.marches, 2);

    let names: Vec<&str> = summary.tasks.iter().map(|task| task.task_id.as_str()).collect();
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
    assert!(!history.planned, "unlabelled rows are history, not a planned task");
    assert_eq!(history.marches, 1);
}

#[tokio::test]
async fn a_returned_march_is_counted_against_its_task() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    seed_account(&store, "ventrilo").await;
    store.record_march(&march(Some("sand_kunai"), 77, 25)).await.unwrap();

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
