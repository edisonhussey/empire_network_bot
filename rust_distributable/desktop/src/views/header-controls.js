import { api } from "../api.js";
import { $, element } from "../dom.js";

/// A bot "involves fortresses" when any task in its mode targets one. The target
/// kind lives on the task's subscription, so that is what is read - the same
/// place the task builder writes it.
export function modeUsesFortress(library, mode) {
  if (!mode) return false;
  return (mode.task_ids || []).some(
    (taskId) => library.subscriptions.find((value) => value.task_id === taskId)?.target_kind === "fortress",
  );
}

const sameAccount = (left, right) => String(left).toLowerCase() === String(right).toLowerCase();

/// Log in / log out and the bot picker in the page header.
///
/// This only presents state and forwards intent. Whether a bot may start is the
/// service's decision (it refuses unless the account is connected and ready) and
/// log out stopping the bot is done by the service when the connection closes;
/// the greyed-out states here just keep the window from offering what the
/// service would reject.
///
/// `read()` returns `{ direct, library, accounts }`; `connect(account, password,
/// output)` opens the connection; `refreshAll()` re-reads everything after a change.
export function createHeaderControls({ read, connect, refreshAll }) {
  const sessionButton = $("#session-button");
  const loginForm = $("#login-form");
  const loginPassword = $("#login-password");
  const loginResult = $("#login-result");
  const botButton = $("#bot-menu-button");
  const botMenu = $("#bot-menu");
  let menuKey = "";
  let busy = false;

  function view() {
    const { direct, library, accounts } = read();
    const account = accounts.find((value) => direct.account_id && sameAccount(value.account_id, direct.account_id)) || accounts[0] || null;
    const connected = Boolean(direct.connected);
    const ready = connected && direct.phase === "sands_ready";
    const current = account && library.account_modes.find((value) => sameAccount(value.account_id, account.account_id));
    const currentRecruit = account && library.account_recruit_bots.find((value) => sameAccount(value.account_id, account.account_id));
    const running = Boolean(current?.running && connected);
    return { library, account, connected, ready, current, currentRecruit, running };
  }

  function closeMenu() {
    botMenu.hidden = true;
    botButton.setAttribute("aria-expanded", "false");
  }

  async function startMode(modeId, running) {
    const { account, currentRecruit } = view();
    if (!account || busy) return;
    busy = true;
    closeMenu();
    try {
      await api("/plans/start", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ account_id: account.account_id, mode_id: modeId, recruit_bot_id: currentRecruit?.recruit_bot_id ?? null, running }),
      });
    } catch (error) {
      botButton.title = error.message;
    } finally {
      busy = false;
      await refreshAll();
      render();
    }
  }

  function menuItem(label, onClick, { checked = false, disabled = false, danger = false } = {}) {
    const item = element("button", `bot-menu-item${danger ? " danger" : ""}`, label);
    item.type = "button";
    item.setAttribute("role", "menuitem");
    if (checked) item.classList.add("checked");
    item.disabled = disabled;
    if (onClick) item.addEventListener("click", onClick);
    return item;
  }

  function renderMenu({ library, current, running }) {
    const key = JSON.stringify([running, current?.mode_id, library.modes.map((mode) => [mode.mode_id, mode.name])]);
    if (key === menuKey) return;
    menuKey = key;
    const items = [];
    if (running) items.push(menuItem("Turn off bot", () => startMode(current.mode_id, false), { danger: true }));
    if (library.modes.length) {
      items.push(element("p", "bot-menu-label", running ? "Switch to" : "Start"));
      for (const mode of library.modes) {
        items.push(menuItem(mode.name, () => startMode(mode.mode_id, true), { checked: running && current?.mode_id === mode.mode_id }));
      }
    } else {
      items.push(element("p", "bot-menu-label", "No attack bots yet"));
    }
    botMenu.replaceChildren(...items);
  }

  function render() {
    const state = view();
    const { account, connected, ready, running } = state;

    sessionButton.textContent = connected ? "Log out" : "Log in";
    sessionButton.disabled = busy || (!connected && !account);
    sessionButton.title = !connected && !account ? "Add an account in Initialize first" : "";
    if (connected) loginForm.hidden = true;

    $("#bot-menu-label-text").textContent = running ? "Bot running" : "Bot stopped";
    $("#bot-menu-dot").classList.toggle("on", running);
    // Greyed out unless the account is ready to take a bot; a running bot stays
    // reachable so it can always be turned off.
    botButton.disabled = busy || !connected || (!ready && !running);
    botButton.title = connected ? "" : "Log in to choose a bot";
    if (botButton.disabled) closeMenu();
    renderMenu(state);
  }

  sessionButton.addEventListener("click", async () => {
    const { connected } = view();
    if (!connected) {
      loginForm.hidden = !loginForm.hidden;
      loginResult.textContent = "";
      if (!loginForm.hidden) loginPassword.focus();
      return;
    }
    busy = true;
    render();
    try {
      // The service stops the bot as part of closing the connection.
      await api("/direct", { method: "DELETE" });
    } finally {
      busy = false;
      await refreshAll();
      render();
    }
  });

  loginForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const { account } = view();
    if (!account) return;
    try {
      await connect(account, loginPassword.value, loginResult);
      if (loginPassword.value) loginForm.hidden = true;
    } catch (error) {
      loginResult.textContent = error.message;
    } finally {
      loginPassword.value = "";
      render();
    }
  });

  botButton.addEventListener("click", () => {
    if (botButton.disabled) return;
    const open = botMenu.hidden;
    botMenu.hidden = !open;
    botButton.setAttribute("aria-expanded", String(open));
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".bot-menu-wrap")) closeMenu();
    if (!event.target.closest(".login-wrap")) loginForm.hidden = true;
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") { closeMenu(); loginForm.hidden = true; }
  });

  return { render };
}
