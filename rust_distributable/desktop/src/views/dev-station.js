import { element } from "../dom.js";
import { pointOnPath, shortDuration, visible } from "./dev-math.js";
import { bandShare, histogramBars, jobStatus, seconds, supportMarker, timeline } from "./station-model.js";

/// The station's part of the Development tab: the frequency chart of the
/// human-paced waits, the list of stationed castles with their timings, and the
/// support marches drawn on the kingdom map.
///
/// Read-only, like the rest of the tab: it draws what `/v1/dev` reports, once per
/// animation frame from the timestamps in it. Nothing here sends anything.

const SUPPORT = "#6cc4ff";
const AMBER = "#e6c35c";
const DIM = "rgba(255,255,255,.40)";
const FAINT = "rgba(255,255,255,.16)";
const OK = "#5ed08a";

/// Finished jobs stay on the map only while their troops are still walking home.
function onMap(job, nowMs) {
  if (job.phase !== "done") return true;
  return job.return_eta_ms != null && job.return_eta_ms > nowMs;
}

/// Frequency of every wait the generators produced, log-spaced bins. The 1-5 s
/// band the design aims about 60 % of waits into is shaded and its share printed.
export function drawHistogram(canvas, sizeOf, histogram, band) {
  const { context, width, height } = sizeOf(canvas);
  context.clearRect(0, 0, width, height);
  context.font = "10px ui-monospace, Menlo, monospace";
  if (!histogram || !histogram.total) {
    context.fillStyle = DIM; context.fillText("No waits used yet.", 12, 24);
    return;
  }
  const bars = histogramBars(histogram);
  const pad = { l: 36, r: 10, t: 12, b: 22 };
  const plotWidth = width - pad.l - pad.r;
  const plotHeight = height - pad.t - pad.b;
  const top = Math.max(...bars.map((bar) => bar.share), 0.05);
  const slot = plotWidth / bars.length;
  context.lineWidth = 1; context.strokeStyle = FAINT; context.fillStyle = DIM;
  for (const fraction of [0.25, 0.5, 0.75, 1]) {
    const value = top * fraction;
    const y = pad.t + plotHeight * (1 - fraction);
    context.beginPath(); context.moveTo(pad.l, y); context.lineTo(width - pad.r, y); context.stroke();
    context.fillText(`${Math.round(value * 100)}%`, 4, y + 3);
  }
  bars.forEach((bar, index) => {
    const x = pad.l + index * slot;
    const h = (bar.share / top) * plotHeight;
    const inBand = bar.low >= band[0] && bar.high <= band[1];
    context.fillStyle = inBand ? "rgba(94,208,138,.75)" : "rgba(255,255,255,.55)";
    context.fillRect(x + 1, pad.t + plotHeight - h, Math.max(1, slot - 2), h);
  });
  // Labels where a bin edge falls near a round number of seconds.
  context.fillStyle = DIM;
  for (const mark of [0.5, 1, 2, 5, 10, 30, 60, 120]) {
    const index = histogram.edges_s.findIndex((edge) => edge >= mark);
    if (index < 0) continue;
    const x = pad.l + index * slot;
    context.fillText(`${mark}s`, x - 6, height - 6);
  }
  const share = bandShare(histogram, band[0], band[1]);
  const text = share == null ? "" : `${Math.round(share * 100)}% of ${histogram.total} in ${band[0]}–${band[1]} s`;
  const summary = document.querySelector("#dev-hist-summary");
  if (summary && summary.textContent !== text) summary.textContent = text;
}

/// The stationed castles, newest activity first, each with the numbers that show
/// whether the window was kept (time before the landing at every step).
export function renderStationPanel(station, nowMs) {
  const gate = document.querySelector("#dev-station-gate");
  const note = document.querySelector("#dev-station-note");
  const list = document.querySelector("#dev-station-jobs");
  if (!list) return;
  if (!station) { list.replaceChildren(); return; }
  gate.textContent = station.enabled ? `gate ${station.gate} · delay ${station.latency_ms} ms` : "off";
  note.textContent = station.enabled
    ? `Station bot "${station.bot}" is running: an attack on an owned castle by another alliance sends its troops out about five seconds before it lands.`
    : "No station bot is running. Choose one in the Control Panel, step 2.";
  const rows = station.jobs.slice(0, 8).map((job) => {
    const status = jobStatus(job);
    const item = element("li", "dev-job");
    const head = element("div", "dev-job-head");
    head.append(
      element("span", "", `castle ${job.castle[0]}:${job.castle[1]} · kingdom ${job.kingdom_id}`),
      element("span", `dev-tone-${status.tone}`, status.text),
      element("b", "", job.phase === "done" ? seconds(job.lead_ms) + " lead" : `lands in ${shortDuration(Math.max(0, job.landing_ms - nowMs))}`),
    );
    item.append(head);
    const attacks = job.attacks.map((attack) => `${attack.attacker || "unknown"}${attack.retreated ? " (retreated)" : ""}`).join(", ");
    item.append(element("small", "", `${job.attacks.length} attack${job.attacks.length === 1 ? "" : "s"}: ${attacks}`));
    const steps = timeline(job).map(([label, before]) => `${label} ${seconds(before)} before`).join(" · ");
    if (steps) item.append(element("small", "", steps));
    if (job.note) item.append(element("small", "", job.note));
    return item;
  });
  list.replaceChildren(...rows);
  if (!rows.length) list.append(element("li", "dev-none", "No attacks on your castles seen this session."));
}

