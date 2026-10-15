//! The portal account ID (`AID` in the `lli` login) is a property of whoever signs in, not of the
//! build, so it is learned on this machine instead of being compiled in.
//!
//! The first sign-in may supply it; it is then remembered per account in `app_state`, and later
//! reconnects (which only carry the username and password) reuse it. Nothing supplied and nothing
//! remembered generates a fresh 19-digit one, kept from then on.
//!
//! pygge sends one fixed, unrelated `AID` for every account's password login and the server
//! accepts it, so the value need not belong to the account; it only needs the portal id's shape.

use empire_core::{pacing::Rng, store::Store};
use serde_json::Value;

fn key(account_id: &str) -> String {
    format!("portal_account_id.{account_id}")
}

/// A portal-shaped id: 19 digits starting `17`, like the ids the game's portal hands out.
fn generate() -> String {
    format!("17{:017}", Rng::from_entropy().next_u64() % 100_000_000_000_000_000)
}

/// The ID to send for `account_id`: the one just supplied, else the one remembered, else new.
pub(super) async fn resolve(
    store: &Store,
    account_id: &str,
    supplied: Option<&str>,
) -> Result<String, sqlx::Error> {
    if let Some(supplied) = supplied.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(supplied.to_owned());
    }
    Ok(store
        .app_state(&key(account_id))
        .await?
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(generate))
}

/// Keep a non-empty ID for the account's later reconnects.
pub(super) async fn remember(
    store: &Store,
    account_id: &str,
    portal_account_id: &str,
    now_ms: i64,
) -> Result<(), sqlx::Error> {
    if portal_account_id.is_empty() {
        return Ok(());
    }
    store
        .set_app_state(&key(account_id), &Value::String(portal_account_id.to_owned()), now_ms)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_first_sign_in_teaches_the_machine_and_a_reconnect_reuses_it() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let generated = resolve(&store, "alice", Some("  ")).await.unwrap();
        assert_eq!(generated.len(), 19);
        assert!(generated.starts_with("17") && generated.bytes().all(|b| b.is_ascii_digit()));

        let learned = resolve(&store, "alice", Some(" 42 ")).await.unwrap();
        assert_eq!(learned, "42");
        remember(&store, "alice", &learned, 1).await.unwrap();

        assert_eq!(resolve(&store, "alice", None).await.unwrap(), "42");
        assert_eq!(resolve(&store, "alice", Some("7")).await.unwrap(), "7");
        assert_eq!(resolve(&store, "bob", None).await.unwrap().len(), 19);
    }

    #[tokio::test]
    async fn an_empty_id_never_overwrites_a_remembered_one() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        remember(&store, "alice", "42", 1).await.unwrap();
        remember(&store, "alice", "", 2).await.unwrap();
        assert_eq!(resolve(&store, "alice", None).await.unwrap(), "42");
    }

    #[tokio::test]
    async fn a_generated_id_is_kept_across_reconnects() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let first = resolve(&store, "alice", None).await.unwrap();
        remember(&store, "alice", &first, 1).await.unwrap();
        assert_eq!(resolve(&store, "alice", None).await.unwrap(), first);
    }
}
