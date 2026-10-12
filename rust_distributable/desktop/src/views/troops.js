import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { compactNumber } from "../format.js";
import { STATE_LABEL, armyText, liveState, outcomeLabel, sortRows, summarise, until } from "./troops-model.js";
import { kingdomLabel } from "./pvp-model.js";

/// The Troop overview: which commanders are out, where, with how many troops, how much
/// loot they carry and when they are due back, for the account in view.
///
/// It reads what the service already keeps (the march ledger and the commander states),
/// polls every few seconds while the tab is open, and counts down from the timestamps
/// itself every second. Inactive commanders are listed too, so the whole roster is
/// visible and a stuck one stands out. Read-only; nothing is sent to the game.

const POLL_MS = 3000;

export function createTroops() {
  const $ = (selector) => document.querySelector(selector);
  const state = { open: false, data: null, error: "", timers: [] };

  function cell(text, className = "") {
    return element("span", className, text);
  }

  function rowElement(row, now) {
    const live = liveState(row, now);
    const line = element("div", `troop-row troop-${live}`);
    line.dataset.free = String(row.free_at_ms ?? "");
    line.dataset.lands = String(row.lands_at_ms ?? "");
    const target = row.x == null ? "–" : `${kingdomLabel(row.kingdom_id)} ${row.x}:${row.y}${row.level ? ` · L${row.level}` : ""}`;
    const active = live !== "idle";
    line.append(
      cell(String(row.number), "troop-number"),
      cell(row.task_name || (row.reserved ? "Support (reserved)" : "unassigned"), "troop-task"),
      cell(row.estimated_return && live === "returning" ? "Returning (est.)" : STATE_LABEL[live], "troop-state"),
      cell(active ? target : "–"),
      cell(active ? armyText(row.army) : "–", "troop-army"),
      cell(active && row.troop_total ? row.troop_total.toLocaleString() : "–"),
      cell(active && row.coin_loot != null ? compactNumber(row.coin_loot) : "–"),
      cell(active && row.ruby_loot != null ? String(row.ruby_loot) : "–"),
      cell(active && live === "outbound" ? until(row.lands_at_ms, now) : "–", "troop-lands"),
      cell(active ? until(row.free_at_ms, now, row.estimated_return) : "–", "troop-free"),
    );
    line._row = row;
    return line;
  }

  function renderSummary(now) {
    const totals = summarise(state.data.rows, now);
    const box = $("#troops-summary");
    box.replaceChildren(...[
      ["Outbound", `${totals.outbound}${totals.sending ? ` (+${totals.sending} sending)` : ""}`],
      ["Returning", String(totals.returning)],
      ["Support", String(totals.station)],
      ["Inactive", `${totals.idle} of ${state.data.rows.length}`],
      ["Troops out", totals.troops_out.toLocaleString()],
      ["Loot coming home", `${compactNumber(totals.coins)} coin · ${totals.rubies} rubies`],
    ].map(([label, value]) => {
      const block = element("div");
      block.append(element("dt", "", label), element("dd", "", value));
      return block;
    }));
  }

  function renderRecent() {
    const list = $("#troops-recent");
    list.replaceChildren(...state.data.recent.map((march) => {
      const item = element("li");
      item.append(
        element("span", "", new Date(march.sent_at_ms).toLocaleTimeString()),
        element("span", "", `${kingdomLabel(march.kingdom_id)} ${march.x}:${march.y}`),
        element("span", "", outcomeLabel(march)),
        element("b", "", `${march.troop_total ?? "–"} troops · ${march.coin_loot != null ? compactNumber(march.coin_loot) : "–"} coin · ${march.ruby_loot ?? "–"} rubies`),
      );
      return item;
    }));
    if (!state.data.recent.length) list.append(element("li", "dev-none", "No finished marches recorded yet."));
  }

  function render() {
    const note = $("#troops-note");
    const body = $("#troops-rows");
    if (!body) return;
    if (!state.data) {
      note.textContent = state.error || "Loading…";
      body.replaceChildren();
      return;
    }
    const now = Date.now();
    note.textContent = state.error || (state.data.connected ? "" : "This account is not connected: the last recorded state is shown.");
    renderSummary(now);
    body.replaceChildren(...sortRows(state.data.rows).map((row) => rowElement(row, now)));
    if (!state.data.rows.length) body.append(element("p", "empty-copy", "No commanders are known yet. Initialize the account first."));
    renderRecent();
  }

  /// Move the countdowns forward without a request.
  function tick() {
    if (!state.data) return;
    const now = Date.now();
    for (const line of document.querySelectorAll("#troops-rows .troop-row")) {
      const row = line._row;
      if (!row) continue;
      const live = liveState(row, now);
      if (!line.classList.contains(`troop-${live}`)) { render(); return; }
      if (live === "idle") continue;
      const lands = line.querySelector(".troop-lands");
      const free = line.querySelector(".troop-free");
      const landsText = live === "outbound" ? until(row.lands_at_ms, now) : "–";
      const freeText = until(row.free_at_ms, now, row.estimated_return);
      if (lands.textContent !== landsText) lands.textContent = landsText;
      if (free.textContent !== freeText) free.textContent = freeText;
    }
  }

  async function load() {
    try {
      state.data = await api(`/troops${accountContext.query()}`);
      state.error = "";
    } catch (error) {
      state.error = error.message;
    }
    render();
  }

  accountContext.subscribe(() => {
    state.data = null;
    if (state.open) load(); else render();
  });

  return {
    show() {
      if (state.open) return;
      state.open = true;
      load();
      state.timers.push(setInterval(load, POLL_MS), setInterval(tick, 1000));
    },
    hide() {
      state.open = false;
      state.timers.forEach(clearInterval);
      state.timers = [];
    },
  };
}
