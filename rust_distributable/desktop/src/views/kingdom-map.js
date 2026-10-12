import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import {
  DEFAULT_VIEW, clampSize, endpoints, makeViewport, movementState, panBy,
  pointOnPath, shortDuration, visible, zoomAbout,
} from "./dev-math.js";
import { drawStationJobs } from "./dev-station.js";
import { KINDS, KINGDOMS, arrowFor } from "./pvp-model.js";
import { accountsToDraw, mergedThreats } from "./shared-layers.js";

/// One kingdom map, for one account.
///
/// The map is that account's own: its towers, its spotlight. On top of it go the
/// colour-coded movements of every signed-in account on the same world (a colour per
/// account): their armies, the threats against them, and their stationed castles. The
/// service decides who may be shown (only accounts sharing the viewed account's world),
/// so a map never leaks an account from another world.
///
/// It polls slowly (towers every 30 s, the shared overlay every 2 s) and animates itself
/// from timestamps on every frame; nothing is pushed and nothing is sent to the game.

const MAP_REFRESH_MS = 30_000;
const SHARED_REFRESH_MS = 2_000;
const INK = "#fff";
const DIM = "rgba(255,255,255,.40)";
const FAINT = "rgba(255,255,255,.16)";
const OK = "#5ed08a";
const HOSTILE = "#ff5a5a";

function arrowHead(context, from, to, size) {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  context.beginPath();
  context.moveTo(to.x, to.y);
  context.lineTo(to.x - size * Math.cos(angle - 0.45), to.y - size * Math.sin(angle - 0.45));
  context.moveTo(to.x, to.y);
  context.lineTo(to.x - size * Math.cos(angle + 0.45), to.y - size * Math.sin(angle + 0.45));
  context.stroke();
}

