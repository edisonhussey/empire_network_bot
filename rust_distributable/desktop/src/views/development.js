import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { logPosition, shareInBand, shortDuration, stepProgress, waitAt } from "./dev-math.js";
import { drawHistogram, renderStationPanel } from "./dev-station.js";
import { canvasSize, createKingdomMap } from "./kingdom-map.js";
import { createPaceSlider } from "./pace-slider.js";

/// The Development tab: a read-only window onto the scheduler of the account in view.
///
/// It observes. Nothing here starts, stops or changes a bot, and it asks the service
/// only for snapshots it already keeps. While the tab is hidden every timer and
/// animation frame is stopped; the backend is unaffected either way.
///
/// The charts and panels are about the account chosen in the header. The maps are one
/// per signed-in account on that account's world (`kingdom-map.js`), each in its own
/// colour, so what one account is doing can be watched from another's tab: an attack on
/// one, the support it sends, the retreat that follows. Accounts on other worlds are
/// never shown. The timing chart is not a clock trace: the generator is stepped once
/// per action (`t = t + 1`), so it shows the last waits actually used and the next ones
/// the generator would give.

const HISTORY_STEPS = 40;
const AXIS_LOW_S = 0.5;
const AXIS_HIGH_S = 120;
const INK = "#fff";
const DIM = "rgba(255,255,255,.40)";
const FAINT = "rgba(255,255,255,.16)";
const OK = "#5ed08a";

