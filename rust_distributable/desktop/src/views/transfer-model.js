/// Pure rules for the transfer widget in the Castle Overview: no DOM, no network (see
/// transfer-model.test.js). The service decides what is allowed; this only shapes what it
/// sends and receives for the form.

import { kingdomLabel } from "./pvp-model.js";

/// "2 h", "1 h 30 min", "45 s": a journey or a wait in the unit people think in.
export function durationLabel(seconds) {
  const total = Math.max(0, Math.round(seconds || 0));
  if (total < 60) return `${total} s`;
  const hours = Math.floor(total / 3600);
  const minutes = Math.round((total % 3600) / 60);
  if (!hours) return `${minutes} min`;
  return minutes ? `${hours} h ${minutes} min` : `${hours} h`;
}

export function coinsLabel(value) {
  return new Intl.NumberFormat("en-US").format(Math.round(value ?? 0));
}

/// A typed count kept inside what the castle has.
export function clampCount(value, max) {
  const number = Math.floor(Number(value));
  if (!Number.isFinite(number) || number < 0) return 0;
  return Math.min(number, Math.max(0, max));
}

/// Kingdoms a castle can send to: unlocked, and not the one it is in.
export function targetOptions(kingdoms, sourceKingdomId) {
  return kingdoms
    .filter((kingdom) => kingdom.unlocked && kingdom.kingdom_id !== sourceKingdomId)
    .map((kingdom) => ({
      id: kingdom.kingdom_id,
      label: `${kingdomLabel(kingdom.kingdom_id)} (${kingdom.tax_percent}% tax, ${durationLabel(kingdom.travel_s)})`,
      travellingNow: kingdom.in_flight != null,
    }));
}

/// The request body for `POST /transfers` and `/transfers/check`: only units with a count.
export function buildDraft({ castleId, targetKingdomId, counts, skip }) {
  return {
    source_castle_id: castleId,
    target_kingdom_id: targetKingdomId,
    units: Object.entries(counts)
      .map(([id, count]) => [Number(id), Math.floor(Number(count))])
      .filter(([, count]) => count > 0),
    skip: skip || null,
  };
}

/// Skip choices: waiting is always one; each item shows how many are in stock and how many a
/// journey would use.
export function skipOptions(skips, travelSeconds) {
  const options = [{ key: "", label: `Wait it out${travelSeconds ? ` (${durationLabel(travelSeconds)})` : ""}` }];
  for (const item of skips) {
    const uses = travelSeconds ? Math.ceil(travelSeconds / item.seconds) : null;
    if (uses != null && uses > 10) continue; // the service refuses it; do not offer it
    const stock = item.stock == null ? "stock unknown" : `${item.stock.toLocaleString()} in stock`;
    options.push({ key: item.key, label: `Skip with ${item.key} (${durationLabel(item.seconds)} each${uses ? `, uses ${uses}` : ""}; ${stock})` });
  }
  return options;
}

const PHASES = {
  queued: "Waiting to be sent",
  sent: "Sent",
  travelling: "On its way",
  done: "Landed",
  failed: "Failed",
  cancelled: "Cancelled",
};

export function phaseLabel(phase) {
  return PHASES[phase] ?? phase;
}

export function isOpen(job) {
  return job.phase === "queued" || job.phase === "sent" || job.phase === "travelling";
}

/// One line for the history: what moved where.
export function jobTitle(job, unitName) {
  const units = job.units.map(([id, count]) => `${count.toLocaleString()} ${unitName(id)}`).join(", ");
  return `${units}: ${kingdomLabel(job.source_kingdom_id)} to ${kingdomLabel(job.target_kingdom_id)}`;
}

/// Whether the Send button may be pressed: the service has no objection and nothing is in flight.
export function canSend(check, overview) {
  if (!check || !overview) return false;
  if (check.problems.length) return false;
  return !overview.jobs.some(isOpen);
}
