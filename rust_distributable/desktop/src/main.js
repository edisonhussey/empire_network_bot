import { api } from "./api.js";
import { $, element } from "./dom.js";
import { compactNumber, expiryLabel, humanWait, humanize, relativeTime, sessionDuration, tempoLabel } from "./format.js";
import { createHeaderControls, modeUsesFortress } from "./views/header-controls.js";
import { renderRubyChart } from "./views/ruby-chart.js";
import { renderSparkBars } from "./views/spark-bars.js";
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
let dashboardAccountId = null;
let scanHealthFetchedAt = 0;
let scanHealthRefresh = null;
let addingAccount = false;
const scanDrafts = new Map();
const fallbackKingdoms = [
  { id: 0, name: "green_kingdom" }, { id: 1, name: "sand_kingdom" },
  { id: 2, name: "ice_kingdom" }, { id: 3, name: "fire_kingdom" },
  { id: 4, name: "storm_kingdom" }, { id: 10, name: "berimond_kingdom" },
];

/// The worlds OpenAuto can sign in to. `value` is what `/accounts` expects, and
/// `match` finds the world inside a stored endpoint - the endpoint is the world,
/// so an account's server is read back from it rather than remembered separately.
const SERVERS = [
  { value: "US1", label: "US1", match: "us1-game" },
  { value: "WORLD2", label: "World 2", match: "world2-game" },
];

function serverFor(endpoint) {
  return SERVERS.find((server) => String(endpoint || "").includes(server.match))?.value || SERVERS[0].value;
}

function serverLabel(endpoint) {
  return SERVERS.find((server) => String(endpoint || "").includes(server.match))?.label || "Unknown world";
}

function renderServerOptions() {
  const select = $("#server");
  const previous = select.value;
  select.replaceChildren(...SERVERS.map((server) => { const option = element("option", "", server.label); option.value = server.value; return option; }));
  if (SERVERS.some((server) => server.value === previous)) select.value = previous;
}

/// Adding an account is a pro-tier action, gated on the same entitlement that
/// covers account work in the first place.
function canAddAccount() {
  return Boolean(currentLicence?.active && currentLicence.tier === "pro" && (currentLicence.features || []).includes("account_initialize"));
}
const blankSide = () => ({ troops: [], tools: [] });
const blankWave = () => ({ left: blankSide(), middle: blankSide(), right: blankSide() });
let attackWaves = Array.from({ length: 4 }, blankWave);

const phaseCopy = {
  disconnected: "Ready to connect", socket_handshake: "Opening a secure connection…",
  awaiting_room: "Contacting your world…", awaiting_version: "Preparing your session…",
  authenticating: "Signing in…", authenticated: "Sign-in complete",
  loading_account: "Discovering your account…", loading_castle: "Finding your castles…",
  loading_sands: "Scanning permanent kingdoms…", discovering_fortresses: "Finding every fortress…", sands_ready: "Setup complete", failed: "Needs attention",
};

function showActivation(canClose = false) {
  activation.hidden = false;
  $("#close-activation").hidden = !canClose;
}

