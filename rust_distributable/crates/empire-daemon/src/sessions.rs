//! The live game sessions, one per account.
//!
//! Each session is its own task with its own socket, status, injection queue and
//! diagnostic snapshot (`direct::run` was always per account; only the service held a
//! single slot for it). Sessions are independent: connecting, failing or ending one
//! never touches another, and the same account connecting again replaces only its own.
//! Everything an account needs lives in the database keyed by account id, so sessions
//! share no state beyond that and the licence gate.

use std::{collections::HashMap, sync::Arc};

use empire_core::{injection::InjectionRequest, store::canonical_account_id};
use tokio::{
    sync::{Mutex, RwLock, mpsc},
    task::JoinHandle,
};

use crate::direct::{DevSnapshot, DirectStatus};

/// The shared cells one session writes and the service reads.
#[derive(Clone)]
pub struct SessionCells {
    pub status: Arc<RwLock<DirectStatus>>,
    pub transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    pub dev: Arc<RwLock<DevSnapshot>>,
}

impl SessionCells {
    fn new(account_id: &str) -> Self {
        Self {
            status: Arc::new(RwLock::new(DirectStatus {
                account_id: Some(account_id.to_owned()),
                ..DirectStatus::default()
            })),
            transport: Arc::new(RwLock::new(None)),
            dev: Arc::new(RwLock::new(DevSnapshot::default())),
        }
    }
}

struct Session {
    cells: SessionCells,
    task: JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct Sessions {
    inner: Arc<Mutex<HashMap<String, Session>>>,
}

impl Sessions {
    /// Start a session for `account_id`, ending any earlier one for the same account.
    /// `spawn` receives the cells to give the session and returns its task.
    pub async fn replace(
        &self,
        account_id: &str,
        spawn: impl FnOnce(SessionCells) -> JoinHandle<()>,
    ) -> SessionCells {
        let key = canonical_account_id(account_id);
        let cells = SessionCells::new(&key);
        let task = spawn(cells.clone());
        let previous = self
            .inner
            .lock()
            .await
            .insert(key, Session { cells: cells.clone(), task });
        if let Some(previous) = previous {
            previous.task.abort();
            *previous.cells.transport.write().await = None;
        }
        cells
    }

    /// End one account's session. `false` when it had none.
    pub async fn remove(&self, account_id: &str) -> bool {
        let session = self.inner.lock().await.remove(&canonical_account_id(account_id));
        match session {
            Some(session) => {
                session.task.abort();
                *session.cells.transport.write().await = None;
                true
            }
            None => false,
        }
    }

    pub async fn cells(&self, account_id: &str) -> Option<SessionCells> {
        self.inner
            .lock()
            .await
            .get(&canonical_account_id(account_id))
            .map(|session| session.cells.clone())
    }

    /// Every account that has a session, connected or failed, in a stable order.
    pub async fn all(&self) -> Vec<(String, SessionCells)> {
        let mut sessions = self
            .inner
            .lock()
            .await
            .iter()
            .map(|(account, session)| (account.clone(), session.cells.clone()))
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| a.0.cmp(&b.0));
        sessions
    }

    /// The status of one account's session, or `None` when it has none.
    pub async fn status(&self, account_id: &str) -> Option<DirectStatus> {
        let cells = self.cells(account_id).await?;
        let status = cells.status.read().await.clone();
        Some(status)
    }

    pub async fn statuses(&self) -> Vec<DirectStatus> {
        let mut statuses = Vec::new();
        for (_, cells) in self.all().await {
            statuses.push(cells.status.read().await.clone());
        }
        statuses
    }

    /// Accounts whose socket is open right now.
    pub async fn connected_accounts(&self) -> Vec<String> {
        let mut accounts = Vec::new();
        for (account, cells) in self.all().await {
            if cells.status.read().await.connected {
                accounts.push(account);
            }
        }
        accounts
    }

    /// The injection queue of one account, when its session is connected.
    // The binding releases the read guard before `cells` goes out of scope.
    #[allow(clippy::let_and_return)]
    pub async fn transport(&self, account_id: &str) -> Option<mpsc::Sender<InjectionRequest>> {
        let cells = self.cells(account_id).await?;
        let transport = cells.transport.read().await.clone();
        transport
    }

    pub async fn dev(&self, account_id: &str) -> Option<DevSnapshot> {
        let cells = self.cells(account_id).await?;
        let snapshot = cells.dev.read().await.clone();
        Some(snapshot)
    }

