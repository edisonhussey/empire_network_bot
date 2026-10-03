const API = "http://127.0.0.1:47821/v1";

// Matches the store's `STALE_MARCH_MILLIS`: a march sent longer ago than this
// without a return is treated as lost rather than still in flight.
const STALE_MARCH_MILLIS = 60 * 60 * 1000;
const activation = document.querySelector("#activation");
const appShell = document.querySelector("#app-shell");
const licenceResult = document.querySelector("#licence-result");
const closeActivation = document.querySelector("#close-activation");
const status = document.querySelector("#status");
const gateway = document.querySelector("#gateway-label");
const phase = document.querySelector("#phase");
const accounts = document.querySelector("#accounts");
const directResult = document.querySelector("#direct-result");
const initializeButton = document.querySelector("#initialize");
const usernameInput = document.querySelector("#player-name");
const runLabel = document.querySelector("#run-label");
const runState = document.querySelector("#run-state");
const runTasks = document.querySelector("#run-tasks");
const runRecent = document.querySelector("#run-recent");
let currentLicence = null;

const USERNAME_STORAGE_KEY = "openauto.username";

function rememberedUsername() {
  try {
    return localStorage.getItem(USERNAME_STORAGE_KEY) || "";
  } catch {
    return "";
  }
}

function rememberUsername(username) {
  try {
    localStorage.setItem(USERNAME_STORAGE_KEY, username);
  } catch {
    return;
  }
}

usernameInput.value = rememberedUsername();

const phaseCopy = {
  disconnected: "Ready to connect",
  socket_handshake: "Opening a secure connection…",
  awaiting_room: "Contacting US1…",
  awaiting_version: "Preparing your session…",
  authenticating: "Signing in…",
  authenticated: "Sign-in complete",
  loading_account: "Discovering your account…",
  loading_castle: "Finding your castles…",
  loading_sands: "Preparing Burning Sands…",
  sands_ready: "Setup complete",
  failed: "Needs attention",
};

function expiryLabel(epoch) {
  if (!epoch) return "Token required";
  const remaining = Math.max(0, epoch * 1000 - Date.now());
  const days = Math.floor(remaining / 86400000);
  const hours = Math.floor((remaining % 86400000) / 3600000);
  return days > 0 ? `${days}d ${hours}h remaining` : `${hours}h remaining`;
}

function showActivation(canClose = false) {
  activation.hidden = false;
  closeActivation.hidden = !canClose;
}

function showApplication(licence) {
  currentLicence = licence;
  activation.hidden = true;
  appShell.hidden = false;
  document.querySelector("#licence-expiry").textContent = expiryLabel(licence.expires_at);
}

