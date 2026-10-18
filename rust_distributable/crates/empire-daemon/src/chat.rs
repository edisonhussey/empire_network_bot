//! HTTP side of the Chat tab: the alliance messages the account's session has heard.
//!
//! Read-only. The listener is `direct/chat.rs`; the window polls this while the tab is open.

use axum::{
    Json,
    extract::{Query, State},
};
use empire_core::store::ChatRow;
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState};

/// Enough for a day of a busy alliance; the store keeps 2,000.
const DEFAULT_LIMIT: i64 = 300;

#[derive(Debug, Deserialize)]
pub struct ChatParams {
    account_id: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    account_id: String,
    connected: bool,
    /// Oldest first.
    messages: Vec<ChatRow>,
}

pub async fn chat(
    State(state): State<AppState>,
    Query(params): Query<ChatParams>,
) -> Result<Json<ChatResponse>, ApiError> {
    let account_id = crate::focus_account(&state, params.account_id).await?;
    let connected = state.sessions.status(&account_id).await.is_some_and(|status| status.connected);
    let messages = state.store.recent_chat(&account_id, params.limit.unwrap_or(DEFAULT_LIMIT)).await?;
    Ok(Json(ChatResponse { account_id, connected, messages }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use empire_core::chat::ChatMessage;
    use empire_core::store::Store;
    use tokio::sync::RwLock;

    use super::*;
    use crate::sessions::Sessions;

    async fn app() -> AppState {
        let store = Store::open("sqlite::memory:").await.unwrap();
        for account in ["ventrilo", "pingpoko"] {
            store.upsert_account_profile(account, account, "wss://x/", "EmpireEx_21", 1).await.unwrap();
        }
        AppState {
            licence: crate::licence::LicenceGate::new(store.clone()).unwrap(),
            store,
            sessions: Sessions::default(),
            relay_transport: Arc::new(RwLock::new(None)),
        }
    }

    fn said(name: &str, text: &str) -> ChatMessage {
        ChatMessage { player_id: 1, player_name: name.to_owned(), text: text.to_owned(), age_s: 0 }
    }

    fn params(account: &str, limit: Option<i64>) -> Query<ChatParams> {
        Query(ChatParams { account_id: Some(account.to_owned()), limit })
    }

    #[tokio::test]
    async fn each_account_sees_only_its_own_alliance_chat_oldest_first() {
        let state = app().await;
        state.store.record_chat("ventrilo", &[said("Dawn Reaver", "Can I get rage bonus please")], 1_000_000).await.unwrap();
        state.store.record_chat("ventrilo", &[said("Kozz", "yes")], 1_060_000).await.unwrap();
        state.store.record_chat("pingpoko", &[said("Someone", "elsewhere")], 1_000_000).await.unwrap();
        let Json(mine) = chat(State(state.clone()), params("ventrilo", None)).await.unwrap();
        assert_eq!(mine.messages.iter().map(|m| m.player_name.as_str()).collect::<Vec<_>>(), vec!["Dawn Reaver", "Kozz"]);
        assert!(!mine.connected);
        let Json(theirs) = chat(State(state.clone()), params("pingpoko", None)).await.unwrap();
        assert_eq!(theirs.messages.len(), 1);
        let Json(few) = chat(State(state), params("ventrilo", Some(1))).await.unwrap();
        assert_eq!(few.messages[0].text, "yes", "a limit keeps the newest");
    }
}
