import { api } from "../api.js";
import { element } from "../dom.js";
import {
  DEFAULT_VIEW, clampSize, endpoints, intervalFromSignal, makeViewport, movementState, panBy,
  pointOnPath, shortDuration, signalAt, visible, zoomAbout,
} from "./dev-math.js";

/// The Development tab: a read-only window onto the scheduler.
///
/// It observes. Nothing here starts, stops or changes the bot, and it asks the
/// service only for snapshots it already keeps. While the tab is hidden every timer
/// and animation frame is stopped; the backend is unaffected either way.
///
/// Polling is deliberately slow (one snapshot a second, movements every two, the
/// map when it opens, the kingdom changes, or every half minute for cooldowns).
/// Smoothness comes from interpolating those snapshots against the clock on every
/// animation frame: the waveform evolves from its parameters and each march is
/// placed from its start and end timestamps, so no backend timer exists per
/// movement and no frames are pushed.

const HISTORY_WINDOW_MS = 180_000;
const PROJECTION_MS = 60_000;
const MAP_REFRESH_MS = 30_000;
const INK = "#fff";
const DIM = "rgba(255,255,255,.40)";
const FAINT = "rgba(255,255,255,.16)";
const OK = "#5ed08a";

export function createDevelopment() {
  const waveCanvas = document.querySelector("#dev-wave");
  const mapCanvas = document.querySelector("#dev-map");
  const state = {
    open: false,
    snapshot: null,
    map: null,
    movements: [],
    movementsAt: 0,
    mapAt: 0,
    mapKingdom: null,
    viewport: null,
    viewSize: DEFAULT_VIEW,
    follow: "castle",
    frame: 0,
    timers: [],
    lastText: 0,
    drag: null,
    error: "",
  };

  const text = (id, value) => {
    const node = document.querySelector(id);
    if (node && node.textContent !== value) node.textContent = value;
  };

  // ---- data ---------------------------------------------------------------

  async function pollSnapshot() {
    try {
      state.snapshot = await api("/dev");
      state.error = "";
      const kingdom = state.snapshot.spatial?.kingdom_id ?? null;
      if (kingdom !== null && kingdom !== state.mapKingdom) await pollMap(true);
      renderLists();
    } catch (error) {
      state.error = error.message;
    }
  }

  async function pollMap(force = false) {
    if (!force && Date.now() - state.mapAt < MAP_REFRESH_MS) return;
    const kingdom = state.snapshot?.spatial?.kingdom_id;
    try {
      state.map = await api(kingdom == null ? "/dev/map" : `/dev/map?kingdom=${kingdom}`);
      state.mapKingdom = state.map.kingdom_id;
      state.mapAt = Date.now();
      if (!state.viewport) recentre();
    } catch (_) { /* no account connected yet */ }
  }

  async function pollMovements() {
    try {
      const kingdom = state.mapKingdom ?? state.snapshot?.spatial?.kingdom_id;
      const data = await api(kingdom == null ? "/dev/movements" : `/dev/movements?kingdom=${kingdom}`);
      state.movements = data.movements;
      state.movementsAt = Date.now();
    } catch (_) { state.movements = []; }
  }

  // ---- viewport -------------------------------------------------------------

  function canvasSize(canvas) {
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(canvas.clientWidth));
    const height = Math.max(1, Math.round(canvas.clientHeight));
    if (canvas.width !== width * ratio || canvas.height !== height * ratio) {
      canvas.width = width * ratio;
      canvas.height = height * ratio;
    }
    const context = canvas.getContext("2d");
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    return { context, width, height };
  }

  function focusPoint() {
    const spatial = state.snapshot?.spatial;
    if (state.follow === "spotlight" && spatial) return { x: spatial.spotlight[0], y: spatial.spotlight[1] };
    const castle = state.map?.castle || spatial?.castle;
    return castle ? { x: castle[0], y: castle[1] } : { x: 500, y: 500 };
  }

  function recentre(follow = state.follow) {
    state.follow = follow;
    const { width, height } = canvasSize(mapCanvas);
    const point = focusPoint();
    state.viewport = makeViewport(point.x, point.y, state.viewSize, width, height);
  }

  function setSize(size) {
    state.viewSize = clampSize(size);
    const { width, height } = canvasSize(mapCanvas);
    const v = state.viewport || makeViewport(500, 500, state.viewSize, width, height);
    state.viewport = makeViewport(v.cx, v.cy, state.viewSize, width, height);
  }

  // ---- waveform -------------------------------------------------------------

  function drawWave() {
    const { context, width, height } = canvasSize(waveCanvas);
    context.clearRect(0, 0, width, height);
    const snap = state.snapshot;
    if (!snap || !snap.timing.waves.length) {
      context.fillStyle = DIM; context.font = "12px ui-monospace, Menlo, monospace";
      context.fillText("Waiting for a running session…", 12, 24);
      return;
    }
    const timing = snap.timing;
    const now = Date.now();
    const left = now - HISTORY_WINDOW_MS;
    const right = now + PROJECTION_MS;
    const budget = timing.waves.reduce((sum, wave) => sum + wave.amplitude, 0) || 1;
    const pad = { l: 40, r: 12, t: 12, b: 22 };
    const x = (t) => pad.l + ((t - left) / (right - left)) * (width - pad.l - pad.r);
    const y = (v) => pad.t + (1 - (v + budget * 1.25) / (budget * 2.5)) * (height - pad.t - pad.b);

    context.lineWidth = 1; context.font = "10px ui-monospace, Menlo, monospace";
    context.strokeStyle = FAINT; context.fillStyle = DIM;
    for (const level of [-budget, 0, budget]) {
      context.beginPath(); context.moveTo(pad.l, y(level)); context.lineTo(width - pad.r, y(level)); context.stroke();
      context.fillText(level === 0 ? "0" : level.toFixed(2), 4, y(level) + 3);
    }
    for (let seconds = -150; seconds <= 60; seconds += 30) {
      const px = x(now + seconds * 1000);
      context.beginPath(); context.moveTo(px, height - pad.b); context.lineTo(px, height - pad.b + 4); context.stroke();
      context.fillText(seconds === 0 ? "now" : `${seconds > 0 ? "+" : ""}${seconds}s`, px - 10, height - 6);
    }

    // Recorded history from the backend, then the live edge evolved from the
    // current parameters so the line keeps moving between snapshots.
    context.strokeStyle = INK; context.lineWidth = 1.25; context.beginPath();
    let started = false;
    for (const [t, value] of timing.history) {
      if (t < left || t > snap.at_ms) continue;
      const px = x(t);
      if (started) context.lineTo(px, y(value)); else { context.moveTo(px, y(value)); started = true; }
    }
    for (let t = snap.at_ms; t <= now; t += 1000) {
      const px = x(t); const py = y(signalAt(timing.waves, snap.at_ms, t));
      if (started) context.lineTo(px, py); else { context.moveTo(px, py); started = true; }
    }
    context.stroke();

    // The projection, drawn dashed and labelled as one.
    context.setLineDash([4, 4]); context.strokeStyle = DIM; context.beginPath();
    for (let t = now; t <= right; t += 1000) {
      const px = x(t); const py = y(signalAt(timing.waves, snap.at_ms, t));
      if (t === now) context.moveTo(px, py); else context.lineTo(px, py);
    }
    context.stroke(); context.setLineDash([]);
    context.fillStyle = DIM; context.fillText("projection, not a promise", x(now) + 8, pad.t + 10);

    // Intervals actually generated: signal + noise. The gap between each dot and
    // the line is the stochastic residual.
    context.fillStyle = OK; context.strokeStyle = "rgba(94,208,138,.45)";
    for (const sample of timing.recent) {
      if (sample.at_ms < left) continue;
      const px = x(sample.at_ms);
      context.beginPath(); context.moveTo(px, y(sample.signal)); context.lineTo(px, y(sample.signal + sample.noise)); context.stroke();
      context.beginPath(); context.arc(px, y(sample.signal + sample.noise), 2.2, 0, Math.PI * 2); context.fill();
    }

    context.strokeStyle = FAINT; context.beginPath(); context.moveTo(x(now), pad.t); context.lineTo(x(now), height - pad.b); context.stroke();
    const live = signalAt(timing.waves, snap.at_ms, now);
    context.fillStyle = INK; context.beginPath(); context.arc(x(now), y(live), 3, 0, Math.PI * 2); context.fill();
  }

  // ---- map ------------------------------------------------------------------

  function arrowHead(context, from, to, size) {
    const angle = Math.atan2(to.y - from.y, to.x - from.x);
    context.beginPath();
    context.moveTo(to.x, to.y);
    context.lineTo(to.x - size * Math.cos(angle - 0.45), to.y - size * Math.sin(angle - 0.45));
    context.moveTo(to.x, to.y);
    context.lineTo(to.x - size * Math.cos(angle + 0.45), to.y - size * Math.sin(angle + 0.45));
    context.stroke();
  }

  function drawMap() {
    const { context, width, height } = canvasSize(mapCanvas);
    context.clearRect(0, 0, width, height);
    if (!state.viewport) recentre();
    // Keep the window on what it is following (the castle or the moving
    // spotlight) unless the user has dragged away.
    if (state.follow !== "free") {
      const point = focusPoint();
      state.viewport = makeViewport(point.x, point.y, state.viewSize, width, height);
    } else {
      state.viewport = makeViewport(state.viewport.cx, state.viewport.cy, state.viewSize, width, height);
    }
    const view = state.viewport;
    const map = state.map;
    const now = Date.now();

    // Coordinate ticks every 10, thin and quiet.
    context.lineWidth = 1; context.strokeStyle = "rgba(255,255,255,.06)"; context.fillStyle = DIM; context.font = "10px ui-monospace, Menlo, monospace";
    const step = view.size > 300 ? 50 : view.size > 120 ? 20 : 10;
    for (let gx = Math.ceil(view.bounds.left / step) * step; gx <= view.bounds.right; gx += step) {
      const px = view.toScreen(gx, 0).x;
      context.beginPath(); context.moveTo(px, 0); context.lineTo(px, height); context.stroke();
      context.fillText(String(gx), px + 3, 11);
    }
    for (let gy = Math.ceil(view.bounds.top / step) * step; gy <= view.bounds.bottom; gy += step) {
      const py = view.toScreen(0, gy).y;
      context.beginPath(); context.moveTo(0, py); context.lineTo(width, py); context.stroke();
      context.fillText(String(gy), 3, py - 3);
    }

    if (!map) {
      context.fillStyle = DIM; context.fillText("Connect an account to see its kingdom.", 12, height / 2);
      return;
    }
    const castle = map.castle ? { x: map.castle[0], y: map.castle[1] } : null;
    const radius = Math.max(1.6, Math.min(4, view.scale * 0.5));

    // Towers: white circles, solid when ready, hollow while cooling.
    context.lineWidth = 1;
    for (const tower of map.towers) {
      if (!visible(view, tower.x, tower.y, 2)) continue;
      const p = view.toScreen(tower.x, tower.y);
      context.beginPath(); context.arc(p.x, p.y, radius, 0, Math.PI * 2);
      if (tower.ready_at_ms <= now) { context.fillStyle = INK; context.fill(); }
      else { context.strokeStyle = "rgba(255,255,255,.45)"; context.stroke(); }
    }

    // Spotlight, its local radius, and the direction it is heading.
    const spatial = state.snapshot?.spatial;
    if (spatial && spatial.kingdom_id === map.kingdom_id) {
      const spot = view.toScreen(spatial.spotlight[0], spatial.spotlight[1]);
      context.strokeStyle = "rgba(94,208,138,.7)"; context.setLineDash([3, 4]);
      context.beginPath(); context.arc(spot.x, spot.y, spatial.local_radius * view.scale, 0, Math.PI * 2); context.stroke();
      context.setLineDash([]);
      context.beginPath(); context.moveTo(spot.x - 5, spot.y); context.lineTo(spot.x + 5, spot.y); context.moveTo(spot.x, spot.y - 5); context.lineTo(spot.x, spot.y + 5); context.stroke();
      const v = spatial.velocity;
      if (Math.hypot(v[0], v[1]) > 0.05) {
        const tip = view.toScreen(spatial.spotlight[0] + v[0] * 3, spatial.spotlight[1] + v[1] * 3);
        context.beginPath(); context.moveTo(spot.x, spot.y); context.lineTo(tip.x, tip.y); context.stroke();
        arrowHead(context, spot, tip, 6);
      }
      if (spatial.selected) {
        const sel = view.toScreen(spatial.selected[0], spatial.selected[1]);
        context.strokeStyle = INK; context.beginPath(); context.arc(sel.x, sel.y, radius + 3, 0, Math.PI * 2); context.stroke();
      }
    }

    // Marches, placed from their timestamps on every frame.
    if (castle) {
      context.font = "10px ui-monospace, Menlo, monospace";
      for (const movement of state.movements) {
        const s = movementState(movement, now);
        if (s.status === "completed") continue;
        const { from, to } = endpoints(movement, castle);
        if (!visible(view, from.x, from.y, 400) && !visible(view, to.x, to.y, 400)) continue;
        const a = view.toScreen(from.x, from.y); const b = view.toScreen(to.x, to.y);
        const outbound = movement.phase === "outbound";
        if (s.status === "unknown" || s.status === "arrived") {
          // Return time unknown: say so, with a dashed line and no marker.
          context.strokeStyle = FAINT; context.setLineDash([2, 4]);
          context.beginPath(); context.moveTo(a.x, a.y); context.lineTo(b.x, b.y); context.stroke(); context.setLineDash([]);
          context.fillStyle = DIM; context.fillText(s.status === "arrived" ? "landed, return unknown" : "return unknown", b.x + 6, b.y - 6);
          continue;
        }
        const here = view.toScreen(...Object.values(pointOnPath(from, to, s.progress)));
        // The path ahead of the marker stays; behind it the line has gone.
        context.strokeStyle = outbound ? "rgba(255,255,255,.6)" : "rgba(94,208,138,.7)"; context.lineWidth = 1;
        context.beginPath(); context.moveTo(here.x, here.y); context.lineTo(b.x, b.y); context.stroke();
        arrowHead(context, here, { x: here.x + (b.x - here.x) * 0.15, y: here.y + (b.y - here.y) * 0.15 }, 5);
        context.fillStyle = outbound ? INK : OK;
        context.beginPath(); context.arc(here.x, here.y, 3, 0, Math.PI * 2); context.fill();
        context.fillStyle = DIM;
        context.fillText(`${outbound ? "out" : "back"} ${shortDuration(s.remainingMs)}`, here.x + 6, here.y - 6);
      }
    }

    // The main castle: a white square.
    if (castle && visible(view, castle.x, castle.y, 4)) {
      const p = view.toScreen(castle.x, castle.y);
      context.fillStyle = INK; context.fillRect(p.x - 5, p.y - 5, 10, 10);
    }

    context.fillStyle = DIM; context.textAlign = "right";
    context.fillText(`${Math.round(view.size)} × ${Math.round(view.size)} · kingdom ${map.kingdom_id}`, width - 8, height - 8);
    context.textAlign = "left";
  }

  // ---- text panels ----------------------------------------------------------

  function updateText() {
    const snap = state.snapshot;
    if (!snap) { text("#dev-state", state.error || "Waiting for a running session…"); return; }
    const now = Date.now();
    const timing = snap.timing; const sched = snap.scheduler; const last = timing.last;
    text("#dev-signal", signalAt(timing.waves, snap.at_ms, now).toFixed(3));
    text("#dev-base", `${timing.baseline_s.toFixed(2)} s`);
    text("#dev-min", `${timing.min_interval_s.toFixed(2)} s`);
    text("#dev-interval", last ? `${last.stochastic_s.toFixed(2)} s` : "–");
    text("#dev-applied", last ? `${last.applied_s.toFixed(2)} s${last.applied_s - last.stochastic_s > 0.05 ? ` (+${(last.applied_s - last.stochastic_s).toFixed(2)} s by limit)` : ""}` : "–");
    text("#dev-parts", last ? `wave ${last.signal.toFixed(2)} · noise ${last.noise.toFixed(2)} · floor ${last.floor_s.toFixed(2)} s` : "–");
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
      row.append(element("span", "", new Date(item.at_ms).toLocaleTimeString()), element("span", "", item.wait === "before_attack" ? "before attack" : "after ack"), element("b", "", `${item.applied_s.toFixed(2)} s`));
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
  }

  // ---- loop -----------------------------------------------------------------

  function frame(timestamp) {
    if (!state.open) return;
    drawWave();
    drawMap();
    if (timestamp - state.lastText > 250) { state.lastText = timestamp; updateText(); }
    state.frame = requestAnimationFrame(frame);
  }

  function start() {
    if (state.open) return;
    state.open = true;
    pollSnapshot().then(() => pollMap(true)).then(pollMovements);
    state.timers.push(setInterval(pollSnapshot, 1000));
    state.timers.push(setInterval(pollMovements, 2000));
    state.timers.push(setInterval(() => pollMap(), 5000));
    state.frame = requestAnimationFrame(frame);
  }

  function stop() {
    state.open = false;
    state.timers.forEach(clearInterval); state.timers = [];
    cancelAnimationFrame(state.frame);
  }

  // ---- controls -------------------------------------------------------------

  mapCanvas.addEventListener("pointerdown", (event) => {
    mapCanvas.setPointerCapture(event.pointerId);
    state.drag = { x: event.clientX, y: event.clientY };
  });
  mapCanvas.addEventListener("pointermove", (event) => {
    if (!state.drag || !state.viewport) return;
    const { width, height } = canvasSize(mapCanvas);
    state.viewport = panBy(state.viewport, event.clientX - state.drag.x, event.clientY - state.drag.y, width, height);
    state.drag = { x: event.clientX, y: event.clientY };
    state.follow = "free";
  });
  const endDrag = () => { state.drag = null; };
  mapCanvas.addEventListener("pointerup", endDrag);
  mapCanvas.addEventListener("pointercancel", endDrag);
  mapCanvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    if (!state.viewport) return;
    const { width, height } = canvasSize(mapCanvas);
    const rect = mapCanvas.getBoundingClientRect();
    const anchor = state.viewport.toMap(event.clientX - rect.left, event.clientY - rect.top);
    state.viewport = zoomAbout(state.viewport, event.deltaY > 0 ? 1.15 : 1 / 1.15, anchor, width, height);
    state.viewSize = state.viewport.size;
    state.follow = "free";
  }, { passive: false });
  const on = (selector, handler) => document.querySelector(selector)?.addEventListener("click", handler);
  on("#dev-zoom-in", () => setSize(state.viewSize / 1.4));
  on("#dev-zoom-out", () => setSize(state.viewSize * 1.4));
  on("#dev-reset", () => { state.viewSize = DEFAULT_VIEW; recentre("castle"); });
  on("#dev-centre-castle", () => recentre("castle"));
  on("#dev-centre-spotlight", () => recentre("spotlight"));
  document.addEventListener("visibilitychange", () => { if (document.hidden) stop(); else if (!document.querySelector("#view-development").hidden) start(); });

  return { show: start, hide: stop };
}
