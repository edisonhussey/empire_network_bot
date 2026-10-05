//! Storage footprint and the two reset levels.
//!
//! The database lives in the per-user application-data directory that the host
//! resolves (Tauri on macOS and Windows), so it is removed by the same tooling
//! that manages other application state. Because drag-installed apps on macOS
//! have no uninstall hook, these operations are the honest way to reclaim space.

use serde::{Deserialize, Serialize};

use super::{Store, canonical_account_id, schema};

/// Row count for one table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableFootprint {
    pub table: String,
    pub rows: i64,
}

/// What the local database currently costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageReport {
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub tables: Vec<TableFootprint>,
    pub schema_version: i64,
}

impl StorageReport {
    /// Total bytes on disk, including the write-ahead log.
    pub fn total_bytes(&self) -> u64 {
        self.database_bytes + self.wal_bytes + self.shm_bytes
    }

    pub fn total_rows(&self) -> i64 {
        self.tables.iter().map(|table| table.rows).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PruneOutcome {
    pub ledger_rows: u64,
    pub message_rows: u64,
    pub vacuumed: bool,
}

/// Account-scoped tables, cleared by `reset_account`.
const ACCOUNT_TABLES: &[&str] = &[
    "attack_ledger",
    "commander_state",
    "account_castle_unit",
    "recruit_castle_state",
    "account_navigation",
    "fortress_scan_frontier",
    "fortress_target",
    "rbc_target",
    "account_commander",
    "owned_castle",
];

impl Store {
    pub async fn storage_report(&self) -> Result<StorageReport, sqlx::Error> {
        let mut tables = Vec::with_capacity(schema::DATA_TABLES.len());
        for table in schema::DATA_TABLES {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&self.pool)
                .await?;
            tables.push(TableFootprint {
                table: (*table).to_owned(),
                rows: count,
            });
        }
        let (database_bytes, wal_bytes, shm_bytes) = match self.path() {
            Some(path) => {
                let base = file_size(path);
                let wal = file_size(&path.with_extension("sqlite3-wal"));
                let shm = file_size(&path.with_extension("sqlite3-shm"));
                (base, wal, shm)
            }
            None => (0, 0, 0),
        };
        Ok(StorageReport {
            database_bytes,
            wal_bytes,
            shm_bytes,
            tables,
            schema_version: schema::version(&self.pool).await?,
        })
    }

    /// Deletes ledger and message history older than `older_than_days`.
    ///
    /// Targets, castles, commanders and configuration are never pruned: they are
    /// what a resumed run needs. Reclaims disk space with a `VACUUM` when rows
    /// were actually removed.
    pub async fn prune(
        &self,
        older_than_days: i64,
        now_ms: i64,
    ) -> Result<PruneOutcome, sqlx::Error> {
        let cutoff = now_ms - older_than_days.max(0).saturating_mul(86_400_000);
        let ledger_rows = sqlx::query("DELETE FROM attack_ledger WHERE sent_at_ms < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?
            .rows_affected();
        let message_rows = sqlx::query("DELETE FROM network_message WHERE observed_at_ms < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?
            .rows_affected();
        let vacuumed = ledger_rows + message_rows > 0;
        if vacuumed {
            self.vacuum().await?;
        }
        Ok(PruneOutcome {
            ledger_rows,
            message_rows,
            vacuumed,
        })
    }

    /// Forgets everything learned about one account, keeping the account itself
    /// so it can simply be signed in again.
    pub async fn reset_account(&self, account_id: &str) -> Result<(), sqlx::Error> {
        // Identity is case-insensitive; see `canonical_account_id`.
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        for table in ACCOUNT_TABLES {
            sqlx::query(&format!("DELETE FROM {table} WHERE account_id = ?"))
                .bind(&account_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE account_profile SET initialized_at_ms = NULL WHERE account_id = ?")
            .bind(&account_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.vacuum().await
    }

    /// Removes every stored account, including the licence, returning the
    /// application to first-run. The schema itself is kept.
    pub async fn wipe_all(&self) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for table in schema::DATA_TABLES {
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.vacuum().await
    }

    async fn vacuum(&self) -> Result<(), sqlx::Error> {
        sqlx::query("VACUUM").execute(&self.pool).await?;
        Ok(())
    }
}

fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}
