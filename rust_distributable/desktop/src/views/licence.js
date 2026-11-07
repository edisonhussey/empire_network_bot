import { api } from "../api.js";
import { accountContext } from "../account-context.js";
import { element } from "../dom.js";
import { licenceState, planLabel, tokenFrom } from "./licence-model.js";

/// The Licence tab: the licence of the account in view, and a drop box that replaces it.
/// The service decides whether a token fits the account (`licence.rs`, `replace_for_account`);
/// this only shows the answer. One request when the tab opens or the account changes, and
/// one when a token is dropped. The sidebar plan line is drawn from the same answer.

export function createLicence({ onReplaced = () => {} } = {}) {
  const $ = (selector) => document.querySelector(selector);
  const state = { open: false, data: null, error: "", result: "" };

  function row(label, value) {
    const item = element("div", "licence-row");
    item.append(element("span", "", label), element("b", "", value));
    return item;
  }

  function render() {
    $("#licence-expiry").textContent = state.error ? "Unknown" : state.data ? planLabel(state.data) : "Checking…";
    $("#plan-badge").hidden = !(state.data?.licence?.valid && state.data.licence.tier === "pro");
    if (!state.open) return;
    const data = state.data;
    const licence = data?.licence;
    $("#licence-account").textContent = data ? `${data.player_name} · ${data.server}` : "";
    $("#licence-state").textContent = state.error || (data ? licenceState(data) : "Loading…");
    $("#licence-details").replaceChildren(...(licence ? [
      row("Licence", `${licence.subject} (${licence.license_id}, revision ${licence.revision})`),
      row("Plan", licence.tier || "–"),
      row("Time left", planLabel(data)),
      row("Expires", new Date(licence.expires_at * 1000).toLocaleString()),
    ] : []));
    $("#licence-drop-result").textContent = state.result;
  }

  async function load() {
    if (!accountContext.viewed()) { state.data = null; state.error = ""; render(); return; }
    try {
      state.data = await api(`/licence/account${accountContext.query()}`);
      state.error = "";
    } catch (error) {
      state.error = error.message;
    }
    render();
  }

  async function replace(text) {
    const token = tokenFrom(text);
    if (!token) { state.result = "That is not a licence token (it starts with OA2.)."; render(); return; }
    state.result = "Checking the licence…"; render();
    try {
      state.data = await api("/licence/account", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ account_id: accountContext.viewed(), token }),
      });
      state.error = "";
      state.result = "Licence replaced.";
      onReplaced(state.data);
    } catch (error) {
      state.result = error.message;
    }
    render();
  }

  const drop = $("#licence-drop");
  drop.addEventListener("dragover", (event) => { event.preventDefault(); drop.classList.add("dragging"); });
  drop.addEventListener("dragleave", () => drop.classList.remove("dragging"));
  drop.addEventListener("drop", async (event) => {
    event.preventDefault();
    drop.classList.remove("dragging");
    const file = event.dataTransfer.files?.[0];
    replace(file ? await file.text() : event.dataTransfer.getData("text/plain"));
  });
  $("#licence-file").addEventListener("change", async (event) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (file) replace(await file.text());
  });
  $("#licence-paste").addEventListener("submit", (event) => {
    event.preventDefault();
    const field = $("#licence-paste-token");
    replace(field.value);
    field.value = "";
  });

  // Another account in view: its own licence, in the tab and in the sidebar.
  accountContext.subscribe(() => { state.data = null; state.result = ""; load(); });

  return {
    load,
    show() { state.open = true; state.result = ""; load(); },
    hide() { state.open = false; },
  };
}
