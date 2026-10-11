import { api } from "../api.js";
import { element } from "../dom.js";
import { KINGDOMS, kingdomLabel } from "./pvp-model.js";

/// The support bot form and library: a name and, for each permanent kingdom, the
/// spot troops wait at while a castle there is attacked. A kingdom left empty has
/// no destination, so a castle there is never stationed.
///
/// The window only stores these. Nothing here sends anything to the game, and which
/// bot an account uses is chosen in the Control Panel (step 2).

/// The kingdoms a bot can name a destination for; Berimond has no stations.
export const STATION_KINGDOM_IDS = [0, 1, 2, 3, 4];
const MAP_LIMIT = 1300;

/// Read the form's rows into the request body, or say what is wrong with them.
/// Rows left unticked are simply absent: that is how "no destination" is stated.
export function destinationsFrom(rows) {
  const kingdoms = [];
  for (const row of rows) {
    if (!row.enabled) continue;
    const x = Number(row.x);
    const y = Number(row.y);
    if (row.x === "" || row.y === "" || !Number.isInteger(x) || !Number.isInteger(y)) {
      return { error: `${kingdomLabel(row.kingdom_id)}: enter both coordinates as whole numbers.` };
    }
    if (x < 0 || y < 0 || x > MAP_LIMIT || y > MAP_LIMIT) {
      return { error: `${kingdomLabel(row.kingdom_id)}: coordinates must be between 0 and ${MAP_LIMIT}.` };
    }
    kingdoms.push({ kingdom_id: row.kingdom_id, x, y });
  }
  if (!kingdoms.length) return { error: "Give at least one kingdom a destination." };
  return { kingdoms };
}

/// "Green 468:507 · Burning Sands 600:600".
export function describeBot(bot) {
  return bot.kingdoms.map((kingdom) => `${kingdomLabel(kingdom.kingdom_id)} ${kingdom.x}:${kingdom.y}`).join(" · ");
}

export function createSupportBots({ getLibrary, reload }) {
  const $ = (selector) => document.querySelector(selector);
  const rows = new Map();

  function buildForm() {
    const holder = $("#support-kingdoms");
    if (!holder || rows.size) return;
    for (const id of STATION_KINGDOM_IDS) {
      const label = KINGDOMS.find((kingdom) => kingdom.id === id)?.name ?? `Kingdom ${id}`;
      const line = element("label", "support-kingdom");
      const enabled = document.createElement("input");
      enabled.type = "checkbox";
      const x = document.createElement("input"); x.type = "number"; x.placeholder = "X"; x.min = "0"; x.max = String(MAP_LIMIT); x.disabled = true;
      const y = document.createElement("input"); y.type = "number"; y.placeholder = "Y"; y.min = "0"; y.max = String(MAP_LIMIT); y.disabled = true;
      enabled.addEventListener("change", () => { x.disabled = y.disabled = !enabled.checked; });
      line.append(enabled, element("span", "", label), x, y);
      holder.append(line);
      rows.set(id, { enabled, x, y });
    }
  }

  function formRows() {
    return [...rows.entries()].map(([kingdom_id, row]) => ({ kingdom_id, enabled: row.enabled.checked, x: row.x.value, y: row.y.value }));
  }

  function renderLibrary() {
    const list = $("#support-bot-library");
    if (!list) return;
    const library = getLibrary();
    const bots = library.station_bots || [];
    list.replaceChildren(...bots.map((bot) => {
      const row = element("article", "library-row");
      const identity = element("div");
      const using = (library.account_station_bots || []).filter((value) => value.station_bot_id === bot.station_bot_id && value.running).length;
      identity.append(
        element("b", "", bot.name),
        element("small", "", describeBot(bot)),
        element("small", "castle-queue", using ? `In use by ${using} account${using === 1 ? "" : "s"}` : "Not running"),
      );
      const remove = element("button", "mini-action danger", "Delete");
      remove.type = "button";
      remove.addEventListener("click", async () => {
        try {
          await api(`/plans/station-bots/${bot.station_bot_id}`, { method: "DELETE" });
          $("#support-bot-result").textContent = "Deleted.";
          await reload();
        } catch (error) { $("#support-bot-result").textContent = error.message; }
      });
      const actions = element("span", "row-actions"); actions.append(remove);
      row.append(identity, actions);
      return row;
    }));
    if (!bots.length) list.append(element("p", "empty-copy", "No support bots saved yet."));
  }

  $("#support-bot-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const result = $("#support-bot-result");
    const parsed = destinationsFrom(formRows());
    if (parsed.error) { result.textContent = parsed.error; return; }
    try {
      await api("/plans/station-bots", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: $("#support-bot-name").value.trim(), kingdoms: parsed.kingdoms }),
      });
      result.textContent = "Support bot saved.";
      $("#support-bot-name").value = "";
      for (const row of rows.values()) { row.enabled.checked = false; row.x.value = row.y.value = ""; row.x.disabled = row.y.disabled = true; }
      await reload();
    } catch (error) { result.textContent = error.message; }
  });

  buildForm();
  return { render: () => { buildForm(); renderLibrary(); } };
}
