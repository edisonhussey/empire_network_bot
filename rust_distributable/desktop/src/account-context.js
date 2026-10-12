/// Which account the window is showing, and what is known about every account.
///
/// The window shows one account at a time, chosen in the header; the service runs
/// all signed-in accounts side by side. Every view asks this module which account it
/// is about instead of assuming one, and is told when that changes. The pieces here
/// that decide something (which account to show first, which colour an account gets,
/// who shares a world) are pure functions, tested in account-context.test.js.

/// Colours for accounts, apart from the red and green the maps already use for
/// hostile and friendly movements.
export const PALETTE = ["#6cc4ff", "#c792ea", "#ff9e64", "#4dd0e1", "#f48fb1", "#b0bec5"];

const sameAccount = (left, right) => String(left ?? "").toLowerCase() === String(right ?? "").toLowerCase();

/// The account to show: the remembered one when it still exists, else the first that
/// is signed in, else the first stored account.
export function chooseViewed(remembered, accounts, statuses) {
  if (remembered && accounts.some((account) => sameAccount(account.account_id, remembered))) {
    return accounts.find((account) => sameAccount(account.account_id, remembered)).account_id;
  }
  const live = accounts.find((account) => statuses.some((status) => status.connected && sameAccount(status.account_id, account.account_id)));
  return (live || accounts[0])?.account_id ?? null;
}

/// Keep every colour already given and hand each new account the lowest unused one
/// (wrapping once all are taken), so adding an account never recolours the others.
export function assignColours(existing, ids) {
  const assigned = { ...existing };
  for (const id of ids) {
    const key = String(id).toLowerCase();
    if (assigned[key] !== undefined) continue;
    const used = new Set(Object.values(assigned));
    let index = 0;
    while (used.has(index) && index < PALETTE.length) index += 1;
    assigned[key] = index % PALETTE.length;
  }
  return assigned;
}

/// One account's status: its session if it has one, else a disconnected stand-in that
/// still carries the id, so code reading `account_id` always reads the viewed account.
export function statusFor(statuses, accountId) {
  const found = statuses.find((status) => sameAccount(status.account_id, accountId));
  return found || { connected: false, phase: "disconnected", account_id: accountId ?? null, bot_state: "stopped" };
}

/// The world an account plays on: the game endpoint of its profile.
export function worldOf(accounts, accountId) {
  return accounts.find((account) => sameAccount(account.account_id, accountId))?.endpoint ?? null;
}

/// Signed-in accounts on the same world as `accountId`, the account itself included
/// when it is signed in. These are the accounts that may see each other on the maps.
export function worldMates(accounts, statuses, accountId) {
  const world = worldOf(accounts, accountId);
  if (!world) return [];
  return accounts.filter((account) => account.endpoint === world
    && statuses.some((status) => status.connected && sameAccount(status.account_id, account.account_id)));
}

/// Whether any bot (attack, recruit or station) is running for the account.
export function botsRunning(library, accountId) {
  const mine = (value) => sameAccount(value.account_id, accountId) && value.running;
  return Boolean(
    (library.account_modes || []).some(mine)
    || (library.account_recruit_bots || []).some(mine)
    || (library.account_station_bots || []).some(mine),
  );
}

/// What the switcher says about an account: running, connected, connecting, failed or
/// offline, with a tone for its dot.
export function stateLabel(status, running) {
  if (status.phase === "failed") return { text: "needs attention", tone: "bad" };
  if (status.connected && running) return { text: "bot running", tone: "ok" };
  if (status.connected && status.phase === "sands_ready") return { text: "connected", tone: "live" };
  if (status.connected) return { text: "connecting…", tone: "warn" };
  return { text: "offline", tone: "off" };
}

function storage() {
  try { return window.localStorage; } catch (_) { return null; }
}

const REMEMBER_KEY = "oa.viewedAccount";
const COLOURS_KEY = "oa.accountColours";

function read(key, fallback) {
  try { return JSON.parse(storage()?.getItem(key) ?? "null") ?? fallback; } catch (_) { return fallback; }
}

function write(key, value) {
  try { storage()?.setItem(key, JSON.stringify(value)); } catch (_) { /* private window: not remembered */ }
}

const state = {
  accounts: [],
  statuses: [],
  viewed: null,
  colours: read(COLOURS_KEY, {}),
  listeners: new Set(),
};

function notify() {
  for (const listener of state.listeners) listener(state.viewed);
}

export const accountContext = {
  viewed: () => state.viewed,
  accounts: () => state.accounts,
  statuses: () => state.statuses,
  status: (id = state.viewed) => statusFor(state.statuses, id),
  colour: (id) => PALETTE[state.colours[String(id).toLowerCase()] ?? 0],
  mates: (id = state.viewed) => worldMates(state.accounts, state.statuses, id),
  /// Tell every view the account changed.
  subscribe(listener) { state.listeners.add(listener); return () => state.listeners.delete(listener); },
  setAccounts(accounts) {
    state.accounts = accounts;
    state.colours = assignColours(state.colours, accounts.map((account) => account.account_id));
    write(COLOURS_KEY, state.colours);
    const next = state.viewed && accounts.some((account) => sameAccount(account.account_id, state.viewed))
      ? state.viewed
      : chooseViewed(read(REMEMBER_KEY, null), accounts, state.statuses);
    if (!sameAccount(next, state.viewed)) { state.viewed = next; notify(); }
  },
  setStatuses(statuses) { state.statuses = statuses; },
  /// Show another account. Remembered, so the window reopens on it.
  setViewed(id) {
    if (sameAccount(id, state.viewed)) return;
    state.viewed = id ? String(id).toLowerCase() : null;
    write(REMEMBER_KEY, state.viewed);
    notify();
  },
  /// `?account_id=...` for the viewed account, or an empty string.
  query(extra = "") {
    const parts = [state.viewed ? `account_id=${encodeURIComponent(state.viewed)}` : "", extra].filter(Boolean);
    return parts.length ? `?${parts.join("&")}` : "";
  },
};
