//! The server refusing the client version in its `vck` reply, and what the session does about it.
//!
//! The game ships a new web client every few weeks and the server then hangs up on the old version straight after
//! the reply ([problem 10], [problem 20], [problem 23]). No credentials are sent after a refusal. While the servers
//! restart for an update the link also dies before `vck` is even reached (reset, or `RSV2 or RSV3 set` on the
//! upgrade, [problem 24]). Any attempt that fails before the login is accepted counts as a handshake failure, and the
//! session then follows a fixed schedule, counted from each failure:
//!
//! 1. 5 minutes later, the same versions again (the servers may still be restarting for the update);
//! 2. 2 hours after that, the same versions again;
//! 3. 2 hours 5 minutes after that, **only** if the live web client yields versions that differ from the refused
//!    ones and are not older (`empire-core::client_release`, the client's own rule). Those are used for this
//!    attempt. If they cannot be read, or they are the ones just refused, there is no third attempt.
//!
//! After that the session ends. Derived versions the server accepted are remembered, so the next session starts
//! with them instead of the built-in ones (until a build ships newer ones).
//!
//! [problem 10]: ../../../../docs/problems/10.md
//! [problem 20]: ../../../../docs/problems/20.md
//! [problem 23]: ../../../../docs/problems/23.md
//! [problem 24]: ../../../../docs/problems/24.md

use std::{future::Future, io, sync::Arc, time::Duration};

use empire_core::{
    client_release::HandshakeVersions,
    protocol::XtPacket,
    session::{CLIENT_VERSION, SessionPhase, SessionSettings},
    store::Store,
};
use serde_json::json;
use tokio::sync::RwLock;

use super::{DirectStatus, automation::log_event, now_ms};

#[derive(Debug)]
pub(super) struct ClientVersionRejected {
    sent: String,
    server_build: String,
}

impl std::fmt::Display for ClientVersionRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the game refused client version {} (server build {}). The game has updated",
            self.sent, self.server_build
        )
    }
}

impl std::error::Error for ClientVersionRejected {}

/// `Some` when this `vck` reply is a refusal (any status but 0).
pub(super) fn refusal(packet: &XtPacket, sent_version: &str) -> Option<ClientVersionRejected> {
    if packet.command != "vck" || packet.status.as_deref().is_none_or(|status| status == "0") {
        return None;
    }
    Some(ClientVersionRejected {
        sent: sent_version.to_owned(),
        server_build: packet.payload.as_str().map_or_else(|| packet.payload.to_string(), str::to_owned),
    })
}

pub(super) fn is_refusal(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ClientVersionRejected>().is_some()
}

/// Waits between refusals, in order. The last one is the derived-versions attempt.
pub(super) const VERSION_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(5 * 60),
    Duration::from_secs(2 * 60 * 60),
    Duration::from_secs(2 * 60 * 60 + 5 * 60),
];

/// What to do after refusal number `refusals` (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Plan {
    /// Wait, then try the same versions.
    Same(Duration),
    /// Wait, then try only with versions read from the live client.
    Derived(Duration),
    /// The schedule is spent.
    Stop,
}

pub(super) fn plan(refusals: u32) -> Plan {
    let last = VERSION_RETRY_DELAYS.len();
    match refusals as usize {
        0 => Plan::Stop,
        n if n < last => Plan::Same(VERSION_RETRY_DELAYS[n - 1]),
        n if n == last => Plan::Derived(VERSION_RETRY_DELAYS[n - 1]),
        _ => Plan::Stop,
    }
}

fn number(version: &str) -> Option<u64> {
    version.parse().ok()
}

/// The versions for the final attempt, or why there is none. Only versions that differ from the refused ones and are
/// not older are worth an attempt: the same ones would be refused again, and an older client (a stale CDN copy) too.
pub(super) fn final_versions(sent: &HandshakeVersions, derived: io::Result<HandshakeVersions>) -> Result<HandshakeVersions, String> {
    let derived = derived.map_err(|error| format!("could not read the live client's versions: {error}"))?;
    if derived == *sent {
        return Err(format!("the live client still uses {derived}, the versions just refused"));
    }
    match (number(&derived.client_version), number(&sent.client_version)) {
        (Some(new), Some(old)) if new < old => {
            return Err(format!("the live client reports an older {derived} than the refused {sent}"));
        }
        (Some(_), Some(_)) => {}
        _ => return Err(format!("{derived} or {sent} is not a number")),
    }
    match (number(&derived.smartfox_api_version), number(&sent.smartfox_api_version)) {
        (Some(new), Some(old)) if new < old => Err(format!("the live client reports an older {derived} than the refused {sent}")),
        (Some(_), Some(_)) => Ok(derived),
        _ => Err(format!("{derived} or {sent} is not a number")),
    }
}

pub(super) fn versions_of(settings: &SessionSettings) -> HandshakeVersions {
    HandshakeVersions {
        client_version: settings.client_version.clone(),
        smartfox_api_version: settings.smartfox_api_version.clone(),
    }
}

