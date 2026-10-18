//! Alliance chat: reading the messages the server pushes.
//!
//! Pure parsing, no I/O. The session part is `empire-daemon/src/direct/chat.rs`, the stored
//! messages `store/chat.rs`. Packet facts and the client code they were read from:
//! `docs/network/acm.md`.
//!
//! One message is `{"PID": player id, "PN": name, "MA": age in seconds, "MT": text}`. The server
//! sends each new one alone as `acm` (`{"CM": {message}}`), and the history the client asks for
//! as `acl` (`{"CM": [message, ...]}`). There is no timestamp: `MA` is how old the message is
//! when it arrives (0 for a live one), so it was written at `received - MA`.

use serde::Serialize;
use serde_json::Value;

/// One message, as the server sent it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatMessage {
    /// A negative id is the game itself (a system line), not a member.
    pub player_id: i64,
    pub player_name: String,
    /// Already decoded by [`decode_text`].
    pub text: String,
    /// Seconds between the message being written and this packet arriving.
    pub age_s: i64,
}

impl ChatMessage {
    pub fn is_system(&self) -> bool {
        self.player_id < 0
    }

    /// When it was written, from when it arrived.
    pub fn sent_at_ms(&self, received_at_ms: i64) -> i64 {
        received_at_ms.saturating_sub(self.age_s.max(0).saturating_mul(1_000))
    }
}

/// What the client does to `MT` before showing it (`TextValide.parseChatJSONMessage`): the server
/// escapes the characters the packet format cannot carry. Square brackets are shown as spaces.
pub fn decode_text(raw: &str) -> String {
    raw.replace("&percnt;", "%")
        .replace("&quot;", "\"")
        .replace("&145;", "'")
        .replace("<br />", "\n")
        .replace("%5C", "\\")
        .replace(['[', ']'], " ")
}

/// One message object. A message with no text is nothing to show.
pub fn parse_message(value: &Value) -> Option<ChatMessage> {
    let text = decode_text(value.get("MT")?.as_str()?);
    if text.trim().is_empty() {
        return None;
    }
    Some(ChatMessage {
        player_id: value.get("PID").and_then(Value::as_i64)?,
        player_name: value.get("PN").and_then(Value::as_str).unwrap_or("").to_owned(),
        text,
        age_s: value.get("MA").and_then(Value::as_i64).unwrap_or(0).max(0),
    })
}

/// The messages in a packet: `acm` (one) or `acl` (the history). Anything else has none.
pub fn messages(command: &str, payload: &Value) -> Vec<ChatMessage> {
    if command != "acm" && command != "acl" {
        return Vec::new();
    }
    match payload.get("CM") {
        Some(Value::Array(rows)) => rows.iter().filter_map(parse_message).collect(),
        Some(row @ Value::Object(_)) => parse_message(row).into_iter().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The packet captured on 2026-10-17 (`proxy_packet_monitor`, session 34d3316a).
    fn captured() -> Value {
        json!({"CM":{"PID":4616599,"PN":"Dawn Reaver","MA":0,"MT":"Can I get rage bonus please"}})
    }

    #[test]
    fn the_captured_acm_is_one_message_from_dawn_reaver() {
        let found = messages("acm", &captured());
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0],
            ChatMessage {
                player_id: 4_616_599,
                player_name: "Dawn Reaver".to_owned(),
                text: "Can I get rage bonus please".to_owned(),
                age_s: 0,
            }
        );
        assert!(!found[0].is_system());
    }

    #[test]
    fn a_history_packet_carries_a_list_in_the_same_shape() {
        let history = json!({"CM":[
            {"PID":1,"PN":"Kozz","MA":300,"MT":"camp cooldwon, khan and rage on for 6 hrs"},
            {"PID":2,"PN":"THE_ECLIPSE","MA":120,"MT":"good morning"},
            {"PID":3,"PN":"Nobody","MA":5}
        ]});
        let found = messages("acl", &history);
        assert_eq!(found.iter().map(|m| m.player_name.as_str()).collect::<Vec<_>>(), vec!["Kozz", "THE_ECLIPSE"]);
        assert_eq!(found[0].age_s, 300);
    }

    #[test]
    fn other_packets_and_odd_payloads_have_no_messages() {
        assert!(messages("gcu", &captured()).is_empty());
        assert!(messages("acm", &json!({})).is_empty());
        assert!(messages("acm", &json!({"CM": 5})).is_empty());
        assert!(messages("acm", &json!({"CM": {"PID": 1, "PN": "x", "MT": "   "}})).is_empty(), "blank text");
        assert!(messages("acm", &json!({"CM": {"PN": "x", "MT": "hi"}})).is_empty(), "no player id");
    }

    #[test]
    fn the_time_a_message_was_written_is_when_it_arrived_minus_its_age() {
        let live = parse_message(&json!({"PID":1,"PN":"a","MA":0,"MT":"hi"})).unwrap();
        assert_eq!(live.sent_at_ms(1_000_000), 1_000_000);
        let old = parse_message(&json!({"PID":1,"PN":"a","MA":90,"MT":"hi"})).unwrap();
        assert_eq!(old.sent_at_ms(1_000_000), 910_000);
        let odd = parse_message(&json!({"PID":1,"PN":"a","MA":-40,"MT":"hi"})).unwrap();
        assert_eq!(odd.sent_at_ms(1_000_000), 1_000_000, "a negative age is read as now");
    }

    #[test]
    fn escapes_are_undone_the_way_the_client_undoes_them() {
        assert_eq!(decode_text("100&percnt; sure"), "100% sure");
        assert_eq!(decode_text("say &quot;hi&quot;"), "say \"hi\"");
        assert_eq!(decode_text("don&145;t"), "don't");
        assert_eq!(decode_text("one<br />two"), "one\ntwo");
        assert_eq!(decode_text("a%5Cb"), "a\\b");
        assert_eq!(decode_text("[rage] on"), " rage  on");
        assert_eq!(decode_text("plain"), "plain");
    }

    #[test]
    fn a_negative_player_id_is_the_game_speaking() {
        let system = parse_message(&json!({"PID":-1,"PN":"","MT":"A member joined"})).unwrap();
        assert!(system.is_system());
    }

    // The chat the operator pasted on 2026-10-17: names and order are what the tab must show.
    #[test]
    fn the_pasted_chat_reads_as_nine_messages_from_four_senders() {
        let lines = [
            ("Dawn Reaver", "Can I get rage bonus please"), ("Kozz", "yes"), ("Kozz", "on"),
            ("Dawn Reaver", "luv u"), ("Kozz", "luv u twoo"), ("Dawn Reaver", "luv u three"),
            ("THE_ECLIPSE", "good morning"), ("Dawn Reaver", "Can I get fast cooldown for khan too please"),
            ("Kozz", "camp cooldwon, khan and rage on for 6 hrs"),
        ];
        let parsed = lines
            .iter()
            .map(|(name, text)| parse_message(&json!({"PID": 1, "PN": name, "MT": text})).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 9);
        let mut senders = parsed.iter().map(|m| m.player_name.as_str()).collect::<Vec<_>>();
        senders.sort_unstable();
        senders.dedup();
        assert_eq!(senders, vec!["Dawn Reaver", "Kozz", "THE_ECLIPSE"]);
    }
}