function showApplication(licence) {
  currentLicence = licence;
  activation.hidden = true;
  appShell.hidden = false;
  $("#licence-expiry").textContent = expiryLabel(licence.expires_at);
  $("#plan-badge").hidden = licence.tier !== "pro";
  if (licence.stage === "unactivated") {
    switchView("initialize");
    $("#direct-result").textContent = "Access accepted. Initialize the matching account to activate this installation.";
  }
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

const viewTitles = {
  dashboard: ["Overview", "Dashboard"],
  attacks: ["Attack", "Create attack"],
  tasks: ["Attack", "Create task"],
  modes: ["Attack", "Create attack bot"],
  recruitments: ["Recruit", "Create recruitment"],
  "recruit-bots": ["Recruit", "Create recruit bot"],
  accounts: ["Workspace", "Control Panel"],
  initialize: ["Workspace", "Initialize"],
  presets: ["Library", "Manage presets"],
  logs: ["Diagnostics", "Logs"],
  support: ["Help", "Support"],
};

function switchView(name) {
  document.querySelectorAll(".view").forEach((view) => { view.hidden = view.id !== `view-${name}`; });
  document.querySelectorAll("nav [data-view]").forEach((button) => button.classList.toggle("active", button.dataset.view === name));
  // Reveal the group that owns the page being shown. The others stay as the user
  // left them, so a group they opened on purpose does not snap shut.
  document.querySelectorAll(".nav-group").forEach((group) => {
    if (group.querySelector(`[data-view="${name}"]`)) setNavGroup(group, true);
  });
  [$("#page-eyebrow").textContent, $("#page-title").textContent] = viewTitles[name] || ["Overview", "Dashboard"];
}

function setNavGroup(group, open) {
  group.classList.toggle("open", open);
  group.querySelector(".nav-parent").setAttribute("aria-expanded", String(open));
  group.querySelector(".nav-sub").hidden = !open;
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

/// Kingdom display name from an id, so activity rows can say "Burning Sands"
/// rather than "K1".
function kingdomLabel(kingdomId) {
  if (kingdomId === null || kingdomId === undefined) return "Current kingdom";
  const found = library.kingdoms.find((value) => value.id === kingdomId) || fallbackKingdoms.find((value) => value.id === kingdomId);
  return kingdomName(found?.name || `Kingdom ${kingdomId}`);
}

/// A bot is running when the service holds a live subscription for it, not
/// merely when a socket is open. The running flags live in the plan tables, so
/// this reads the same source the runner does.
function botIsRunning(direct) {
  if (!direct.connected) return false;
  return library.account_modes.some((value) => value.running) || library.account_recruit_bots.some((value) => value.running);
}

/// The two indicators in the sidebar footer: socket state with how long it has
/// been up, and whether automation is actually driving it.
function renderLinkState(direct) {
  const connected = Boolean(direct.connected);
  $("#link-label").closest(".link-row").classList.toggle("on", connected);
  $("#link-label").textContent = connected ? "Connected" : "Not connected";
  $("#link-since").textContent = connected ? sessionDuration(direct.connected_at_ms) : "";
  const running = botIsRunning(direct);
  $("#bot-label").closest(".link-row").classList.toggle("on", running);
  $("#bot-label").textContent = running ? "Bot running" : "Bot stopped";
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
  const berimond = $("#target-kind option[value='berimond']");
  
  const fortressSupported = [1, 2, 3].includes(kingdomId);
  fortress.hidden = !fortressSupported;
  fortress.disabled = !fortressSupported;
  if (!fortressSupported && $("#target-kind").value === "fortress") $("#target-kind").value = "rbc";
  
  const berimondSupported = kingdomId === 10;
  berimond.hidden = !berimondSupported;
  berimond.disabled = !berimondSupported;
  if (!berimondSupported && $("#target-kind").value === "berimond") $("#target-kind").value = "rbc";
  const isFortress = $("#target-kind").value === "fortress";
  const isBerimond = $("#target-kind").value === "berimond";
  const hasLevels = !(isFortress || isBerimond);
  $("#target-level-fields").hidden = !hasLevels;
  $("#target-level-min").required = hasLevels;
  $("#target-level-max").required = hasLevels;
  
  let hint = "OpenAuto chooses a learned Robber Baron in this level range; no destination coordinate is required.";
  if (isFortress) hint = "OpenAuto chooses a learned fortress in this kingdom.";
  if (isBerimond) hint = "OpenAuto chooses a valid Berimond camp. (Kingdom 10 only)";
  $("#target-hint").textContent = hint;
  updateSourceCoordinates();
}

/// The account the operator is driving.
///
/// `/accounts` is alphabetical, so "the first account" is not "the connected
/// account". Reading the wrong one showed another account's scan coverage and
/// castle coordinates, which looked exactly like scanning had stopped working.
function activeAccount() {
  const id = (latestDirect?.account_id || connectedAccounts[0]?.account_id || "").toLowerCase();
  if (!id) return null;
  return connectedAccounts.find((account) => account.account_id.toLowerCase() === id) || connectedAccounts[0] || null;
}

function updateSourceCoordinates() {
  const automatic = $("#use-main-castle").checked;
  const kingdomId = Number($("#source-kid").value);
  const health = activeAccount()?.kingdom_health?.find((value) => value.kingdom_id === kingdomId);
  for (const input of [$("#source-x"), $("#source-y")]) input.disabled = automatic;
  $(".source-coordinates").hidden = automatic;
  if (automatic && health) {
    $("#source-x").value = health.x;
    $("#source-y").value = health.y;
  }
}

function slotButton(sideName, kind, index) {
  const slot = attackWaves[activeWave][sideName][kind][index];
  const button = element("button", `slot ${kind === "troops" ? "troop-slot" : "tool-slot"}`);
  button.type = "button";
  button.append(element("small", "", kind === "troops" ? `Troop ${index + 1}` : `Tool ${index + 1}`));
  button.append(element("b", "", slot ? itemName(slot.item_id) : "Empty"));
  if (slot) button.append(element("span", "", `× ${slot.amount}`));
  button.addEventListener("click", () => openPicker(sideName, kind, index));
  return button;
}

function renderWave() {
  const builder = $("#wave-builder");
  builder.replaceChildren();

  const actions = element("div", "wave-actions");
  actions.style.gridColumn = "1 / -1";
  actions.style.display = "flex";
  actions.style.justifyContent = "flex-end";
  actions.style.marginBottom = "-20px";
  
  if (activeWave > 0) {
     const copyBtn = element("button", "secondary-action");
     copyBtn.type = "button";
     copyBtn.textContent = `Copy Wave 1 to Wave ${activeWave + 1}`;
     copyBtn.addEventListener("click", () => {
         attackWaves[activeWave] = JSON.parse(JSON.stringify(attackWaves[0]));
         renderWave();
     });
     actions.append(copyBtn);
  } else {
     const copyAllBtn = element("button", "secondary-action");
     copyAllBtn.type = "button";
     copyAllBtn.textContent = "Copy to all waves";
     copyAllBtn.addEventListener("click", () => {
         for (let i = 1; i < 4; i++) {
             attackWaves[i] = JSON.parse(JSON.stringify(attackWaves[0]));
         }
         alert("Copied Wave 1 to all subsequent waves.");
         renderWave();
     });
     actions.append(copyAllBtn);
  }
  builder.append(actions);

  const SLOTS = { left: { troops: 2, tools: 2 }, middle: { troops: 6, tools: 3 }, right: { troops: 2, tools: 2 } };

  for (const [key, name] of [["left", "Left flank"], ["middle", "Center"], ["right", "Right flank"]]) {
    const side = element("section", "side-builder");
    side.append(element("h3", "", name));
    const slots = element("div", "slot-grid");
    
    for (let i = 0; i < SLOTS[key].troops; i++) {
      slots.append(slotButton(key, "troops", i));
    }
    for (let i = 0; i < SLOTS[key].tools; i++) {
      slots.append(slotButton(key, "tools", i));
    }
    
    side.append(slots);
    builder.append(side);
  }
}

function openPicker(side, kind, index) {
  pickerTarget = { side, kind, index };
  const current = attackWaves[activeWave][side][kind][index];
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
  attackWaves[activeWave][pickerTarget.side][pickerTarget.kind][pickerTarget.index] = { item_id: pickerSelection.id, amount };
  $("#item-picker").close();
  renderWave();
}

function clearPickerItem() {
  if (!pickerTarget) return;
  attackWaves[activeWave][pickerTarget.side][pickerTarget.kind][pickerTarget.index] = null;
  $("#item-picker").close();
  renderWave();
}

function attackDraft() {
  const waves = attackWaves.map(wave => {
    return {
       left: { troops: wave.left.troops.filter(Boolean), tools: wave.left.tools.filter(Boolean) },
       middle: { troops: wave.middle.troops.filter(Boolean), tools: wave.middle.tools.filter(Boolean) },
       right: { troops: wave.right.troops.filter(Boolean), tools: wave.right.tools.filter(Boolean) }
    };
  });
  return { name: $("#attack-name").value.trim(), waves };
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
  const sourceKind = runtime.source_kind || "coordinate";
  return {
    name: task.name,
    attack_profile_id: task.profile_id,
    source: { kingdom_id: runtime.source_kingdom_id, x: sourceKind === "main_castle" ? 0 : runtime.source_x, y: sourceKind === "main_castle" ? 0 : runtime.source_y },
    source_kind: sourceKind,
    destination: targetKind === "fortress" ? {
      kind: "fortress", kingdom_id: task.kingdom_id,
    } : task.target_level_min !== null ? {
      kind: targetKind === "fortress" ? "fortress_level_range" : "rbc_level_range", kingdom_id: task.kingdom_id,
      minimum: task.target_level_min, maximum: task.target_level_max,
    } : {
      kind: "coordinate", kingdom_id: runtime.target_kingdom_id, x: runtime.target_x, y: runtime.target_y,
    },
    algorithm: subscription?.filter?.algorithm || "advanced",
    travel: runtime.travel_mode || "coin", priority: priorityName(task.priority), commander_count: 1,
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

// Deletes report into the result line of the library they live in. These calls
// used to be bare fetches whose response was dropped, so a delete the server
// refused - a recruitment a recruit bot still uses, for instance - looked like a
// button that simply did nothing.
async function deleteEntry(path, resultSelector) {
  const result = $(resultSelector);
  try {
    await api(path, { method: "DELETE" });
    result.textContent = "Deleted.";
    await loadLibrary();
  } catch (error) {
    result.textContent = error.message;
  }
}

function renderLibrary() {
  const attackList = $("#attack-library");
  attackList.replaceChildren(...library.attacks.map((attack) => {
    const row = element("article", "library-row");
    const copy = actionButton("Copy JSON", () => copyJson(profileToDraft(attack), $("#attack-result")));
    const remove = actionButton("Delete", () => deleteEntry(`/plans/attacks/${attack.profile_id}`, "#attack-result"), true);
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
    const remove = actionButton("Delete", () => deleteEntry(`/plans/tasks/${task.task_id}`, "#task-result"), true);
    const subscription = library.subscriptions.find((value) => value.task_id === task.task_id);
    const targetLabel = subscription?.target_kind === "fortress" ? "Fortress" : (subscription?.target_kind === "berimond_camp" ? "Berimond Camp" : "Robber Baron");
    const levelLabel = (subscription?.target_kind === "fortress" || subscription?.target_kind === "berimond_camp") ? "" : (task.target_level_min === null ? "fixed target" : `levels ${task.target_level_min}–${task.target_level_max}`);
    const algorithm = subscription?.filter?.algorithm || "advanced";
    const identity = element("div"); identity.append(element("b", "", task.name), element("small", "", `${kingdomName((library.kingdoms.find((value) => value.id === task.kingdom_id) || fallbackKingdoms.find((value) => value.id === task.kingdom_id))?.name || `Kingdom ${task.kingdom_id}`)} · ${targetLabel} ${levelLabel} · ${humanize(algorithm)} · ${priorityName(task.priority).replace("_", " ")}`));
    const actions = element("span", "row-actions"); actions.append(copy, remove); row.append(identity, actions); return row;
  }));
  if (!library.tasks.length) taskList.append(element("p", "empty-copy", "Create an attack first, then add a task."));
  renderAvailableTasks();
  renderModes();
  renderPresets();
}

function portableTask(task) {
  const profile = library.attacks.find((value) => value.profile_id === task.profile_id);
  return { schema: 2, kind: "task", attack: profileToDraft(profile), task: taskToDraft(task) };
}

function portableRecruitBot(bot) {
  const recruitmentIds = new Set(bot.castles.map((value) => value.recruitment_id));
  const recruitments = library.recruitments.filter((value) => recruitmentIds.has(value.recruitment_id));
  const subscriptions = bot.castles.map((value) => {
    const castle = library.castles.find((item) => item.account_id.toLowerCase() === value.account_id.toLowerCase() && item.castle_id === value.castle_id);
    const peers = library.castles.filter((item) => item.account_id.toLowerCase() === value.account_id.toLowerCase() && item.kingdom_id === castle?.kingdom_id && item.area_type === castle?.area_type).sort((a, b) => a.castle_id - b.castle_id);
    return { kingdom_id: castle?.kingdom_id, castle_role: castle?.area_type === 4 ? "outpost" : castle?.kingdom_id === 0 ? "main_castle" : "kingdom_castle", occurrence: Math.max(0, peers.findIndex((item) => item.castle_id === castle?.castle_id)), recruitment: library.recruitments.find((item) => item.recruitment_id === value.recruitment_id)?.name };
  });
  return { schema: 2, kind: "recruit_bot", recruit_bot: { name: bot.name, algorithm: bot.algorithm, recruitments: recruitments.map((value) => ({ name: value.name, troop_id: value.troop_id, quantity: value.quantity, slot_count: value.slot_count, ask_alliance_help: value.ask_alliance_help })), subscriptions } };
}

function presetRow(name, detail, copied, deletePath) {
  const row = element("article", "library-row");
  const identity = element("div"); identity.append(element("b", "", name), element("small", "", detail));
  const actions = element("span", "row-actions");
  actions.append(actionButton("Copy config", () => copyJson(copied, $("#preset-result"))));
  if (deletePath) actions.append(actionButton("Delete", () => deleteEntry(deletePath, "#preset-result"), true));
  row.append(identity, actions); return row;
}

function presetGroup(title, rows) {
  const section = element("section", "card preset-group");
  const head = element("div", "card-title"); const copy = element("div");
  copy.append(element("p", "eyebrow", "Reusable component"), element("h2", "", title)); head.append(copy); section.append(head);
  const list = element("div", "library-list");
  list.append(...(rows.length ? rows : [element("p", "empty-copy", `No ${title.toLowerCase()} saved yet.`)])); section.append(list); return section;
}

function renderPresets() {
  const container = $("#preset-groups");
  if (!container) return;
  const attacks = library.attacks.map((value) => presetRow(value.name, "Attack structure", { schema: 2, kind: "attack", attack: profileToDraft(value) }, `/plans/attacks/${value.profile_id}`));
  const tasks = library.tasks.map((value) => presetRow(value.name, `${kingdomLabel(value.kingdom_id)} task`, portableTask(value), `/plans/tasks/${value.task_id}`));
  const modes = library.modes.map((value) => { const allocations = value.allocations?.length ? value.allocations : value.task_ids.map((task_id) => ({ task_id, commander_count: 1 })); return presetRow(value.name, `${value.commander_count} commanders · includes attacks and tasks`, bundleForMode(value.name, allocations), `/plans/modes/${value.mode_id}`); });
  const recruitments = library.recruitments.map((value) => presetRow(value.name, `${itemName(value.troop_id)} · ${value.quantity} × ${value.slot_count}`, { schema: 2, kind: "recruitment", recruitment: { name: value.name, troop_id: value.troop_id, quantity: value.quantity, slot_count: value.slot_count, ask_alliance_help: value.ask_alliance_help } }, `/plans/recruitments/${value.recruitment_id}`));
  const recruitBots = library.recruit_bots.map((value) => presetRow(value.name, `${value.castles.length} logical castle subscriptions`, portableRecruitBot(value), `/plans/recruit-bots/${value.recruit_bot_id}`));
  const combinations = library.account_modes.map((assignment) => {
    const mode = library.modes.find((value) => value.mode_id === assignment.mode_id);
    if (!mode) return null;
    const allocations = mode.allocations?.length ? mode.allocations : mode.task_ids.map((task_id) => ({ task_id, commander_count: 1 }));
    const recruitAssignment = library.account_recruit_bots.find((value) => value.account_id.toLowerCase() === assignment.account_id.toLowerCase());
    const recruitBot = recruitAssignment && library.recruit_bots.find((value) => value.recruit_bot_id === recruitAssignment.recruit_bot_id);
    const config = { schema: 2, kind: "complete_bot", name: `${mode.name}${recruitBot ? ` + ${recruitBot.name}` : ""}`, attack_bot: bundleForMode(mode.name, allocations), recruit_bot: recruitBot ? portableRecruitBot(recruitBot).recruit_bot : null };
    return presetRow(config.name, recruitBot ? "Attack and recruitment configuration" : "Complete attack configuration", config, null);
  }).filter(Boolean);
  container.replaceChildren(presetGroup("Complete bots", combinations), presetGroup("Attacks", attacks), presetGroup("Tasks", tasks), presetGroup("Attack bots", modes), presetGroup("Recruitments", recruitments), presetGroup("Recruit bots", recruitBots));
}

async function importPreset(config) {
  if (config.schema === 1 && Array.isArray(config.tasks)) return api("/plans/import", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(config) });
  if (config.schema !== 2) throw new Error("Unsupported config schema.");
  if (config.kind === "complete_bot") {
    const mode = await api("/plans/import", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(config.attack_bot) });
    const recruit = config.recruit_bot ? await importPreset({ schema: 2, kind: "recruit_bot", recruit_bot: config.recruit_bot }) : null;
    return { mode_id: mode.mode_id, recruit_bot_id: recruit?.id || null };
  }
  if (config.kind === "attack") return api("/plans/attacks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(config.attack) });
  if (config.kind === "recruitment") return api("/plans/recruitments", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(config.recruitment) });
  if (config.kind === "task") {
    const attack = await api("/plans/attacks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(config.attack) });
    return api("/plans/tasks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ ...config.task, attack_profile_id: attack.id }) });
  }
  if (config.kind === "recruit_bot") {
    const account = connectedAccounts[0];
    if (!account) throw new Error("Initialize an account before importing a recruit bot.");
    const ids = new Map();
    for (const recruitment of config.recruit_bot.recruitments || []) {
      const created = await api("/plans/recruitments", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(recruitment) }); ids.set(recruitment.name, created.id);
    }
    const castles = (config.recruit_bot.subscriptions || []).map((subscription, position) => {
      const candidates = library.castles.filter((castle) => castle.account_id.toLowerCase() === account.account_id.toLowerCase() && castle.kingdom_id === subscription.kingdom_id && (subscription.castle_role !== "outpost" || castle.area_type === 4)).sort((a, b) => a.castle_id - b.castle_id);
      const castle = candidates[subscription.occurrence || 0];
      if (!castle) throw new Error(`No matching ${kingdomLabel(subscription.kingdom_id)} castle exists on this account.`);
      return { account_id: account.account_id, castle_id: castle.castle_id, recruitment_id: ids.get(subscription.recruitment), position };
    });
    return api("/plans/recruit-bots", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name: config.recruit_bot.name, algorithm: config.recruit_bot.algorithm, castles }) });
  }
  throw new Error(`Unsupported preset kind: ${config.kind}`);
}

