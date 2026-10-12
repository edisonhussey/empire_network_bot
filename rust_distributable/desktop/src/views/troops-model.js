/// Pure rules for the Troop overview: no DOM, no network (see troops-model.test.js).

export const STATE_LABEL = {
  idle: "Inactive",
  sending: "Sending…",
  outbound: "Outbound",
  returning: "Returning",
  station: "Support",
};

/// In commander-number order, always: the number is the human one (the commander's place
/// in the account's roster) and each task owns a range of them, so the table reads as the
/// bot is laid out and a commander stays on the same line while its state changes.
export function sortRows(rows) {
  return [...rows].sort((a, b) => a.number - b.number);
}

/// The state a row is in *now*: an outbound or returning hold that has run out reads
/// as inactive without waiting for the next answer from the service.
export function liveState(row, nowMs) {
  if (row.state !== "idle" && row.free_at_ms != null && row.free_at_ms <= nowMs) return "idle";
  if (row.state === "outbound" && row.lands_at_ms != null && row.lands_at_ms <= nowMs) return "outbound";
  return row.state;
}

/// Totals for the strip above the table.
export function summarise(rows, nowMs) {
  const totals = { outbound: 0, returning: 0, sending: 0, station: 0, idle: 0, troops_out: 0, coins: 0, rubies: 0 };
  for (const row of rows) {
    const state = liveState(row, nowMs);
    totals[state] += 1;
    if (state !== "idle" && state !== "station") {
      totals.troops_out += row.troop_total || 0;
      totals.coins += row.coin_loot || 0;
      totals.rubies += row.ruby_loot || 0;
    }
  }
  return totals;
}

/// "4m 12s" / "now" / "–" for a time that is not known; `estimated` marks a bound.
export function until(targetMs, nowMs, estimated = false) {
  const text = untilExact(targetMs, nowMs);
  return estimated && text !== "–" && text !== "now" ? `≤ ${text}` : text;
}

function untilExact(targetMs, nowMs) {
  if (targetMs == null) return "–";
  const total = Math.round((targetMs - nowMs) / 1000);
  if (total <= 0) return "now";
  if (total < 60) return `${total}s`;
  if (total < 3600) return `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, "0")}s`;
  return `${Math.floor(total / 3600)}h ${String(Math.floor((total % 3600) / 60)).padStart(2, "0")}m`;
}

/// The army as one line, largest first: "50 × crossbowman, 20 × ...".
export function armyText(army, limit = 3) {
  if (!army || !army.length) return "–";
  const shown = army.slice(0, limit).map((unit) => `${unit.count.toLocaleString()} × ${unit.name.replaceAll("_", " ")}`);
  return army.length > limit ? `${shown.join(", ")} +${army.length - limit} more` : shown.join(", ");
}

/// The state of a finished march as a short word.
export function outcomeLabel(march) {
  if (march.status === "returning") return "returned";
  if (march.status === "sent") return "out";
  return march.status.replaceAll("_", " ");
}