/// One account's support marches on the map (the jobs given are already for the
/// kingdom shown): the attacked castle ringed, a dashed line to its destination moving
/// while troops are out, the countdown to the next step. Drawn in the account's colour.
export function drawStationJobs(context, view, jobs, nowMs, colour = SUPPORT) {
  context.font = "10px ui-monospace, Menlo, monospace";
  for (const job of jobs) {
    if (!onMap(job, nowMs)) continue;
    const [cx, cy] = job.castle;
    const dest = job.dest;
    if (!visible(view, cx, cy, 400) && !(dest && visible(view, dest[0], dest[1], 400))) continue;
    const castle = view.toScreen(cx, cy);
    const marker = supportMarker(job, nowMs);
    const pulse = 8 + 3 * Math.sin(nowMs / 250);
    context.lineWidth = 2;
    context.strokeStyle = marker.phase === "preparing" ? AMBER : colour;
    context.beginPath(); context.arc(castle.x, castle.y, pulse, 0, Math.PI * 2); context.stroke();
    context.lineWidth = 1;
    let label;
    if (dest) {
      const target = view.toScreen(dest[0], dest[1]);
      // Out and back run the same line, so the dashes move the way the troops do.
      context.strokeStyle = marker.phase === "home" ? FAINT : colour;
      context.setLineDash([5, 5]);
      const direction = marker.phase === "back" ? -1 : 1;
      context.lineDashOffset = -direction * ((nowMs / 60) % 10);
      context.beginPath(); context.moveTo(castle.x, castle.y); context.lineTo(target.x, target.y); context.stroke();
      context.setLineDash([]); context.lineDashOffset = 0;
      context.strokeStyle = colour;
      context.beginPath(); context.moveTo(target.x, target.y - 5); context.lineTo(target.x + 5, target.y); context.lineTo(target.x, target.y + 5); context.lineTo(target.x - 5, target.y); context.closePath(); context.stroke();
      context.fillStyle = colour;
      context.fillText(`station ${dest[0]}:${dest[1]}`, target.x + 8, target.y + 12);
      if (marker.phase === "out" || marker.phase === "back") {
        const here = view.toScreen(...Object.values(pointOnPath({ x: cx, y: cy }, { x: dest[0], y: dest[1] }, marker.fraction)));
        context.fillStyle = colour;
        context.beginPath(); context.arc(here.x, here.y, 3.5, 0, Math.PI * 2); context.fill();
      }
    }
    if (marker.phase === "preparing") {
      label = job.next_at_ms ? `preparing · ${job.doing.replace(/^next: /, "")} in ${shortDuration(Math.max(0, job.next_at_ms - nowMs))}` : job.doing;
    } else if (marker.phase === "out") {
      label = marker.remainingMs == null ? "support out" : `support out · recall in ${shortDuration(marker.remainingMs)}`;
    } else if (marker.phase === "back") {
      label = `support returning · home in ${shortDuration(marker.remainingMs)}`;
    } else {
      label = "support home";
    }
    context.fillStyle = marker.phase === "preparing" ? AMBER : job.phase === "uncertain" ? "#ff7a7a" : OK;
    context.fillText(label, castle.x + 12, castle.y - 10);
    const next = job.attacks.filter((attack) => !attack.retreated && attack.landing_ms > nowMs).map((attack) => attack.landing_ms).sort((a, b) => a - b)[0];
    if (next) {
      context.fillStyle = "#ff7a7a";
      context.fillText(`attack lands in ${shortDuration(next - nowMs)}`, castle.x + 12, castle.y + 22);
    }
  }
}