function renderAvailableTasks() {
  const container = $("#available-tasks");
  container.replaceChildren(...library.tasks.map((task) => poolTask(task)));
  if (!library.tasks.length) container.append(element("p", "empty-copy", "Save a task first, then place it here."));
  renderSelectedTasks();
}

/// Tasks waiting to be scheduled. Adding is a button, not a drag: the drop
/// gesture never worked reliably and a drag image told the user nothing.
function poolTask(task) {
  const row = element("div", "pool-task");
  const copy = element("span");
  copy.append(element("b", "", task.name), element("small", "", "Reusable task"));
  row.append(copy, actionButton("Add", () => addModeTask(task.task_id)));
  return row;
}

function moveModeTask(taskId, delta) {
  const from = selectedModeTasks.findIndex((entry) => entry.task_id === taskId);
  const to = Math.max(0, Math.min(selectedModeTasks.length - 1, from + delta));
  if (from < 0 || from === to) return;
  selectedModeTasks.splice(to, 0, selectedModeTasks.splice(from, 1)[0]);
  rebuildBoardSoon();
}

/// Rebuilding the board removes the very control that was clicked or edited, so
/// doing it inside the event leaves the browser dispatching to a detached node
/// (Chromium throws "node to be removed is no longer a child"). Waiting for the
/// next tick keeps the rebuild out of the event that asked for it.
function rebuildBoardSoon() {
  setTimeout(renderSelectedTasks, 0);
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
  rebuildBoardSoon();
}

