use super::*;

pub(super) fn as_map_target(target: &ReservedTarget) -> MapTarget {
    MapTarget {
        kingdom_id: target.kingdom_id,
        x: target.x,
        y: target.y,
        level: target.level,
    }
}

/// The 13×13 client tile centred on one coordinate.
///
/// This is for *opening* a kingdom's map on a point of interest — a castle to
/// inspect, or a fortress whose cooldown is being re-read — not for discovery.
/// Discovery uses [`fortress_gaa_packet`], which asks for a whole lattice block.
pub(super) fn tile_gaa_packet(
    server_header: &str,
    kingdom_id: i64,
    x: i64,
    y: i64,
) -> Result<String, empire_core::protocol::PacketError> {
    let ax1 = x.saturating_sub(6);
    let ay1 = y.saturating_sub(6);
    encode_client_xt(
        server_header,
        "gaa",
        "1",
        &json!({
            "KID": kingdom_id,
            "AX1": ax1,
            "AY1": ay1,
            "AX2": ax1 + 12,
            "AY2": ay1 + 12
        }),
    )
}

pub(super) fn fortress_gaa_packet(
    server_header: &str,
    kingdom_id: i64,
    block: FortressBlock,
) -> Result<String, empire_core::protocol::PacketError> {
    // One request answers a whole 2×2 block of slots, so the window is the
    // smallest rectangle that contains all four of them.
    let window = block.window();
    encode_client_xt(
        server_header,
        "gaa",
        "1",
        &json!({
            "KID": kingdom_id,
            "AX1": window.left,
            "AY1": window.top,
            "AX2": window.right,
            "AY2": window.bottom
        }),
    )
}

pub(super) fn heartbeat_packet(server_header: &str) -> String {
    format!("%xt%{server_header}%pin%1%<RoundHouseKick>%")
}