export function createDevelopment() {
  const waveCanvas = document.querySelector("#dev-wave");
  const histCanvas = document.querySelector("#dev-hist");
  const mapsHost = document.querySelector("#dev-maps");
  // The test pace is the operator's to set and the service's to keep; the slider shows
  // what the service last reported unless it is being held.
  const paceSlider = createPaceSlider({
    host: document.querySelector("#dev-pace"),
    onChange: (pace) => api("/dev/pace", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ account_id: accountContext.viewed() || null, pace }) }).catch(() => {}),
  });
  const state = {
    open: false,
    snapshot: null,
    frame: 0,
    timers: [],
    lastText: 0,
    error: "",
    maps: new Map(),
  };

  const text = (id, value) => {
    const node = document.querySelector(id);
    if (node && node.textContent !== value) node.textContent = value;
  };

  // ---- data ---------------------------------------------------------------

  async function pollSnapshot() {
    try {
      state.snapshot = await api(`/dev${accountContext.query()}`);
      paceSlider.set(state.snapshot.timing.pace);
      state.error = "";
      syncMaps();
      renderLists();
    } catch (error) {
      state.error = error.message;
    }
  }

  // ---- maps: one per signed-in account on the viewed account's world ---------------

  /// The accounts that get a map: the one in view, then every other signed-in account
  /// on its world. Maps appear as accounts sign in and go as they sign out.
  function wantedAccounts() {
    const viewed = accountContext.viewed();
    if (!viewed) return [];
    const mates = accountContext.mates(viewed).map((account) => account.account_id.toLowerCase());
    return [viewed, ...mates.filter((id) => id !== viewed)];
  }

  function syncMaps() {
    if (!mapsHost) return;
    const wanted = wantedAccounts();
    for (const [id, map] of state.maps) {
      if (!wanted.includes(id)) { map.destroy(); state.maps.delete(id); }
    }
    for (const id of wanted) {
      if (state.maps.has(id)) { state.maps.get(id).refreshTitle(); continue; }
      const map = createKingdomMap({ host: mapsHost, accountId: id });
      state.maps.set(id, map);
      if (state.open) map.start();
    }
    // The account in view comes first.
    wanted.forEach((id, index) => { const card = state.maps.get(id)?.element; if (card && mapsHost.children[index] !== card) mapsHost.insertBefore(card, mapsHost.children[index] || null); });
  }

  // ---- waveform -------------------------------------------------------------

  /// The generator's wave sum as one continuous curve, with the waits actually used
  /// (filled) and the next ones (hollow) sitting on it at whole steps. The window slides
  /// with time: "now" is a fixed line and the curve flows past it, moving through each
  /// wait as it elapses, so nothing jumps when an action happens. The curve is the signal
  /// without noise, so a used wait lies a little off it; one drawn before the last change
  /// of pace keeps the pace it was waited at.
  function drawWave(now) {
    const { context, width, height } = canvasSize(waveCanvas);
    context.clearRect(0, 0, width, height);
    const snap = state.snapshot;
    context.font = "10px ui-monospace, Menlo, monospace";
    if (!snap || !snap.timing.waves.length) {
      context.fillStyle = DIM; context.fillText("Waiting for a running session…", 12, 24);
      return;
    }
    const timing = snap.timing;
    const pace = paceSlider.get();
    const ahead = Math.max(1, timing.projection.length);
    const pad = { l: 44, r: 12, t: 14, b: 22 };
    const last = timing.last;
    // "Now" is part-way through the wait in progress: that wait was generated at step -1.
    const here = last ? -1 + stepProgress(last.at_ms, snap.scheduler.next_action_at_ms, now) : -1;
    const span = HISTORY_STEPS + ahead;
    const x = (offset) => pad.l + ((offset - here + HISTORY_STEPS) / span) * (width - pad.l - pad.r);
    const high = AXIS_HIGH_S * Math.max(1, pace);
    const y = (seconds) => pad.t + (1 - logPosition(seconds, AXIS_LOW_S, high)) * (height - pad.t - pad.b);
    const inside = (px) => px >= pad.l - 3 && px <= width - pad.r + 3;

    // The band the design aims about 60 % of waits into (at pace 1).
    const [bandLow, bandHigh] = timing.target_band_s;
    context.fillStyle = "rgba(94,208,138,.10)";
    context.fillRect(pad.l, y(bandHigh * pace), width - pad.l - pad.r, y(bandLow * pace) - y(bandHigh * pace));
    context.lineWidth = 1; context.strokeStyle = FAINT; context.fillStyle = DIM;
    for (const seconds of [1, 2, 5, 10, 30, 60, 120, 240]) {
      if (seconds > high) break;
      context.beginPath(); context.moveTo(pad.l, y(seconds)); context.lineTo(width - pad.r, y(seconds)); context.stroke();
      context.fillText(`${seconds}s`, 8, y(seconds) + 3);
    }
    context.setLineDash([2, 4]); context.strokeStyle = DIM;
    context.beginPath(); context.moveTo(pad.l, y(timing.floor_s * pace)); context.lineTo(width - pad.r, y(timing.floor_s * pace)); context.stroke();
    context.setLineDash([]);
    context.fillText(`floor ${(timing.floor_s * pace).toFixed(2)}s`, width - 84, y(timing.floor_s * pace) - 4);

    // The continuous curve, one sample every couple of pixels. Behind "now" it is the
    // signal as it stands, ahead of it the projection.
    const first = here - HISTORY_STEPS;
    const samples = Math.max(2, Math.floor((width - pad.l - pad.r) / 2));
    context.lineWidth = 1.5; context.strokeStyle = "rgba(94,208,138,.55)"; context.beginPath();
    for (let i = 0; i <= samples; i += 1) {
      const offset = first + (i / samples) * span;
      const px = x(offset); const py = y(waitAt(timing, offset, pace));
      if (i === 0) context.moveTo(px, py); else context.lineTo(px, py);
    }
    context.stroke(); context.lineWidth = 1;

    // Waits actually used, placed by their own step number.
    for (const item of timing.recent.slice(0, HISTORY_STEPS)) {
      const px = x(item.index - timing.index);
      if (!inside(px)) continue;
      context.fillStyle = OK; context.beginPath(); context.arc(px, y(item.waited_s), 2.4, 0, Math.PI * 2); context.fill();
      if (item.applied_s - item.waited_s > 0.05) {
        // A global limit made this one longer than the generator's own wait.
        context.strokeStyle = "rgba(255,255,255,.6)"; context.beginPath(); context.arc(px, y(item.applied_s), 3, 0, Math.PI * 2); context.stroke();
      }
    }
    // The next waits as the generator stands now, hollow: a projection.
    context.strokeStyle = INK;
    for (let step = 0; step < timing.projection.length; step += 1) {
      const px = x(step);
      if (!inside(px)) continue;
      context.beginPath(); context.arc(px, y(waitAt(timing, step, pace)), 2.4, 0, Math.PI * 2); context.stroke();
    }

    // "Now": a fixed line the curve flows through.
    const nowX = x(here);
    context.strokeStyle = "rgba(255,255,255,.5)"; context.beginPath(); context.moveTo(nowX, pad.t); context.lineTo(nowX, height - pad.b); context.stroke();
    context.fillStyle = DIM;
    context.fillText("used", nowX - 44, height - 6);
    context.fillText(pace === 1 ? "next waits: a projection" : `next waits at ×${pace.toFixed(2)}`, nowX + 8, height - 6);
  }

  // ---- text panels ----------------------------------------------------------

  function updateText() {
    const snap = state.snapshot;
    if (!snap) { text("#dev-state", state.error || "Waiting for a running session…"); return; }
    const now = Date.now();
    const timing = snap.timing; const sched = snap.scheduler; const last = timing.last;
    text("#dev-signal", timing.signal_now.toFixed(3));
    text("#dev-base", `${timing.floor_s.toFixed(2)} s`);
    text("#dev-min", String(timing.index));
    text("#dev-interval", last ? `${last.raw_s.toFixed(2)} s` : "–");
    text("#dev-applied", last ? `${last.applied_s.toFixed(2)} s${last.applied_s - last.raw_s > 0.05 ? ` (+${(last.applied_s - last.raw_s).toFixed(2)} s from limits)` : ""}` : "–");
    text("#dev-parts", last ? `wave ${last.signal.toFixed(2)} · noise ${last.noise.toFixed(2)} · protocol min ${last.protocol_floor_s.toFixed(2)} s` : "–");
    const share = shareInBand(timing.recent.map((item) => item.raw_s), timing.target_band_s[0], timing.target_band_s[1]);
    text("#dev-share", share == null ? "–" : `${Math.round(share * 100)}% of ${timing.recent.length}`);
    const next = sched.next_action_at_ms;
    text("#dev-countdown", next == null ? "–" : next <= now ? "due" : shortDuration(next - now));
    text("#dev-state", sched.state.replaceAll("_", " "));
    text("#dev-detail", sched.detail || "–");
    text("#dev-restriction", sched.restriction || "none");
    text("#dev-kingdom", sched.kingdom_id == null ? "–" : String(sched.kingdom_id));
    text("#dev-eligible", sched.eligible_towers == null ? "–" : String(sched.eligible_towers));
    text("#dev-local", sched.local_towers == null ? "–" : `${sched.local_towers}${sched.spotlight_has_local ? "" : " (spotlight will relocate)"}`);
    const done = sched.last_completed;
    text("#dev-completed", done ? `#${done.march_id} → ${done.x}:${done.y} · commander ${done.lord_id} · ${shortDuration(now - done.at_ms)} ago` : "–");
    const health = snap.health;
    text("#dev-health-count", `${health.count_last_hour} of ${health.tolerance} tolerated`);
    text("#dev-health-state", health.paused ? "Paused: no new actions" : health.count_last_hour >= health.tolerance ? "Tolerance reached" : health.count_last_hour > 0 ? "Warning" : "Healthy");
    document.querySelector("#dev-health-state")?.classList.toggle("dev-bad", health.paused);
  }

  function renderLists() {
    const snap = state.snapshot;
    if (!snap) return;
    const recent = document.querySelector("#dev-recent");
    recent.replaceChildren(...snap.timing.recent.slice(0, 10).map((item) => {
      const row = element("li");
      row.append(element("span", "", `#${item.index}`), element("span", "", ({ before_attack: "before attack", station: "station step" })[item.wait] || "after ack"), element("b", "", `${item.applied_s.toFixed(2)} s`));
      if (item.restriction) row.append(element("small", "", item.restriction));
      return row;
    }));
    if (!snap.timing.recent.length) recent.append(element("li", "dev-none", "No intervals yet."));
    const incidents = document.querySelector("#dev-incidents");
    incidents.replaceChildren(...snap.health.incidents.map((item) => {
      const row = element("li", item.qualifies ? "" : "dev-quiet");
      row.append(element("span", "", new Date(item.at_ms).toLocaleTimeString()), element("span", "", item.outcome.replaceAll("_", " ")), element("small", "", item.detail));
      return row;
    }));
    if (!snap.health.incidents.length) incidents.append(element("li", "dev-none", "No incidents in the last hour."));
    renderStationPanel(snap.station, Date.now());
  }

  // ---- loop -----------------------------------------------------------------

  function frame(timestamp) {
    if (!state.open) return;
    drawWave(Date.now());
    drawHistogram(histCanvas, canvasSize, state.snapshot?.timing.histogram, state.snapshot?.timing.target_band_s ?? [1, 5]);
    const now = Date.now();
    for (const map of state.maps.values()) map.draw(now);
    if (timestamp - state.lastText > 250) { state.lastText = timestamp; updateText(); }
    state.frame = requestAnimationFrame(frame);
  }

  function start() {
    if (state.open) return;
    state.open = true;
    syncMaps();
    for (const map of state.maps.values()) map.start();
    pollSnapshot();
    state.timers.push(setInterval(pollSnapshot, 1000));
    state.frame = requestAnimationFrame(frame);
  }

  function stop() {
    state.open = false;
    state.timers.forEach(clearInterval); state.timers = [];
    for (const map of state.maps.values()) map.stop();
    cancelAnimationFrame(state.frame);
  }

  // Another account in view: its own charts, and a map for each account on its world.
  accountContext.subscribe(() => {
    state.snapshot = null;
    paceSlider.set(1);
    for (const map of state.maps.values()) map.destroy();
    state.maps.clear();
    if (state.open) { syncMaps(); for (const map of state.maps.values()) map.start(); pollSnapshot(); }
  });
  document.addEventListener("visibilitychange", () => { if (document.hidden) stop(); else if (!document.querySelector("#view-development").hidden) start(); });

  return { show: start, hide: stop };
}
