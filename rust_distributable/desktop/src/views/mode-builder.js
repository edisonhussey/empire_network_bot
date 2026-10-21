import { element } from "../dom.js";
import { commanderRanges, followPrevious, moveBlock, needed, problems, supportCommanders, toRequest, withRange } from "./commander-ranges.js";

/// The attack bot builder: lay tasks (and commanders kept for support) out along the
/// commander numbers.
///
/// The board is horizontal, like a ruler: every block covers the commander numbers it
/// owns, so which commanders belong to what, and which are unused, reads at a glance.
/// Click a block to edit it: its first and last commander, move it left or right past
/// its neighbour, or take it off. A new block follows the one before until its numbers
/// are edited. Numbers may be skipped, and a task and the support object may alternate
/// (support 1-9, a task at 10, support 11-20); a commander can never belong to two
/// blocks. Support is not a setting: a bot that reserves commanders for it (their count
/// is not zero) is a hybrid and support is on; one that reserves none is a plain attack
/// bot. Reserving the first block reveals the one choice still needed, where the troops
/// wait; no attack ever uses the reserved commanders.
///
/// All the arithmetic is in commander-ranges.js; this draws it and reports the entries.
/// `getLibrary()` supplies the saved tasks and support objects, `kingdomLabel(id)`
/// names a kingdom.
export function createModeBuilder({ getLibrary, kingdomLabel }) {
  const $ = (selector) => document.querySelector(selector);
  let entries = [];
  let selected = null;
  let counter = 0;

  const task = (id) => getLibrary().tasks.find((value) => value.task_id === id);
  const supports = () => getLibrary().station_bots || [];
  const reserved = () => supportCommanders(entries);
  /// Where support waits, or `null`. Only meaningful while commanders are reserved: a
  /// leftover choice never switches support on by itself.
  const supportId = () => {
    const value = $("#mode-support")?.value;
    return reserved() > 0 && value ? Number(value) : null;
  };
  const supportName = () => supports().find((bot) => bot.station_bot_id === supportId())?.name || "Support";

  function summary(value) {
    const levels = value.target_level_min == null ? "" : ` · levels ${value.target_level_min}–${value.target_level_max}`;
    return `${kingdomLabel(value.kingdom_id)}${levels}`;
  }

  const labelOf = (entry) => (entry.kind === "support" ? supportName() : task(entry.task_id)?.name || "(deleted task)");

  /// Rebuilding removes the control that was just used; doing it inside that event leaves
  /// the browser dispatching to a detached node, so it waits a tick.
  const redrawSoon = () => setTimeout(renderBoard, 0);

  function change(uid, update) {
    entries = entries.map((entry) => (entry.uid === uid ? update(entry) : entry));
    redrawSoon();
  }

  function button(label, handler, danger = false, title = "") {
    const item = element("button", danger ? "mini-action danger" : "mini-action", label);
    item.type = "button";
    if (title) item.title = title;
    item.addEventListener("click", handler);
    return item;
  }

  function numberInput(value, label, handler) {
    const input = document.createElement("input");
    input.type = "number"; input.min = "1"; input.value = String(value);
    input.setAttribute("aria-label", label);
    input.addEventListener("change", () => handler(input.value));
    return input;
  }

  function move(uid, direction) {
    const index = entries.findIndex((entry) => entry.uid === uid);
    entries = moveBlock(entries, index, direction);
    redrawSoon();
  }

  function remove(uid) {
    entries = entries.filter((entry) => entry.uid !== uid);
    if (selected === uid) selected = null;
    renderPool();
    redrawSoon();
  }

  /// The row each block is drawn on: blocks that overlap (a mistake, shown in red) are
  /// stacked on separate rows instead of hiding one another.
  function rowsFor(ranges) {
    const rows = [];
    ranges.forEach((range, index) => {
      let row = 1;
      while (ranges.slice(0, index).some((other, before) => rows[before] === row && range.first <= other.last && other.first <= range.last)) row += 1;
      rows[index] = row;
    });
    return rows;
  }

  function renderBoard() {
    const board = $("#mode-tasks");
    if (!board) return;
    board.replaceChildren();
    showDestination();
    const note = $("#commander-warning");
    if (!entries.length) {
      board.append(element("p", "board-empty", "Add a task, or reserve commanders for support, to begin."));
      note.textContent = "0 commanders allocated";
      note.classList.remove("range-conflict");
      return;
    }
    const ranges = commanderRanges(entries);
    const bad = problems(ranges);
    const clashing = new Set(bad.flatMap((item) => [item.index, item.with ?? item.index]));
    const total = needed(ranges);
    const rows = rowsFor(ranges);
    const lastRow = Math.max(...rows);
    const grid = element("div", "range-board");
    grid.style.gridTemplateColumns = `repeat(${Math.max(total, 1)}, minmax(30px, 1fr))`;
    ranges.forEach((range, index) => {
      const entry = entries[index];
      const block = element("button", `range-block ${range.kind}${entry.uid === selected ? " selected" : ""}${clashing.has(index) ? " range-conflict" : ""}`);
      block.type = "button";
      block.style.gridColumn = `${range.first} / span ${range.count}`;
      block.style.gridRow = String(rows[index]);
      block.title = `${labelOf(entry)}: ${range.first === range.last ? range.first : `${range.first}–${range.last}`}`;
      block.append(element("b", "", labelOf(entry)), element("small", "", `${range.count}`));
      block.addEventListener("click", () => { selected = entry.uid; redrawSoon(); });
      grid.append(block);
    });
    const covered = new Set(ranges.flatMap((range) => Array.from({ length: range.count }, (_, offset) => range.first + offset)));
    for (let number = 1; number <= total; number += 1) {
      const cell = element("span", `ruler-cell${covered.has(number) ? "" : " free"}`, String(number));
      cell.style.gridColumn = String(number);
      cell.style.gridRow = String(lastRow + 1);
      grid.append(cell);
    }
    board.append(grid, selectionPanel(ranges));
    const used = ranges.reduce((sum, range) => sum + range.count, 0);
    const kept = ranges.filter((range) => range.kind === "support").reduce((sum, range) => sum + range.count, 0);
    note.classList.toggle("range-conflict", bad.length > 0);
    if (bad.length) note.textContent = "Two blocks share a commander. A commander can belong to one block only; change the numbers.";
    else {
      const parts = [`${used - kept} for attacks`, ...(kept ? [`${kept} kept for support · hybrid, support on`] : [])];
      note.textContent = `${parts.join(" · ")} · numbers 1–${total}${total > used ? `; ${total - used} left unused` : ""}`;
    }
  }

  /// The "where support waits" choice shows only while commanders are reserved. With
  /// exactly one saved support it is picked for the user.
  function showDestination() {
    const label = $("#support-destination");
    const select = $("#mode-support");
    if (!label || !select) return;
    label.hidden = reserved() === 0;
    if (!select.value && supports().length === 1) select.value = String(supports()[0].station_bot_id);
  }

  /// The editor for the clicked block, under the board.
  function selectionPanel(ranges) {
    const panel = element("div", "range-edit");
    const index = entries.findIndex((entry) => entry.uid === selected);
    if (index < 0) {
      panel.append(element("p", "field-hint", "Click a block to edit its commanders, move it left or right, or remove it."));
      return panel;
    }
    const entry = entries[index];
    const range = ranges[index];
    panel.append(
      element("b", "", labelOf(entry)),
      element("label", "", "Commanders"),
      numberInput(range.first, "First commander", (from) => change(entry.uid, (current) => withRange(current, from, from <= range.last ? range.last : from))),
      element("span", "", "–"),
      numberInput(range.last, "Last commander", (to) => change(entry.uid, (current) => withRange(current, commanderRanges(entries)[index].first, to))),
      element("em", "range-count", `${range.count} commander${range.count === 1 ? "" : "s"}${range.explicit ? "" : " · follows the block before"}`),
    );
    const actions = element("div", "block-controls");
    actions.append(button("← Left", () => move(entry.uid, -1), false, "Swap with the block to its left"), button("Right →", () => move(entry.uid, 1), false, "Swap with the block to its right"));
    if (range.explicit) actions.append(button("Follow previous", () => change(entry.uid, followPrevious), false, "Start right after the block before"));
    actions.append(button("Remove", () => remove(entry.uid), true));
    panel.append(actions);
    return panel;
  }

  function renderPool() {
    const pool = $("#available-tasks");
    if (!pool) return;
    const tasks = getLibrary().tasks;
    pool.replaceChildren(...tasks.map((value) => {
      const row = element("div", "pool-task");
      const copy = element("span");
      copy.append(element("b", "", value.name), element("small", "", summary(value)));
      const placed = entries.some((entry) => entry.kind === "task" && entry.task_id === value.task_id);
      row.append(copy, button(placed ? "Placed" : "Add", () => addTask(value.task_id)));
      row.lastChild.disabled = placed;
      return row;
    }));
    if (!tasks.length) pool.append(element("p", "empty-copy", "Save a task first, then place it here."));
    // Where support waits, from the saved support objects. Only asked once commanders are reserved.
    const select = $("#mode-support");
    if (select) {
      const chosen = select.value;
      select.replaceChildren(new Option(supports().length ? "Choose where support waits" : "No support saved yet", ""), ...supports().map((bot) => new Option(bot.name, String(bot.station_bot_id))));
      if (supports().some((bot) => String(bot.station_bot_id) === chosen)) select.value = chosen;
      showDestination();
    }
  }

  function addTask(taskId) {
    if (entries.some((entry) => entry.kind === "task" && entry.task_id === taskId)) return;
    counter += 1;
    entries.push({ uid: counter, kind: "task", task_id: taskId, commander_count: 1, commander_start: null });
    selected = counter;
    renderPool();
    renderBoard();
  }

  function addSupport() {
    counter += 1;
    entries.push({ uid: counter, kind: "support", task_id: "", commander_count: 1, commander_start: null });
    selected = counter;
    renderBoard();
  }

  $("#add-support-range")?.addEventListener("click", addSupport);

  return {
    render() { renderPool(); renderBoard(); },
    add: addTask,
    clear() {
      entries = []; selected = null;
      if ($("#mode-support")) $("#mode-support").value = "";
      renderPool(); renderBoard();
    },
    /// The slots to send, in board order.
    allocations: () => toRequest(entries),
    /// Where support waits, or `null` when no commanders are reserved (a plain attack bot).
    supportId,
    /// How many commanders are reserved for support; above zero the bot is a hybrid.
    reserved,
    /// The current entries with their starts, for exporting.
    entries: () => entries,
    /// Why this cannot be saved yet, or `null`.
    problem() {
      if (!entries.length) return "Add at least one task.";
      if (!entries.some((entry) => entry.kind === "task")) return "Add at least one task: support alone does not attack.";
      if (reserved() > 0 && !supportId()) return supports().length ? "Choose where support waits for the commanders you reserved." : "Create a support first (Station → Create support), then choose where it waits.";
      return problems(commanderRanges(entries)).length ? "Two blocks share a commander." : null;
    },
  };
}
