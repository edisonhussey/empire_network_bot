import { $, element } from "../dom.js";
import { accountContext, botsRunning, stateLabel } from "../account-context.js";

/// The account menu in the header, to the left of Log in.
///
/// It lists every stored account with its colour, world, tier and what it is doing,
/// and chooses which one the whole window shows. Choosing only changes what is
/// *shown*: every account's session, bot and background work carry on in the service
/// whichever one is in view. "Add account" opens Initialize for a new one.

const dotTone = { ok: "var(--ok)", live: "var(--ok)", warn: "#e6c35c", bad: "#ff7a7a", off: "var(--ink-3)" };

/// `read()` returns `{ library, worldLabel }`; `onAdd()` opens the add-account flow.
export function createAccountSwitcher({ read, onAdd }) {
  const button = $("#account-switch-button");
  const menu = $("#account-menu");
  let key = "";

  const running = (accountId, library) => botsRunning(library, accountId);

  function item(account, library, worldLabel) {
    const status = accountContext.status(account.account_id);
    const state = stateLabel(status, running(account.account_id, library));
    const row = element("button", `bot-menu-item account-item-row${account.account_id.toLowerCase() === accountContext.viewed() ? " checked" : ""}`);
    row.type = "button";
    row.setAttribute("role", "menuitem");
    const swatch = element("i", "account-swatch");
    swatch.style.background = accountContext.colour(account.account_id);
    const text = element("span", "account-item-text");
    text.append(
      element("b", "", account.player_name),
      element("small", "", `${worldLabel(account.endpoint)} · ${state.text}${status.tier ? ` · ${status.tier}` : ""}`),
    );
    const dot = element("i", "account-state-dot");
    dot.style.background = dotTone[state.tone];
    row.append(swatch, text, dot);
    row.addEventListener("click", () => { close(); accountContext.setViewed(account.account_id); });
    return row;
  }

  function close() {
    menu.hidden = true;
    button.setAttribute("aria-expanded", "false");
  }

  /// Redraw the button and, when something it shows changed, the menu.
  function render() {
    const { library, worldLabel } = read();
    const accounts = accountContext.accounts();
    const viewed = accounts.find((account) => account.account_id.toLowerCase() === (accountContext.viewed() || ""));
    const status = accountContext.status();
    const state = stateLabel(status, viewed ? running(viewed.account_id, library) : false);
    $("#account-switch-label").textContent = viewed ? viewed.player_name : "No account";
    const dot = $("#account-switch-dot");
    dot.style.background = viewed ? accountContext.colour(viewed.account_id) : "var(--ink-3)";
    dot.title = viewed ? `${viewed.player_name}: ${state.text}` : "";
    const next = JSON.stringify([accountContext.viewed(), accounts.map((account) => [account.account_id, account.player_name, account.endpoint, accountContext.status(account.account_id).connected, accountContext.status(account.account_id).phase, accountContext.status(account.account_id).tier, running(account.account_id, library)])]);
    if (next === key) return;
    key = next;
    const rows = accounts.map((account) => item(account, library, worldLabel));
    const add = element("button", "bot-menu-item", "Add account…");
    add.type = "button";
    add.addEventListener("click", () => { close(); onAdd(); });
    menu.replaceChildren(element("p", "bot-menu-label", "Accounts"), ...rows, add);
  }

  button.addEventListener("click", () => {
    const open = menu.hidden;
    menu.hidden = !open;
    button.setAttribute("aria-expanded", String(open));
  });
  document.addEventListener("click", (event) => { if (!event.target.closest(".account-switch-wrap")) close(); });
  document.addEventListener("keydown", (event) => { if (event.key === "Escape") close(); });
  accountContext.subscribe(() => { key = ""; render(); });

  return { render };
}
