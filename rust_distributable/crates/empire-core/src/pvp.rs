//! Incoming and alliance movements (the PVP tab), parsed from `gam` and `mrm`.
//!
//! Pure functions only: nothing here touches the store or the clock, so the rules
//! are tested against the captured packets in `docs/network/captures/`.
//!
//! Three steps, each its own function:
//!
//! 1. [`alliance_identity`] reads who *we* are from the login `gbd`.
//! 2. [`seen_movements`] lists every movement in a `gam`, as the wire states it.
//! 3. [`classify`] decides whether we record it and how it relates to us.
//!
//! Packet facts and their limits: `docs/network/topics/incoming_movements.md`.

use serde::Serialize;
use serde_json::Value;

/// Who the connected account is. Taken from the login `gbd`: `gpi.PID` is the
/// player and `gal` ({`AID`, `N`}) the alliance, absent for a player without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllianceIdentity {
    pub player_id: i64,
    pub player_name: Option<String>,
    pub alliance_id: Option<i64>,
    pub alliance_name: Option<String>,
}

pub fn alliance_identity(gbd: &Value) -> Option<AllianceIdentity> {
    let player_id = gbd.pointer("/gpi/PID").and_then(Value::as_i64)?;
    let alliance_id = gbd
        .pointer("/gal/AID")
        .and_then(Value::as_i64)
        .filter(|id| *id >= 0);
    Some(AllianceIdentity {
        player_id,
        player_name: gbd
            .pointer("/gpi/PN")
            .and_then(Value::as_str)
            .map(str::to_owned),
        alliance_id,
        alliance_name: alliance_id.and_then(|_| {
            gbd.pointer("/gal/N")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }),
    })
}

/// One end of a movement: the player and the castle they own there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Party {
    pub player_id: Option<i64>,
    pub name: Option<String>,
    pub alliance_id: Option<i64>,
    pub alliance_name: Option<String>,
    /// The record's `AID` exactly as sent, `-1` (no alliance) included; `None`
    /// only when no player record matched. `alliance_id` drops the `-1`.
    pub alliance_raw: Option<i64>,
    pub x: i64,
    pub y: i64,
    /// The castle's area id (index 3 of the area array), the id `cds` and `dcl`
    /// know it by.
    pub area_id: Option<i64>,
    /// Castle or outpost name from the area array (index 10).
    pub place: Option<String>,
}

/// A movement exactly as the packet states it, before any judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenMovement {
    pub mid: i64,
    pub kingdom_id: i64,
    pub sender: Party,
    pub target: Party,
    /// Seconds already travelled when the packet was sent (`PT`).
    pub progress_s: i64,
    /// Total travel time (`TT`).
    pub duration_s: i64,
    /// Units in the movement, when the packet carries them (see [`army_size`]).
    pub army_size: Option<i64>,
    /// `M.T`. Its meaning is not established for other players' movements, so it
    /// is kept, never interpreted.
    pub kind: Option<i64>,
    /// `M.D`; kept for the same reason.
    pub flag: Option<i64>,
}

/// Every well-formed movement in a `gam` payload. A malformed element is skipped
/// without discarding the others.
pub fn seen_movements(payload: &Value) -> Vec<SeenMovement> {
    let players = payload.get("O").and_then(Value::as_array);
    payload
        .get("M")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| seen_movement(entry, players.map_or(&[][..], Vec::as_slice)))
        .collect()
}

fn seen_movement(entry: &Value, players: &[Value]) -> Option<SeenMovement> {
    let movement = entry.get("M")?;
    let mid = movement.get("MID").and_then(Value::as_i64)?;
    let duration_s = movement.get("TT").and_then(Value::as_i64)?;
    let sender_id = movement
        .get("SID")
        .or_else(|| movement.get("OID"))
        .and_then(Value::as_i64);
    let target_id = movement.get("TID").and_then(Value::as_i64);
    Some(SeenMovement {
        mid,
        kingdom_id: movement.get("KID").and_then(Value::as_i64).unwrap_or(0),
        sender: party(players, sender_id, movement.get("SA")?)?,
        target: party(players, target_id, movement.get("TA")?)?,
        progress_s: movement.get("PT").and_then(Value::as_i64).unwrap_or(0),
        duration_s,
        army_size: army_size(entry),
        kind: movement.get("T").and_then(Value::as_i64),
        flag: movement.get("D").and_then(Value::as_i64),
    })
}

