//! The store's connection pools: one writer, a few readers.
//!
//! SQLite in WAL mode allows any number of readers alongside one writer, and a
//! reader never blocks the writer (or the other way round). The store used to
//! have a single connection for everything, so one slow read - the map view, the
//! castle overview - held the only connection while every live session queued
//! behind it. A session that waited past the 30 s acquire timeout was dropped
//! with `pool timed out while waiting for an open connection`
//! (`docs/problems/21.md`, `docs/problems/22.md`).
//!
//! * **Writer** - exactly one connection. Every statement that changes data goes
//!   through it, which also keeps SQLite from ever answering `SQLITE_BUSY` to
//!   the transactions that read first and write second.
//! * **Readers** - [`READ_CONNECTIONS`] connections opened `query_only`, so a
//!   read that was routed here by mistake fails loudly instead of writing.
//!   They connect on first use, after the schema exists.
//!
//! A database with no file (`sqlite::memory:`) cannot be shared between
//! connections, so there the reader *is* the writer and nothing changes.
//!
//! sqlx already logs any statement slower than one second at `WARN`
//! (target `sqlx::query`); that line names the SQL, which is what was missing
//! when the connection was last held for over 30 s.

use std::{str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous},
};

/// Concurrent read connections. Small on purpose: the service has a handful of
/// windows and sessions, and every connection carries its own page cache.
pub(super) const READ_CONNECTIONS: u32 = 4;

/// How long SQLite itself waits on a lock held by another connection or process
/// (`empire-hunt` can open the same file) before reporting `SQLITE_BUSY`.
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct Pools {
    pub(super) writer: SqlitePool,
    pub(super) reader: SqlitePool,
}

/// Open both pools. `file_backed` is false for `sqlite::memory:`.
pub(super) async fn open(url: &str, file_backed: bool) -> Result<Pools, sqlx::Error> {
    // `NORMAL` is the documented pairing for WAL: a crash of the app cannot
    // corrupt the file or lose a committed write; only a power cut can drop the
    // last few commits, which the game state re-learns. It removes the sync on
    // every packet that `FULL` (sqlx's default) performs.
    let writer_options = SqliteConnectOptions::from_str(url)?
        .busy_timeout(BUSY_TIMEOUT)
        .synchronous(SqliteSynchronous::Normal);
    let writer = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(writer_options.clone())
        .await?;
    if !file_backed {
        return Ok(Pools {
            reader: writer.clone(),
            writer,
        });
    }
    let reader_options = writer_options
        .create_if_missing(false)
        .pragma("query_only", "1");
    let reader = SqlitePoolOptions::new()
        .max_connections(READ_CONNECTIONS)
        .connect_lazy_with(reader_options);
    Ok(Pools { writer, reader })
}
