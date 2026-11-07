//! The writer/reader split (`pools.rs`) and the speed fixes of `docs/problems/22.md`.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::Row;

use super::{MarchRecord, Store, dev::MAP_TOWERS_SQL, pools::READ_CONNECTIONS};

/// A database file that is removed again, with its `-wal` and `-shm`, when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("openauto-pools-{tag}-{}-{nanos}.sqlite3", std::process::id()));
        Self(path)
    }

    fn url(&self) -> String {
        format!("sqlite://{}?mode=rwc", self.0.display())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut candidate = self.0.as_os_str().to_owned();
            candidate.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(candidate));
        }
    }
}

async fn open(scratch: &Scratch) -> Store {
    let store = Store::open(&scratch.url()).await.expect("open database");
    store
        .upsert_account_profile("ventrilo", "Ventrilo", "wss://example/", "EmpireEx_21", 1)
        .await
        .unwrap();
    store
}

const WAIT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn a_read_connection_sees_what_the_writer_committed() {
    let scratch = Scratch::new("visible");
    let store = open(&scratch).await;
    let world = store.account_world("Ventrilo").await.unwrap();
    assert_eq!(world.map(|world| world.player_name), Some("Ventrilo".to_owned()));
}

#[tokio::test]
async fn the_read_pool_refuses_to_write() {
    let scratch = Scratch::new("readonly");
    let store = open(&scratch).await;
    let attempt = sqlx::query("CREATE TABLE should_not_exist (x INTEGER)")
        .execute(store.reader())
        .await;
    assert!(attempt.is_err(), "a write sent to the read pool must fail, not succeed");
}

#[tokio::test]
async fn writes_do_not_wait_for_busy_readers() {
    let scratch = Scratch::new("readers-busy");
    let store = open(&scratch).await;
    let mut held = Vec::new();
    for _ in 0..READ_CONNECTIONS {
        held.push(store.reader().acquire().await.unwrap());
    }

    // Every read connection is taken, so one more read has to wait...
    let extra_read = tokio::time::timeout(Duration::from_millis(200), store.account_world("ventrilo")).await;
    assert!(extra_read.is_err(), "the readers should be exhausted");
    // ...but the writer is a separate connection and goes straight through.
    tokio::time::timeout(WAIT, store.set_app_state("probe", &serde_json::json!(1), 1))
        .await
        .expect("a write must not queue behind reads")
        .unwrap();
    drop(held);
}

#[tokio::test]
async fn reads_do_not_wait_for_a_busy_writer() {
    let scratch = Scratch::new("writer-busy");
    let store = open(&scratch).await;

    // A write transaction that stays open, with its change not yet committed.
    let mut tx = store.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app_state (key, value_json, updated_at_ms) VALUES ('pending', '1', 1)")
        .execute(&mut *tx)
        .await
        .unwrap();

    let during = tokio::time::timeout(WAIT, store.app_state("pending"))
        .await
        .expect("a read must not queue behind the writer")
        .unwrap();
    assert!(during.is_none(), "an uncommitted write must not be visible");

    tx.commit().await.unwrap();
    let after = store.app_state("pending").await.unwrap();
    assert_eq!(after, Some(serde_json::json!(1)));
}

#[tokio::test]
async fn an_in_memory_store_reads_through_its_only_connection() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile("ventrilo", "Ventrilo", "wss://example/", "EmpireEx_21", 1)
        .await
        .unwrap();
    assert!(store.account_world("ventrilo").await.unwrap().is_some());
}

#[tokio::test]
async fn the_map_query_finds_marches_by_index_not_by_scanning_the_ledger() {
    let scratch = Scratch::new("plan");
    let store = open(&scratch).await;
    let plan = sqlx::query(&format!("EXPLAIN QUERY PLAN {MAP_TOWERS_SQL}"))
        .bind("ventrilo")
        .bind(1_i64)
        .fetch_all(store.reader())
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        plan.contains("SEARCH a USING INDEX attack_ledger_target_idx"),
        "the per-tower ledger lookup must use attack_ledger_target_idx; plan was:\n{plan}"
    );
    assert!(
        !plan.contains("SCAN a"),
        "the ledger must never be scanned once per tower; plan was:\n{plan}"
    );
}