/// Player records are matched by `OID`, never by position (`O[]` order is not a
/// protocol guarantee). A missing record leaves the names unknown.
fn party(players: &[Value], player_id: Option<i64>, area: &Value) -> Option<Party> {
    let area = area.as_array()?;
    let x = area.get(1).and_then(Value::as_i64)?;
    let y = area.get(2).and_then(Value::as_i64)?;
    let record = player_id.and_then(|id| {
        players
            .iter()
            .find(|player| player.get("OID").and_then(Value::as_i64) == Some(id))
    });
    let text = |key: &str| {
        record
            .and_then(|player| player.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let alliance_raw = record
        .and_then(|player| player.get("AID"))
        .and_then(Value::as_i64);
    let alliance_id = alliance_raw.filter(|id| *id >= 0);
    Some(Party {
        player_id,
        name: text("N"),
        alliance_id,
        alliance_name: alliance_id.and_then(|_| text("AN")),
        alliance_raw,
        x,
        y,
        area_id: area.get(3).and_then(Value::as_i64),
        place: area
            .get(10)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    })
}

/// Sum of the unit counts in a movement.
///
/// Observed layouts: other players' movements carry `GA` = {`L`,`M`,`R`,`RW`}
/// (flanks and reserve), each a list of `[unit_id, count]`; our own carry a flat
/// `A` list of the same pairs. A packet with neither (the earlier `GS` form) gives
/// `None`: unknown stays unknown rather than becoming zero. `GS` is not summed
/// because its meaning is not established.
pub fn army_size(entry: &Value) -> Option<i64> {
    let sum_pairs = |pairs: &Value| -> i64 {
        pairs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|pair| pair.get(1).and_then(Value::as_i64))
            .sum()
    };
    if let Some(groups) = entry.get("GA").and_then(Value::as_object) {
        return Some(groups.values().map(sum_pairs).sum());
    }
    entry.get("A").map(sum_pairs)
}

/// The movement a retreat frame refers to. Whether a frame is a request or a
/// confirmation is the caller's decision; this only reads the `MID`.
pub fn retreated_mid(payload: &Value) -> Option<i64> {
    payload.get("MID").and_then(Value::as_i64)
}

/// How a recorded movement relates to the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Towards us or our alliance.
    Incoming,
    /// From our alliance towards someone else.
    Outgoing,
    /// Between members of our own alliance.
    Internal,
    /// Neither end is ours or our alliance's.
    Other,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
            Self::Internal => "internal",
            Self::Other => "other",
        }
    }
}

/// Whose castle the movement is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// The target is one of the account's own castles.
    Mine,
    /// Anything else on the record.
    Alliance,
}

impl Owner {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Alliance => "alliance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    pub direction: Direction,
    pub owner: Owner,
}

/// Whether a movement is between players. A player's end always has a record in `O[]`;
/// a movement whose sender has none (a robber baron, a fortress or any other NPC), or
/// whose target has none and is not ours, is NPC traffic. That includes the way back:
/// our own army returning from an NPC arrives as a movement from the NPC's castle to ours
/// and would otherwise read as an attack on us. Neither is PVP, so neither is recorded.
pub fn is_player_movement(movement: &SeenMovement, me: &AllianceIdentity, target_is_mine: bool) -> bool {
    let known = |party: &Party| party.name.is_some();
    known(&movement.sender) && (known(&movement.target) || target_is_mine || movement.target.player_id == Some(me.player_id))
}