async function responseJson(response) {
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Request failed (${response.status})`);
  return body;
}

async function refreshLicence() {
  const licence = await responseJson(await fetch(`${API}/licence`));
  currentLicence = licence;
  if (licence.active) {
    showApplication(licence);
    return true;
  }
  appShell.hidden = true;
  showActivation(false);
  licenceResult.textContent = licence.reason || "A valid access token is required.";
  return false;
}

function emptyAccounts() {
  const empty = document.createElement("div");
  empty.className = "empty-state";
  const icon = document.createElement("i");
  icon.textContent = "+";
  const copy = document.createElement("p");
  copy.textContent = "Your connected accounts will appear here.";
  empty.append(icon, copy);
  accounts.replaceChildren(empty);
}

async function refreshAccounts() {
  const list = await responseJson(await fetch(`${API}/accounts`));
  renderAccounts(list, await currentDirectStatus());
}

/// Everything we learned about an account is kept, so it can be shown on a later
/// visit even when nothing is signed in. An empty screen is never the more
/// honest answer than the state we already hold.
function renderAccounts(list, direct) {
  if (!list.length) {
    emptyAccounts();
    return;
  }
  const connected = Boolean(direct?.connected);
  accounts.replaceChildren(...list.map((account) => {
    const card = document.createElement("article");
    card.className = "account-item";
    const avatar = document.createElement("div");
    avatar.className = "account-avatar";
    avatar.textContent = account.player_name.slice(0, 1).toUpperCase();
    const identity = document.createElement("div");
    const title = document.createElement("strong");
    title.textContent = account.player_name;
    const server = document.createElement("span");
    server.textContent = connected
      ? "US1 · Connected"
      : `US1 · Last seen ${relativeTime(account.initialized_at_ms)}`;
    identity.append(title, server);
    const facts = document.createElement("dl");
    for (const [name, value] of [["Castles", account.castle_count], ["Commanders", account.commander_count], ["Targets", account.rbc_count]]) {
      const group = document.createElement("div");
      const detail = document.createElement("dd"); detail.textContent = value;
      const term = document.createElement("dt"); term.textContent = name;
      group.append(detail, term);
      facts.append(group);
    }
    card.append(avatar, identity, facts);
    return card;
  }));
}

/// Human-readable age, used for "last seen" rather than a raw timestamp.
function relativeTime(ms) {
  if (!ms) return "unknown";
  const seconds = Math.max(0, (Date.now() - ms) / 1000);
  if (seconds < 90) return "just now";
  const minutes = seconds / 60;
  if (minutes < 90) return `${Math.round(minutes)}m ago`;
  const hours = minutes / 60;
  if (hours < 36) return `${Math.round(hours)}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

async function currentDirectStatus() {
  try {
    return await responseJson(await fetch(`${API}/direct`));
  } catch {
    return null;
  }
}

const RUN_FIELDS = {
  "#run-marches": "marches",
  "#run-inflight": "in_flight",
  "#run-coins": "coins",
  "#run-rubies": "rubies",
};

function renderRun(summary) {
  runLabel.textContent = summary.label || "No run yet";
  const active = Boolean(summary.active);
  runState.textContent = summary.marches ? (active ? "Running" : "Stopped") : "No data";
  runState.className = active ? "status connected" : "status";
  for (const [selector, key] of Object.entries(RUN_FIELDS)) {
    document.querySelector(selector).textContent = Number(summary[key] || 0).toLocaleString();
  }
  runTasks.replaceChildren(...(summary.tasks || []).map((task) => {
    const item = document.createElement("li");
    const name = document.createElement("b");
    name.textContent = task.task_id;
    const band = document.createElement("span");
    const levels =
      task.level_min == null
        ? ""
        : task.level_min === task.level_max
          ? `level ${task.level_min}`
          : `levels ${task.level_min}–${task.level_max}`;
    const commanders = (task.commanders || []).length;
    band.className = "run-task-band";
    band.textContent = [levels, commanders ? `${commanders} commanders` : ""]
      .filter(Boolean)
      .join(" · ");
    const detail = document.createElement("span");
    detail.className = "run-task-totals";
    detail.textContent = `${task.marches} sent · ${task.returned} back · ${Number(task.coins).toLocaleString()} coins`;
    item.append(name, band, detail);
    return item;
  }));
  runRecent.replaceChildren(...(summary.recent || []).slice(0, 6).map((march) => {
    const row = document.createElement("div");
    const where = document.createElement("span");
    where.textContent = `${march.kingdom_id} \u00b7 ${march.x}:${march.y}`;
    const outcome = document.createElement("span");
    if (march.status === "returning") {
      outcome.textContent = `${Number(march.coin_loot || 0).toLocaleString()} coins`;
    } else if (!summary.active || Date.now() - march.sent_at_ms > STALE_MARCH_MILLIS) {
      // Either no run is alive to receive it, or it was sent long ago.
      outcome.textContent = "no return";
      outcome.className = "run-stale";
    } else {
      outcome.textContent = "in flight";
    }
    row.append(where, outcome);
    return row;
  }));
}

function updateProgress(currentPhase, remembered) {
  const order = ["socket_handshake", "authenticating", "loading_castle", "loading_sands", "sands_ready"];
  const position = order.indexOf(currentPhase);
  const completed = currentPhase === "sands_ready" ? 3 : position >= 3 ? 2 : position >= 1 ? 1 : 0;
  document.querySelectorAll(".progress-list > div").forEach((item, index) => {
    // A remembered account with castles and commanders already completed setup,
    // even if nothing is signed in right now.
    const done = index < completed || currentPhase === "sands_ready" || remembered;
    item.classList.toggle("done", done);
    item.classList.toggle("current", !remembered && index === completed && currentPhase !== "sands_ready" && currentPhase !== "disconnected");
  });
}

async function refresh() {
  try {
    if (!currentLicence?.active && !(await refreshLicence())) return;
    const [health, direct, run, remembered] = await Promise.all([
      fetch(`${API}/health`).then(responseJson),
      fetch(`${API}/direct`).then(responseJson),
      fetch(`${API}/hunt`).then(responseJson).catch(() => null),
      fetch(`${API}/accounts`).then(responseJson).catch(() => []),
    ]);
    if (run) renderRun(run);
    if (!health.licence_active) {
      currentLicence = null;
      await refreshLicence();
      return;
    }
    // The account list is stored state, so it is rendered on every visit; only
    // the "connected" wording depends on a live session.
    renderAccounts(remembered, direct);
    const ready = direct.phase === "sands_ready";
    const seen = remembered.length > 0;
    status.textContent = ready ? "Account ready" : direct.connected ? "Setting up" : seen ? "Signed out" : "OpenAuto ready";
    status.className = ready ? "status connected" : "status";
    gateway.textContent = direct.connected ? "Connected to US1" : "OpenAuto is ready";
    phase.textContent = direct.connected
      ? phaseCopy[direct.phase] || "Getting ready…"
      : seen
        ? `Last set up ${relativeTime(remembered[0].initialized_at_ms)}`
        : phaseCopy[direct.phase] || "Getting ready…";
    updateProgress(direct.phase, seen);
    if (direct.error) directResult.textContent = direct.error;
    if (ready) {
      initializeButton.disabled = false;
      initializeButton.textContent = "Connect account";
      directResult.textContent = "Account connected and ready.";
    } else if (seen && !direct.connected) {
      initializeButton.disabled = false;
      initializeButton.textContent = "Connect account";
      directResult.textContent = `Signed in as ${remembered[0].player_name} previously. Enter the password to sign in again.`;
    }
  } catch (_) {
    status.textContent = "OpenAuto unavailable";
    status.className = "status error";
    gateway.textContent = "Reconnecting…";
  }
}

document.querySelector("#licence-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  licenceResult.textContent = "Checking your token…";
  try {
    const licence = await responseJson(await fetch(`${API}/licence`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ token: document.querySelector("#licence-token").value.trim() }),
    }));
    document.querySelector("#licence-token").value = "";
    showApplication(licence);
    await refresh();
  } catch (error) {
    licenceResult.textContent = error.message;
  }
});

