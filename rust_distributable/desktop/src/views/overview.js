import { api } from "../api.js";
import { element } from "../dom.js";
import { compactNumber, humanize } from "../format.js";
import { kingdomLabel } from "./pvp-model.js";

/// The Overview tab: every castle's troops, tools, food and mead as the game last
/// reported them.
///
/// It reads the service's stored reading and asks for a fresh one only when the tab
/// opens with an old one (or none) or when the button is pressed; the service decides
/// when it is safe to send the request (see `direct/inventory.rs`). While the tab is
/// hidden nothing polls.

const POLL_MS = 4000;
/// A reading older than this is refreshed when the tab opens.
const STALE_MS = 2 * 60_000;

/// "12 s ago", "4 min ago", "2 h ago".
export function ageLabel(observedAtMs, nowMs) {
  if (observedAtMs == null) return "never read";
  const seconds = Math.max(0, Math.round((nowMs - observedAtMs) / 1000));
  if (seconds < 5) return "just now";
  if (seconds < 90) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return `${minutes} min ago`;
  return `${Math.round(minutes / 60)} h ago`;
}

/// Whether opening the tab should ask for a fresh reading: never read, or old.
export function needsRefresh(castles, nowMs) {
  const newest = Math.max(0, ...castles.map((castle) => castle.observed_at_ms ?? 0));
  return newest === 0 || nowMs - newest > STALE_MS;
}

/// A count as people read it: 1,234 up to ten thousand, then 12.3K.
export function readable(value) {
  return compactNumber(Math.round(value));
}

export function createOverview() {
  const $ = (selector) => document.querySelector(selector);
  const state = { open: false, timer: 0, data: null, error: "", asked: false };

  function unitList(title, units) {
    const block = element("div", "overview-units");
    block.append(element("p", "list-label", title));
    if (!units.length) { block.append(element("p", "empty-copy", "None")); return block; }
    const list = element("ul", "dev-list");
    for (const unit of units) {
      const item = element("li");
      item.append(element("span", "", humanize(unit.name)), element("b", "", unit.count.toLocaleString()));
      list.append(item);
    }
    block.append(list);
    return block;
  }

  function castleCard(castle, now) {
    const card = element("article", "overview-castle");
    const head = element("header");
    head.append(
      element("b", "", castle.name || `Castle ${castle.castle_id}`),
      element("small", "", `${kingdomLabel(castle.kingdom_id)} · ${castle.x}:${castle.y} · ${ageLabel(castle.observed_at_ms, now)}`),
    );
    const supplies = element("dl", "dev-stats");
    for (const [label, value] of [["Food", castle.food], ["Mead", castle.mead]]) {
      const cell = element("div");
      cell.append(element("dt", "", label), element("dd", "", castle.observed_at_ms == null ? "–" : readable(value)));
      supplies.append(cell);
    }
    card.append(head, supplies, unitList("Troops", castle.troops), unitList("Tools", castle.tools));
    if (castle.other.length) card.append(unitList("Other", castle.other.map((unit) => ({ ...unit, name: `unit ${unit.id}` }))));
    return card;
  }

  function render() {
    const grid = $("#overview-castles");
    const note = $("#overview-note");
    if (!grid) return;
    if (!state.data) {
      note.textContent = state.error || "Loading…";
      grid.replaceChildren();
      return;
    }
    const now = Date.now();
    const { castles, connected, refresh_pending_since_ms: pending } = state.data;
    const newest = Math.max(0, ...castles.map((castle) => castle.observed_at_ms ?? 0));
    $("#overview-age").textContent = newest ? `read ${ageLabel(newest, now)}` : "not read yet";
    $("#overview-refresh").disabled = !connected;
    note.textContent = state.error
      || (!connected ? "Connect the account to read its castles; the last reading is shown."
        : pending ? "A new reading has been asked for and will arrive when the bot is between actions."
          : !castles.length ? "No castles are known yet. Initialize the account first." : "");
    grid.replaceChildren(...castles.map((castle) => castleCard(castle, now)));
  }

  async function load() {
    try {
      state.data = await api("/overview");
      state.error = "";
      if (!state.asked && state.data.connected && needsRefresh(state.data.castles, Date.now())) {
        state.asked = true;
        await ask();
      }
    } catch (error) {
      state.error = error.message;
    }
    render();
  }

  async function ask() {
    try {
      await api("/overview/refresh", { method: "POST" });
      state.error = "";
    } catch (error) { state.error = error.message; }
  }

  $("#overview-refresh")?.addEventListener("click", async () => { await ask(); await load(); });

  return {
    show() {
      if (state.open) return;
      state.open = true;
      state.asked = false;
      load();
      state.timer = setInterval(load, POLL_MS);
    },
    hide() {
      state.open = false;
      clearInterval(state.timer);
    },
  };
}