fn apply(settings: &mut SessionSettings, versions: &HandshakeVersions) {
    settings.client_version = versions.client_version.clone();
    settings.smartfox_api_version = versions.smartfox_api_version.clone();
}

/// After refusal number `refusals`: wait as the schedule says and set the versions for the next attempt. `Err` (with
/// the reason) means there is no next attempt. `derive` reads the live client; it is called only before the final
/// attempt, after its wait, so it sees the newest client. `sleep` is `tokio::time::sleep` in the service (a parameter
/// so tests can record the waits).
pub(super) async fn wait_for_next_attempt<F, Fut, S, SleepFut>(
    store: &Store,
    account_id: &str,
    status: &Arc<RwLock<DirectStatus>>,
    settings: &mut SessionSettings,
    refusals: u32,
    derive: F,
    sleep: S,
) -> Result<(), String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = io::Result<HandshakeVersions>>,
    S: FnOnce(Duration) -> SleepFut,
    SleepFut: Future<Output = ()>,
{
    let (delay, derived) = match plan(refusals) {
        Plan::Same(delay) => (delay, false),
        Plan::Derived(delay) => (delay, true),
        Plan::Stop => return Err(format!("the handshake failed {refusals} times")),
    };
    let total = VERSION_RETRY_DELAYS.len();
    let detail = format!(
        "The handshake failed (game update or restart?); attempt {} of {total} in {}{}",
        refusals,
        human(delay),
        if derived { ", only if the live client shows new versions" } else { "" }
    );
    log_event(store, account_id, "connection.handshake_retry_scheduled", &detail).await;
    {
        let mut current = status.write().await;
        current.connected = false;
        current.connected_at_ms = None;
        current.phase = SessionPhase::Disconnected;
        current.bot_state = "waiting for game update".to_owned();
        current.bot_detail = Some(detail);
    }
    sleep(delay).await;
    if !derived {
        return Ok(());
    }
    let sent = versions_of(settings);
    match final_versions(&sent, derive().await) {
        Ok(versions) => {
            log_event(
                store,
                account_id,
                "connection.version_derived",
                &format!("refused {sent}; the live client uses {versions}; trying them once"),
            )
            .await;
            apply(settings, &versions);
            Ok(())
        }
        Err(reason) => Err(format!("{reason}. Not trying again")),
    }
}

const REMEMBERED_KEY: &str = "handshake.accepted_versions";

/// Keep versions the server accepted when they are not the built-in ones, so the next session starts with them.
pub(super) async fn remember_accepted(store: &Store, settings: &SessionSettings) {
    if settings.client_version == CLIENT_VERSION && settings.smartfox_api_version == empire_core::session::SMARTFOX_API_VERSION {
        return;
    }
    let value = json!({
        "client_version": settings.client_version,
        "smartfox_api_version": settings.smartfox_api_version,
    });
    let _ = store.set_app_state(REMEMBERED_KEY, &value, now_ms()).await;
}

/// Use remembered versions when they are newer than the ones this session would send (a newer build wins).
pub(super) async fn apply_remembered(store: &Store, settings: &mut SessionSettings) {
    let Ok(Some(value)) = store.app_state(REMEMBERED_KEY).await else { return };
    let field = |name: &str| value.get(name).and_then(|value| value.as_str()).map(str::to_owned);
    let (Some(client_version), Some(smartfox_api_version)) = (field("client_version"), field("smartfox_api_version")) else {
        return;
    };
    let remembered = HandshakeVersions { client_version, smartfox_api_version };
    if let Ok(newer) = final_versions(&versions_of(settings), Ok(remembered))
        && number(&newer.client_version) > number(&settings.client_version)
    {
        apply(settings, &newer);
    }
}

