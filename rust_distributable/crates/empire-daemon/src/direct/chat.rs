//! Alliance chat: listen in the background and store what the account's own session hears.
//!
//! Nothing here sends a packet. The server pushes every new alliance message as `acm` (and a
//! history as `acl` when something asks for one); this stores them as they arrive and nothing
//! waits for them. Parsing: `empire_core::chat`; storage: `store/chat.rs`; packet facts:
//! `docs/network/acm.md`.

use super::*;
use empire_core::{chat, protocol::XtPacket};

/// Store the messages in this packet, if it has any. Cheap for every other packet.
pub(super) async fn observe(store: &Store, account_id: &str, packet: Option<&XtPacket>, now: i64) {
    let Some(packet) = packet else { return };
    if !matches!(packet.command.as_str(), "acm" | "acl") {
        return;
    }
    if packet.status.as_deref().is_some_and(|status| status != "0") {
        return;
    }
    let messages = chat::messages(&packet.command, &packet.payload);
    if messages.is_empty() {
        return;
    }
    if let Err(error) = store.record_chat(account_id, &messages, now).await {
        warn!(%error, %account_id, "alliance chat was not stored");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    async fn store() -> Store {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store.upsert_account_profile("ventrilo", "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1).await.unwrap();
        store
    }

    fn packet(raw: &str) -> XtPacket {
        parse_xt_packet(raw).unwrap()
    }

    #[tokio::test]
    async fn the_captured_acm_is_stored_with_the_senders_name() {
        let store = store().await;
        let live = packet(r#"%xt%acm%1%0%{"CM":{"PID":4616599,"PN":"Dawn Reaver","MA":0,"MT":"Can I get rage bonus please"}}%"#);
        observe(&store, "ventrilo", Some(&live), NOW).await;
        let rows = store.recent_chat("ventrilo", 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].player_name.as_str(), rows[0].text.as_str(), rows[0].sent_at_ms), ("Dawn Reaver", "Can I get rage bonus please", NOW));
    }

    #[tokio::test]
    async fn a_conversation_arrives_message_by_message_in_order() {
        let store = store().await;
        for (second, name, text) in [(0, "Kozz", "yes"), (5, "Kozz", "on"), (7, "Dawn Reaver", "luv u")] {
            let raw = format!(r#"%xt%acm%1%0%{{"CM":{{"PID":1,"PN":"{name}","MA":0,"MT":"{text}"}}}}%"#);
            observe(&store, "ventrilo", Some(&packet(&raw)), NOW + second * 1_000).await;
        }
        let texts = store.recent_chat("ventrilo", 10).await.unwrap().into_iter().map(|row| row.text).collect::<Vec<_>>();
        assert_eq!(texts, vec!["yes", "on", "luv u"]);
    }

    #[tokio::test]
    async fn a_history_packet_fills_in_without_doubling_what_was_heard() {
        let store = store().await;
        observe(&store, "ventrilo", Some(&packet(r#"%xt%acm%1%0%{"CM":{"PID":7,"PN":"Kozz","MA":0,"MT":"on"}}%"#)), NOW).await;
        let history = r#"%xt%acl%1%0%{"CM":[{"PID":9,"PN":"THE_ECLIPSE","MA":3600,"MT":"good morning"},{"PID":7,"PN":"Kozz","MA":10,"MT":"on"}]}%"#;
        observe(&store, "ventrilo", Some(&packet(history)), NOW + 10_000).await;
        let rows = store.recent_chat("ventrilo", 10).await.unwrap();
        assert_eq!(rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(), vec!["good morning", "on"]);
    }

    #[tokio::test]
    async fn refusals_and_other_packets_store_nothing() {
        let store = store().await;
        observe(&store, "ventrilo", Some(&packet(r#"%xt%acm%1%12%{"CM":{"PID":1,"PN":"x","MT":"no"}}%"#)), NOW).await;
        observe(&store, "ventrilo", Some(&packet(r#"%xt%gcu%1%0%{"C1":1,"C2":2}%"#)), NOW).await;
        observe(&store, "ventrilo", None, NOW).await;
        assert!(store.recent_chat("ventrilo", 10).await.unwrap().is_empty());
    }
}