function renderSelectedTasks() {
  const board = $("#mode-tasks");
  board.replaceChildren();
  if (!selectedModeTasks.length) {
    board.append(element("p", "board-empty", "Add a task to begin the schedule."));
    $("#commander-warning").textContent = "0 commanders allocated";
    return;
  }
  // One grid column per commander. A task block spans the columns it owns and
  // the numbered cells sit in the row below, so every block starts on the same
  // line and the numbers form one continuous ruler underneath.
  const total = selectedModeTasks.reduce((sum, entry) => sum + Math.max(1, Math.floor(entry.commander_count) || 1), 0);
  const grid = element("div", "board-grid");
  grid.style.gridTemplateColumns = `repeat(${total}, minmax(0, 1fr))`;
  let first = 1;
  selectedModeTasks.forEach((allocation) => {
    const task = library.tasks.find((value) => value.task_id === allocation.task_id);
    if (!task) return;
    const count = Math.max(1, Math.floor(allocation.commander_count) || 1);
    const last = first + count - 1;
    const block = element("article", "allocation-block");
    block.style.gridColumn = `${first} / span ${count}`;
    block.style.gridRow = "1";
    block.dataset.taskId = task.task_id;
    const head = element("div", "block-head");
    head.append(element("b", "", task.name), element("span", "block-count", String(count)));
    const number = document.createElement("input");
    number.type = "number"; number.min = "1"; number.value = String(count);
    number.setAttribute("aria-label", `${task.name} commander count`);
    number.addEventListener("change", () => setAllocation(task.task_id, number.value));
    const controls = element("div", "block-controls");
    controls.append(
      actionButton("−", () => setAllocation(task.task_id, count - 1)), number, actionButton("+", () => setAllocation(task.task_id, count + 1)),
      actionButton("←", () => moveModeTask(task.task_id, -1)), actionButton("→", () => moveModeTask(task.task_id, 1)),
      actionButton("Remove", () => { selectedModeTasks = selectedModeTasks.filter((entry) => entry.task_id !== task.task_id); rebuildBoardSoon(); }, true),
    );
    block.append(head, controls, element("em", "block-span", `Commanders ${first}–${last}`));
    const cells = [];
    for (let commander = first; commander <= last; commander += 1) {
      const cell = element("span", "ruler-cell", String(commander));
      cell.style.gridColumn = String(commander);
      cell.style.gridRow = "2";
      cells.push(cell);
      grid.append(cell);
    }
    // Hovering the block marks exactly the cells it holds.
    block.addEventListener("mouseenter", () => cells.forEach((cell) => cell.classList.add("covered")));
    block.addEventListener("mouseleave", () => cells.forEach((cell) => cell.classList.remove("covered")));
    grid.append(block);
    first = last + 1;
  });
  board.append(grid);
  $("#commander-warning").textContent = `${total} commander${total === 1 ? "" : "s"} allocated · numbered 1–${total}`;
}

function renderModes() {
  const container = $("#mode-library");
  container.replaceChildren(...library.modes.map((mode) => {
    const row = element("article", "library-row mode-row");
    const allocations = mode.allocations?.length ? mode.allocations : mode.task_ids.map((task_id) => ({ task_id, commander_count: 1 }));
    const identity = element("div"); identity.append(element("b", "", mode.name), element("small", "", `${allocations.length} task${allocations.length === 1 ? "" : "s"} · ${mode.commander_count} commander${mode.commander_count === 1 ? "" : "s"}`));
    const actions = element("span", "row-actions");
    actions.append(actionButton("Copy JSON", () => copyJson(bundleForMode(mode.name, allocations), $("#mode-result"))));
    actions.append(actionButton("Delete", () => deleteEntry(`/plans/modes/${mode.mode_id}`, "#mode-result"), true));
    row.append(identity, actions); return row;
  }));
  if (!library.modes.length) container.append(element("p", "empty-copy", "No attack bots saved yet."));
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
    const actions = element("span", "row-actions"); actions.append(actionButton("Delete", () => deleteEntry(`/plans/recruitments/${value.recruitment_id}`, "#recruitment-result"), true));
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
    const actions = element("span", "row-actions"); actions.append(actionButton("Delete", () => deleteEntry(`/plans/recruit-bots/${bot.recruit_bot_id}`, "#recruit-bot-result"), true));
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
  const empty = element("div", "empty-state"); empty.append(element("i", "", "+"), element("p", "", "No account yet. Add one in Initialize, then start it here.")); $("#accounts").replaceChildren(empty);
  renderAccountProfiles();
}

async function connectSavedAccount(account, password, radius, reuseExistingMap, output) {
  if (!password) { output.textContent = "Enter the account password first."; return; }
  output.textContent = reuseExistingMap ? "Establishing connection…" : "Connecting and extending the map scan…";
  await api("/accounts", {
    method: "POST",
    headers: { "content-type": "application/json" },
    // The account's own world, so reconnecting never lands on the wrong server.
    body: JSON.stringify({ server: serverFor(account.endpoint), username: account.player_name, password, scan_radius: radius, kingdom_scans: kingdomScans(radius), reuse_existing_map: reuseExistingMap }),
  });
  await refresh();
}

/// Everything OpenAuto knows about each stored account. This belongs next to the
/// place accounts are set up, not beside the daily start/stop controls.
function renderAccountProfiles() {
  const container = $("#account-profiles");
  container.replaceChildren();
  if (!connectedAccounts.length) {
    container.append(element("p", "empty-copy", "No account yet — sign in above."));
    return;
  }
  for (const account of connectedAccounts) {
    const row = element("article", "account-item profile-row");
    const avatar = element("div", "account-avatar", account.player_name.slice(0, 1).toUpperCase());
    const isConnected = latestDirect.connected && latestDirect.account_id?.toLowerCase() === account.account_id.toLowerCase();
    const identity = element("div");
    identity.append(
      element("strong", "", account.player_name),
      element("span", isConnected ? "connection-label live" : "connection-label", `${serverLabel(account.endpoint)} · ${isConnected ? "Connected" : "Stored locally"}`),
    );
    const facts = element("dl");
    for (const [name, value] of [["Castles", account.castle_count], ["Commanders", account.commander_count], ["Targets", account.rbc_count]]) {
      const group = element("div"); group.append(element("dd", "", value), element("dt", "", name)); facts.append(group);
    }
    row.append(avatar, identity, facts);
    container.append(row);
  }
}

function renderAddAccount() {
  const button = $("#add-account");
  button.textContent = addingAccount ? "Cancel" : "Add account";
  button.disabled = !addingAccount && !canAddAccount();
  button.title = canAddAccount() ? "Sign in to another world or account" : "Adding an account needs a pro plan with account access";
}

