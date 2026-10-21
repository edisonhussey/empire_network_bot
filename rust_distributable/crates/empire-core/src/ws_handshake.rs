//! What the game's WebSocket upgrade must look like.
//!
//! The web client opens its socket from `https://empire-html5.goodgamestudios.com`, and the server only advances a
//! socket to `apiOK` when it looks like that page's: a direct connection with another `Origin` still gets HTTP 101 and
//! then nothing (see `docs/problems/17.md`). These are the headers Chrome sent in the 2026-10-21 capture
//! (`proxy_packet_monitor` "Record full handshake"), minus `Cookie` (only consent and analytics cookies, no game
//! session) and the `Sec-WebSocket-Version/Key` headers, which the WebSocket library writes itself. `Sec-WebSocket-Extensions` is ours:
//! the library has no extension support, so the offer is listed here and `empire-socket` handles what it negotiates.
//!
//! Names are lower-case so they can be used as `HeaderName::from_static`. Callers insert every pair; the one place to
//! change when the game moves its page is this file.

/// The page the web client runs on. Also the `Origin` of the upgrade request.
pub const GAME_ORIGIN: &str = "https://empire-html5.goodgamestudios.com";

/// What Chrome offers; the server answers `permessage-deflate` (no parameters, so a 15-bit window and context takeover).
pub const PERMESSAGE_DEFLATE_OFFER: &str = "permessage-deflate; client_max_window_bits";

/// Every extra header the browser sends on the upgrade, `Origin` first.
pub const UPGRADE_HEADERS: [(&str, &str); 6] = [
    ("origin", GAME_ORIGIN),
    (
        "user-agent",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36",
    ),
    ("accept-language", "en-GB,en-US;q=0.9,en;q=0.8"),
    ("pragma", "no-cache"),
    ("cache-control", "no-cache"),
    // REQUIRED: the server answers `400 Bad Request` to an upgrade that offers no deflate extension (docs/problems/19.md).
    // `empire-socket` is what understands the compressed frames that come back.
    ("sec-websocket-extensions", PERMESSAGE_DEFLATE_OFFER),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_lower_case_and_values_are_plain_header_text() {
        for (name, value) in UPGRADE_HEADERS {
            assert_eq!(name, name.to_ascii_lowercase(), "{name} must be lower-case for from_static");
            assert!(value.bytes().all(|byte| (0x20..0x7f).contains(&byte)), "{name} has a non-printable byte");
        }
    }

    #[test]
    fn the_deflate_offer_is_in_the_upgrade() {
        assert!(UPGRADE_HEADERS.iter().any(|(name, value)| *name == "sec-websocket-extensions" && value.starts_with("permessage-deflate")));
    }

    #[test]
    fn the_origin_header_is_the_game_page() {
        assert_eq!(UPGRADE_HEADERS[0], ("origin", GAME_ORIGIN));
        assert!(GAME_ORIGIN.starts_with("https://") && !GAME_ORIGIN.ends_with('/'));
    }
}