    /// The account to act for when none was named: the only connected one, else the
    /// one connected most recently.
    pub async fn default_account(&self) -> Option<String> {
        let mut best: Option<(i64, String)> = None;
        for (account, cells) in self.all().await {
            let status = cells.status.read().await;
            if !status.connected {
                continue;
            }
            let at = status.connected_at_ms.unwrap_or(0);
            if best.as_ref().is_none_or(|(newest, _)| at >= *newest) {
                best = Some((at, account));
            }
        }
        best.map(|(_, account)| account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn forever() -> JoinHandle<()> {
        tokio::spawn(async { tokio::time::sleep(Duration::from_secs(3_600)).await })
    }

    #[tokio::test]
    async fn sessions_are_independent_and_keyed_by_the_canonical_account() {
        let sessions = Sessions::default();
        let a = sessions.replace("Ventrilo", |_| forever()).await;
        sessions.replace("pingpoko", |_| forever()).await;
        a.status.write().await.connected = true;
        assert_eq!(sessions.all().await.len(), 2);
        assert_eq!(sessions.connected_accounts().await, vec!["ventrilo".to_owned()]);
        assert_eq!(
            sessions.status("VENTRILO").await.unwrap().account_id.as_deref(),
            Some("ventrilo"),
            "the account is known from the moment the session exists"
        );
        assert!(sessions.status("nobody").await.is_none());
    }

    #[tokio::test]
    async fn ending_one_session_leaves_the_others_running() {
        let sessions = Sessions::default();
        let first = sessions.replace("a", |_| forever()).await;
        let second = sessions.replace("b", |_| forever()).await;
        first.status.write().await.connected = true;
        second.status.write().await.connected = true;
        assert!(sessions.remove("A").await);
        assert!(!sessions.remove("a").await, "already gone");
        assert!(sessions.status("a").await.is_none());
        assert!(sessions.status("b").await.unwrap().connected);
    }

    #[tokio::test]
    async fn connecting_an_account_again_replaces_only_its_own_session() {
        let sessions = Sessions::default();
        let old = sessions.replace("a", |_| forever()).await;
        let other = sessions.replace("b", |_| forever()).await;
        let (tx, _rx) = mpsc::channel(1);
        *old.transport.write().await = Some(tx);
        *other.status.write().await = DirectStatus { connected: true, account_id: Some("b".into()), ..DirectStatus::default() };
        let fresh = sessions.replace("A", |_| forever()).await;
        assert!(old.transport.read().await.is_none(), "the old queue is closed");
        assert!(!Arc::ptr_eq(&old.status, &fresh.status));
        assert!(sessions.status("b").await.unwrap().connected, "the other account is untouched");
    }

    #[tokio::test]
    async fn a_session_that_would_reconnect_forever_stops_for_good_when_ended() {
        // Stands in for the supervisor: it retries in a loop until told otherwise.
        let ticks = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = ticks.clone();
        let sessions = Sessions::default();
        sessions
            .replace("a", move |_| {
                tokio::spawn(async move {
                    loop {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
            })
            .await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(sessions.remove("a").await);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let after_end = ticks.load(std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(ticks.load(std::sync::atomic::Ordering::SeqCst), after_end, "no further attempts after the operator ended it");
    }

    #[tokio::test]
    async fn the_default_account_is_the_only_or_most_recently_connected_one() {
        let sessions = Sessions::default();
        assert_eq!(sessions.default_account().await, None);
        let a = sessions.replace("a", |_| forever()).await;
        let b = sessions.replace("b", |_| forever()).await;
        assert_eq!(sessions.default_account().await, None, "nothing is connected yet");
        a.status.write().await.connected = true;
        a.status.write().await.connected_at_ms = Some(100);
        assert_eq!(sessions.default_account().await.as_deref(), Some("a"));
        b.status.write().await.connected = true;
        b.status.write().await.connected_at_ms = Some(200);
        assert_eq!(sessions.default_account().await.as_deref(), Some("b"));
    }

    #[tokio::test]
    async fn injections_go_to_the_named_accounts_queue_only() {
        let sessions = Sessions::default();
        let a = sessions.replace("a", |_| forever()).await;
        sessions.replace("b", |_| forever()).await;
        let (tx, mut rx) = mpsc::channel(2);
        *a.transport.write().await = Some(tx);
        assert!(sessions.transport("b").await.is_none());
        let sender = sessions.transport("a").await.unwrap();
        sender.send(InjectionRequest::new("%xt%x%1%{}%".into(), 0, 1_000)).await.unwrap();
        assert!(rx.recv().await.is_some());
    }
}