fn human(delay: Duration) -> String {
    let minutes = delay.as_secs() / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use empire_core::protocol::parse_xt_packet;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn versions(client: &str, smartfox: &str) -> HandshakeVersions {
        HandshakeVersions { client_version: client.to_owned(), smartfox_api_version: smartfox.to_owned() }
    }

    #[test]
    fn only_a_non_zero_vck_status_is_a_refusal() {
        let refused = parse_xt_packet("%xt%vck%1%1%6.1174.6%41.8.0%").unwrap();
        let error = refusal(&refused, "1169011").expect("status 1 is a refusal");
        assert!(error.to_string().contains("1169011") && error.to_string().contains("6.1174.6"));
        assert!(is_refusal(&anyhow::Error::from(error)));
        assert!(refusal(&parse_xt_packet("%xt%vck%1%0%6.1174.6%41.8.0%").unwrap(), "1").is_none());
        assert!(refusal(&parse_xt_packet("%xt%lli%1%1%{}%").unwrap(), "1").is_none());
    }

    #[test]
    fn the_schedule_is_five_minutes_two_hours_then_one_derived_attempt() {
        assert_eq!(plan(1), Plan::Same(Duration::from_secs(300)));
        assert_eq!(plan(2), Plan::Same(Duration::from_secs(7_200)));
        assert_eq!(plan(3), Plan::Derived(Duration::from_secs(7_500)));
        assert_eq!(plan(4), Plan::Stop);
        assert_eq!(plan(0), Plan::Stop);
    }

    #[test]
    fn the_final_attempt_needs_new_versions_that_are_not_older() {
        let sent = versions("1171009", "166");
        assert_eq!(final_versions(&sent, Ok(versions("1172000", "166"))), Ok(versions("1172000", "166")));
        assert_eq!(final_versions(&sent, Ok(versions("1171009", "167"))), Ok(versions("1171009", "167")));
        assert!(final_versions(&sent, Ok(versions("1171009", "166"))).is_err(), "the refused versions again");
        assert!(final_versions(&sent, Ok(versions("1170022", "166"))).is_err(), "an older client");
        assert!(final_versions(&sent, Ok(versions("1172000", "165"))).is_err(), "an older verChk");
        assert!(final_versions(&sent, Err(io::Error::other("HTTP 503"))).is_err(), "nothing could be read");
    }

    async fn test_store() -> (Store, Arc<RwLock<DirectStatus>>) {
        (Store::open("sqlite::memory:").await.unwrap(), Arc::new(RwLock::new(DirectStatus::default())))
    }

    /// A mocked clock: records each wait instead of sleeping, along with how many client reads had happened by then.
    struct Clock {
        waits: std::sync::Mutex<Vec<(u64, u32)>>,
        reads: AtomicU32,
    }

    impl Clock {
        fn new() -> Self {
            Self { waits: std::sync::Mutex::new(Vec::new()), reads: AtomicU32::new(0) }
        }

        fn sleep(&self) -> impl FnOnce(Duration) -> std::future::Ready<()> + '_ {
            move |delay| {
                self.waits.lock().unwrap().push((delay.as_secs(), self.reads.load(Ordering::SeqCst)));
                std::future::ready(())
            }
        }
    }

    /// Mocked clock and client: every wait is exactly as scheduled, and the client is read once, before the final attempt.
    #[tokio::test]
    async fn the_waits_and_the_one_read_of_the_client_follow_the_schedule() {
        let (store, status) = test_store().await;
        let clock = Clock::new();
        let derive = || async {
            clock.reads.fetch_add(1, Ordering::SeqCst);
            Ok(versions("1173000", "166"))
        };
        let mut settings = SessionSettings::default();
        for refusals in 1..=3 {
            wait_for_next_attempt(&store, "pingpoko", &status, &mut settings, refusals, derive, clock.sleep()).await.unwrap();
            if refusals < 3 {
                assert_eq!(settings.client_version, CLIENT_VERSION, "the first two retries resend the same versions");
            }
        }
        // (seconds waited, client reads before that wait): 5 min, 2 h, 2 h 5 min, and the client is read only after
        // the last wait.
        assert_eq!(*clock.waits.lock().unwrap(), [(300, 0), (7_200, 0), (7_500, 0)]);
        assert_eq!(clock.reads.load(Ordering::SeqCst), 1);
        assert_eq!(settings.client_version, "1173000");
        assert_eq!(status.read().await.bot_state, "waiting for game update");
        let stop = wait_for_next_attempt(&store, "pingpoko", &status, &mut settings, 4, derive, clock.sleep()).await;
        assert!(stop.is_err(), "no fourth attempt");
        assert_eq!(clock.waits.lock().unwrap().len(), 3, "and no fourth wait");
    }

    #[tokio::test]
    async fn no_final_attempt_when_the_client_has_not_changed() {
        let (store, status) = test_store().await;
        let mut settings = SessionSettings::default();
        let unchanged = || async { Ok(versions(CLIENT_VERSION, "166")) };
        let clock = Clock::new();
        let result = wait_for_next_attempt(&store, "ventrilo", &status, &mut settings, 3, unchanged, clock.sleep()).await;
        assert!(result.unwrap_err().contains("Not trying again"));
        assert_eq!(settings.client_version, CLIENT_VERSION);
        let unreadable = || async { Err(io::Error::other("HTTP 403")) };
        assert!(wait_for_next_attempt(&store, "ventrilo", &status, &mut settings, 3, unreadable, clock.sleep()).await.is_err());
    }

    #[tokio::test]
    async fn accepted_derived_versions_carry_over_until_a_newer_build() {
        let (store, _) = test_store().await;
        let mut fresh = SessionSettings::default();
        apply_remembered(&store, &mut fresh).await;
        assert_eq!(fresh.client_version, CLIENT_VERSION, "nothing remembered yet");

        let accepted = SessionSettings { client_version: "9999001".to_owned(), ..SessionSettings::default() };
        remember_accepted(&store, &accepted).await;
        let mut next = SessionSettings::default();
        apply_remembered(&store, &mut next).await;
        assert_eq!(next.client_version, "9999001");

        let mut newer_build = SessionSettings { client_version: "9999002".to_owned(), ..SessionSettings::default() };
        apply_remembered(&store, &mut newer_build).await;
        assert_eq!(newer_build.client_version, "9999002", "a build with newer versions is not overridden");
    }
}
