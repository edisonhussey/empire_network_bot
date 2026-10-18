//! Stored alliance chat. Parsing is `crate::chat`; the listener is `empire-daemon/src/direct/chat.rs`.

use serde::Serialize;
use sqlx::Row;

use super::{Store, canonical_account_id};
use crate::chat::ChatMessage;

/// Messages kept per account; older ones are deleted as new ones arrive.
pub const CHAT_KEEP: i64 = 2_000;
/// The same sender saying the same thing within this many milliseconds of an existing row is the
/// same message (a history read overlaps what was heard live, and `MA` is whole seconds).
const SAME_MESSAGE_WINDOW_MS: i64 = 5_000;

/// A stored message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatRow {
    pub id: i64,
    pub player_id: i64,
    pub player_name: String,
    pub text: String,
    pub sent_at_ms: i64,
    pub system: bool,
}

impl Store {
    /// Store what a packet said, skipping messages already held. Returns how many were new.
    pub async fn record_chat(
        &self,
        account_id: &str,
        messages: &[ChatMessage],
        received_at_ms: i64,
    ) -> Result<usize, sqlx::Error> {
        let account_id = canonical_account_id(account_id);
        let mut tx = self.pool.begin().await?;
        let mut added = 0;
        for message in messages {
            let sent_at = message.sent_at_ms(received_at_ms);
            let known: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM alliance_chat
                 WHERE account_id = ? AND player_id = ? AND text = ? AND ABS(sent_at_ms - ?) <= ?",
            )
            .bind(&account_id)
            .bind(message.player_id)
            .bind(&message.text)
            .bind(sent_at)
            .bind(SAME_MESSAGE_WINDOW_MS)
            .fetch_one(&mut *tx)
            .await?;
            if known > 0 {
                continue;
            }
            sqlx::query(
                "INSERT INTO alliance_chat (account_id, player_id, player_name, text, sent_at_ms, received_at_ms)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&account_id)
            .bind(message.player_id)
            .bind(&message.player_name)
            .bind(&message.text)
            .bind(sent_at)
            .bind(received_at_ms)
            .execute(&mut *tx)
            .await?;
            added += 1;
        }
        if added > 0 {
            sqlx::query(
                "DELETE FROM alliance_chat WHERE account_id = ? AND id <= (
                     SELECT id FROM alliance_chat WHERE account_id = ? ORDER BY id DESC LIMIT 1 OFFSET ?)",
            )
            .bind(&account_id)
            .bind(&account_id)
            .bind(CHAT_KEEP)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(added)
    }

    /// The newest `limit` messages, oldest first (the order a chat is read in).
    pub async fn recent_chat(&self, account_id: &str, limit: i64) -> Result<Vec<ChatRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, player_id, player_name, text, sent_at_ms FROM (
                 SELECT * FROM alliance_chat WHERE account_id = ? ORDER BY sent_at_ms DESC, id DESC LIMIT ?
             ) ORDER BY sent_at_ms, id",
        )
        .bind(canonical_account_id(account_id))
        .bind(limit.clamp(1, CHAT_KEEP))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let player_id: i64 = row.get("player_id");
                ChatRow {
                    id: row.get("id"),
                    player_id,
                    player_name: row.get("player_name"),
                    text: row.get("text"),
                    sent_at_ms: row.get("sent_at_ms"),
                    system: player_id < 0,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    async fn store() -> Store {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store.upsert_account_profile("ventrilo", "Ventrilo", "wss://x/", "EmpireEx_21", 1).await.unwrap();
        store.upsert_account_profile("other", "Other", "wss://x/", "EmpireEx_21", 1).await.unwrap();
        store
    }

    fn said(player: i64, name: &str, text: &str, age_s: i64) -> ChatMessage {
        ChatMessage { player_id: player, player_name: name.to_owned(), text: text.to_owned(), age_s }
    }

    #[tokio::test]
    async fn messages_come_back_oldest_first_with_the_time_they_were_written() {
        let store = store().await;
        store.record_chat("Ventrilo", &[said(1, "Dawn Reaver", "Can I get rage bonus please", 0)], NOW).await.unwrap();
        store.record_chat("ventrilo", &[said(2, "Kozz", "yes", 0)], NOW + 120_000).await.unwrap();
        // A history line from before both: written 10 minutes before the first.
        store.record_chat("ventrilo", &[said(3, "THE_ECLIPSE", "good morning", 600)], NOW + 130_000).await.unwrap();
        let rows = store.recent_chat("ventrilo", 50).await.unwrap();
        assert_eq!(
            rows.iter().map(|row| (row.player_name.as_str(), row.sent_at_ms)).collect::<Vec<_>>(),
            vec![("THE_ECLIPSE", NOW + 130_000 - 600_000), ("Dawn Reaver", NOW), ("Kozz", NOW + 120_000)]
        );
        assert!(rows.iter().all(|row| !row.system));
    }

    #[tokio::test]
    async fn a_history_read_that_overlaps_what_was_heard_adds_nothing() {
        let store = store().await;
        assert_eq!(store.record_chat("ventrilo", &[said(1, "Kozz", "on", 0)], NOW).await.unwrap(), 1);
        // The same line again from the history, 40 s later, now 40 s old: the same moment, +- a second.
        assert_eq!(store.record_chat("ventrilo", &[said(1, "Kozz", "on", 41)], NOW + 40_000).await.unwrap(), 0);
        // Same words a minute later are a new message.
        assert_eq!(store.record_chat("ventrilo", &[said(1, "Kozz", "on", 0)], NOW + 60_000).await.unwrap(), 1);
        // Same moment, someone else.
        assert_eq!(store.record_chat("ventrilo", &[said(2, "Dawn Reaver", "on", 0)], NOW + 60_000).await.unwrap(), 1);
        assert_eq!(store.recent_chat("ventrilo", 50).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn each_accounts_chat_is_its_own() {
        let store = store().await;
        store.record_chat("ventrilo", &[said(1, "Kozz", "hello", 0)], NOW).await.unwrap();
        store.record_chat("other", &[said(9, "Someone", "hi", 0)], NOW).await.unwrap();
        assert_eq!(store.recent_chat("ventrilo", 10).await.unwrap()[0].text, "hello");
        assert_eq!(store.recent_chat("other", 10).await.unwrap().len(), 1);
        assert!(store.recent_chat("nobody", 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_the_newest_messages_are_kept() {
        let store = store().await;
        let batch = (0..CHAT_KEEP + 25)
            .map(|index| said(1, "Kozz", &format!("line {index}"), 0))
            .collect::<Vec<_>>();
        store.record_chat("ventrilo", &batch, NOW).await.unwrap();
        let all = store.recent_chat("ventrilo", CHAT_KEEP + 500).await.unwrap();
        assert_eq!(all.len() as i64, CHAT_KEEP);
        assert_eq!(all.last().unwrap().text, format!("line {}", CHAT_KEEP + 24));
        assert_eq!(store.recent_chat("ventrilo", 3).await.unwrap().len(), 3, "the limit is the newest few");
    }

    #[tokio::test]
    async fn a_system_line_is_marked() {
        let store = store().await;
        store.record_chat("ventrilo", &[said(-1, "", "A member joined", 0)], NOW).await.unwrap();
        assert!(store.recent_chat("ventrilo", 5).await.unwrap()[0].system);
    }
}