document.querySelector("#add-credits").addEventListener("click", () => showActivation(true));
closeActivation.addEventListener("click", () => {
  if (currentLicence?.active) activation.hidden = true;
});

document.querySelector("#toggle-password").addEventListener("click", (event) => {
  const password = document.querySelector("#password");
  const showing = password.type === "text";
  password.type = showing ? "password" : "text";
  event.currentTarget.textContent = showing ? "Show" : "Hide";
  event.currentTarget.setAttribute("aria-label", showing ? "Show password" : "Hide password");
});

document.querySelector("#direct-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  initializeButton.disabled = true;
  initializeButton.textContent = "Connecting…";
  directResult.textContent = "OpenAuto is signing in and preparing your account.";
  const password = document.querySelector("#password");
  const username = usernameInput.value.trim();
  try {
    await responseJson(await fetch(`${API}/accounts`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        server: document.querySelector("#server").value,
        username,
        password: password.value,
      }),
    }));
    rememberUsername(username);
    password.value = "";
    await refresh();
  } catch (error) {
    initializeButton.disabled = false;
    initializeButton.textContent = "Connect account";
    directResult.textContent = error.message;
  }
});

document.querySelector("#refresh-accounts").addEventListener("click", refreshAccounts);
refreshLicence().then((active) => { if (active) refresh(); }).catch(() => showActivation(false));
setInterval(refresh, 2000);
