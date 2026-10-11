/// Pure rules for what the Development tab shows of the station: no DOM, no
/// network, so each can be checked on its own (see station-model.test.js).

/// How the support march is placed on the map right now.
///
/// The march leaves the attacked castle for the configured destination and is
/// recalled a few seconds later, so it only ever covers a sliver of a journey that
/// would take minutes. Its position comes from the acknowledgement time and the
/// duration the server gave; the way back is the same sliver, walked in reverse over
/// the return time the recall reported.
///
/// `phase` is `preparing` (nothing has left yet), `out`, `back` or `home`.
/// `fraction` is how far along the castle-to-destination line the troops are, 0..1.
export function supportMarker(job, nowMs) {
  const acked = job.cds_ack_ms;
  if (acked == null) return { phase: "preparing", fraction: 0, remainingMs: null };
  const duration = (job.support_duration_s ?? 0) * 1000;
  const clamp = (value) => Math.min(1, Math.max(0, value));
  const outFraction = (at) => (duration > 0 ? clamp((at - acked) / duration) : 0);
  if (job.mcm_ack_ms == null) {
    return {
      phase: "out",
      fraction: outFraction(nowMs),
      remainingMs: job.recall_due_ms == null ? null : Math.max(0, job.recall_due_ms - nowMs),
    };
  }
  const turned = outFraction(job.mcm_ack_ms);
  const trip = (job.return_eta_ms ?? job.mcm_ack_ms) - job.mcm_ack_ms;
  const back = trip > 0 ? clamp((nowMs - job.mcm_ack_ms) / trip) : 1;
  if (back >= 1) return { phase: "home", fraction: 0, remainingMs: 0 };
  return {
    phase: "back",
    fraction: turned * (1 - back),
    remainingMs: Math.max(0, (job.return_eta_ms ?? nowMs) - nowMs),
  };
}

/// A short human label and a tone (`ok`, `warn`, `bad`, `quiet`) for one job.
export function jobStatus(job) {
  const outcome = job.outcome || "";
  if (job.phase === "done") {
    if (outcome.startsWith("saved")) {
      return outcome === "saved" ? { text: "saved", tone: "ok" } : { text: outcome.replace(/[:_]/g, " "), tone: "warn" };
    }
    if (outcome === "late") return { text: "late", tone: "bad" };
    if (outcome.startsWith("cancelled")) return { text: "cancelled (retreat)", tone: "quiet" };
    return { text: outcome.replace(/[:_]/g, " ") || "finished", tone: outcome.startsWith("blocked") || outcome === "no_troops" ? "warn" : "bad" };
  }
  if (job.phase === "uncertain") return { text: "unconfirmed", tone: "bad" };
  if (job.phase === "away") return { text: "support out", tone: "ok" };
  return { text: job.doing || job.phase, tone: "warn" };
}

/// Milliseconds as "4.4 s" (or "310 ms" below a second), signed when asked.
export function seconds(ms, signed = false) {
  if (ms == null) return "–";
  const sign = signed && ms > 0 ? "+" : "";
  return Math.abs(ms) < 1000 ? `${sign}${Math.round(ms)} ms` : `${sign}${(ms / 1000).toFixed(1)} s`;
}

/// Where the sequence stood at each step, relative to the landing, for the job
/// list: the numbers that show whether the window was kept.
export function timeline(job) {
  const before = (at) => (at == null ? null : job.landing_ms - at);
  return [
    ["dcl", before(job.dcl_sent_ms)],
    ["sdi", before(job.sdi_sent_ms)],
    ["cds sent", before(job.cds_sent_ms)],
    ["cds acknowledged", before(job.cds_ack_ms)],
  ].filter(([, value]) => value != null);
}

/// The share of waits that fall in `[low, high]` seconds, from the bins, taking a
/// bin that straddles an edge in proportion (the bins are log-spaced, so by log
/// distance). `null` with nothing counted.
export function bandShare(histogram, low, high) {
  const { edges_s: edges, counts, total } = histogram;
  if (!total) return null;
  let inside = 0;
  for (let index = 0; index < counts.length; index += 1) {
    const upper = index < edges.length ? edges[index] : Infinity;
    const lower = index === 0 ? 0 : edges[index - 1];
    if (upper <= low || lower >= high) continue;
    if (lower >= low && upper <= high) { inside += counts[index]; continue; }
    if (!Number.isFinite(upper) || lower <= 0) {
      // Open-ended bins: count them only if their known edge is inside the band.
      if (lower >= low && lower < high) inside += counts[index];
      continue;
    }
    const span = Math.log(upper) - Math.log(lower);
    const overlap = Math.log(Math.min(upper, high)) - Math.log(Math.max(lower, low));
    inside += counts[index] * Math.max(0, overlap / span);
  }
  return inside / total;
}

/// Bars for the frequency chart: each bin's range and its share of all waits.
export function histogramBars(histogram) {
  const { edges_s: edges, counts, total } = histogram;
  return counts.map((count, index) => ({
    low: index === 0 ? 0 : edges[index - 1],
    high: index < edges.length ? edges[index] : Infinity,
    count,
    share: total ? count / total : 0,
  }));
}
