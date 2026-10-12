/// Pure rules for the PVP tab and the Development tab's arrows: no DOM, no
/// network, so each can be checked on its own (see pvp-model.test.js).

export const KINGDOMS = [
  { id: 0, name: "Green" },
  { id: 1, name: "Burning Sands" },
  { id: 2, name: "Everwinter Ice" },
  { id: 3, name: "Fire Peaks" },
  { id: 4, name: "Storm" },
  { id: 10, name: "Berimond" },
];

export function kingdomLabel(id) {
  return KINGDOMS.find((kingdom) => kingdom.id === id)?.name ?? `Kingdom ${id}`;
}

/// The state a row is in *now*. The service works it out when it answers; this
/// carries it forward between answers so a landing is shown on time without a
/// request. A retreat is final and never overridden.
export function liveState(row, nowMs) {
  if (row.state === "retreated") return "retreated";
  return nowMs < row.land_at_ms ? "active" : "landed";
}

/// Rows keyed by MID, newest detected first (MID breaks ties, as the service
/// does). A row already held is updated in place, so a fresh first page can be
/// merged over pages loaded earlier without duplicating or losing any.
export function mergeRows(held, incoming) {
  const byMid = new Map(held.map((row) => [row.mid, row]));
  for (const row of incoming) byMid.set(row.mid, row);
  return [...byMid.values()].sort((a, b) => b.detected_at_ms - a.detected_at_ms || b.mid - a.mid);
}

/// Whole-history counts, summed over what the service reports. `direction` and
/// `state` are optional filters; both absent gives the grand total.
export function countOf(counts, direction = null, state = null) {
  return (counts || [])
    .filter((entry) => (direction == null || entry.direction === direction) && (state == null || entry.state === state))
    .reduce((total, entry) => total + entry.count, 0);
}

/// "name | alliance", whichever parts are known. Unknown parts are left out, not
/// guessed.
export function partyLabel(name, alliance) {
  return [name, alliance].filter(Boolean).join(" | ") || "unknown";
}

/// The colour scheme of the PVP tab and the map's arrows. What a movement is:
///   support         an ally sending troops to an ally            light blue
///   incoming        an attack on one of our own castles          red
///   ally_incoming   an attack on an ally                         orange
///   ally_outgoing   an ally attacking someone outside            light green
///   own_outgoing    our own attack (the ledger, not this tab)    green
///   other           neither end is ours                          grey
export const KINDS = {
  support: { label: "Support", colour: "#7cc8ff" },
  incoming: { label: "Incoming on us", colour: "#ff5c5c" },
  ally_incoming: { label: "Incoming on an ally", colour: "#ff9f43" },
  ally_outgoing: { label: "Ally outgoing", colour: "#a6e3a1" },
  own_outgoing: { label: "Our outgoing", colour: "#5ed08a" },
  other: { label: "Other", colour: "#9aa0a6" },
};

/// Which of those a recorded movement is. Between allies it can only be support.
export function movementKind(row) {
  if (row.direction === "internal") return "support";
  if (row.direction === "incoming") return row.owner === "mine" ? "incoming" : "ally_incoming";
  if (row.direction === "outgoing") return "ally_outgoing";
  return "other";
}

/// Whether the sender is in our alliance. Directions are judged from both ends;
/// the sender is an ally exactly when the movement is outgoing or internal.
export function senderIsAlly(row) {
  return row.direction === "outgoing" || row.direction === "internal";
}

/// What to draw for one recorded movement at `nowMs`, or `null` when it should
/// not be on the map: only active movements get an arrow (a landed or retreated
/// one is gone). The colour says whose attack it is: green when the sender is in
/// our alliance, red when it is anyone else's. The journey runs for `duration_s`
/// and ends at the estimated landing time.
export function arrowFor(row, nowMs) {
  if (liveState(row, nowMs) !== "active") return null;
  const startMs = row.land_at_ms - row.duration_s * 1000;
  const span = row.land_at_ms - startMs;
  return {
    mid: row.mid,
    colour: senderIsAlly(row) ? "out" : "in",
    kind: movementKind(row),
    from: { x: row.sender_x, y: row.sender_y },
    to: { x: row.target_x, y: row.target_y },
    progress: span > 0 ? Math.min(1, Math.max(0, (nowMs - startMs) / span)) : 1,
    remainingMs: Math.max(0, row.land_at_ms - nowMs),
    senderLabel: partyLabel(row.sender_name, row.sender_alliance),
    targetLabel: partyLabel(row.target_name, row.target_alliance),
  };
}
