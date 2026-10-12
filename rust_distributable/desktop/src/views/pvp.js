import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { shortDuration } from "./dev-math.js";
import { KINDS, countOf, kingdomLabel, liveState, mergeRows, movementKind, partyLabel } from "./pvp-model.js";

/// The PVP tab: other players' movements the account has seen, from the service's
/// own record. It reads, never writes.
///
/// Only a page of rows is ever held. The first page is fetched when the tab opens
/// and refreshed while it stays open; older pages are fetched on demand with the
/// cursor the service hands back. The summary counts come from the service and
/// cover the whole history, not just what is loaded. Countdowns tick locally from
/// each row's landing time, so no request is made per second. While the tab is
/// hidden every timer is stopped.

const PAGE = 50;
const REFRESH_MS = 5000;

const STATE_LABEL = { active: "Active", landed: "Landed", retreated: "Retreated" };
const DIRECTION_LABEL = { incoming: "Incoming", outgoing: "Outgoing", internal: "Support", other: "Other" };

export function createPvp() {
  const state = {
    open: false,
    rows: [],
    next: null,
    counts: [],
    player: null,
    alliance: null,
    filters: { direction: "", state: "", owner: "" },
    loading: false,
    error: "",
    timers: [],
  };
  const $ = (selector) => document.querySelector(selector);

  function params(cursor) {
    const query = new URLSearchParams({ limit: String(PAGE) });
    if (accountContext.viewed()) query.set("account_id", accountContext.viewed());
    for (const [key, value] of Object.entries(state.filters)) if (value) query.set(key, value);
    if (cursor) query.set("before", cursor);
    return query.toString();
  }

  /// Fetch the newest page and fold it into what is held; or, with `more`, the
  /// page after the last one held.
  async function load({ more = false, reset = false } = {}) {
    if (state.loading) return;
    state.loading = true;
    try {
      const data = await api(`/pvp?${params(more ? state.next : null)}`);
      if (reset) state.rows = [];
      state.rows = mergeRows(state.rows, data.rows);
      if (more || reset || state.next === null) state.next = data.next;
      if (data.counts) state.counts = data.counts;
      state.player = data.player_name;
      state.alliance = data.alliance_name;
      state.error = "";
    } catch (error) {
      state.error = error.message;
    } finally {
      state.loading = false;
      render();
    }
  }

  function cell(text, className = "") {
    return element("span", className, text);
  }

  function party(name, alliance, x, y) {
    const wrap = element("span", "pvp-party");
    wrap.append(element("b", "", partyLabel(name, alliance)), element("small", "", `${x}:${y}`));
    return wrap;
  }

  function clock(ms) {
    return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  function landing(row, now) {
    const current = liveState(row, now);
    if (current === "active") return `in ${shortDuration(row.land_at_ms - now)}`;
    // A retreated movement never landed: the estimate means nothing, so show
    // when it was called back instead.
    if (current === "retreated") return row.retreated_at_ms ? `recalled ${clock(row.retreated_at_ms)}` : "recalled";
    return clock(row.land_at_ms);
  }

  function renderRow(row, now) {
    const current = liveState(row, now);
    const line = element("div", `pvp-row pvp-${row.direction} pvp-${current}`);
    line.dataset.mid = String(row.mid);
    const lands = cell(landing(row, now), "pvp-lands");
    lands.dataset.land = String(row.land_at_ms);
    const status = cell(STATE_LABEL[current], "pvp-state");
    // The direction is coloured by what the movement is; the state is green while it is
    // live and uncoloured once it is over (see the colour key above the table).
    const kind = movementKind(row);
    const direction = cell(DIRECTION_LABEL[row.direction] ?? row.direction, "pvp-direction");
    direction.style.color = KINDS[kind].colour;
    direction.title = KINDS[kind].label;
    line.dataset.kind = kind;
    line.append(
      cell(clock(row.detected_at_ms)),
      direction,
      status,
      party(row.sender_name, row.sender_alliance, row.sender_x, row.sender_y),
      party(row.target_name, row.target_alliance, row.target_x, row.target_y),
      cell(row.army_size == null ? "?" : row.army_size.toLocaleString()),
      cell(shortDuration(row.duration_s * 1000)),
      lands,
      cell(row.owner === "mine" ? "mine" : "alliance"),
      cell(kingdomLabel(row.kingdom_id)),
    );
    if (row.state === "retreated") status.dataset.final = "1";
    line._row = row;
    return line;
  }

  /// The colour key, in the order the scheme was defined.
  function renderLegend() {
    const legend = $("#pvp-legend");
    if (!legend || legend.childElementCount) return;
    for (const kind of ["support", "incoming", "ally_incoming", "ally_outgoing", "own_outgoing"]) {
      const item = element("span", "dev-legend-item", KINDS[kind].label);
      const swatch = element("i", "pvp-swatch");
      swatch.style.background = KINDS[kind].colour;
      item.prepend(swatch);
      legend.append(item);
    }
    const live = element("span", "dev-legend-item", "Active");
    const dot = element("i", "pvp-swatch"); dot.style.background = "var(--ok)";
    live.prepend(dot);
    legend.append(live);
  }

  function renderSummary() {
    renderLegend();
    const box = $("#pvp-summary");
    const parts = ["incoming", "outgoing", "internal", "other"].map((direction) => {
      const block = element("div");
      block.append(
        element("dt", "", DIRECTION_LABEL[direction]),
        element("dd", "", `${countOf(state.counts, direction)} · ${countOf(state.counts, direction, "active")} active · ${countOf(state.counts, direction, "retreated")} retreated`),
      );
      return block;
    });
    box.replaceChildren(...parts);
    const who = state.player ? `${state.player}${state.alliance ? ` · ${state.alliance}` : " · no alliance"}` : "No account seen yet";
    $("#pvp-who").textContent = who;
  }

  function render() {
    const now = Date.now();
    renderSummary();
    const list = $("#pvp-rows");
    list.replaceChildren(...state.rows.map((row) => renderRow(row, now)));
    $("#pvp-empty").hidden = state.rows.length > 0;
    $("#pvp-empty").textContent = state.error || "No movements recorded for these filters yet.";
    const more = $("#pvp-more");
    more.hidden = state.next == null;
    more.disabled = state.loading;
    $("#pvp-loaded").textContent = `${state.rows.length} loaded${state.next == null ? "" : " · more available"}`;
  }

  /// Move countdowns and landings forward without touching the network.
  function tick() {
    const now = Date.now();
    for (const line of document.querySelectorAll("#pvp-rows .pvp-row")) {
      const row = line._row;
      if (!row) continue;
      const current = liveState(row, now);
      const lands = line.querySelector(".pvp-lands");
      const text = landing(row, now);
      if (lands.textContent !== text) lands.textContent = text;
      const status = line.querySelector(".pvp-state");
      if (status.textContent !== STATE_LABEL[current]) {
        status.textContent = STATE_LABEL[current];
        line.classList.remove("pvp-active", "pvp-landed", "pvp-retreated");
        line.classList.add(`pvp-${current}`);
      }
    }
  }

  function start() {
    if (state.open) return;
    state.open = true;
    load();
    state.timers.push(setInterval(() => load(), REFRESH_MS));
    state.timers.push(setInterval(tick, 1000));
  }

  function stop() {
    state.open = false;
    state.timers.forEach(clearInterval);
    state.timers = [];
  }

  for (const key of Object.keys(state.filters)) {
    $(`#pvp-filter-${key}`)?.addEventListener("change", (event) => {
      state.filters[key] = event.target.value;
      state.next = null;
      load({ reset: true });
    });
  }
  $("#pvp-more")?.addEventListener("click", () => load({ more: true }));
  // Another account in view: its own record, from the top.
  accountContext.subscribe(() => {
    state.rows = []; state.next = null; state.counts = []; state.player = null; state.alliance = null;
    if (state.open) load({ reset: true }); else render();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) stop(); else if (!$("#view-pvp").hidden) start();
  });

  return { show: start, hide: stop };
}