#[tokio::test]
async fn a_recorded_march_is_stored_under_the_canonical_account_id() {
    let scratch = Scratch::new("march-case");
    let store = open(&scratch).await;
    store
        .record_march(&MarchRecord {
            account_id: "Ventrilo".into(),
            march_id: 1,
            kingdom_id: 1,
            x: 10,
            y: 20,
            task_id: None,
            profile_id: None,
            level: None,
            lord_id: None,
            commander_number: None,
            troop_count: None,
            duration_s: None,
            coin_loot: None,
            ruby_loot: None,
            status: "sent".into(),
            result_flag: None,
            error_message: None,
            sent_at_ms: 1,
            landed_at_ms: None,
            result_at_ms: None,
        })
        .await
        .unwrap();
    let stored: String = sqlx::query_scalar("SELECT account_id FROM attack_ledger WHERE march_id = 1")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(stored, "ventrilo");
}

#[tokio::test]
async fn migration_28_brings_ledger_account_ids_into_canonical_form() {
    let scratch = Scratch::new("v28");
    {
        let store = open(&scratch).await;
        for (account, march) in [("Ventrilo", 1_i64), ("ventrilo", 2), ("VENTRILO", 2)] {
            sqlx::query(
                "INSERT INTO attack_ledger (account_id, march_id, kingdom_id, x, y, sent_at_ms)
                 VALUES (?, ?, 1, 1, 1, 1)",
            )
            .bind(account)
            .bind(march)
            .execute(&store.pool)
            .await
            .unwrap();
        }
        // Rewind, as a database that has not seen V28 yet.
        sqlx::query("UPDATE schema_version SET version = 27").execute(&store.pool).await.unwrap();
        store.pool.close().await;
    }

    let store = Store::open(&scratch.url()).await.unwrap();
    let rows: Vec<(String, i64)> =
        sqlx::query("SELECT account_id, march_id FROM attack_ledger ORDER BY march_id, account_id")
            .fetch_all(store.reader())
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.get("account_id"), row.get("march_id")))
            .collect();
    // March 1 is renamed. March 2 already existed under the canonical id, so the
    // other-cased twin is left exactly as it was rather than overwrite either row.
    assert_eq!(
        rows,
        vec![
            ("ventrilo".to_owned(), 1),
            ("VENTRILO".to_owned(), 2),
            ("ventrilo".to_owned(), 2),
        ]
    );
}

/// Two sessions recording packets and state while several views read, all at once:
/// nothing may fail with `database is locked` or time out waiting for a connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sessions_and_views_run_together_without_errors() {
    let scratch = Scratch::new("together");
    let store = open(&scratch).await;
    store
        .upsert_account_profile("pingpoko", "Pingpoko", "wss://example/", "EmpireEx_49", 1)
        .await
        .unwrap();

    let writers = ["ventrilo", "pingpoko"].map(|account| {
        let store = store.clone();
        tokio::spawn(async move {
            for i in 0..300_i64 {
                store
                    .record_message_for(
                        Some(account),
                        1_000 + i,
                        crate::event::Direction::ServerToClient,
                        Some("gaa"),
                        &serde_json::json!({ "n": i }),
                    )
                    .await?;
                store.set_app_state(&format!("{account}.cursor"), &serde_json::json!(i), i).await?;
            }
            Ok::<_, sqlx::Error>(())
        })
    });
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let readers = (0..3).map(|_| {
        let store = store.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            let mut reads = 0_u32;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                store.account_world("ventrilo").await?;
                store.recent_messages_for(Some("pingpoko"), 50).await?;
                store.app_state("ventrilo.cursor").await?;
                reads += 1;
            }
            Ok::<_, sqlx::Error>(reads)
        })
    });
    let readers: Vec<_> = readers.collect();

    for writer in writers {
        writer.await.unwrap().expect("a session write failed");
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for reader in readers {
        let reads = reader.await.unwrap().expect("a view read failed");
        assert!(reads > 0, "every view should have managed at least one read");
    }
    assert_eq!(store.app_state("pingpoko.cursor").await.unwrap(), Some(serde_json::json!(299)));
}
