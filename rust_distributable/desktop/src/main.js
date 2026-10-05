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
let dashboardAccountId = null;
let addingAccount = false;
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

/// Kingdom display name from an id, so activity rows can say "Burning Sands"
/// rather than "K1".
function kingdomLabel(kingdomId) {
  const found = library.kingdoms.find((value) => value.id === kingdomId) || fallbackKingdoms.find((value) => value.id === kingdomId);
  return kingdomName(found?.name || `Kingdom ${kingdomId}`);
}

/// How long a session has been up, as "2h30" or "45m".
function sessionDuration(ms) {
  if (!ms) return "";
  const minutes = Math.floor((Date.now() - ms) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, "0")}`;
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
    ? "OpenAuto chooses a learned fortress in this kingdom."
    : "OpenAuto chooses a learned Robber Baron in this level range; no destination coordinate is required.";
  updateSourceCoordinates();
}

function updateSourceCoordinates() {
  const automatic = $("#use-main-castle").checked;
  const kingdomId = Number($("#source-kid").value);
  const health = connectedAccounts.flatMap((account) => account.kingdom_health || []).find((value) => value.kingdom_id === kingdomId);
  for (const input of [$("#source-x"), $("#source-y")]) input.disabled = automatic;
  $(".source-coordinates").hidden = automatic;
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
    const targetLabel = subscription?.target_kind === "fortress" ? "Fortress placeholder" : "Robber Baron";
    const levelLabel = subscription?.target_kind === "fortress" ? "" : task.target_level_min === null ? "fixed target" : `levels ${task.target_level_min}–${task.target_level_max}`;
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
    body: JSON.stringify({ server: serverFor(account.endpoint), username: account.player_name, password, scan_radius: radius, reuse_existing_map: reuseExistingMap }),
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
    connection.append(password, scanRow, connectionResult);
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
    attackSelect.append(...library.modes.map((mode) => { const option = element("option", "", `${mode.name} · ${mode.commander_count} commander${mode.commander_count === 1 ? "" : "s"}`); option.value = mode.mode_id; return option; }));
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

// STYLE: ruby chart redrawn as a single 1px white line over dashed hairline gridlines.
// No area fill, no gradient, no colour. Both vertical bounds follow the
// lifetime data so a large historic total does not flatten recent changes.
// The viewBox is close to the width it is rendered at - the chart lives in the
// right-hand dashboard column - so the axis text stays legible instead of being
// scaled down with the drawing.
function renderRubyChart(series) {
  const container = $("#ruby-chart");
  if (!series?.length) { container.replaceChildren(element("p", "empty-copy", "No ruby returns recorded yet.")); return; }
  const width = 440; const height = 300; const left = 40; const right = 8; const top = 12; const bottom = 34;
  const minimumValue = Math.min(...series.map((point) => point.value));
  const maximumValue = Math.max(1, ...series.map((point) => point.value));
  const spread = Math.max(1, maximumValue - minimumValue);
  const rough = spread / 4; const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 5, 10].map((unit) => unit * magnitude).find((value) => value >= rough);
  const floor = Math.max(0, Math.floor(minimumValue / step) * step);
  const ceiling = Math.max(floor + step, Math.ceil(maximumValue / step) * step);
  const plotW = width - left - right; const plotH = height - top - bottom;
  const px = (index) => left + index * plotW / Math.max(1, series.length - 1);
  const py = (value) => top + plotH - (value - floor) * plotH / (ceiling - floor);
  const points = series.map((point, index) => `${px(index).toFixed(1)},${py(point.value).toFixed(1)}`).join(" ");
  let grid = "";
  for (let tick = floor; tick <= ceiling; tick += step) {
    grid += `<line class="grid" x1="${left}" y1="${py(tick)}" x2="${width - right}" y2="${py(tick)}"/><text x="${left - 10}" y="${py(tick) + 4}" text-anchor="end">${tick >= 1000 ? `${tick / 1000}k` : tick}</text>`;
  }
  const labels = 4; let axis = "";
  for (let i = 0; i < labels; i += 1) {
    const index = Math.round(i * (series.length - 1) / (labels - 1));
    axis += `<text x="${px(index)}" y="${height - 8}" text-anchor="${i === 0 ? "start" : i === labels - 1 ? "end" : "middle"}">${new Date(series[index].at_ms).toLocaleDateString([], { month: "short", day: "numeric" })}</text>`;
  }
  container.innerHTML = `<svg viewBox="0 0 ${width} ${height}">${grid}${axis}<polyline class="line" points="${points}"/></svg>`;
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
    const scanProgress = direct.scan_total ? `${kingdomLabel(direct.scan_kingdom_id)} scan ${direct.scan_sent + direct.scan_cached}/${direct.scan_total} · ${direct.scan_cached} cached` : null;
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

    const dashboardAccounts = activeAccountId ? connectedAccounts.filter((account) => account.account_id.toLowerCase() === activeAccountId.toLowerCase()) : connectedAccounts.slice(0, 1);
    const totals = dashboardAccounts.reduce((value, account) => {
      value.castles += account.castle_count || 0; value.commanders += account.commander_count || 0; value.targets += account.rbc_count || 0;
      return value;
    }, { castles: 0, commanders: 0, targets: 0 });
    $("#dashboard-health").replaceChildren(dashboardStat(totals.castles, "Castles"), dashboardStat(totals.commanders, "Commanders"), dashboardStat(totals.targets, "Targets"));

    const setup = $("#dashboard-assignment"); setup.replaceChildren();
    const currentKingdom = direct.current_kingdom_id == null ? "Unknown" : kingdomLabel(direct.current_kingdom_id);
    const currentView = direct.map_mode ? "Map View" : direct.current_castle_id ? "Inside Castle" : "Loading";
    for (const [label, value] of [["Account", dashboardAccounts[0]?.player_name || assignment?.account_id || hunt.account_id || "None"], ["Attack bot", mode?.name || hunt.label || "None"], ["Current kingdom", currentKingdom], ["View", currentView], ["Session", connected ? "Connected" : "Disconnected"]]) {
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
        const attack = task ? library.attacks.find((value) => value.profile_id === task.profile_id) : null;
        const where = [kingdomLabel(march.kingdom_id), `${march.x}:${march.y}`, march.level == null ? null : `level ${march.level}`].filter(Boolean).join(" · ");
        detail.append(element("b", "", task?.name || "Attack"), element("small", "", `${attack?.name || "Unknown attack"} · ${where}`));
        row.append(time, detail, element("span", "activity-tag", humanize(march.status)));
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
  const areas = scanAreasFor(radius);
  const side = Math.ceil(Math.sqrt(areas));
  const minimumMinutes = Math.ceil(areas * 0.7 / 60);
  const maximumMinutes = Math.ceil(areas * 1.3 / 60);
  const grid = radius === 0 ? "the observed 3×2 viewport" : `a ${side}×${side} grid`;
  $("#scan-estimate").textContent = `Radius ${radius} uses ${grid} per available permanent kingdom (${areas} paced requests each) · roughly ${minimumMinutes}–${maximumMinutes} min per kingdom if none are cached.`;
}

/// Map areas a scan has to walk: the client asks in 13-area windows, so the grid
/// is the radius doubled and rounded up to whole windows.
function scanAreasFor(radius) {
  if (radius <= 0) return 6;
  const side = Math.ceil((radius * 2 + 1) / 13);
  return side * side;
}

function clampRadius(value) {
  return Math.max(0, Math.min(500, Math.floor(Number(value) || 0)));
}

const PERMANENT_KINGDOM_IDS = new Set([0, 1, 2, 3]);

/// How far the map around each castle is learned, and how far it should be.
///
/// One row per owned permanent kingdom. The single setup radius is deliberately
/// shared: initialization walks every existing kingdom with the same policy.
function renderScanPlan() {
  const container = $("#scan-castles");
  if (!container) return;
  const health = connectedAccounts[0]?.kingdom_health || [];
  container.replaceChildren();
  $("#scan-actions").hidden = !health.length;
  if (!health.length) { container.append(element("p", "empty-copy", "Initialize an account to set a scan radius per castle.")); return; }
  for (const entry of health) {
    const scannable = PERMANENT_KINGDOM_IDS.has(entry.kingdom_id);
    const radius = clampRadius($("#scan-radius").value) || 50;
    const target = scanAreasFor(radius);
    const learned = entry.scan_window_count || 0;
    const name = entry.castle_name || `Castle ${entry.castle_id}`;
    const row = element("div", scannable ? "scan-row" : "scan-row read-only");
    const info = element("div");
    info.append(element("b", "", `${name} · ${kingdomLabel(entry.kingdom_id)}`));
    info.append(element("small", "", scannable
      ? `Shared radius ${radius} covers ${target} map areas · ${learned} learned`
      : `${learned} map areas learned · ${entry.last_scanned_at_ms ? `last scanned ${relativeTime(entry.last_scanned_at_ms)}` : "not scanned yet"}`));
    const bar = element("div", "scan-coverage");
    const fill = element("i");
    fill.style.width = `${Math.round(Math.min(1, learned / target) * 100)}%`;
    bar.append(fill);
    info.append(bar);
    const control = element("div", "scan-radius");
    if (scannable) control.append(element("span", "scan-tag", `radius ${radius}`));
    row.append(info, control);
    container.append(row);
  }
}

/// Live progress of the scan the service is running. This used to sit on the
/// dashboard, where it described something the dashboard could not change.
function renderScanProgress(direct) {
  const total = direct.scan_total || 0;
  const done = (direct.scan_sent || 0) + (direct.scan_cached || 0);
  $("#scan-bar-fill").style.width = total ? `${Math.round(Math.min(1, done / total) * 100)}%` : "0%";
  $("#scan-state").textContent = !total ? "Idle" : done >= total ? "Complete" : "Scanning";
  $("#scan-progress-copy").textContent = total
    ? `${kingdomLabel(direct.scan_kingdom_id)} · ${done} of ${total} map areas processed · ${direct.scan_cached || 0} reused from cache`
    : "No scan running.";
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
    renderLinkState(direct);
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

$("#task-form").addEventListener("submit", async (event) => { event.preventDefault(); const kingdomId = Number($("#source-kid").value); const automatic = $("#use-main-castle").checked; const fortress = $("#target-kind").value === "fortress"; const destination = fortress ? { kind: "fortress", kingdom_id: kingdomId } : { kind: "rbc_level_range", kingdom_id: kingdomId, minimum: Number($("#target-level-min").value), maximum: Number($("#target-level-max").value) }; const draft = { name: $("#task-name").value.trim(), attack_profile_id: $("#task-attack").value, source: { kingdom_id: kingdomId, x: Number($("#source-x").value), y: Number($("#source-y").value) }, source_kind: automatic ? "main_castle" : "coordinate", destination, algorithm: $("#target-algorithm").value, travel: $("#travel").value, priority, commander_count: 1 }; try { await api("/plans/tasks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(draft) }); $("#task-result").textContent = "Dynamic farming task saved."; $("#task-name").value = ""; await loadLibrary(); } catch (error) { $("#task-result").textContent = error.message; } });

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
$("#direct-form").addEventListener("submit", async (event) => { event.preventDefault(); const button = $("#initialize"); const password = $("#password"); button.disabled = true; button.textContent = "Initializing…"; $("#direct-result").textContent = "OpenAuto is signing in, discovering the account, and preparing its initial map data."; try { await api("/accounts", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ server: $("#server").value, username: $("#player-name").value.trim(), password: password.value, scan_radius: Number($("#scan-radius").value) || 0, reuse_existing_map: false }) }); password.value = ""; await refresh(); } catch (error) { password.value = ""; button.disabled = false; button.textContent = "Initialize account"; $("#direct-result").textContent = error.message; } });
$("#refresh-accounts").addEventListener("click", refreshAccounts);
$("#add-account").addEventListener("click", () => {
  addingAccount = !addingAccount;
  if (addingAccount) { $("#player-name").value = ""; $("#password").value = ""; }
  refreshAccounts(latestDirect);
  if (addingAccount) $("#player-name").focus();
});
$("#scan-radius").addEventListener("input", updateScanEstimate);
$("#scan-now").addEventListener("click", async () => {
  const account = connectedAccounts[0];
  const output = $("#scan-result");
  if (!account) { output.textContent = "Initialize an account first."; return; }
  const password = $("#scan-password").value;
  if (!password) { output.textContent = "Enter the account password to re-scan."; return; }
  const radius = clampRadius($("#scan-radius").value) || 50;
  output.textContent = `Scanning every available permanent kingdom at radius ${radius}…`;
  try {
    await connectSavedAccount(account, password, radius, false, output);
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
$("#dashboard-refresh").addEventListener("click", () => refreshDashboard(null, true));
$("#refresh-logs").addEventListener("click", refreshLogs);
$("#copy-logs").addEventListener("click", async () => { if (!accountLogText) await refreshLogs(); await navigator.clipboard.writeText(accountLogText); $("#log-result").textContent = "Sanitized logs copied."; });

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
setInterval(() => { if (!$("#view-logs").hidden && latestDirect.connected) refreshLogs(); }, 1500);
