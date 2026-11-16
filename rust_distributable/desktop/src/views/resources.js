import { api } from "../api.js";
import { element } from "../dom.js";
import { REFINERY_RECIPES, describeRecipes, nextVisitLabel, presetDraft } from "./resources-model.js";

/// The Resources tab: "Create refined resource" (a refinery preset: which recipes to cycle) and
/// "Create resource bot" (the object the Control Panel picks; today it carries one create-resource
/// preset, later resource movement and transfers too).
///
/// The window only stores these. The daemon visits the Green main castle on its own schedule
/// (`docs/architecture/resource-bot.md`); nothing here sends a game packet.
export function createResources({ getLibrary, reload }) {
  const $ = (selector) => document.querySelector(selector);
  const json = (body) => ({ method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });

  function buildChecks() {
    const holder = $("#refine-recipes");
    if (!holder || holder.childElementCount) return;
    for (const recipe of REFINERY_RECIPES) {
      const line = element("label", "support-kingdom");
      const box = document.createElement("input");
      box.type = "checkbox"; box.dataset.recipe = recipe.id;
      line.append(box, element("span", "", `${recipe.label} · 54,400 ${recipe.raw} → 100, 1 h`));
      holder.append(line);
    }
  }

  function row(title, lines, path, resultId) {
    const item = element("article", "library-row");
    const identity = element("div");
    identity.append(element("b", "", title), ...lines.map((line) => element("small", "", line)));
    const remove = element("button", "mini-action danger", "Delete");
    remove.type = "button";
    remove.addEventListener("click", async () => {
      try { await api(path, { method: "DELETE" }); $(resultId).textContent = "Deleted."; await reload(); }
      catch (error) { $(resultId).textContent = error.message; }
    });
    const actions = element("span", "row-actions"); actions.append(remove);
    item.append(identity, actions);
    return item;
  }

  function render() {
    buildChecks();
    const resources = getLibrary().resources || {};
    const presets = resources.create_presets || [];
    const bots = resources.resource_bots || [];
    const select = $("#resource-bot-preset");
    if (select) {
      const keep = select.value;
      select.replaceChildren(...presets.map((preset) => {
        const option = element("option", "", `${preset.name} · ${describeRecipes(preset.recipes)}`);
        option.value = preset.preset_id; return option;
      }));
      if (keep) select.value = keep;
    }
    const presetList = $("#refine-preset-library");
    presetList?.replaceChildren(...presets.map((preset) => row(preset.name, ["Refinery · Green main castle", describeRecipes(preset.recipes)], `/resources/create-presets/${preset.preset_id}`, "#refine-preset-result")));
    if (presetList && !presets.length) presetList.append(element("p", "empty-copy", "No refined-resource presets yet."));
    const botList = $("#resource-bot-library");
    botList?.replaceChildren(...bots.map((bot) => {
      const preset = presets.find((value) => value.preset_id === bot.create_preset_id);
      const using = (resources.account_resource_bots || []).filter((value) => value.resource_bot_id === bot.resource_bot_id && value.running);
      const schedules = using.map((value) => {
        const state = (resources.refinery_states || []).find((item) => item.account_id === value.account_id);
        return `${value.account_id}: ${state ? nextVisitLabel(state.next_due_ms) : "first visit pending"}${state?.last_detail ? ` · ${state.last_detail}` : ""}`;
      });
      return row(bot.name, [`Create resource: ${preset ? preset.name : "none"}`, ...(schedules.length ? schedules : ["Not running"])], `/resources/bots/${bot.resource_bot_id}`, "#resource-bot-result");
    }));
    if (botList && !bots.length) botList.append(element("p", "empty-copy", "No resource bots yet."));
  }

  $("#refine-preset-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const checked = Object.fromEntries([...document.querySelectorAll("#refine-recipes input")].map((box) => [box.dataset.recipe, box.checked]));
    const parsed = presetDraft($("#refine-preset-name").value, checked);
    if (parsed.error) { $("#refine-preset-result").textContent = parsed.error; return; }
    try {
      await api("/resources/create-presets", json(parsed.draft));
      $("#refine-preset-result").textContent = "Refined-resource preset saved. Add it to a resource bot next.";
      $("#refine-preset-name").value = "";
      await reload();
    } catch (error) { $("#refine-preset-result").textContent = error.message; }
  });

  $("#resource-bot-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const preset = $("#resource-bot-preset").value;
    if (!preset) { $("#resource-bot-result").textContent = "Create a refined-resource preset first."; return; }
    try {
      await api("/resources/bots", json({ name: $("#resource-bot-name").value.trim(), create_preset_id: Number(preset) }));
      $("#resource-bot-result").textContent = "Resource bot saved. Pick it in the Control Panel, step 2.";
      $("#resource-bot-name").value = "";
      await reload();
    } catch (error) { $("#resource-bot-result").textContent = error.message; }
  });

  return { render };
}