/// Decide whether a movement is recorded and how it relates to the account.
///
/// `None` for the account's own outward movements (the ledger already holds those) and
/// for NPC movements ([`is_player_movement`]).
/// `own_castles` are the `(kingdom, x, y)` tuples found when the account was
/// initialised; a target matching one is `Owner::Mine`, everything else
/// `Owner::Alliance`.
pub fn classify(
    movement: &SeenMovement,
    me: &AllianceIdentity,
    own_castles: &[(i64, i64, i64)],
) -> Option<Classified> {
    if movement.sender.player_id == Some(me.player_id) {
        return None;
    }
    let target_is_mine = own_castles.contains(&(
        movement.kingdom_id,
        movement.target.x,
        movement.target.y,
    )) || movement.target.player_id == Some(me.player_id);
    if !is_player_movement(movement, me, target_is_mine) {
        return None;
    }
    let in_alliance = |party: &Party| {
        me.alliance_id
            .is_some_and(|alliance| party.alliance_id == Some(alliance))
    };
    let sender_ally = in_alliance(&movement.sender);
    let target_ally = target_is_mine || in_alliance(&movement.target);
    let direction = match (sender_ally, target_ally) {
        (true, true) => Direction::Internal,
        (false, true) => Direction::Incoming,
        (true, false) => Direction::Outgoing,
        (false, false) => Direction::Other,
    };
    Some(Classified {
        direction,
        owner: if target_is_mine {
            Owner::Mine
        } else {
            Owner::Alliance
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_xt_packet;

    /// The alliance-target capture in `docs/network/captures/gam_alliance_target.txt`.
    const ALLIANCE_GAM: &str = include_str!("../../../docs/network/captures/gam_alliance_target.txt");
    const MRM: &str = include_str!("../../../docs/network/captures/mrm_retreat.txt");

    fn me(alliance: Option<(i64, &str)>) -> AllianceIdentity {
        AllianceIdentity {
            player_id: 16862926,
            player_name: Some("Ventrilo".into()),
            alliance_id: alliance.map(|(id, _)| id),
            alliance_name: alliance.map(|(_, name)| name.to_owned()),
        }
    }

    fn captured() -> SeenMovement {
        let packet = parse_xt_packet(ALLIANCE_GAM.trim()).unwrap();
        let mut found = seen_movements(&packet.payload);
        assert_eq!(found.len(), 1);
        found.remove(0)
    }

    #[test]
    fn identity_reads_player_and_alliance_from_the_login_packet() {
        let gbd = serde_json::json!({
            "gpi": {"PID": 16862926, "PN": "Ventrilo"},
            "gal": {"AID": 189639, "N": "Sons of Guelon"}
        });
        let identity = alliance_identity(&gbd).unwrap();
        assert_eq!(identity.player_id, 16862926);
        assert_eq!(identity.alliance_id, Some(189639));
        assert_eq!(identity.alliance_name.as_deref(), Some("Sons of Guelon"));
        let loner = alliance_identity(&serde_json::json!({"gpi": {"PID": 5}})).unwrap();
        assert_eq!((loner.alliance_id, loner.alliance_name), (None, None));
        assert!(alliance_identity(&serde_json::json!({})).is_none());
    }

    #[test]
    fn the_captured_gam_reads_both_ends_and_the_army() {
        let movement = captured();
        assert_eq!(movement.mid, 155307796);
        assert_eq!(movement.kingdom_id, 0);
        assert_eq!((movement.duration_s, movement.progress_s), (57, 4));
        assert_eq!(movement.army_size, Some(9), "GA.M = [[602, 9]]");
        assert_eq!(movement.sender.name.as_deref(), Some("great emperor 9"));
        assert_eq!(movement.sender.alliance_name.as_deref(), Some("top secret"));
        assert_eq!((movement.sender.x, movement.sender.y), (543, 520));
        assert_eq!(movement.sender.place.as_deref(), Some("Castle great em"));
        assert_eq!(movement.target.name.as_deref(), Some("WIZRAD007"));
        assert_eq!(movement.target.alliance_name.as_deref(), Some("Electi"));
        assert_eq!((movement.target.x, movement.target.y), (546, 520));
        assert_eq!(movement.target.place.as_deref(), Some("wendlestein"));
    }

    #[test]
    fn players_are_matched_by_id_not_by_position() {
        let packet = parse_xt_packet(ALLIANCE_GAM.trim()).unwrap();
        let mut payload = packet.payload;
        payload["O"].as_array_mut().unwrap().reverse();
        let found = seen_movements(&payload);
        assert_eq!(found[0].sender.name.as_deref(), Some("great emperor 9"));
        assert_eq!(found[0].target.name.as_deref(), Some("WIZRAD007"));
    }

    #[test]
    fn a_missing_player_record_leaves_names_unknown_but_keeps_the_movement() {
        let packet = parse_xt_packet(ALLIANCE_GAM.trim()).unwrap();
        let mut payload = packet.payload;
        payload["O"] = serde_json::json!([]);
        let found = seen_movements(&payload);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].sender.name, None);
        assert_eq!(found[0].sender.alliance_id, None);
        assert_eq!((found[0].sender.x, found[0].sender.y), (543, 520));
    }

    #[test]
    fn a_malformed_element_does_not_discard_the_rest() {
        let packet = parse_xt_packet(ALLIANCE_GAM.trim()).unwrap();
        let mut payload = packet.payload;
        payload["M"]
            .as_array_mut()
            .unwrap()
            .insert(0, serde_json::json!({"M": {"MID": 1}}));
        assert_eq!(seen_movements(&payload).len(), 1);
    }

    #[test]
    fn army_size_sums_every_flank_and_stays_unknown_without_units() {
        let both = serde_json::json!({"GA": {"L": [[1, 10], [2, 5]], "M": [[3, 7]], "R": [], "RW": [[4, 1]]}});
        assert_eq!(army_size(&both), Some(23));
        let own = serde_json::json!({"A": [[216, 659041], [227, 556966]]});
        assert_eq!(army_size(&own), Some(1_216_007));
        assert_eq!(army_size(&serde_json::json!({"GS": 292})), None);
        let empty = serde_json::json!({"GA": {"L": [], "M": [], "R": [], "RW": []}});
        assert_eq!(army_size(&empty), Some(0), "an empty army is known to be empty");
    }

    #[test]
    fn a_target_matching_an_owned_castle_is_mine_and_incoming() {
        let found = classify(&captured(), &me(Some((3652, "Electi"))), &[(0, 546, 520)]).unwrap();
        assert_eq!(found.owner, Owner::Mine);
        assert_eq!(found.direction, Direction::Incoming);
    }

    #[test]
    fn an_attack_on_an_ally_is_incoming_and_recorded_as_alliance() {
        let found = classify(&captured(), &me(Some((3652, "Electi"))), &[(0, 1, 1)]).unwrap();
        assert_eq!(found.owner, Owner::Alliance);
        assert_eq!(found.direction, Direction::Incoming);
    }

    #[test]
    fn the_kingdom_is_part_of_the_castle_match() {
        let found = classify(&captured(), &me(None), &[(1, 546, 520)]).unwrap();
        assert_eq!(found.owner, Owner::Alliance, "same x,y in another kingdom is not ours");
        assert_eq!(found.direction, Direction::Other);
    }

    #[test]
    fn an_ally_attacking_an_outsider_is_outgoing_alliance() {
        let found = classify(&captured(), &me(Some((3026, "top secret"))), &[]).unwrap();
        assert_eq!(found.direction, Direction::Outgoing);
        assert_eq!(found.owner, Owner::Alliance);
    }

    #[test]
    fn our_own_outward_movements_are_not_recorded() {
        let mut movement = captured();
        movement.sender.player_id = Some(16862926);
        assert!(classify(&movement, &me(Some((3026, "top secret"))), &[]).is_none());
    }

    #[test]
    fn movements_between_allies_are_internal() {
        let mut movement = captured();
        movement.target.alliance_id = Some(3026);
        let found = classify(&movement, &me(Some((3026, "top secret"))), &[]).unwrap();
        assert_eq!(found.direction, Direction::Internal);
    }

    fn without_record(mut movement: SeenMovement, sender: bool) -> SeenMovement {
        let party = if sender { &mut movement.sender } else { &mut movement.target };
        party.name = None;
        party.alliance_id = None;
        party.alliance_name = None;
        party.alliance_raw = None;
        movement
    }

    #[test]
    fn a_movement_from_something_that_is_not_a_player_is_npc_traffic_and_ignored() {
        // A robber baron's castle has no player record: an attack from it, or our own army
        // coming home from it, is not PVP whoever the target is.
        let npc = without_record(captured(), true);
        assert!(classify(&npc, &me(Some((3652, "Electi"))), &[(0, 546, 520)]).is_none());
        assert!(classify(&npc, &me(None), &[]).is_none());
    }

    #[test]
    fn a_target_that_is_not_a_player_and_not_ours_is_npc_traffic() {
        let npc = without_record(captured(), false);
        assert!(classify(&npc, &me(Some((3026, "top secret"))), &[]).is_none(), "an ally attacking an NPC");
    }

    #[test]
    fn our_own_castle_is_a_target_even_without_a_player_record() {
        let ours = without_record(captured(), false);
        let found = classify(&ours, &me(Some((3652, "Electi"))), &[(0, 546, 520)]).unwrap();
        assert_eq!((found.direction, found.owner), (Direction::Incoming, Owner::Mine));
    }

    #[test]
    fn the_retreat_frame_names_its_movement() {
        let packet = parse_xt_packet(MRM.trim()).unwrap();
        assert_eq!(packet.command, "mrm");
        assert_eq!(retreated_mid(&packet.payload), Some(155312686));
    }
}
