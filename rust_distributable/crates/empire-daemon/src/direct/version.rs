//! The server refusing the client version in its `vck` reply.
//!
//! The game ships a new web client every few weeks and the server then hangs up on the old
//! version straight after the reply. Retrying cannot help, so this ends the session at once
//! with the version we sent and the one the server named, instead of looking like a dropped link.

use empire_core::protocol::XtPacket;

#[derive(Debug)]
pub(super) struct ClientVersionRejected {
    sent: String,
    server_build: String,
}

impl std::fmt::Display for ClientVersionRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the game refused client version {} (server build {}). The game has updated: set the new version in empire-core `CLIENT_VERSION`. Not retrying",
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

#[cfg(test)]
mod tests {
    use super::*;
    use empire_core::protocol::parse_xt_packet;

    #[test]
    fn only_a_non_zero_vck_status_is_a_refusal() {
        let refused = parse_xt_packet("%xt%vck%1%1%6.1174.6%41.8.0%").unwrap();
        let error = refusal(&refused, "1169011").expect("status 1 is a refusal");
        assert!(error.to_string().contains("1169011") && error.to_string().contains("6.1174.6"));
        assert!(refusal(&parse_xt_packet("%xt%vck%1%0%6.1174.6%41.8.0%").unwrap(), "1").is_none());
        assert!(refusal(&parse_xt_packet("%xt%lli%1%1%{}%").unwrap(), "1").is_none());
    }
}