async function refreshAccounts(knownDirect = null) {
  connectedAccounts = await api("/accounts");
  const direct = knownDirect || await api("/direct");
  latestDirect = direct;
  // The setup form hides itself once an account exists, unless the user asked to
  // add another one.
  $("#account-onboarding").hidden = connectedAccounts.length > 0 && !addingAccount;
  renderAccountProfiles();
  renderAddAccount();
    const pendingAttack = new Map();
  const pendingRecruit = new Map();
  document.querySelectorAll("[data-account-attack]").forEach(sel => pendingAttack.set(sel.dataset.accountAttack, sel.value));
  document.querySelectorAll("[data-account-recruit]").forEach(sel => pendingRecruit.set(sel.dataset.accountRecruit, sel.value));
if (!connectedAccounts.length) { emptyAccounts(); renderModes(); return; }
  $("#accounts").replaceChildren(...connectedAccounts.map((account) => {
    const card = element("article", "account-steps");
    const isConnected = direct.connected && direct.account_id?.toLowerCase() === account.account_id.toLowerCase();
    const isReady = isConnected && direct.phase === "sands_ready";
    // Only the name, so a card is identifiable without repeating the profile
    // block that now lives in Initialize.
    card.append(element("p", "account-caption", account.player_name));
    // Step 1 — the socket. Kept separate from automation so it is obvious that
    // signing in and starting a bot are two different decisions.
    const connection = element("div", "account-connection-control");
    connection.append(element("p", "list-label", "Step 1 · Connection"));
    const password = document.createElement("input"); password.type = "password"; password.placeholder = "Password"; password.autocomplete = "current-password";
    const connectionResult = element("span", "connection-result", isConnected ? "Connected. Cached map data is being used." : "Not connected. Your castles and targets are still shown from the last visit.");
    const scanStatus = element("span", "control-scan-status");
    scanStatus.dataset.controlScanAccount = account.account_id.toLowerCase();
    scanStatus.hidden = true;
    const connectionState = element("span", isConnected ? "control-state live" : "control-state", isConnected ? "● Connected" : "● Disconnected");
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
    // Scanning has its own home in Initialize, next to the radius it uses, so
    // this step is only about opening and closing the socket.
    const scanRow = element("div", "connection-scan");
    scanRow.append(login, close, connectionState);
    connection.append(password, scanRow, connectionResult, scanStatus);
    // Only one socket exists, so signing in here ends whoever holds it.
    if (!isConnected && direct.connected) connection.append(element("p", "empty-copy", "Another account holds the connection. Starting this one replaces that session."));

    // Step 2 — automation. One attack bot, plus at most one recruit bot.
    const assignment = element("div", "account-mode-control");
    const current = library.account_modes.find((value) => value.account_id.toLowerCase() === account.account_id.toLowerCase());
    const currentRecruit = library.account_recruit_bots.find((value) => value.account_id.toLowerCase() === account.account_id.toLowerCase());
    const running = Boolean(current?.running && isConnected);
    assignment.append(element("p", "list-label", "Step 2 · Automation"));
    const attackLabel = document.createElement("label"); attackLabel.textContent = "Attack bot";
    const attackSelect = document.createElement("select");
    attackSelect.dataset.accountAttack = account.account_id;
    attackSelect.append(...library.modes.map((mode) => { const option = element("option", "", `${mode.name} · ${mode.commander_count} commander${mode.commander_count === 1 ? "" : "s"}`); option.value = mode.mode_id; return option; }));
    if (pendingAttack.has(account.account_id)) attackSelect.value = pendingAttack.get(account.account_id);
    else if (current) attackSelect.value = current.mode_id;
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
    recruitSelect.dataset.accountRecruit = account.account_id;
    if (pendingRecruit.has(account.account_id)) recruitSelect.value = pendingRecruit.get(account.account_id);
    else if (currentRecruit) recruitSelect.value = currentRecruit.recruit_bot_id;
    const recruitEstimate = element("p", "recruit-estimate");
    const describeRecruit = () => {
      const bot = library.recruit_bots.find((value) => String(value.recruit_bot_id) === recruitSelect.value);
      if (!bot) { recruitEstimate.textContent = "No recruitment will run."; return; }
      const soonest = soonestCastleClear(bot);
      recruitEstimate.textContent = `${bot.castles.length} castle${bot.castles.length === 1 ? "" : "s"} · ${tempoLabel(bot.algorithm)} · ${soonest ? `next free in ${humanWait(soonest)}` : "every castle free"}`;
    };
    recruitSelect.addEventListener("change", describeRecruit);
    describeRecruit();
    const summary = element("span", running ? "bot-state running" : "bot-state", running ? `● Running${currentRecruit?.running ? " with recruitment" : ""}` : "● Stopped");
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
    if (!library.modes.length) assignment.append(element("p", "empty-copy", "Create an attack bot before starting automation."));
    card.append(connection, assignment); return card;
  }));
  renderedConnectionKey = `${direct.connected}:${direct.account_id || ""}:${direct.phase === "sands_ready"}`;
  updateControlScanIndicators(direct);
  updateSourceCoordinates();
  renderModes();
  renderRecruitmentBuilder();
  renderScanPlan();
}

function dashboardStat(value, label) {
  const card = element("div", "health-stat");
  card.append(element("b", "", compactNumber(value)), element("span", "", label));
  return card;
}

/// When the next fortress frees up.
///
/// Fortresses are the only target the server hands us a cooldown for, so this
/// counts down to a real time rather than an estimate. While the map walk is
/// still running it says so, because "none yet" and "still looking" are
/// different things to be told.
function nextFortressSummary(summary) {
  const count = summary?.fortress_count || 0;
  const pending = summary?.fortress_probes_pending || 0;
  const soonest = summary?.fortress_next_available_at_ms ?? null;
  if (!count) {
    return pending
      ? { text: `Mapping · ${pending} areas left`, imminent: false, detail: "Still walking the map for fortresses." }
      : { text: "None found yet", imminent: false, detail: "No fortress has been observed for this account yet." };
  }
  if (soonest === null) return {
    text: "No active window",
    imminent: false,
    detail: `${count} fortresses known. Expired one-minute openings are hidden until fresh server state re-arms them.`,
  };
  const found = `${count} fortress${count === 1 ? "" : "es"} known`;
  const wait = soonest - Date.now();
  if (wait <= 0) return { text: "Ready now", imminent: true, detail: `${found}. The runner sends inside a one-minute window.` };
  if (wait <= 60000) return { text: `Due in ${humanWait(wait)}`, imminent: true, detail: `${found}. The window opens within the minute.` };
  return { text: `in ${humanWait(wait)}`, imminent: false, detail: found };
}

/// What the runner is doing right now, in words.
///
/// The keys are the automation phases themselves, so this reports the runner
/// rather than guessing at it, and the detail line it already publishes becomes
/// the tooltip. Watching a kingdom transition or a fortress cooldown read go by
/// is the point: the state machine is otherwise invisible.
const botActivityCopy = {
  waiting: "Idle — waiting for a free commander",
  opening_attack_map: "Opening the attack map",
  discovering_fortresses: "Mapping fortress coordinates",
  refreshing_fortress: "Reading a fortress cooldown",
  pacing_inspection: "Pacing the target inspection",
  inspecting_target: "Inspecting a target",
  pacing_attack: "Pacing the attack",
  awaiting_attack_ack: "Waiting for the attack to be accepted",
  recruiting: "Recruiting",
  stopped: "Stopped",
};

function botActivity(direct) {
  if (!direct.connected) return { text: "—", detail: "No session is open." };
  const state = direct.bot_state || "stopped";
  return {
    text: botActivityCopy[state] || humanize(state),
    detail: direct.bot_detail || "The runner has not reported a detail yet.",
  };
}

