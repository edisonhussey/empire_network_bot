const API = "http://127.0.0.1:47821/v1";
const $ = (selector) => document.querySelector(selector);
const activation = $("#activation");
const appShell = $("#app-shell");
let currentLicence = null;
let library = { catalog: [], kingdoms: [], attacks: [], tasks: [], task_runtimes: [], subscriptions: [], modes: [], account_modes: [], recruitments: [], recruit_bots: [], account_recruit_bots: [], castles: [] };
let connectedAccounts = [];
let activeWave = 0;
let priority = "medium";
let selectedModeTasks = [];
let pickerTarget = null;
let pickerSelection = null;
let accountLogText = "";
let logsLoading = false;
let latestDirect = { connected: false, phase: "disconnected", account_id: null };
let renderedConnectionKey = "";
let cachedHunt = null;
let huntFetchedAt = 0;
let cachedDashboard = null;
let dashboardFetchedAt = 0;
const fallbackKingdoms = [
  { id: 0, name: "green_kingdom" }, { id: 1, name: "sand_kingdom" },
  { id: 2, name: "ice_kingdom" }, { id: 3, name: "fire_kingdom" },
  { id: 4, name: "storm_kingdom" }, { id: 10, name: "berimond_kingdom" },
];
const blankSide = () => ({ troops: [], tools: [] });
const blankWave = () => ({ left: blankSide(), middle: blankSide(), right: blankSide() });
let attackWaves = Array.from({ length: 4 }, blankWave);

const phaseCopy = {
  disconnected: "Ready to connect", socket_handshake: "Opening a secure connection…",
  awaiting_room: "Contacting US1…", awaiting_version: "Preparing your session…",
  authenticating: "Signing in…", authenticated: "Sign-in complete",
  loading_account: "Discovering your account…", loading_castle: "Finding your castles…",
  loading_sands: "Preparing Burning Sands…", sands_ready: "Setup complete", failed: "Needs attention",
};