export function canvasSize(canvas) {
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

/// `host` receives the card. `accountId` is the account whose map this is.
export function createKingdomMap({ host, accountId }) {
  const state = {
    accountId,
    chosenKingdom: null,
    map: null,
    shared: null,
    mapAt: 0,
    mapKingdom: null,
    viewport: null,
    viewSize: DEFAULT_VIEW,
    follow: "castle",
    drag: null,
    timers: [],
    open: false,
  };

  // ---- the card -----------------------------------------------------------------

  const card = element("section", "card dev-panel dev-map-card");
  const title = element("div", "card-title");
  const heading = element("div");
  const swatch = element("i", "account-swatch");
  const name = element("h2", "dev-map-title");
  heading.append(element("p", "eyebrow", "Kingdom"), name);
  const controls = element("div", "row-actions dev-controls");
  const chooser = document.createElement("select");
  chooser.title = "Kingdom to show";
  chooser.append(new Option("Follow the bot", ""), ...KINGDOMS.map((kingdom) => new Option(kingdom.name, String(kingdom.id))));
  const button = (label, titleText, handler) => {
    const item = element("button", "secondary-action", label);
    item.type = "button";
    if (titleText) item.title = titleText;
    item.addEventListener("click", handler);
    return item;
  };
  controls.append(
    chooser,
    button("−", "Show more map", () => setSize(state.viewSize * 1.4)),
    button("+", "Show less map", () => setSize(state.viewSize / 1.4)),
    button("Castle", "", () => recentre("castle")),
    button("Spotlight", "", () => recentre("spotlight")),
    button("Reset 100 × 100", "", () => { state.viewSize = DEFAULT_VIEW; recentre("castle"); }),
  );
  title.append(heading, controls);
  const canvas = element("canvas", "dev-canvas dev-map-canvas");
  canvas.setAttribute("aria-label", "Kingdom map");
  const legend = element("p", "dev-note dev-legend-line");
  card.append(title, canvas, legend);
  host.append(card);

  function paintTitle() {
    const account = accountContext.accounts().find((value) => value.account_id.toLowerCase() === state.accountId.toLowerCase());
    swatch.style.background = accountContext.colour(state.accountId);
    name.replaceChildren(swatch, document.createTextNode(` ${account?.player_name || state.accountId}`));
  }
  paintTitle();

  // ---- data ---------------------------------------------------------------------

  const account = () => encodeURIComponent(state.accountId);

  /// The kingdom on screen: the one chosen, else the one this account's bot is in.
  function shownKingdom() {
    return state.chosenKingdom ?? state.shared?.focus_kingdom ?? null;
  }

  async function pollMap(force = false) {
    if (!force && Date.now() - state.mapAt < MAP_REFRESH_MS) return;
    const kingdom = shownKingdom();
    try {
      state.map = await api(`/dev/map?account_id=${account()}${kingdom == null ? "" : `&kingdom=${kingdom}`}`);
      state.mapKingdom = state.map.kingdom_id;
      state.mapAt = Date.now();
      if (!state.viewport) recentre();
    } catch (_) { /* nothing stored for this account yet */ }
  }

  async function pollShared() {
    const kingdom = shownKingdom();
    try {
      state.shared = await api(`/dev/shared?account_id=${account()}${kingdom == null ? "" : `&kingdom=${kingdom}`}`);
      if (state.map?.kingdom_id !== state.shared.kingdom_id) await pollMap(true);
      paintLegend();
    } catch (_) { state.shared = null; }
  }

  function paintLegend() {
    const shown = accountsToDraw(state.shared?.accounts || []);
    legend.replaceChildren(
      ...shown.map((value) => {
        const item = element("span", "dev-legend-item");
        const dot = element("i", "account-swatch");
        dot.style.background = accountContext.colour(value.account_id);
        item.append(dot, document.createTextNode(` ${value.player_name || value.account_id}${value.connected ? "" : " (offline)"}`));
        return item;
      }),
      document.createTextNode(shown.length > 1 ? " — accounts on the same world are shown together; red arrowheads are hostile, green are allied." : " — red arrowheads are hostile movements, green are allied."),
    );
  }

  // ---- viewport -------------------------------------------------------------------

  const own = () => state.shared?.accounts.find((value) => value.account_id.toLowerCase() === state.accountId.toLowerCase());

  function focusPoint() {
    const spatial = own()?.spatial;
    if (state.follow === "spotlight" && spatial) return { x: spatial.spotlight[0], y: spatial.spotlight[1] };
    const castle = state.map?.castle || own()?.castle;
    return castle ? { x: castle[0], y: castle[1] } : { x: 500, y: 500 };
  }

  function recentre(follow = state.follow) {
    state.follow = follow;
    const { width, height } = canvasSize(canvas);
    const point = focusPoint();
    state.viewport = makeViewport(point.x, point.y, state.viewSize, width, height);
  }

  function setSize(size) {
    state.viewSize = clampSize(size);
    const { width, height } = canvasSize(canvas);
    const v = state.viewport || makeViewport(500, 500, state.viewSize, width, height);
    state.viewport = makeViewport(v.cx, v.cy, state.viewSize, width, height);
  }

  // ---- drawing ----------------------------------------------------------------------

  /// One account's armies out or coming back, placed from their timestamps.
  function drawMarches(context, view, shared, colour, now) {
    const castle = shared.castle ? { x: shared.castle[0], y: shared.castle[1] } : null;
    if (!castle) return;
    context.font = "10px ui-monospace, Menlo, monospace";
    for (const movement of shared.movements || []) {
      const s = movementState(movement, now);
      if (s.status === "completed") continue;
      const { from, to } = endpoints(movement, castle);
      if (!visible(view, from.x, from.y, 400) && !visible(view, to.x, to.y, 400)) continue;
      const a = view.toScreen(from.x, from.y); const b = view.toScreen(to.x, to.y);
      const outbound = movement.phase === "outbound";
      if (s.status === "unknown" || s.status === "arrived") {
        context.strokeStyle = FAINT; context.setLineDash([2, 4]);
        context.beginPath(); context.moveTo(a.x, a.y); context.lineTo(b.x, b.y); context.stroke(); context.setLineDash([]);
        context.fillStyle = DIM; context.fillText(s.status === "arrived" ? "landed, return unknown" : "return unknown", b.x + 6, b.y - 6);
        continue;
      }
      const here = view.toScreen(...Object.values(pointOnPath(from, to, s.progress)));
      // The path ahead of the marker stays; behind it the line has gone. Outbound is
      // solid in the account's colour, the way home is dashed.
      context.strokeStyle = colour; context.lineWidth = 1;
      if (!outbound) context.setLineDash([4, 3]);
      context.beginPath(); context.moveTo(here.x, here.y); context.lineTo(b.x, b.y); context.stroke();
      context.setLineDash([]);
      arrowHead(context, here, { x: here.x + (b.x - here.x) * 0.15, y: here.y + (b.y - here.y) * 0.15 }, 5);
      context.fillStyle = colour;
      context.beginPath(); context.arc(here.x, here.y, 3, 0, Math.PI * 2); context.fill();
      context.fillStyle = DIM;
      context.fillText(`${outbound ? "out" : movement.estimated ? "back ≤" : "back"} ${shortDuration(s.remainingMs)}`, here.x + 6, here.y - 6);
    }
  }

  /// Other players' travelling movements, once each. The line is the colour of the
  /// account it concerns; the head and the label say whether it is hostile or allied.
  function drawThreats(context, view, accounts, now) {
    context.font = "10px ui-monospace, Menlo, monospace";
    for (const { row, owner } of mergedThreats(accounts)) {
      const arrow = arrowFor(row, now);
      if (!arrow) continue;
      if (!visible(view, arrow.from.x, arrow.from.y, 400) && !visible(view, arrow.to.x, arrow.to.y, 400)) continue;
      const colour = accountContext.colour(owner);
      const verdict = KINDS[arrow.kind]?.colour ?? (arrow.colour === "out" ? OK : HOSTILE);
      const a = view.toScreen(arrow.from.x, arrow.from.y);
      const b = view.toScreen(arrow.to.x, arrow.to.y);
      const here = view.toScreen(...Object.values(pointOnPath(arrow.from, arrow.to, arrow.progress)));
      context.lineCap = "round";
      context.strokeStyle = colour; context.globalAlpha = 0.3; context.lineWidth = 2;
      context.beginPath(); context.moveTo(a.x, a.y); context.lineTo(b.x, b.y); context.stroke();
      context.globalAlpha = 1; context.lineWidth = 3.5;
      context.beginPath(); context.moveTo(a.x, a.y); context.lineTo(here.x, here.y); context.stroke();
      context.strokeStyle = verdict;
      if (Math.hypot(here.x - a.x, here.y - a.y) > 4) arrowHead(context, a, here, 11);
      context.lineWidth = 1; context.lineCap = "butt";
      context.fillStyle = colour;
      context.beginPath(); context.arc(a.x, a.y, 4, 0, Math.PI * 2); context.fill();
      context.strokeStyle = colour;
      context.beginPath(); context.arc(b.x, b.y, 6, 0, Math.PI * 2); context.stroke();
      context.fillText(arrow.senderLabel, a.x + 8, a.y - 7);
      context.fillStyle = verdict;
      context.fillText(`${arrow.targetLabel} · ${shortDuration(arrow.remainingMs)}`, b.x + 9, b.y + 14);
    }
  }

  function draw(now = Date.now()) {
    const { context, width, height } = canvasSize(canvas);
    context.clearRect(0, 0, width, height);
    if (!state.viewport) recentre();
    if (state.follow !== "free") {
      const point = focusPoint();
      state.viewport = makeViewport(point.x, point.y, state.viewSize, width, height);
    } else {
      state.viewport = makeViewport(state.viewport.cx, state.viewport.cy, state.viewSize, width, height);
    }
    const view = state.viewport;
    const map = state.map;

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
      context.fillStyle = DIM; context.fillText("Nothing is stored for this account yet.", 12, height / 2);
      return;
    }
    const radius = Math.max(1.6, Math.min(4, view.scale * 0.5));

    // This account's towers: white circles, solid when ready, hollow while cooling.
    context.lineWidth = 1;
    for (const tower of map.towers) {
      if (!visible(view, tower.x, tower.y, 2)) continue;
      const p = view.toScreen(tower.x, tower.y);
      context.beginPath(); context.arc(p.x, p.y, radius, 0, Math.PI * 2);
      if (tower.ready_at_ms <= now) { context.fillStyle = INK; context.fill(); }
      else { context.strokeStyle = "rgba(255,255,255,.45)"; context.stroke(); }
    }

    // This account's spotlight, its local radius, and the direction it is heading.
    const spatial = own()?.spatial;
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

    // Every account on this world, each in its own colour.
    const accounts = state.shared?.accounts || [];
    for (const shared of accounts) {
      const colour = accountContext.colour(shared.account_id);
      drawMarches(context, view, shared, colour, now);
      drawStationJobs(context, view, shared.station || [], now, colour);
    }
    drawThreats(context, view, accounts, now);

    // Castles: a square in the account's colour, named when it is not this map's own.
    context.font = "10px ui-monospace, Menlo, monospace";
    for (const shared of accounts) {
      if (!shared.castle || !visible(view, shared.castle[0], shared.castle[1], 4)) continue;
      const p = view.toScreen(shared.castle[0], shared.castle[1]);
      context.fillStyle = accountContext.colour(shared.account_id);
      context.fillRect(p.x - 5, p.y - 5, 10, 10);
      if (accounts.length > 1) { context.fillStyle = DIM; context.fillText(shared.player_name || shared.account_id, p.x + 8, p.y + 14); }
    }

    context.fillStyle = DIM; context.textAlign = "right";
    context.fillText(`${Math.round(view.size)} × ${Math.round(view.size)} · kingdom ${map.kingdom_id}`, width - 8, height - 8);
    context.textAlign = "left";
  }

  // ---- controls ---------------------------------------------------------------------

  canvas.addEventListener("pointerdown", (event) => {
    canvas.setPointerCapture(event.pointerId);
    state.drag = { x: event.clientX, y: event.clientY };
  });
  canvas.addEventListener("pointermove", (event) => {
    if (!state.drag || !state.viewport) return;
    const { width, height } = canvasSize(canvas);
    state.viewport = panBy(state.viewport, event.clientX - state.drag.x, event.clientY - state.drag.y, width, height);
    state.drag = { x: event.clientX, y: event.clientY };
    state.follow = "free";
  });
  const endDrag = () => { state.drag = null; };
  canvas.addEventListener("pointerup", endDrag);
  canvas.addEventListener("pointercancel", endDrag);
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    if (!state.viewport) return;
    const { width, height } = canvasSize(canvas);
    const rect = canvas.getBoundingClientRect();
    const anchor = state.viewport.toMap(event.clientX - rect.left, event.clientY - rect.top);
    state.viewport = zoomAbout(state.viewport, event.deltaY > 0 ? 1.15 : 1 / 1.15, anchor, width, height);
    state.viewSize = state.viewport.size;
    state.follow = "free";
  }, { passive: false });
  chooser.addEventListener("change", async () => {
    state.chosenKingdom = chooser.value === "" ? null : Number(chooser.value);
    state.shared = state.chosenKingdom == null ? state.shared : null;
    await pollMap(true);
    recentre("castle");
    pollShared();
  });

  // ---- life ---------------------------------------------------------------------------

  return {
    accountId,
    element: card,
    draw,
    refreshTitle: paintTitle,
    start() {
      if (state.open) return;
      state.open = true;
      pollShared().then(() => pollMap(true));
      state.timers.push(setInterval(pollShared, SHARED_REFRESH_MS));
      state.timers.push(setInterval(() => pollMap(), 5_000));
    },
    stop() {
      state.open = false;
      state.timers.forEach(clearInterval);
      state.timers = [];
    },
    destroy() {
      this.stop();
      card.remove();
    },
  };
}
