//! Reads every castle's stock with `dcl` when the Overview asks for it.
//!
//! The window never sends game packets. It writes a request time into the
//! application state (`POST /v1/overview/refresh`); this notices it, waits a short
//! human-paced moment, and sends one `dcl` when nothing else is mid-sequence. The
//! reply is stored by the station component (`station::on_dcl`), which also uses
//! it before a support send, so one reading serves both. Never more often than
//! [`MIN_GAP_MS`].

use super::*;

/// The application-state key the API writes an account's refresh request into. Each
/// account has its own, so one account asking never makes another read its castles.
pub fn overview_refresh_key(account_id: &str) -> String {
    format!("overview.refresh_requested_ms.{}", account_id.trim().to_ascii_lowercase())
}
/// No two readings closer than this, however often the window asks.
const MIN_GAP_MS: i64 = 15_000;
const CHECK_EVERY_MS: i64 = 1_500;

pub(super) struct InventoryPoll {
    key: String,
    /// The request time already acted on.
    handled_ms: i64,
    checked_ms: i64,
    last_sent_ms: i64,
    due_ms: Option<i64>,
    rng: Rng,
}

impl InventoryPoll {
    pub(super) fn new(account_id: &str) -> Self {
        Self {
            key: overview_refresh_key(account_id),
            handled_ms: 0,
            checked_ms: i64::MIN / 2,
            last_sent_ms: i64::MIN / 2,
            due_ms: None,
            rng: Rng::from_entropy(),
        }
    }

    /// The `dcl` to send now, if a refresh was asked for, its moment has come and
    /// the connection is free (`allowed`).
    pub(super) async fn next_packet(
        &mut self,
        store: &Store,
        server_header: &str,
        now: i64,
        allowed: bool,
    ) -> anyhow::Result<Option<String>> {
        if self.due_ms.is_none() && now - self.checked_ms >= CHECK_EVERY_MS {
            self.checked_ms = now;
            let requested = store
                .app_state(&self.key)
                .await?
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            if requested > self.handled_ms {
                self.handled_ms = requested;
                let pause = (self.rng.uniform(800.0, 2_500.0)) as i64;
                self.due_ms = Some((now + pause).max(self.last_sent_ms + MIN_GAP_MS));
            }
        }
        match self.due_ms {
            Some(due) if allowed && now >= due => {
                self.due_ms = None;
                self.last_sent_ms = now;
                Ok(Some(empire_core::station::dcl_packet(server_header)?))
            }
            _ => Ok(None),
        }
    }
}