async function responseJson(response) {
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Request failed (${response.status})`);
  return body;
}

async function api(path, options) {
  return responseJson(await fetch(`${API}${path}`, options));
}

function expiryLabel(epoch) {
  if (!epoch) return "Token required";
  const remaining = Math.max(0, epoch * 1000 - Date.now());
  const days = Math.floor(remaining / 86400000);
  const hours = Math.floor((remaining % 86400000) / 3600000);
  return days > 0 ? `${days}d ${hours}h remaining` : `${hours}h remaining`;
}

function showActivation(canClose = false) {
  activation.hidden = false;
  $("#close-activation").hidden = !canClose;
}

function showApplication(licence) {
  currentLicence = licence;
  activation.hidden = true;
  appShell.hidden = false;
  $("#licence-expiry").textContent = expiryLabel(licence.expires_at);
}

async function refreshLicence() {
  const licence = await api("/licence");
  currentLicence = licence;
  if (licence.active) {
    showApplication(licence);
    return true;
  }
  appShell.hidden = true;
  showActivation(false);
  $("#licence-result").textContent = licence.reason || "A valid access token is required.";
  return false;
}

function switchView(name) {
  document.querySelectorAll(".view").forEach((view) => { view.hidden = view.id !== `view-${name}`; });
  document.querySelectorAll("nav [data-view]").forEach((button) => button.classList.toggle("active", button.dataset.view === name));
  const titles = { dashboard: ["Overview", "Dashboard"], attacks: ["Plan builder", "Create attack"], tasks: ["Plan builder", "Create task"], modes: ["Plan builder", "Create mode"], recruitments: ["Recruitment", "Create recruitment"], "recruit-bots": ["Recruitment", "Create recruit bot"], accounts: ["Workspace", "Start"], logs: ["Diagnostics", "Logs"] };
  [$("#page-eyebrow").textContent, $("#page-title").textContent] = titles[name];
}

function compactNumber(value) {
  return new Intl.NumberFormat(undefined, { notation: Math.abs(value || 0) >= 10000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(value || 0);
}

function element(tag, className, text) {
  const value = document.createElement(tag);
  if (className) value.className = className;
  if (text !== undefined) value.textContent = text;
  return value;
}

function humanize(value) {
  return value.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function itemName(id) {
  const name = library.catalog.find((item) => item.id === id)?.name;
  return name ? humanize(name) : `Item ${id}`;
}

function kingdomName(name) {
  const names = {
    green_kingdom: "Green",
    sand_kingdom: "Burning Sands",
    ice_kingdom: "Everwinter Ice",
    fire_kingdom: "Fire Peaks",
    storm_kingdom: "Storm Islands",
    berimond_kingdom: "Berimond",
  };
  return names[name] || humanize(name);
}

/// A short wait, for "clears in 12m" style copy.
function humanWait(ms) {
  if (!ms || ms <= 0) return "now";
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

/// Age of a timestamp, for "last used 2h ago".
function relativeTime(ms) {
  if (!ms) return "never";
  const seconds = Math.max(0, (Date.now() - ms) / 1000);
  if (seconds < 90) return "just now";
  const minutes = seconds / 60;
  if (minutes < 90) return `${Math.round(minutes)}m ago`;
  const hours = minutes / 60;
  if (hours < 36) return `${Math.round(hours)}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/// The cadence names are about request timing, so say that rather than echoing
/// the stored word.
function tempoLabel(value) {
  const names = { greedy: "fast gaps", sporadic: "sporadic gaps", advanced: "randomised gaps" };
  return names[value] || humanize(value);
}

/// The server's own estimate for one castle's recruitment queue.
function recruitStateFor(accountId, castleId) {
  return (library.recruit_states || []).find(
    (value) => value.account_id.toLowerCase() === String(accountId).toLowerCase() && value.castle_id === castleId,
  );
}

/// What a castle's queue is doing, in the server's own terms. `TCT` is already
/// cumulative, so it is used directly rather than multiplied by slot count.
function recruitStateLabel(state) {
  const busy = state && (state.queue_clear_at_ms || state.active_quantity || state.queued_quantity);
  if (!busy) return "Queue empty — nothing recruited yet";
  const parts = [`${(state.active_quantity || 0) + (state.queued_quantity || 0)} queued`];
  const wait = state.queue_clear_at_ms - Date.now();
  parts.push(wait > 0 ? `clears in ${humanWait(wait)}` : "ready to refill");
  if (state.help_active) parts.push("alliance helping");
  return parts.join(" · ");
}

/// The soonest moment any castle in this recruit bot becomes free again.
function soonestCastleClear(bot) {
  const waits = bot.castles
    .map((subscription) => recruitStateFor(subscription.account_id, subscription.castle_id))
    .filter(Boolean)
    .map((state) => state.queue_clear_at_ms - Date.now())
    .filter((wait) => wait > 0);
  return waits.length ? Math.min(...waits) : 0;
}

function renderKingdoms() {
  const kingdoms = library.kingdoms?.length ? library.kingdoms : fallbackKingdoms;
  for (const selector of ["#source-kid"]) {
    const select = $(selector);
    const previous = select.value || "1";
    select.replaceChildren(...kingdoms.map((kingdom) => {
      const option = element("option", "", kingdomName(kingdom.name));
      option.value = kingdom.id;
      return option;
    }));
    select.value = kingdoms.some((kingdom) => String(kingdom.id) === previous) ? previous : "1";
  }
  updateTargetKinds();
}

function updateTargetKinds() {
  const kingdomId = Number($("#source-kid").value);
  const fortress = $("#target-kind option[value='fortress']");
  const supported = [1, 2, 3].includes(kingdomId);
  fortress.hidden = !supported;
  fortress.disabled = !supported;
  if (!supported && $("#target-kind").value === "fortress") $("#target-kind").value = "rbc";
  const isFortress = $("#target-kind").value === "fortress";
  $("#target-level-fields").hidden = isFortress;
  $("#target-level-min").required = !isFortress;
  $("#target-level-max").required = !isFortress;
  $("#target-hint").textContent = $("#target-kind").value === "fortress"
    ? "This kingdom has one fortress target type, so no level range is needed."
    : "OpenAuto chooses a learned Robber Baron in this level range; no destination coordinate is required.";
  updateSourceCoordinates();
}

function updateSourceCoordinates() {
  const automatic = $("#use-main-castle").checked;
  const kingdomId = Number($("#source-kid").value);
  const health = connectedAccounts.flatMap((account) => account.kingdom_health || []).find((value) => value.kingdom_id === kingdomId);
  for (const input of [$("#source-x"), $("#source-y")]) input.disabled = automatic;
  if (automatic && health) {
    $("#source-x").value = health.x;
    $("#source-y").value = health.y;
  }
}

function slotButton(sideName, kind) {
  const values = attackWaves[activeWave][sideName][kind];
  const slot = values[0];
  const button = element("button", `slot ${kind === "troops" ? "troop-slot" : "tool-slot"}`);
  button.type = "button";
  button.append(element("small", "", kind === "troops" ? "Troops" : "Tools"));
  button.append(element("b", "", slot ? itemName(slot.item_id) : "Empty"));
  if (slot) button.append(element("span", "", `× ${slot.amount}`));
  button.addEventListener("click", () => openPicker(sideName, kind));
  return button;
}

function renderWave() {
  const builder = $("#wave-builder");
  builder.replaceChildren();
  for (const [key, name] of [["left", "Left flank"], ["middle", "Center"], ["right", "Right flank"]]) {
    const side = element("section", "side-builder");
    side.append(element("h3", "", name));
    const slots = element("div", "slot-grid");
    slots.append(slotButton(key, "troops"), slotButton(key, "tools"));
    side.append(slots);
    builder.append(side);
  }
}

function openPicker(side, kind) {
  pickerTarget = { side, kind };
  const current = attackWaves[activeWave][side][kind][0];
  pickerSelection = current ? library.catalog.find((item) => item.id === current.item_id) || null : null;
  $("#picker-title").textContent = kind === "troops" ? "Choose troops" : "Choose tools";
  $("#item-search").value = "";
  $("#item-amount").value = current?.amount || 1;
  renderPicker();
  $("#item-picker").showModal();
  $("#item-search").focus();
}

function renderPicker() {
  const kind = pickerTarget?.kind === "troops" ? "troop" : "tool";
  const query = $("#item-search").value.trim().toLowerCase();
  const results = $("#item-results");
  results.replaceChildren();
  const matches = library.catalog.filter((item) => item.kind === kind && humanize(item.name).toLowerCase().includes(query)).sort((a, b) => a.name.localeCompare(b.name));
  matches.forEach((item) => {
    const button = element("button", `picker-item${pickerSelection?.id === item.id ? " selected" : ""}`);
    button.type = "button";
    button.dataset.itemId = item.id;
    button.append(element("b", "", humanize(item.name)), element("small", "", kind === "troop" ? "Troop" : "Tool"));
    button.addEventListener("click", () => choosePickerItem(item));
    button.addEventListener("dblclick", () => { choosePickerItem(item); confirmPickerItem(); });
    results.append(button);
  });
  if (!matches.length) results.append(element("p", "picker-empty", library.catalog.length ? "No matching options." : "The item catalog is unavailable. Restart OpenAuto to refresh its background service."));
  $("#picker-selection").textContent = pickerSelection ? humanize(pickerSelection.name) : "Choose an option above";
  $("#confirm-item").disabled = !pickerSelection;
}

function choosePickerItem(item) {
  pickerSelection = item;
  document.querySelectorAll("#item-results .picker-item").forEach((button) => {
    button.classList.toggle("selected", button.dataset.itemId === String(item.id));
  });
  $("#picker-selection").textContent = humanize(item.name);
  $("#confirm-item").disabled = false;
}

function confirmPickerItem() {
  if (!pickerTarget || !pickerSelection) return;
  const amount = Math.max(1, Number($("#item-amount").value) || 1);
  attackWaves[activeWave][pickerTarget.side][pickerTarget.kind] = [{ item_id: pickerSelection.id, amount }];
  $("#item-picker").close();
  renderWave();
}

function clearPickerItem() {
  if (!pickerTarget) return;
  attackWaves[activeWave][pickerTarget.side][pickerTarget.kind] = [];
  $("#item-picker").close();
  renderWave();
}

function attackDraft() {
  return { name: $("#attack-name").value.trim(), waves: attackWaves };
}

function profileToDraft(profile) {
  const side = (value) => ({
    troops: (value?.U || []).filter(([id, amount]) => id >= 0 && amount > 0).map(([item_id, amount]) => ({ item_id, amount })),
    tools: (value?.T || []).filter(([id, amount]) => id >= 0 && amount > 0).map(([item_id, amount]) => ({ item_id, amount })),
  });
  return {
    name: profile.name,
    waves: profile.payload.map((wave) => ({ left: side(wave.L), middle: side(wave.M), right: side(wave.R) })),
  };
}

function priorityName(value) {
  return value <= 10 ? "extra_high" : value <= 20 ? "high" : value <= 30 ? "medium" : "low";
}

function taskToDraft(task) {
  const runtime = library.task_runtimes.find((value) => value.task_id === task.task_id);
  const subscription = library.subscriptions.find((value) => value.task_id === task.task_id);
  if (!runtime) return null;
  const targetKind = subscription?.target_kind || "rbc";
  return {
    name: task.name,
    attack_profile_id: task.profile_id,
    source: { kingdom_id: runtime.source_kingdom_id, x: runtime.source_x, y: runtime.source_y },
    source_kind: runtime.source_kind || "coordinate",
    destination: targetKind === "fortress" ? {
      kind: "fortress", kingdom_id: task.kingdom_id,
    } : task.target_level_min !== null ? {
      kind: targetKind === "fortress" ? "fortress_level_range" : "rbc_level_range", kingdom_id: task.kingdom_id,
      minimum: task.target_level_min, maximum: task.target_level_max,
    } : {
      kind: "coordinate", kingdom_id: runtime.target_kingdom_id, x: runtime.target_x, y: runtime.target_y,
    },
    algorithm: subscription?.filter?.algorithm || "advanced",
    travel: "coin", priority: priorityName(task.priority), commander_count: 1,
  };
}

function bundleForMode(name, allocations) {
  return {
    schema: 1,
    name,
    tasks: allocations.map((allocation) => {
      const task = library.tasks.find((value) => value.task_id === allocation.task_id);
      const draft = taskToDraft(task);
      const profile = library.attacks.find((value) => value.profile_id === task.profile_id);
      return { name: draft.name, attack: profileToDraft(profile), source: draft.source, source_kind: draft.source_kind, destination: draft.destination, algorithm: draft.algorithm, travel: draft.travel, priority: draft.priority, commander_count: allocation.commander_count };
    }),
  };
}

async function copyJson(value, output) {
  await navigator.clipboard.writeText(JSON.stringify(value, null, 2));
  if (output) output.textContent = "Copied to clipboard.";
}

function actionButton(label, action, danger = false) {
  const button = element("button", danger ? "mini-action danger" : "mini-action", label);
  button.type = "button";
  button.addEventListener("click", action);
  return button;
}

function renderLibrary() {
  const attackList = $("#attack-library");
  attackList.replaceChildren(...library.attacks.map((attack) => {
    const row = element("article", "library-row");
    const copy = actionButton("Copy JSON", () => copyJson(profileToDraft(attack), $("#attack-result")));
    const remove = actionButton("Delete", async () => { await fetch(`${API}/plans/attacks/${attack.profile_id}`, { method: "DELETE" }); await loadLibrary(); }, true);
    row.append(element("div", "", attack.name), element("span", "row-actions"));
    row.lastChild.append(copy, remove);
    return row;
  }));
  if (!library.attacks.length) attackList.append(element("p", "empty-copy", "No attacks saved yet."));

  const attackSelect = $("#task-attack");
  attackSelect.replaceChildren(...library.attacks.map((attack) => {
    const option = element("option", "", attack.name); option.value = attack.profile_id; return option;
  }));

  const taskList = $("#task-library");
  taskList.replaceChildren(...library.tasks.map((task) => {
    const row = element("article", "library-row");
    const copy = actionButton("Copy JSON", () => copyJson(taskToDraft(task), $("#task-result")));
    const remove = actionButton("Delete", async () => { await fetch(`${API}/plans/tasks/${task.task_id}`, { method: "DELETE" }); await loadLibrary(); }, true);
    const subscription = library.subscriptions.find((value) => value.task_id === task.task_id);
    const targetLabel = subscription?.target_kind === "fortress" ? "Fortress placeholder" : "Robber Baron";
    const levelLabel = subscription?.target_kind === "fortress" ? "" : task.target_level_min === null ? "fixed target" : `levels ${task.target_level_min}–${task.target_level_max}`;
    const algorithm = subscription?.filter?.algorithm || "advanced";
    const identity = element("div"); identity.append(element("b", "", task.name), element("small", "", `${kingdomName((library.kingdoms.find((value) => value.id === task.kingdom_id) || fallbackKingdoms.find((value) => value.id === task.kingdom_id))?.name || `Kingdom ${task.kingdom_id}`)} · ${targetLabel} ${levelLabel} · ${humanize(algorithm)} · ${priorityName(task.priority).replace("_", " ")}`));
    const actions = element("span", "row-actions"); actions.append(copy, remove); row.append(identity, actions); return row;
  }));
  if (!library.tasks.length) taskList.append(element("p", "empty-copy", "Create an attack first, then add a task."));
  renderAvailableTasks();
  renderModes();
}

function renderAvailableTasks() {
  const container = $("#available-tasks");
  container.replaceChildren(...library.tasks.map((task) => draggableTask(task, false)));
  renderSelectedTasks();
}

function draggableTask(task, selected) {
  const row = element("div", "drag-task");
  row.draggable = true;
  row.dataset.taskId = task.task_id;
  row.addEventListener("dragstart", (event) => { event.dataTransfer.effectAllowed = "move"; event.dataTransfer.setData("text/plain", task.task_id); event.dataTransfer.setData("text", task.task_id); });
  row.append(element("i", "drag-handle", "⠿"));
  const copy = element("span"); copy.append(element("b", "", task.name), element("small", "", "Reusable task")); row.append(copy);
  if (selected) {
    const controls = element("span", "task-order-actions");
    controls.append(
      actionButton("↑", () => moveModeTask(task.task_id, -1)),
      actionButton("↓", () => moveModeTask(task.task_id, 1)),
      actionButton("Remove", () => { selectedModeTasks = selectedModeTasks.filter((entry) => entry.task_id !== task.task_id); renderAvailableTasks(); }),
    );
    row.append(controls);
  } else {
    row.append(actionButton("Add", () => addModeTask(task.task_id)));
  }
  return row;
}

function moveModeTask(taskId, delta) {
  const from = selectedModeTasks.findIndex((entry) => entry.task_id === taskId);
  const to = Math.max(0, Math.min(selectedModeTasks.length - 1, from + delta));
  if (from < 0 || from === to) return;
  selectedModeTasks.splice(to, 0, selectedModeTasks.splice(from, 1)[0]);
  renderAvailableTasks();
}

function addModeTask(taskId) {
  const existing = selectedModeTasks.find((entry) => entry.task_id === taskId);
  if (existing) existing.commander_count += 1;
  else selectedModeTasks.push({ task_id: taskId, commander_count: 1 });
  renderAvailableTasks();
}

function setAllocation(taskId, count) {
  const allocation = selectedModeTasks.find((entry) => entry.task_id === taskId);
  if (!allocation) return;
  allocation.commander_count = Math.max(1, Math.floor(Number(count) || 1));
  renderSelectedTasks();
}

function renderSelectedTasks() {
  const container = $("#mode-tasks");
  container.replaceChildren();
  let first = 1;
  selectedModeTasks.forEach((allocation) => {
    const task = library.tasks.find((value) => value.task_id === allocation.task_id);
    if (!task) return;
    const row = element("article", "allocation-card");
    row.draggable = true;
    row.dataset.taskId = task.task_id;
    row.addEventListener("dragstart", (event) => event.dataTransfer.setData("text/plain", task.task_id));
    const head = element("div", "allocation-head");
    const title = element("span"); title.append(element("b", "", task.name), element("small", "", "Assigned task"));
    const count = document.createElement("input"); count.type = "number"; count.min = "1"; count.value = allocation.commander_count; count.setAttribute("aria-label", `${task.name} commander count`); count.addEventListener("change", () => setAllocation(task.task_id, count.value));
    const controls = element("span", "allocation-actions");
    controls.append(actionButton("−", () => setAllocation(task.task_id, allocation.commander_count - 1)), count, actionButton("+", () => setAllocation(task.task_id, allocation.commander_count + 1)), actionButton("←", () => moveModeTask(task.task_id, -1)), actionButton("→", () => moveModeTask(task.task_id, 1)), actionButton("Remove", () => { selectedModeTasks = selectedModeTasks.filter((entry) => entry.task_id !== task.task_id); renderAvailableTasks(); }));
    head.append(title, controls);
    const last = first + allocation.commander_count - 1;
    const track = element("div", "commander-track");
    for (let commander = first; commander <= last; commander += 1) track.append(element("span", "", commander));
    row.append(head, track, element("em", "commander-range", `Commanders ${first}–${last}`));
    first = last + 1;
    container.append(row);
  });
  if (!selectedModeTasks.length) container.append(element("div", "drop-hint", "Drag tasks here"));
  const total = first - 1;
  $("#commander-warning").textContent = `${total} commanders allocated${total ? ` · numbered 1–${total}` : ""}`;
}

function renderModes() {
  const container = $("#mode-library");
  container.replaceChildren(...library.modes.map((mode) => {
    const row = element("article", "library-row mode-row");
    const allocations = mode.allocations?.length ? mode.allocations : mode.task_ids.map((task_id) => ({ task_id, commander_count: 1 }));
    const identity = element("div"); identity.append(element("b", "", mode.name), element("small", "", `${allocations.length} tasks · ${mode.commander_count} commanders`));
    const actions = element("span", "row-actions");
    actions.append(actionButton("Copy JSON", () => copyJson(bundleForMode(mode.name, allocations), $("#mode-result"))));
    actions.append(actionButton("Delete", async () => { await fetch(`${API}/plans/modes/${mode.mode_id}`, { method: "DELETE" }); await loadLibrary(); }, true));
    row.append(identity, actions); return row;
  }));
  if (!library.modes.length) container.append(element("p", "empty-copy", "No modes saved yet."));
}

function renderRecruitmentBuilder() {
  const select = $("#recruitment-troop");
  const previous = select.value;
  const troops = library.catalog.filter((item) => item.kind === "troop").sort((a, b) => a.name.localeCompare(b.name));
  select.replaceChildren(...troops.map((troop) => { const option = element("option", "", humanize(troop.name)); option.value = troop.id; return option; }));
  if (troops.some((item) => String(item.id) === previous)) select.value = previous;

  const list = $("#recruitment-library");
  list.replaceChildren(...library.recruitments.map((value) => {
    const row = element("article", "library-row");
    const identity = element("div"); identity.append(element("b", "", value.name), element("small", "", `${itemName(value.troop_id)} · ${value.quantity} × ${value.slot_count} slots${value.ask_alliance_help ? " · alliance help" : ""}`));
    const actions = element("span", "row-actions"); actions.append(actionButton("Delete", async () => { await fetch(`${API}/plans/recruitments/${value.recruitment_id}`, { method: "DELETE" }); await loadLibrary(); }, true));
    row.append(identity, actions); return row;
  }));
  if (!library.recruitments.length) list.append(element("p", "empty-copy", "No recruitment objects saved yet."));

  const castles = $("#recruit-castles"); castles.replaceChildren();
  for (const account of connectedAccounts) {
    const heading = element("p", "castle-account-label", account.player_name);
    castles.append(heading);
    for (const castle of library.castles.filter((value) => value.account_id.toLowerCase() === account.account_id.toLowerCase())) {
      const row = element("label", "castle-subscription");
      const enabled = document.createElement("input"); enabled.type = "checkbox"; enabled.dataset.castleId = castle.castle_id; enabled.dataset.accountId = account.account_id;
      const name = element("span"); name.append(
        element("b", "", castle.name || `Castle ${castle.castle_id}`),
        element("small", "", `${kingdomName((library.kingdoms.find((value) => value.id === castle.kingdom_id) || fallbackKingdoms.find((value) => value.id === castle.kingdom_id))?.name || `Kingdom ${castle.kingdom_id}`)} · ${castle.x}:${castle.y}`),
        element("small", "castle-queue", recruitStateLabel(recruitStateFor(account.account_id, castle.castle_id))),
      );
      const assignment = document.createElement("select"); assignment.disabled = true; assignment.dataset.assignmentFor = castle.castle_id;
      assignment.append(...library.recruitments.map((value) => { const option = element("option", "", value.name); option.value = value.recruitment_id; return option; }));
      enabled.addEventListener("change", () => { assignment.disabled = !enabled.checked; });
      row.append(enabled, name, assignment); castles.append(row);
    }
  }
  if (!connectedAccounts.length) castles.append(element("p", "empty-copy", "Initialize an account before assigning its castles."));
  else if (!library.recruitments.length) castles.append(element("p", "empty-copy", "Create a recruitment object first."));

  const botList = $("#recruit-bot-library"); botList.replaceChildren(...library.recruit_bots.map((bot) => {
    const row = element("article", "library-row");
    const identity = element("div");
    const soonest = soonestCastleClear(bot);
    identity.append(
      element("b", "", bot.name),
      element("small", "", `${tempoLabel(bot.algorithm)} · ${bot.castles.length} castle${bot.castles.length === 1 ? "" : "s"}`),
      element("small", "castle-queue", soonest ? `Next castle free in ${humanWait(soonest)}` : "Every subscribed castle is free"),
    );
    const actions = element("span", "row-actions"); actions.append(actionButton("Delete", async () => { await fetch(`${API}/plans/recruit-bots/${bot.recruit_bot_id}`, { method: "DELETE" }); await loadLibrary(); }, true));
    row.append(identity, actions); return row;
  }));
  if (!library.recruit_bots.length) botList.append(element("p", "empty-copy", "No recruit bots saved yet."));
}

async function loadLibrary() {
  library = await api("/plans");
  library.subscriptions ||= [];
  library.account_modes ||= [];
  library.recruitments ||= [];
  library.recruit_bots ||= [];
  library.account_recruit_bots ||= [];
  library.recruit_states ||= [];
  library.castles ||= [];
  library.kingdoms ||= fallbackKingdoms;
  renderKingdoms();
  renderWave();
  renderPicker();
  renderLibrary();
  renderRecruitmentBuilder();
}

function emptyAccounts() {
  const empty = element("div", "empty-state"); empty.append(element("i", "", "+"), element("p", "", "Your saved accounts will appear here.")); $("#accounts").replaceChildren(empty);
}

async function connectSavedAccount(account, password, radius, reuseExistingMap, output) {
  if (!password) { output.textContent = "Enter the account password first."; return; }
  output.textContent = reuseExistingMap ? "Establishing connection…" : "Connecting and extending the Sands scan…";
  await api("/accounts", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ server: "US1", username: account.player_name, password, scan_radius: radius, reuse_existing_map: reuseExistingMap }),
  });
  await refresh();
}

