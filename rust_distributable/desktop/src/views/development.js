import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { logPosition, shareInBand, shortDuration } from "./dev-math.js";
import { drawHistogram, renderStationPanel } from "./dev-station.js";
import { canvasSize, createKingdomMap } from "./kingdom-map.js";

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

  function drawWave() {
    const { context, width, height } = canvasSize(waveCanvas);
    context.clearRect(0, 0, width, height);
    const snap = state.snapshot;
    context.font = "10px ui-monospace, Menlo, monospace";
    if (!snap || !snap.timing.waves.length) {
      context.fillStyle = DIM; context.fillText("Waiting for a running session…", 12, 24);
      return;
    }
    const timing = snap.timing;
    const ahead = Math.max(1, timing.projection.length);
    const pad = { l: 44, r: 12, t: 14, b: 22 };
    // Step offsets run from -HISTORY_STEPS (oldest shown) to ahead-1 (furthest projection);
    // 0 is the next wait to be generated.
    const x = (offset) => pad.l + ((offset + HISTORY_STEPS + 0.5) / (HISTORY_STEPS + ahead)) * (width - pad.l - pad.r);
    const y = (seconds) => pad.t + (1 - logPosition(seconds, AXIS_LOW_S, AXIS_HIGH_S)) * (height - pad.t - pad.b);

    // The band the design aims about 60 % of waits into.
    const [bandLow, bandHigh] = timing.target_band_s;
    context.fillStyle = "rgba(94,208,138,.10)";
    context.fillRect(pad.l, y(bandHigh), width - pad.l - pad.r, y(bandLow) - y(bandHigh));
    context.lineWidth = 1; context.strokeStyle = FAINT; context.fillStyle = DIM;
    for (const seconds of [1, 2, 5, 10, 30, 60]) {
      context.beginPath(); context.moveTo(pad.l, y(seconds)); context.lineTo(width - pad.r, y(seconds)); context.stroke();
      context.fillText(`${seconds}s`, 8, y(seconds) + 3);
    }
    context.setLineDash([2, 4]); context.strokeStyle = DIM;
    context.beginPath(); context.moveTo(pad.l, y(timing.floor_s)); context.lineTo(width - pad.r, y(timing.floor_s)); context.stroke();
    context.setLineDash([]);
    context.fillText(`floor ${timing.floor_s.toFixed(2)}s`, width - 84, y(timing.floor_s) - 4);

    // Waits actually used, oldest to newest, ending just before "now".
    const used = timing.recent.slice(0, HISTORY_STEPS).reverse();
    context.strokeStyle = "rgba(94,208,138,.35)"; context.beginPath();
    used.forEach((item, position) => {
      const px = x(position - used.length); const py = y(item.waited_s);
      if (position === 0) context.moveTo(px, py); else context.lineTo(px, py);
    });
    context.stroke();
    used.forEach((item, position) => {
      const px = x(position - used.length);
      context.fillStyle = OK; context.beginPath(); context.arc(px, y(item.waited_s), 2.4, 0, Math.PI * 2); context.fill();
      if (item.applied_s - item.waited_s > 0.05) {
        // A global limit made this one longer than the generator's own wait.
        context.strokeStyle = "rgba(255,255,255,.6)"; context.beginPath(); context.arc(px, y(item.applied_s), 3, 0, Math.PI * 2); context.stroke();
      }
    });

    // The next waits as the generator stands now. Dashed and hollow: a projection.
    context.setLineDash([3, 4]); context.strokeStyle = DIM; context.beginPath();
    timing.projection.forEach((item, position) => {
      const px = x(position); const py = y(item.interval_s);
      if (position === 0) context.moveTo(px, py); else context.lineTo(px, py);
    });
    context.stroke(); context.setLineDash([]);
    context.strokeStyle = INK;
    timing.projection.forEach((item, position) => {
      context.beginPath(); context.arc(x(position), y(item.interval_s), 2.4, 0, Math.PI * 2); context.stroke();
    });

    context.strokeStyle = FAINT; context.beginPath(); context.moveTo(x(-0.5), pad.t); context.lineTo(x(-0.5), height - pad.b); context.stroke();
    context.fillStyle = DIM;
    context.fillText("used", x(-HISTORY_STEPS / 2), height - 6);
    context.fillText("projection of the next waits, not a promise", x(1), height - 6);
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
    drawWave();
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
    for (const map of state.maps.values()) map.destroy();
    state.maps.clear();
    if (state.open) { syncMaps(); for (const map of state.maps.values()) map.start(); pollSnapshot(); }
  });
  document.addEventListener("visibilitychange", () => { if (document.hidden) stop(); else if (!document.querySelector("#view-development").hidden) start(); });

  return { show: start, hide: stop };
}
