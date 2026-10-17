import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { humanize } from "../format.js";
import { kingdomLabel } from "./pvp-model.js";
import {
  buildDraft, canSend, clampCount, coinsLabel, durationLabel, isOpen, jobTitle, phaseLabel, skipOptions, targetOptions,
} from "./transfer-model.js";

/// The "Send troops to another kingdom" widget of the Castle Overview.
///
/// A projection of `/v1/transfers`: it shows what the service knows (coins, kingdoms, skip
/// items, recent transfers), asks the service to check a draft (`/transfers/check`) and submits
/// intent (`/transfers`). The service decides whether it is allowed and runs it only while the
/// bot is off (see `direct/transfer.rs`); nothing here computes a cost or a rule.

const POLL_MS = 4000;
const CHECK_DELAY_MS = 250;

export function createTransferWidget() {
  const state = {
    open: false, timer: 0, checkTimer: 0, overview: null, castles: [],
    source: null, target: null, counts: {}, skip: "", check: null, error: "", note: "", signature: "",
  };
  const root = () => document.querySelector("#transfer-widget");

  const castleOf = (id) => state.castles.find((castle) => castle.castle_id === id);
  const unitName = (id) => {
    for (const castle of state.castles) {
      const found = castle.troops.find((unit) => unit.id === id);
      if (found) return humanize(found.name);
    }
    return `unit ${id}`;
  };
  const targets = () => targetOptions(state.overview?.kingdoms ?? [], castleOf(state.source)?.kingdom_id);
  const travelSeconds = () => state.overview?.kingdoms.find((kingdom) => kingdom.kingdom_id === state.target)?.travel_s;

  function select(label, options, value, onChange) {
    const field = element("label", "", label);
    const control = element("select");
    for (const option of options) {
      const item = element("option", "", option.label);
      item.value = String(option.value ?? option.id ?? option.key ?? "");
      control.append(item);
    }
    control.value = String(value ?? "");
    control.addEventListener("change", () => onChange(control.value));
    field.append(control);
    return field;
  }

  function unitRows() {
    const castle = castleOf(state.source);
    const rows = element("div", "transfer-units");
    if (!castle) return rows;
    if (!castle.troops.length) { rows.append(element("p", "empty-copy", "This castle has no troops on record.")); return rows; }
    for (const unit of castle.troops) {
      const row = element("div", "transfer-unit");
      const input = element("input");
      input.type = "number"; input.min = "0"; input.max = String(unit.count);
      input.value = state.counts[unit.id] ? String(state.counts[unit.id]) : "";
      input.addEventListener("input", () => { state.counts[unit.id] = clampCount(input.value, unit.count); scheduleCheck(); });
      const all = element("button", "mini-action", "All");
      all.type = "button";
      all.addEventListener("click", () => { state.counts[unit.id] = unit.count; input.value = String(unit.count); scheduleCheck(); });
      row.append(element("span", "", humanize(unit.name)), element("small", "", `${unit.count.toLocaleString()} here`), input, all);
      rows.append(row);
    }
    return rows;
  }

  function buildForm(host) {
    const overview = state.overview;
    const sources = state.castles.filter((castle) => castle.troops.length);
    const kingdoms = targets();
    host.replaceChildren();
    if (!overview || !state.castles.length) { host.append(element("p", "empty-copy", "Castles appear here once they have been read.")); return; }
    if (state.source == null || !castleOf(state.source)) state.source = sources[0]?.castle_id ?? state.castles[0].castle_id;
    if (!kingdoms.some((kingdom) => kingdom.id === state.target)) state.target = kingdoms[0]?.id ?? null;
    const grid = element("div", "transfer-grid");
    grid.append(
      select("From castle", state.castles.map((castle) => ({ value: castle.castle_id, label: `${castle.name || `Castle ${castle.castle_id}`} · ${kingdomLabel(castle.kingdom_id)}` })), state.source, (value) => {
        state.source = Number(value); state.counts = {}; state.check = null; state.signature = ""; render();
      }),
      select("To kingdom", kingdoms.length ? kingdoms.map((kingdom) => ({ value: kingdom.id, label: kingdom.label })) : [{ value: "", label: "No other kingdom is unlocked" }], state.target ?? "", (value) => {
        state.target = Number(value); scheduleCheck(); render(true);
      }),
      select("Journey", skipOptions(overview.skips, travelSeconds()).map((option) => ({ value: option.key, label: option.label })), state.skip, (value) => { state.skip = value; scheduleCheck(); }),
    );
    host.append(grid, element("p", "list-label", "Troops to send"), unitRows());
  }

  function renderStatus() {
    const summary = document.querySelector("#transfer-summary");
    const send = document.querySelector("#transfer-send");
    const overview = state.overview;
    if (!summary || !send || !overview) return;
    const lines = [];
    if (overview.coins) lines.push(`Coins: ${coinsLabel(overview.coins.coins)}`);
    if (state.check?.cost_c1 != null && Object.values(state.counts).some((count) => count > 0)) {
      lines.push(`Cost: ${coinsLabel(state.check.cost_c1)} coins`);
      if (state.check.travel_s) lines.push(`Journey: ${durationLabel(state.check.travel_s)}`);
      if (state.check.skips_needed) lines.push(`Skips used: ${state.check.skips_needed}`);
    }
    summary.replaceChildren(element("p", "transfer-line", lines.join("  ·  ")));
    for (const problem of state.check?.problems ?? []) summary.append(element("p", "transfer-problem", problem.message));
    if (overview.bot_running) summary.append(element("p", "transfer-problem", "Stop the bot first: a transfer only runs while it is off."));
    if (state.error) summary.append(element("p", "transfer-problem", state.error));
    if (state.note) summary.append(element("p", "dev-note", state.note));
    send.disabled = state.busy || !overview.connected || overview.bot_running || !canSend(state.check, overview);
  }

  function renderJobs() {
    const list = document.querySelector("#transfer-jobs");
    if (!list || !state.overview) return;
    list.replaceChildren();
    if (!state.overview.jobs.length) { list.append(element("p", "empty-copy", "No transfers yet.")); return; }
    for (const job of state.overview.jobs) {
      const row = element("div", `transfer-job transfer-${job.phase}`);
      const text = element("div");
      text.append(element("b", "", jobTitle(job, unitName)), element("small", "", `${phaseLabel(job.phase)} · ${job.detail}`));
      row.append(text);
      if (job.phase === "queued") {
        const cancel = element("button", "mini-action", "Cancel");
        cancel.type = "button";
        cancel.addEventListener("click", async () => {
          try { await api(`/transfers/${job.job_id}/cancel${accountContext.query()}`, { method: "POST" }); await load(); }
          catch (error) { state.error = error.message; renderStatus(); }
        });
        row.append(cancel);
      }
      list.append(row);
    }
  }

  function render(structural = false) {
    const host = root();
    if (!host) return;
    if (!host.firstChild || structural || state.signature !== signature()) {
      state.signature = signature();
      const title = element("div", "card-title");
      const heading = element("div");
      heading.append(element("p", "eyebrow", "Castles"), element("h3", "", "Send troops to another kingdom"));
      const refresh = element("button", "secondary-action", "Read kingdoms now");
      refresh.type = "button";
      refresh.addEventListener("click", async () => { await ask(); await load(); });
      title.append(heading, refresh);
      host.replaceChildren(title);
      const form = element("div", "transfer-form");
      buildForm(form);
      const summary = element("div"); summary.id = "transfer-summary";
      const send = element("button", "primary-action", "Send troops");
      send.id = "transfer-send"; send.type = "button";
      send.addEventListener("click", submit);
      const jobs = element("div"); jobs.id = "transfer-jobs";
      host.append(form, summary, send, element("p", "list-label", "Recent transfers"), jobs);
    }
    renderStatus();
    renderJobs();
  }

  /// What forces the form to be rebuilt: the castles, the kingdoms and the skip items on offer.
  function signature() {
    return JSON.stringify([
      state.castles.map((castle) => [castle.castle_id, castle.troops.map((unit) => unit.id)]),
      state.overview?.kingdoms.map((kingdom) => [kingdom.kingdom_id, kingdom.unlocked]),
      state.overview?.skips.map((item) => item.key),
      state.source,
    ]);
  }

  function scheduleCheck() {
    clearTimeout(state.checkTimer);
    state.checkTimer = setTimeout(runCheck, CHECK_DELAY_MS);
  }

  async function runCheck() {
    state.error = "";
    const draft = buildDraft({ castleId: state.source, targetKingdomId: state.target, counts: state.counts, skip: state.skip });
    if (state.target == null || !draft.units.length) { state.check = null; renderStatus(); return; }
    try {
      state.check = await api(`/transfers/check${accountContext.query()}`, {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(draft),
      });
    } catch (error) { state.check = null; state.error = error.message; }
    renderStatus();
  }

  async function submit() {
    const draft = buildDraft({ castleId: state.source, targetKingdomId: state.target, counts: state.counts, skip: state.skip });
    state.busy = true; state.error = ""; renderStatus();
    try {
      await api(`/transfers${accountContext.query()}`, {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(draft),
      });
      state.counts = {}; state.check = null;
      state.note = "Queued. It is sent after a short pause, when the connection is free.";
      render(true);
    } catch (error) { state.error = error.message; }
    state.busy = false;
    await load();
  }

  async function ask() {
    try { await api(`/transfers/refresh${accountContext.query()}`, { method: "POST" }); state.error = ""; }
    catch (error) { state.error = error.message; }
  }

  async function load() {
    try {
      state.overview = await api(`/transfers${accountContext.query()}`);
    } catch (error) { state.error = error.message; }
    render();
  }

  accountContext.subscribe(() => {
    state.overview = null; state.counts = {}; state.check = null; state.source = null; state.signature = "";
    if (state.open) load();
  });

  return {
    /// The Overview hands over its castles each time it renders.
    update(castles) { state.castles = castles; if (state.open) render(); },
    selectSource(castleId) { state.source = castleId; state.counts = {}; state.check = null; state.signature = ""; render(true); root()?.scrollIntoView({ behavior: "smooth", block: "nearest" }); },
    show() {
      if (state.open) return;
      state.open = true;
      ask().then(load);
      state.timer = setInterval(load, POLL_MS);
    },
    hide() { state.open = false; clearInterval(state.timer); },
    isOpenJob: () => state.overview?.jobs.some(isOpen) ?? false,
  };
}