async function refreshAccounts(knownDirect = null) {
  connectedAccounts = await api("/accounts");
  const direct = knownDirect || await api("/direct");
  latestDirect = direct;
  $("#account-onboarding").hidden = connectedAccounts.length > 0;
  if (!connectedAccounts.length) { emptyAccounts(); renderModes(); return; }
  $("#accounts").replaceChildren(...connectedAccounts.map((account) => {
    const card = element("article", "account-item"); const avatar = element("div", "account-avatar", account.player_name.slice(0, 1).toUpperCase());
    const isConnected = direct.connected && direct.account_id?.toLowerCase() === account.account_id.toLowerCase();
    const isReady = isConnected && direct.phase === "sands_ready";
    const identity = element("div"); identity.append(element("strong", "", account.player_name), element("span", isConnected ? "connection-label live" : "connection-label", isReady ? "US1 · Connected" : isConnected ? "US1 · Connecting" : "US1 · Stored locally"));
    const facts = element("dl");
    for (const [name, value] of [["Castles", account.castle_count], ["Commanders", account.commander_count], ["Targets", account.rbc_count]]) { const group = element("div"); group.append(element("dd", "", value), element("dt", "", name)); facts.append(group); }
    const discovery = element("div", "kingdom-health");
    discovery.append(element("p", "list-label", "Map discovery by main castle"));
    for (const health of account.kingdom_health || []) {
      const line = element("div", "kingdom-health-row");
      line.append(element("b", "", kingdomName((library.kingdoms.find((value) => value.id === health.kingdom_id) || fallbackKingdoms.find((value) => value.id === health.kingdom_id))?.name || `Kingdom ${health.kingdom_id}`)), element("span", "", `${health.target_count} targets · ${health.scan_window_count} map areas`));
      discovery.append(line);
    }
    if (!(account.kingdom_health || []).length) discovery.append(element("p", "empty-copy", "No main-castle map data learned yet."));
    // Step 1 — the socket. Kept separate from automation so it is obvious that
    // signing in and starting a bot are two different decisions.
    const connection = element("div", "account-connection-control");
    connection.append(element("p", "list-label", "Step 1 · Connection"));
    const password = document.createElement("input"); password.type = "password"; password.placeholder = "Password"; password.autocomplete = "current-password";
    const radius = document.createElement("input"); radius.type = "number"; radius.min = "0"; radius.max = "500"; radius.value = "50"; radius.title = "Sands scan radius"; radius.setAttribute("aria-label", "Additional Sands scan radius");
    const connectionResult = element("span", "connection-result", isConnected ? "Connected. Cached map data is being used." : "Not connected. Your castles and targets are still shown from the last visit.");
    const login = actionButton("Start connection", async () => {
      try { await connectSavedAccount(account, password.value, 0, true, connectionResult); password.value = ""; }
      catch (error) { password.value = ""; connectionResult.textContent = error.message; }
    });
    login.disabled = isConnected;
    const close = actionButton("Close connection", async () => {
      await api("/direct", { method: "DELETE" });
      await loadLibrary(); await refreshAccounts(); await refreshDashboard();
    });
    close.classList.add("danger-outline"); close.disabled = !isConnected;
    const scan = actionButton("Rescan map", async () => {
      try { await connectSavedAccount(account, password.value, Number(radius.value) || 0, false, connectionResult); password.value = ""; }
      catch (error) { password.value = ""; connectionResult.textContent = error.message; }
    });
    const scanRow = element("div", "connection-scan");
    scanRow.append(close, radius, scan);
    connection.append(password, login, scanRow, connectionResult);

    // Step 2 — automation. One attack bot, plus at most one recruit bot.
    const assignment = element("div", "account-mode-control");
    const current = library.account_modes.find((value) => value.account_id.toLowerCase() === account.account_id.toLowerCase());
    const currentRecruit = library.account_recruit_bots.find((value) => value.account_id.toLowerCase() === account.account_id.toLowerCase());
    const running = Boolean(current?.running && isConnected);
    assignment.append(element("p", "list-label", "Step 2 · Automation"));
    const attackLabel = document.createElement("label"); attackLabel.textContent = "Attack bot";
    const attackSelect = document.createElement("select");
    attackSelect.append(...library.modes.map((mode) => { const option = element("option", "", `${mode.name} · ${mode.commander_count} commanders`); option.value = mode.mode_id; return option; }));
    if (current) attackSelect.value = current.mode_id;
    const recruitLabel = document.createElement("label"); recruitLabel.textContent = "Recruit bot (optional)";
    const recruitSelect = document.createElement("select");
    const usedAt = (bot) => library.account_recruit_bots.find((value) => value.recruit_bot_id === bot.recruit_bot_id)?.updated_at_ms || 0;
    // Most recently used first, so the usual choice is the first thing offered.
    const orderedBots = [...library.recruit_bots].sort((a, b) => usedAt(b) - usedAt(a) || a.name.localeCompare(b.name));
    const none = element("option", "", "None — attacks only"); none.value = ""; recruitSelect.append(none);
    recruitSelect.append(...orderedBots.map((bot) => {
      const used = usedAt(bot);
      const option = element("option", "", `${bot.name} · ${tempoLabel(bot.algorithm)}${used ? ` · used ${relativeTime(used)}` : ""}`);
      option.value = bot.recruit_bot_id; return option;
    }));
    if (currentRecruit) recruitSelect.value = currentRecruit.recruit_bot_id;
    const recruitEstimate = element("p", "recruit-estimate");
    const describeRecruit = () => {
      const bot = library.recruit_bots.find((value) => String(value.recruit_bot_id) === recruitSelect.value);
      if (!bot) { recruitEstimate.textContent = "No recruitment will run."; return; }
      const soonest = soonestCastleClear(bot);
      recruitEstimate.textContent = `${bot.castles.length} castle${bot.castles.length === 1 ? "" : "s"} · ${tempoLabel(bot.algorithm)} · ${soonest ? `next free in ${humanWait(soonest)}` : "every castle free"}`;
    };
    recruitSelect.addEventListener("change", describeRecruit);
    describeRecruit();
    const summary = element("span", running ? "bot-state running" : "bot-state", running ? `Running${currentRecruit?.running ? " with recruitment" : ""}` : "Stopped");
    const controls = element("div", "bot-start-actions");
    const start = actionButton(running ? "Bot running" : "Start bot", async () => {
      try {
        await api("/plans/start", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ account_id: account.account_id, mode_id: Number(attackSelect.value), recruit_bot_id: recruitSelect.value ? Number(recruitSelect.value) : null, running: true }) });
        await loadLibrary(); await refreshAccounts(); await refreshDashboard();
      } catch (error) { connectionResult.textContent = error.message; }
    });
    start.disabled = !isReady || running || !library.modes.length;
    const stop = actionButton("End bot", async () => {
      try {
        await api("/plans/start", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ account_id: account.account_id, mode_id: Number(attackSelect.value), recruit_bot_id: recruitSelect.value ? Number(recruitSelect.value) : null, running: false }) });
        await loadLibrary(); await refreshAccounts(); await refreshDashboard();
      } catch (error) { connectionResult.textContent = error.message; }
    }, true);
    stop.disabled = !running;
    attackSelect.disabled = running; recruitSelect.disabled = running;
    controls.append(start, stop, summary);
    assignment.append(attackLabel, attackSelect, recruitLabel, recruitSelect, recruitEstimate, controls);
    if (!isReady && !running) assignment.append(element("p", "empty-copy", "Start the connection in step 1 to run automation."));
    if (!library.modes.length) assignment.append(element("p", "empty-copy", "Create an attack mode before starting automation."));
    card.append(avatar, identity, facts, discovery, connection, assignment); return card;
  }));
  renderedConnectionKey = `${direct.connected}:${direct.account_id || ""}:${direct.phase === "sands_ready"}`;
  updateSourceCoordinates();
  renderModes();
  renderRecruitmentBuilder();
}