async function refreshDashboard(knownDirect = null, force = false) {
  try {
    const now = Date.now();
    const direct = knownDirect || await api("/direct");
    const accountId = direct.account_id || connectedAccounts[0]?.account_id || "";
    const query = accountId ? `?account_id=${encodeURIComponent(accountId)}` : "";
    if (dashboardAccountId !== accountId) { cachedHunt = null; cachedDashboard = null; dashboardAccountId = accountId; }
    const huntRequest = force || !cachedHunt || now - huntFetchedAt >= 10000 ? api(`/hunt${query}`).then((value) => { cachedHunt = value; huntFetchedAt = Date.now(); return value; }) : Promise.resolve(cachedHunt);
    const summaryRequest = force || !cachedDashboard || now - dashboardFetchedAt >= 60000 ? api(`/dashboard${query}`).then((value) => { cachedDashboard = value; dashboardFetchedAt = Date.now(); return value; }) : Promise.resolve(cachedDashboard);
    const [hunt, summary] = await Promise.all([huntRequest, summaryRequest]);
    const activeAccountId = direct.account_id || connectedAccounts[0]?.account_id || null;
    const runningAssignments = library.account_modes.filter((value) => value.running && (!activeAccountId || value.account_id.toLowerCase() === activeAccountId.toLowerCase()));
    const runnerActive = hunt.active && direct.connected && runningAssignments.length > 0;
    const connected = direct.connected;
    const state = runnerActive ? "Bot running" : connected ? "Account connected" : "Automation stopped";
    $("#dashboard-state").textContent = state;
    $("#dashboard-dot").className = `live-dot${runnerActive ? " running" : direct.phase === "failed" ? " error" : ""}`;
    const assignment = runningAssignments[0];
    const mode = assignment && library.modes.find((value) => value.mode_id === assignment.mode_id);
    const usesFortress = modeUsesFortress(library, mode);
    $("#dashboard-subtitle").textContent = runnerActive ? `${mode?.name || hunt.label || "Automation"} · ${direct.account_id || hunt.account_id || "connected account"}` : connected ? "Connected and ready to run a bot" : phaseCopy[direct.phase] || "No active session";
    $("#metric-attacks").textContent = compactNumber(hunt.marches);
    $("#metric-returned").textContent = `${compactNumber(hunt.returned)} returned`;
    // Outbound attacks and commanders coming home have very different
    // operational meaning, so do not collapse them into a generic "away"
    // number. Fall back to the old aggregate only when talking to an older
    // daemon during an application update.
    const outbound = hunt.commanders_outbound ?? hunt.in_flight ?? 0;
    const returning = hunt.commanders_returning ?? 0;
    $("#metric-outbound").textContent = compactNumber(outbound);
    $("#metric-returning").textContent = compactNumber(returning);
    $("#metric-coins").textContent = compactNumber(hunt.coins);
    $("#metric-rubies").textContent = compactNumber(hunt.rubies);
    $("#rate-attacks").textContent = compactNumber(summary.attacks_last_hour);
    $("#rate-returns").textContent = compactNumber(summary.returns_last_hour);
    $("#rate-rubies").textContent = compactNumber(summary.rubies_last_hour);
    $("#rate-coins").textContent = compactNumber(summary.coins_last_hour);
    $("#chart-updated").textContent = `Updated ${new Date(summary.generated_at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
    renderRubyChart(summary.ruby_series);
    for (const name of ["rubies", "coins", "attacks", "returns"]) renderSparkBars($(`#bars-${name}`), summary.hourly_bars?.[name]);

    const dashboardAccounts = activeAccountId ? connectedAccounts.filter((account) => account.account_id.toLowerCase() === activeAccountId.toLowerCase()) : connectedAccounts.slice(0, 1);
    const totals = dashboardAccounts.reduce((value, account) => {
      value.castles += account.castle_count || 0; value.commanders += account.commander_count || 0; value.targets += account.rbc_count || 0;
      return value;
    }, { castles: 0, commanders: 0, targets: 0 });
    $("#dashboard-health").replaceChildren(dashboardStat(totals.castles, "Castles"), dashboardStat(totals.commanders, "Commanders"), dashboardStat(totals.targets, "Targets"));

    const setup = $("#dashboard-assignment"); setup.replaceChildren();
    // Current kingdom and View describe where the client is standing, read from
    // the navigation state it actually holds. They are the two facts that make
    // a stateful kingdom transition visible while it happens.
    const currentKingdom = direct.current_kingdom_id == null ? "—" : kingdomLabel(direct.current_kingdom_id);
    const currentView = direct.map_mode ? "Map View" : direct.current_castle_id ? "Castle View" : "—";
    const doing = botActivity(direct);
    // Fortress availability is the one row that can be acted on right now, so it
    // carries the status colour when its window is open.
    // Fortress rows only appear when the running bot actually has a fortress task.
    const fortress = nextFortressSummary(summary);
    const rows = [
      { label: "Account", value: dashboardAccounts[0]?.player_name || assignment?.account_id || hunt.account_id || "None" },
      { label: "Attack bot", value: mode?.name || hunt.label || "None" },
      { label: "Current kingdom", value: currentKingdom, title: "The kingdom the client is standing in right now." },
      { label: "View", value: currentView, title: "Castle view and map view are separate screens in the client; changing kingdom is a stateful transition between them." },
      { label: "Doing", value: doing.text, title: doing.detail },
      { label: "Session", value: connected ? "Connected" : "Disconnected" },
      ...(usesFortress ? [{ label: "Next fortress", value: fortress.text, title: fortress.detail, due: fortress.imminent }] : []),
    ];
    for (const row of rows) {
      const line = element("div", row.due ? "assignment-line due" : "assignment-line");
      line.append(element("span", "", row.label), element("b", "", row.value));
      if (row.title) line.title = row.title;
      setup.append(line);
    }

    const events = (hunt.recent || []).flatMap((march) => {
      const result = [{ at_ms: march.sent_at_ms, kind: "attack", march }];
      if (march.result_at_ms) {
        result.push({ at_ms: march.result_at_ms, kind: "return", march });
        const homeAt = march.result_at_ms + (march.duration_s ?? 0) * 1000;
        if (march.duration_s != null && homeAt <= Date.now()) result.push({ at_ms: homeAt, kind: "home", march });
      }
      return result;
    });
    events.push(...(summary.scan_activity || []).map((scan) => ({ at_ms: scan.at_ms, kind: "scan", scan })));
    if (direct.scan_total && direct.scan_sent + direct.scan_cached < direct.scan_total) events.push({ at_ms: Date.now(), kind: "scanning", direct });
    events.sort((left, right) => right.at_ms - left.at_ms);
    const activity = $("#dashboard-activity");
    const previousScroll = activity.scrollTop;
    activity.replaceChildren();
    const next = !usesFortress ? undefined : (summary.fortress_upcoming || []).find((entry) => entry.available_at_ms >= Date.now() - 60_000);
    if (next) {
      const line = element("div", "activity-row");
      const detail = element("div", "activity-detail");
      detail.append(element("b", "", "Next fortress"), element("small", "", `${kingdomLabel(next.kingdom_id)} · ${next.x}:${next.y}`));
      line.append(element("time", "", "Next"), detail, element("span", "activity-tag", next.available_at_ms <= Date.now() ? "Ready now" : `in ${humanWait(next.available_at_ms - Date.now())}`));
      activity.append(line);
    }
    for (const event of events.slice(0, 80)) {
      const row = element("div", "activity-row");
      const time = document.createElement("time"); time.textContent = new Date(event.at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      const detail = element("div", "activity-detail");
      if (event.kind === "scan") {
        detail.append(element("b", "", `${kingdomLabel(event.scan.kingdom_id)} map`), element("small", "", `${event.scan.windows} ${event.scan.windows === 1 ? "area" : "areas"} learned`));
        row.append(time, detail, element("span", "activity-tag", "Map scan"));
      } else if (event.kind === "scanning") {
        detail.append(element("b", "", "Map scan in progress"), element("small", "", `${event.direct.scan_sent + event.direct.scan_cached} of ${event.direct.scan_total} areas processed`));
        row.append(time, detail, element("span", "activity-tag", "In progress"));
      } else {
        const march = event.march;
        // Named rather than internal: which task ran, the attack it uses, and
        // the kingdom it landed in, instead of a task id and "K1".
        const task = library.tasks.find((value) => value.task_id === march.task_id);
        const where = [kingdomLabel(march.kingdom_id), `${march.x}:${march.y}`, march.level == null ? null : `level ${march.level}`].filter(Boolean).join(" · ");
        const title = event.kind === "home" ? "Commander home (estimated)" : event.kind === "return" ? "Attack resolved · returning home" : "Attack sent";
        const tag = event.kind === "home" ? "Home" : event.kind === "return" ? `${compactNumber(march.ruby_loot || 0)} rubies` : "Sent";
        detail.append(element("b", "", title), element("small", "", `${task?.name || "Attack"} · ${where}`));
        row.append(time, detail, element("span", "activity-tag", tag));
      }
      activity.append(row);
    }
    if (!events.length && !next) activity.append(element("p", "empty-copy", "No recorded activity yet."));
    activity.scrollTop = previousScroll;
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
    safe[key] = /^(pw|password|lt|rct|abt|abtv2|token|login_token|registration_token)$/i.test(key) ? "[removed]" : sanitizedLogValue(child);
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
  const radius = clampRadius($("#scan-radius").value);
  const minutes = Math.ceil(scanAreasFor(radius) * 1.15 / 60);
  $("#scan-estimate").textContent = radius === 0
    ? "No map learning beyond the sign-in viewport."
    : `Radius ${radius}: about ${minutes} min per kingdom for the map. Fortress discovery runs separately.`;
}

/// Map windows a scan has to walk. The live server accepts inclusive 101×101
/// requests, so the radius diameter is rounded up to those large windows.
function scanAreasFor(radius) {
  if (radius <= 0) return 1;
  const side = Math.ceil((radius * 2 + 1) / 101);
  return side * side;
}

function clampRadius(value) {
  return Math.max(0, Math.min(500, Math.floor(Number(value) || 0)));
}

/// The order kingdoms are walked in: Green is the map the sign-in opens on, so
/// it leads, and the three outer kingdoms follow Ice, Sands, Fire so one
/// finishes before the next starts costing requests.
const SCAN_ORDER = [0, 2, 1, 3];

/// What the account asked to initialise, taken from the scan card once it is on
/// screen and from the setup form's single radius before that.
function kingdomScans(fallbackRadius) {
  const rows = [...document.querySelectorAll("#scan-castles .scan-row[data-kingdom]")];
  if (!rows.length) {
    return SCAN_ORDER.map((kingdom_id) => ({ kingdom_id, enabled: true, radius: fallbackRadius }));
  }
  return rows.map((row) => ({
    kingdom_id: Number(row.dataset.kingdom),
    enabled: row.querySelector("input[type=checkbox]").checked,
    radius: clampRadius(row.querySelector("input[type=number]").value),
  }));
}

/// Fortresses exist only in the three outer kingdoms. Green never has one, so
/// saying so is more useful than an empty count that reads like a failed scan.
const FORTRESS_KINGDOM_IDS = new Set([1, 2, 3]);

/// Account summaries contain the durable per-kingdom fortress totals. Keep
/// them live during initialization without rebuilding the Control Panel (and
/// thereby disturbing password/select input) every two seconds.
async function refreshScanHealth() {
  if (scanHealthRefresh) return scanHealthRefresh;
  scanHealthRefresh = api("/accounts").then((accounts) => {
    connectedAccounts = accounts;
    scanHealthFetchedAt = Date.now();
    renderAccountProfiles();
    renderScanPlan();
  }).finally(() => { scanHealthRefresh = null; });
  return scanHealthRefresh;
}

/// How far the map around each castle is learned, how far it should be, and
/// what the fortress walk has found there.
///
/// One row per owned permanent kingdom, for the account being driven. The setup
/// radius is deliberately shared: initialization walks every existing kingdom
/// with the same policy.
function renderScanPlan() {
  const container = $("#scan-castles");
  if (!container) return;
  const account = activeAccount();
  const health = [...(account?.kingdom_health || [])].sort((a, b) => SCAN_ORDER.indexOf(a.kingdom_id) - SCAN_ORDER.indexOf(b.kingdom_id));
  container.replaceChildren();
  $("#scan-actions").hidden = !health.length;
  if (!health.length) { container.append(element("p", "empty-copy", "Initialize an account to choose what to scan.")); return; }
  container.append(element("p", "scan-account", `Showing ${account?.player_name || "the connected account"} · walked in the order below`));
  for (const entry of health) {
    const defaultRadius = clampRadius($("#scan-radius").value) || 50;
    const draftKey = `${account.account_id.toLowerCase()}:${entry.kingdom_id}`;
    const draft = scanDrafts.get(draftKey);
    const pending = entry.fortress_probes_pending || 0;
    const total = entry.fortress_probes_total || 0;
    const name = entry.castle_name || `Castle ${entry.castle_id}`;
    const row = element("div", "scan-row");
    row.dataset.kingdom = String(entry.kingdom_id);
    const info = element("div");
    info.append(element("b", "", `${name} · ${kingdomLabel(entry.kingdom_id)}`));
    const found = [`${entry.target_count || 0} RBCs`];
    found.push(FORTRESS_KINGDOM_IDS.has(entry.kingdom_id) ? `${entry.fortress_count || 0} fortresses` : "no fortresses here");
    const walk = FORTRESS_KINGDOM_IDS.has(entry.kingdom_id) && total
      ? ` · fortress map ${total - pending}/${total} complete`
      : "";
    info.append(element("small", "", found.join(" · ") + walk));
    const bar = element("div", "scan-coverage");
    const fill = element("i");
    fill.style.width = `${total ? Math.round(Math.min(1, (total - pending) / total) * 100) : 0}%`;
    bar.append(fill);
    info.append(bar);
    const control = element("div", "scan-radius");
    const toggle = element("input");
    toggle.type = "checkbox";
    toggle.checked = draft?.enabled ?? true;
    toggle.setAttribute("aria-label", `Initialise ${kingdomLabel(entry.kingdom_id)}`);
    const radius = element("input");
    radius.type = "number";
    radius.min = "0";
    radius.max = "500";
    radius.value = String(draft?.radius ?? defaultRadius);
    radius.setAttribute("aria-label", `Map radius in ${kingdomLabel(entry.kingdom_id)}`);
    const remember = () => scanDrafts.set(draftKey, {
      enabled: toggle.checked,
      radius: clampRadius(radius.value),
    });
    toggle.addEventListener("change", remember);
    radius.addEventListener("input", remember);
    control.append(toggle, radius);
    row.append(info, control);
    container.append(row);
  }
}

/// Live progress of the scan the service is running. This used to sit on the
/// dashboard, where it described something the dashboard could not change.
function renderScanProgress(direct) {
  const total = direct.scan_total || 0;
  const done = (direct.scan_sent || 0) + (direct.scan_cached || 0);
  const stage = direct.scan_stage || (direct.phase === "discovering_fortresses" ? "fortress_mapping" : "rbc");
  const remaining = Math.max(0, total - done);
  $("#scan-bar-fill").style.width = total ? `${Math.round(Math.min(1, done / total) * 100)}%` : "0%";
  if (!total) {
    $("#scan-state").textContent = "Idle";
    $("#scan-progress-copy").textContent = "No scan running.";
  } else if (stage === "rbc") {
    $("#scan-state").textContent = done >= total ? "RBC scan complete" : "Scanning nearby RBCs";
    $("#scan-progress-copy").textContent = `${kingdomLabel(direct.scan_kingdom_id)} · RBC scan ${done}/${total} complete${direct.scan_cached ? ` · ${direct.scan_cached} reused` : ""}`;
  } else if (stage === "fortress_boundary") {
    $("#scan-state").textContent = "Finding fortress boundary";
    $("#scan-progress-copy").textContent = `${kingdomLabel(direct.scan_kingdom_id)} · ${done}% estimated until the boundary is known`;
  } else {
    const etaSeconds = Math.ceil(remaining * 1.5);
    const eta = remaining ? ` · about ${humanWait(etaSeconds * 1000)} remaining` : "";
    $("#scan-state").textContent = remaining ? "Mapping fortresses" : "Fortress scan complete";
    $("#scan-progress-copy").textContent = `${kingdomLabel(direct.scan_kingdom_id)} · fortress map ${done}/${total} complete${eta}`;
  }
}

function updateControlScanIndicators(direct) {
  document.querySelectorAll("[data-control-scan-account]").forEach((indicator) => {
    const active = direct.connected
      && direct.account_id?.toLowerCase() === indicator.dataset.controlScanAccount
      && (["loading_sands", "discovering_fortresses"].includes(direct.phase)
        || direct.bot_state === "refreshing_fortress");
    indicator.hidden = !active;
    if (!active) return;
    const done = (direct.scan_sent || 0) + (direct.scan_cached || 0);
    const total = direct.scan_total || 0;
    if (direct.bot_state === "refreshing_fortress") {
      indicator.textContent = direct.bot_detail || "Refreshing fortress cooldowns";
      return;
    }
    const boundary = direct.scan_stage === "fortress_boundary";
    const kind = direct.scan_stage === "rbc" ? "RBC scan" : boundary ? "Finding fortress boundary" : "Fortress scan";
    indicator.textContent = `${kind} · ${kingdomLabel(direct.scan_kingdom_id)}${total ? boundary ? ` · ${done}% estimated` : ` · ${done}/${total}` : ""}`;
  });
}

let directProgressRefresh = false;
async function refreshDirectProgress() {
  if (directProgressRefresh || !latestDirect.connected
      || !["loading_sands", "discovering_fortresses"].includes(latestDirect.phase)) return;
  directProgressRefresh = true;
  try {
    const direct = await api("/direct");
    latestDirect = direct;
    updateControlScanIndicators(direct);
    if (!$("#view-initialize").hidden) renderScanProgress(direct);
  } catch (_) {
    // The normal connection refresh owns offline/error presentation.
  } finally {
    directProgressRefresh = false;
  }
}

function updateProgress(currentPhase) {
  const order = ["socket_handshake", "authenticating", "loading_castle", "loading_sands", "discovering_fortresses", "sands_ready"];
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
    const scanning = direct.connected && ["loading_sands", "discovering_fortresses"].includes(direct.phase);
    if (scanning && Date.now() - scanHealthFetchedAt >= 4000) await refreshScanHealth();
    renderLinkState(direct);
    headerControls.render();
    updateControlScanIndicators(direct);
    if (!$("#view-initialize").hidden) renderScanProgress(direct);
    $("#phase").textContent = phaseCopy[direct.phase] || "Getting ready…";
    updateProgress(direct.phase);
    if (direct.error) $("#direct-result").textContent = direct.error;
    if (ready) { $("#initialize").disabled = false; $("#initialize").textContent = "Initialize account"; $("#direct-result").textContent = "Account connected and ready."; }
    if (currentLicence?.stage === "unactivated") {
      const licence = await api("/licence");
      currentLicence = licence;
      if (licence.stage === "unactivated") return;
      showApplication(licence);
    }
    const connectionKey = `${direct.connected}:${direct.account_id || ""}:${ready}`;
    if (connectionKey !== renderedConnectionKey) {
      // Connect and disconnect both rewrite the running flags, so the plan view
      // is reloaded before anything reads them.
      await loadLibrary();
      await refreshAccounts(direct);
    }
    if (!$("#view-dashboard").hidden) await refreshDashboard(direct);
  } catch (_) {
    renderLinkState({ connected: false });
    headerControls.render();
  }
}

document.querySelectorAll(".nav-group").forEach((group) => {
  group.querySelector(".nav-parent").addEventListener("click", () => setNavGroup(group, !group.classList.contains("open")));
});
document.querySelectorAll("nav [data-view]").forEach((button) => button.addEventListener("click", async () => {
  switchView(button.dataset.view);
  if (button.dataset.view === "dashboard") await refreshDashboard();
  if (button.dataset.view === "accounts") await refreshAccounts();
  if (button.dataset.view === "initialize") { renderScanPlan(); renderScanProgress(latestDirect); }
  if (button.dataset.view === "presets") renderPresets();
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

$("#task-form").addEventListener("submit", async (event) => { event.preventDefault(); const kingdomId = Number($("#source-kid").value); const automatic = $("#use-main-castle").checked; const fortress = $("#target-kind").value === "fortress"; const destination = fortress ? { kind: "fortress", kingdom_id: kingdomId } : ($("#target-kind").value === "berimond" ? { kind: "berimond_camp", kingdom_id: kingdomId } : { kind: "rbc_level_range", kingdom_id: kingdomId, minimum: Number($("#target-level-min").value), maximum: Number($("#target-level-max").value) }); const draft = { name: $("#task-name").value.trim(), attack_profile_id: $("#task-attack").value, source: { kingdom_id: kingdomId, x: Number($("#source-x").value), y: Number($("#source-y").value) }, source_kind: automatic ? "main_castle" : "coordinate", destination, algorithm: $("#target-algorithm").value, travel: $("#travel").value, priority, commander_count: 1 }; try { await api("/plans/tasks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(draft) }); $("#task-result").textContent = "Dynamic farming task saved."; $("#task-name").value = ""; await loadLibrary(); } catch (error) { $("#task-result").textContent = error.message; } });

$("#mode-form").addEventListener("submit", async (event) => { event.preventDefault(); try { await api("/plans/modes", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name: $("#mode-name").value.trim(), allocations: selectedModeTasks }) }); $("#mode-result").textContent = "Attack bot saved."; selectedModeTasks = []; $("#mode-name").value = ""; await loadLibrary(); await refreshAccounts(); } catch (error) { $("#mode-result").textContent = error.message; } });
$("#copy-mode").addEventListener("click", () => copyJson(bundleForMode($("#mode-name").value.trim(), selectedModeTasks), $("#mode-result")));
$("#load-example").addEventListener("click", async () => { $("#mode-json").value = JSON.stringify(await api("/plans/example"), null, 2); });
$("#import-mode").addEventListener("click", async () => { try { const bundle = JSON.parse($("#mode-json").value); await api("/plans/import", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(bundle) }); $("#import-result").textContent = "Attack bot imported with new attack and task IDs."; await loadLibrary(); } catch (error) { $("#import-result").textContent = error.message; } });

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

$("#licence-form").addEventListener("submit", async (event) => { event.preventDefault(); $("#licence-result").textContent = "Checking your token…"; try { const licence = await api("/licence", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ token: $("#licence-token").value.trim() }) }); $("#licence-token").value = ""; showApplication(licence); if (licence.stage === "unactivated") return; await loadLibrary(); await refreshAccounts(); await refresh(); } catch (error) { $("#licence-result").textContent = error.message; } });
$("#add-credits").addEventListener("click", () => showActivation(true));
$("#close-activation").addEventListener("click", () => { if (currentLicence?.active) activation.hidden = true; });
$("#toggle-password").addEventListener("click", (event) => { const password = $("#password"); const showing = password.type === "text"; password.type = showing ? "password" : "text"; event.currentTarget.textContent = showing ? "Show" : "Hide"; });
$("#direct-form").addEventListener("submit", async (event) => { event.preventDefault(); const button = $("#initialize"); const password = $("#password"); button.disabled = true; button.textContent = "Initializing…"; $("#direct-result").textContent = "OpenAuto is signing in, discovering the account, and preparing its initial map data."; const radius = Number($("#scan-radius").value) || 0; try { await api("/accounts", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ server: $("#server").value, username: $("#player-name").value.trim(), password: password.value, scan_radius: radius, kingdom_scans: kingdomScans(radius), reuse_existing_map: false }) }); password.value = ""; await refresh(); } catch (error) { password.value = ""; button.disabled = false; button.textContent = "Initialize account"; $("#direct-result").textContent = error.message; } });
$("#refresh-accounts").addEventListener("click", refreshAccounts);
$("#add-account").addEventListener("click", () => {
  addingAccount = !addingAccount;
  if (addingAccount) { $("#player-name").value = ""; $("#password").value = ""; }
  refreshAccounts(latestDirect);
  if (addingAccount) $("#player-name").focus();
});
$("#scan-radius").addEventListener("input", updateScanEstimate);
$("#scan-now").addEventListener("click", async () => {
  const account = activeAccount();
  const output = $("#scan-result");
  if (!account) { output.textContent = "Initialize an account first."; return; }
  const password = $("#scan-password").value;
  if (!password) { output.textContent = "Enter the account password to re-scan."; return; }
  const chosen = kingdomScans(clampRadius($("#scan-radius").value) || 50).filter((scan) => scan.enabled);
  output.textContent = chosen.length
    ? `Scanning ${chosen.map((scan) => kingdomLabel(scan.kingdom_id)).join(", ")}…`
    : "Tick at least one kingdom to scan.";
  if (!chosen.length) return;
  try {
    await connectSavedAccount(account, password, clampRadius($("#scan-radius").value) || 50, false, output);
    $("#scan-password").value = "";
    renderScanPlan();
  } catch (error) { output.textContent = error.message; }
});
$("#import-preset").addEventListener("click", async () => {
  try {
    await importPreset(JSON.parse($("#preset-json").value));
    $("#preset-result").textContent = "Config imported with new internal IDs.";
    await loadLibrary();
  } catch (error) { $("#preset-result").textContent = error.message; }
});
$("#refresh-logs").addEventListener("click", refreshLogs);
$("#copy-logs").addEventListener("click", async () => { if (!accountLogText) await refreshLogs(); await navigator.clipboard.writeText(accountLogText); $("#log-result").textContent = "Sanitized logs copied."; });

const headerControls = createHeaderControls({
  read: () => ({ direct: latestDirect, library, accounts: connectedAccounts }),
  connect: (account, password, output) => connectSavedAccount(account, password, 0, true, output),
  refreshAll: async () => { await loadLibrary(); await refreshAccounts(); await refreshDashboard(); },
});

renderKingdoms();
renderServerOptions();
renderWave();
updateScanEstimate();
refreshLicence().then(async (active) => {
  if (!active) return;
  if (currentLicence?.stage === "unactivated") return;
  try {
    await loadLibrary();
    await refreshAccounts();
    await refresh();
  } catch (error) {
    $("#attack-result").textContent = `Could not load the attack catalog: ${error.message}`;
  }
}).catch(() => showActivation(false));
setInterval(refresh, 2000);
setInterval(refreshDirectProgress, 500);
setInterval(() => { if (!$("#view-logs").hidden && latestDirect.connected) refreshLogs(); }, 1500);