function dashboardStat(value, label) {
  const card = element("div", "health-stat");
  card.append(element("b", "", compactNumber(value)), element("span", "", label));
  return card;
}

function renderRubyChart(series) {
  const container = $("#ruby-chart");
  if (!series?.length) { container.replaceChildren(element("p", "empty-copy", "No ruby returns recorded yet.")); return; }
  const width = 800; const height = 190; const padX = 34; const padY = 22;
  const values = series.map((point) => point.value);
  const minimum = Math.min(...values); const maximum = Math.max(...values); const span = Math.max(1, maximum - minimum);
  const points = series.map((point, index) => {
    const x = padX + index * (width - padX * 2) / Math.max(1, series.length - 1);
    const y = height - padY - (point.value - minimum) * (height - padY * 2) / span;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
  const first = new Date(series[0].at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const last = new Date(series.at(-1).at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const area = `${padX},${height - padY} ${points} ${width - padX},${height - padY}`;
  container.innerHTML = `<svg viewBox="0 0 ${width} ${height}" preserveAspectRatio="none"><defs><linearGradient id="ruby-fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef4d5b" stop-opacity=".22"/><stop offset="1" stop-color="#ef4d5b" stop-opacity=".02"/></linearGradient></defs><line class="grid" x1="${padX}" y1="${padY}" x2="${width - padX}" y2="${padY}"/><line class="grid" x1="${padX}" y1="${height / 2}" x2="${width - padX}" y2="${height / 2}"/><line class="grid" x1="${padX}" y1="${height - padY}" x2="${width - padX}" y2="${height - padY}"/><polygon class="area" points="${area}"/><polyline class="line" points="${points}"/><text x="${padX}" y="${height - 5}">${first}</text><text x="${width - padX}" y="${height - 5}" text-anchor="end">${last}</text><text x="${width - 4}" y="${padY + 3}" text-anchor="end">${compactNumber(maximum)}</text><text x="${width - 4}" y="${height - padY}" text-anchor="end">${compactNumber(minimum)}</text></svg>`;
}

async function refreshDashboard(knownDirect = null, force = false) {
  try {
    const now = Date.now();
    const huntRequest = force || !cachedHunt || now - huntFetchedAt >= 10000 ? api("/hunt").then((value) => { cachedHunt = value; huntFetchedAt = Date.now(); return value; }) : Promise.resolve(cachedHunt);
    const summaryRequest = force || !cachedDashboard || now - dashboardFetchedAt >= 60000 ? api("/dashboard").then((value) => { cachedDashboard = value; dashboardFetchedAt = Date.now(); return value; }) : Promise.resolve(cachedDashboard);
    const [hunt, direct, summary] = await Promise.all([huntRequest, knownDirect ? Promise.resolve(knownDirect) : api("/direct"), summaryRequest]);
    const runningAssignments = library.account_modes.filter((value) => value.running);
    const runnerActive = hunt.active && direct.connected && runningAssignments.length > 0;
    const connected = direct.connected;
    const state = runnerActive ? "Bot running" : connected ? "Account connected" : "Automation stopped";
    $("#dashboard-state").textContent = state;
    $("#dashboard-dot").className = `live-dot${runnerActive ? " running" : direct.phase === "failed" ? " error" : ""}`;
    const assignment = runningAssignments[0];
    const mode = assignment && library.modes.find((value) => value.mode_id === assignment.mode_id);
    const scanProgress = direct.scan_total ? `Sands scan ${direct.scan_sent + direct.scan_cached}/${direct.scan_total} · ${direct.scan_cached} cached` : null;
    $("#dashboard-subtitle").textContent = scanProgress || (runnerActive ? `${mode?.name || hunt.label || "Automation"} · ${direct.account_id || hunt.account_id || "connected account"}` : connected ? "Connected and ready to run a bot" : phaseCopy[direct.phase] || "No active session");
    $("#metric-attacks").textContent = compactNumber(hunt.marches);
    $("#metric-returned").textContent = `${compactNumber(hunt.returned)} returned`;
    $("#metric-flight").textContent = compactNumber(hunt.in_flight);
    $("#metric-coins").textContent = compactNumber(hunt.coins);
    $("#metric-rubies").textContent = compactNumber(hunt.rubies);
    $("#rate-attacks").textContent = compactNumber(summary.attacks_last_hour);
    $("#rate-returns").textContent = compactNumber(summary.returns_last_hour);
    $("#rate-rubies").textContent = compactNumber(summary.rubies_last_hour);
    $("#rate-coins").textContent = compactNumber(summary.coins_last_hour);
    $("#chart-updated").textContent = `Updated ${new Date(summary.generated_at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
    renderRubyChart(summary.ruby_series);

    const totals = connectedAccounts.reduce((value, account) => {
      value.castles += account.castle_count || 0; value.commanders += account.commander_count || 0; value.targets += account.rbc_count || 0;
      value.areas += (account.kingdom_health || []).reduce((sum, health) => sum + (health.scan_window_count || 0), 0);
      return value;
    }, { castles: 0, commanders: 0, targets: 0, areas: 0 });
    $("#dashboard-health").replaceChildren(dashboardStat(totals.castles, "Castles"), dashboardStat(totals.commanders, "Commanders"), dashboardStat(totals.targets, "Targets"), dashboardStat(totals.areas, "Map areas"));

    const setup = $("#dashboard-assignment"); setup.replaceChildren();
    for (const [label, value] of [["Account", assignment?.account_id || hunt.account_id || connectedAccounts[0]?.player_name || "None"], ["Mode", mode?.name || hunt.label || "None"], ["Session", connected ? "Connected" : "Disconnected"], ["Map setup", direct.phase === "sands_ready" ? "Sands ready" : humanize(direct.phase || "disconnected")]]) {
      const line = element("div", "assignment-line"); line.append(element("span", "", label), element("b", "", value)); setup.append(line);
    }

    const events = (hunt.recent || []).map((march) => ({ at_ms: march.result_at_ms || march.sent_at_ms, kind: "attack", march }));
    events.push(...(summary.scan_activity || []).map((scan) => ({ at_ms: scan.at_ms, kind: "scan", scan })));
    if (direct.scan_total && direct.scan_sent + direct.scan_cached < direct.scan_total) events.push({ at_ms: Date.now(), kind: "scanning", direct });
    events.sort((left, right) => right.at_ms - left.at_ms);
    const activity = $("#dashboard-activity"); activity.replaceChildren();
    for (const event of events) {
      const row = element("div", "activity-row");
      const time = document.createElement("time"); time.textContent = new Date(event.at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      if (event.kind === "scan") {
        const name = kingdomName((library.kingdoms.find((value) => value.id === event.scan.kingdom_id) || fallbackKingdoms.find((value) => value.id === event.scan.kingdom_id))?.name || `Kingdom ${event.scan.kingdom_id}`);
        row.append(time, element("b", "", `Learned ${event.scan.windows} ${name} map ${event.scan.windows === 1 ? "area" : "areas"}`), element("span", "activity-tag", "Map scan"));
      } else if (event.kind === "scanning") {
        const completed = event.direct.scan_sent + event.direct.scan_cached;
        row.append(time, element("b", "", `Scanning Sands map · ${completed} of ${event.direct.scan_total} areas processed`), element("span", "activity-tag", "In progress"));
      } else {
        const march = event.march;
        const target = `K${march.kingdom_id} · ${march.x}:${march.y}${march.level == null ? "" : ` · level ${march.level}`}`;
        row.append(time, element("b", "", `${march.task_id || "Attack"} → ${target}`), element("span", "activity-tag", humanize(march.status)));
      }
      activity.append(row);
    }
    if (!events.length) activity.append(element("p", "empty-copy", "No recorded activity yet."));
  } catch (error) {
    $("#dashboard-state").textContent = "Statistics unavailable";
    $("#dashboard-subtitle").textContent = error.message;
    $("#dashboard-dot").className = "live-dot error";
  }
}

function sanitizedLogValue(value) {
  if (Array.isArray(value)) return value.map(sanitizedLogValue);
  if (!value || typeof value !== "object") return value;
  const safe = {};
  for (const [key, child] of Object.entries(value)) {
    safe[key] = /^(pw|password|lt|rct|token|login_token|registration_token)$/i.test(key) ? "[removed]" : sanitizedLogValue(child);
  }
  return safe;
}

async function refreshLogs() {
  if (logsLoading) return;
  logsLoading = true;
  try {
    const consoleElement = $("#account-logs");
    const previousScroll = consoleElement.scrollTop;
    const [messages, direct] = await Promise.all([api(`/messages?limit=80&_=${Date.now()}`, { cache: "no-store" }), api(`/direct?_=${Date.now()}`, { cache: "no-store" })]);
    latestDirect = direct;
    $("#log-live").className = direct.connected ? "log-live" : "log-live offline";
    $("#log-live-label").textContent = direct.connected ? "Live" : "Offline";
    accountLogText = messages.slice().reverse().map((message) => {
      const time = new Date(message.observed_at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
      const direction = { client_to_server: "OUT", server_to_client: "IN ", injected: "SEND" }[message.direction] || message.direction;
      const payload = JSON.stringify(sanitizedLogValue(message.payload));
      return `${time}  ${direction}  ${(message.command || "system").padEnd(8)}  ${payload.slice(0, 1200)}`;
    }).join("\n");
    const scan = direct.scan_total ? ` · scan ${direct.scan_sent + direct.scan_cached}/${direct.scan_total} (${direct.scan_cached} cached)` : "";
    const bot = direct.bot_state ? ` bot=${direct.bot_state}${direct.bot_detail ? ` (${direct.bot_detail})` : ""}` : "";
    const liveStatus = `${new Date().toLocaleTimeString()}  ${direct.connected ? "LIVE " : "OFFLINE"}  status    phase=${direct.phase} connected=${direct.connected}${scan}${bot}`;
    accountLogText = accountLogText ? `${accountLogText}\n${liveStatus}` : liveStatus;
    consoleElement.textContent = accountLogText;
    consoleElement.scrollTop = previousScroll;
    const newest = messages[0]?.sequence ? ` · latest #${messages[0].sequence}` : "";
    $("#log-result").textContent = `${direct.connected ? "Live update" : "Stored history"} ${new Date().toLocaleTimeString()} · ${messages.length} events${newest}`;
  } catch (error) {
    accountLogText = `Could not load logs: ${error.message}`;
    $("#account-logs").textContent = accountLogText;
    $("#log-live").className = "log-live error";
    $("#log-live-label").textContent = "Unavailable";
    $("#log-result").textContent = `Refresh failed at ${new Date().toLocaleTimeString()}`;
  } finally {
    logsLoading = false;
  }
}

function updateScanEstimate() {
  const radius = Math.max(0, Math.min(500, Math.floor(Number($("#scan-radius").value) || 0)));
  const side = radius === 0 ? 0 : Math.ceil((radius * 2 + 1) / 13);
  const areas = radius === 0 ? 6 : side * side;
  const minimumMinutes = Math.ceil(areas * 0.7 / 60);
  const maximumMinutes = Math.ceil(areas * 1.3 / 60);
  const grid = radius === 0 ? "the observed 3×2 viewport" : `a ${side}×${side} grid`;
  $("#scan-estimate").textContent = `Radius ${radius} uses ${grid} (${areas} paced gaa requests) · roughly ${minimumMinutes}–${maximumMinutes} min if none are cached.`;
}

function updateProgress(currentPhase) {
  const order = ["socket_handshake", "authenticating", "loading_castle", "loading_sands", "sands_ready"];
  const position = order.indexOf(currentPhase);
  const completed = currentPhase === "sands_ready" ? 3 : position >= 3 ? 2 : position >= 1 ? 1 : 0;
  document.querySelectorAll(".progress-list > div").forEach((item, index) => { item.classList.toggle("done", index < completed || currentPhase === "sands_ready"); item.classList.toggle("current", index === completed && currentPhase !== "sands_ready" && currentPhase !== "disconnected"); });
}

async function refresh() {
  try {
    if (!currentLicence?.active && !(await refreshLicence())) return;
    const [health, direct] = await Promise.all([api("/health"), api("/direct")]);
    latestDirect = direct;
    if (!health.licence_active) { currentLicence = null; await refreshLicence(); return; }
    const ready = direct.phase === "sands_ready";
    $("#status").textContent = ready ? "Account ready" : direct.connected ? "Setting up" : "OpenAuto ready";
    $("#status").className = ready ? "status connected" : "status";
    $("#gateway-label").textContent = direct.connected ? "Connected to US1" : "OpenAuto is ready";
    $("#phase").textContent = phaseCopy[direct.phase] || "Getting ready…";
    updateProgress(direct.phase);
    if (direct.error) $("#direct-result").textContent = direct.error;
    if (ready) { $("#initialize").disabled = false; $("#initialize").textContent = "Initialize account"; $("#direct-result").textContent = "Account connected and ready."; }
    const connectionKey = `${direct.connected}:${direct.account_id || ""}:${ready}`;
    if (connectionKey !== renderedConnectionKey) await refreshAccounts(direct);
    if (!$("#view-dashboard").hidden) await refreshDashboard(direct);
  } catch (_) { $("#status").textContent = "OpenAuto unavailable"; $("#status").className = "status error"; $("#gateway-label").textContent = "Reconnecting…"; }
}

document.querySelectorAll("nav [data-view]").forEach((button) => button.addEventListener("click", async () => {
  switchView(button.dataset.view);
  if (button.dataset.view === "dashboard") await refreshDashboard();
  if (button.dataset.view === "accounts") await refreshAccounts();
  if (button.dataset.view === "logs") await refreshLogs();
}));
$("#wave-tabs").addEventListener("click", (event) => { const button = event.target.closest("[data-wave]"); if (!button) return; activeWave = Number(button.dataset.wave); document.querySelectorAll("#wave-tabs button").forEach((value) => value.classList.toggle("selected", value === button)); renderWave(); });
$("#priority-buttons").addEventListener("click", (event) => { const button = event.target.closest("[data-priority]"); if (!button) return; priority = button.dataset.priority; document.querySelectorAll("#priority-buttons button").forEach((value) => value.classList.toggle("selected", value === button)); });
$("#source-kid").addEventListener("change", updateTargetKinds);
$("#use-main-castle").addEventListener("change", updateSourceCoordinates);
$("#target-kind").addEventListener("change", updateTargetKinds);
$("#item-search").addEventListener("input", renderPicker);
$("#item-search").addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  if (!pickerSelection) $("#item-results .picker-item")?.click();
  confirmPickerItem();
});
$("#item-amount").addEventListener("keydown", (event) => { if (event.key === "Enter") { event.preventDefault(); confirmPickerItem(); } });
$("#confirm-item").addEventListener("click", confirmPickerItem);
$("#clear-item").addEventListener("click", clearPickerItem);
$("#close-picker").addEventListener("click", () => $("#item-picker").close());

$("#attack-form").addEventListener("submit", async (event) => { event.preventDefault(); try { await api("/plans/attacks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(attackDraft()) }); $("#attack-result").textContent = "Attack saved."; $("#attack-name").value = ""; attackWaves = Array.from({ length: 4 }, blankWave); await loadLibrary(); } catch (error) { $("#attack-result").textContent = error.message; } });
$("#copy-attack").addEventListener("click", () => copyJson(attackDraft(), $("#attack-result")));
$("#clear-attack").addEventListener("click", () => { attackWaves = Array.from({ length: 4 }, blankWave); activeWave = 0; $("#attack-name").value = ""; document.querySelectorAll("#wave-tabs button").forEach((button) => button.classList.toggle("selected", button.dataset.wave === "0")); $("#attack-result").textContent = "Attack cleared."; renderWave(); });

$("#task-form").addEventListener("submit", async (event) => { event.preventDefault(); const kingdomId = Number($("#source-kid").value); const automatic = $("#use-main-castle").checked; const fortress = $("#target-kind").value === "fortress"; const destination = fortress ? { kind: "fortress", kingdom_id: kingdomId } : { kind: "rbc_level_range", kingdom_id: kingdomId, minimum: Number($("#target-level-min").value), maximum: Number($("#target-level-max").value) }; const draft = { name: $("#task-name").value.trim(), attack_profile_id: $("#task-attack").value, source: { kingdom_id: kingdomId, x: Number($("#source-x").value), y: Number($("#source-y").value) }, source_kind: automatic ? "main_castle" : "coordinate", destination, algorithm: $("#target-algorithm").value, travel: "coin", priority, commander_count: 1 }; try { await api("/plans/tasks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(draft) }); $("#task-result").textContent = "Dynamic farming task saved."; $("#task-name").value = ""; await loadLibrary(); } catch (error) { $("#task-result").textContent = error.message; } });

$("#mode-tasks").addEventListener("dragover", (event) => event.preventDefault());
$("#mode-tasks").addEventListener("drop", (event) => { event.preventDefault(); const id = event.dataTransfer.getData("text/plain") || event.dataTransfer.getData("text"); if (!id) return; const from = selectedModeTasks.findIndex((value) => value.task_id === id); if (from < 0) addModeTask(id); else { const [entry] = selectedModeTasks.splice(from, 1); const rows = [...$("#mode-tasks").querySelectorAll(".allocation-card")]; const target = rows.find((row) => event.clientX < row.getBoundingClientRect().left + row.offsetWidth / 2); const index = target ? rows.indexOf(target) : rows.length; selectedModeTasks.splice(index, 0, entry); renderAvailableTasks(); } });
$("#mode-form").addEventListener("submit", async (event) => { event.preventDefault(); try { await api("/plans/modes", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name: $("#mode-name").value.trim(), allocations: selectedModeTasks }) }); $("#mode-result").textContent = "Mode saved."; selectedModeTasks = []; $("#mode-name").value = ""; await loadLibrary(); await refreshAccounts(); } catch (error) { $("#mode-result").textContent = error.message; } });
$("#copy-mode").addEventListener("click", () => copyJson(bundleForMode($("#mode-name").value.trim(), selectedModeTasks), $("#mode-result")));
$("#load-example").addEventListener("click", async () => { $("#mode-json").value = JSON.stringify(await api("/plans/example"), null, 2); });
$("#import-mode").addEventListener("click", async () => { try { const bundle = JSON.parse($("#mode-json").value); await api("/plans/import", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(bundle) }); $("#import-result").textContent = "Mode imported with new attack and task IDs."; await loadLibrary(); } catch (error) { $("#import-result").textContent = error.message; } });

$("#recruitment-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const draft = { name: $("#recruitment-name").value.trim(), troop_id: Number($("#recruitment-troop").value), quantity: Number($("#recruitment-quantity").value), slot_count: Number($("#recruitment-slots").value), ask_alliance_help: $("#recruitment-help").checked };
  try { await api("/plans/recruitments", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(draft) }); $("#recruitment-result").textContent = "Recruitment saved."; $("#recruitment-name").value = ""; await loadLibrary(); }
  catch (error) { $("#recruitment-result").textContent = error.message; }
});

$("#recruit-bot-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const castles = [...document.querySelectorAll("#recruit-castles .castle-subscription")].filter((row) => row.querySelector("input").checked).map((row, position) => ({ account_id: row.querySelector("input").dataset.accountId, castle_id: Number(row.querySelector("input").dataset.castleId), recruitment_id: row.querySelector("select").value, position }));
  const draft = { name: $("#recruit-bot-name").value.trim(), algorithm: $("#recruit-algorithm").value, castles };
  try { await api("/plans/recruit-bots", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(draft) }); $("#recruit-bot-result").textContent = "Recruit bot saved."; $("#recruit-bot-name").value = ""; await loadLibrary(); }
  catch (error) { $("#recruit-bot-result").textContent = error.message; }
});

$("#licence-form").addEventListener("submit", async (event) => { event.preventDefault(); $("#licence-result").textContent = "Checking your token…"; try { const licence = await api("/licence", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ token: $("#licence-token").value.trim() }) }); $("#licence-token").value = ""; showApplication(licence); await loadLibrary(); await refreshAccounts(); await refresh(); } catch (error) { $("#licence-result").textContent = error.message; } });
$("#add-credits").addEventListener("click", () => showActivation(true));
$("#close-activation").addEventListener("click", () => { if (currentLicence?.active) activation.hidden = true; });
$("#toggle-password").addEventListener("click", (event) => { const password = $("#password"); const showing = password.type === "text"; password.type = showing ? "password" : "text"; event.currentTarget.textContent = showing ? "Show" : "Hide"; });
$("#direct-form").addEventListener("submit", async (event) => { event.preventDefault(); const button = $("#initialize"); const password = $("#password"); button.disabled = true; button.textContent = "Initializing…"; $("#direct-result").textContent = "OpenAuto is signing in, discovering the account, and preparing its initial map data."; try { await api("/accounts", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ server: $("#server").value, username: $("#player-name").value.trim(), password: password.value, scan_radius: Number($("#scan-radius").value) || 0, reuse_existing_map: false }) }); password.value = ""; await refresh(); } catch (error) { password.value = ""; button.disabled = false; button.textContent = "Initialize account"; $("#direct-result").textContent = error.message; } });
$("#refresh-accounts").addEventListener("click", refreshAccounts);
$("#scan-radius").addEventListener("input", updateScanEstimate);
$("#dashboard-refresh").addEventListener("click", () => refreshDashboard(null, true));
$("#refresh-logs").addEventListener("click", refreshLogs);
$("#copy-logs").addEventListener("click", async () => { if (!accountLogText) await refreshLogs(); await navigator.clipboard.writeText(accountLogText); $("#log-result").textContent = "Sanitized logs copied."; });

renderKingdoms();
renderWave();
updateScanEstimate();
refreshLicence().then(async (active) => {
  if (!active) return;
  try {
    await loadLibrary();
    await refreshAccounts();
    await refresh();
  } catch (error) {
    $("#attack-result").textContent = `Could not load the attack catalog: ${error.message}`;
  }
}).catch(() => showActivation(false));
setInterval(refresh, 2000);
setInterval(() => { if (!$("#view-logs").hidden && latestDirect.connected) refreshLogs(); }, 1500);
